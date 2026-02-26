//! Composable data-access abstraction.
//!
//! The framework defines the traits and a ready-to-use in-memory
//! implementation. User crates provide production adapters (sqlx, etc.)
//! only when they need them.
//!
//! # Design goals
//!
//! - **Composable**: `QuerySpec<T>` is pure data; filters/ordering/paging compose
//!   via builder methods.
//! - **Referentially transparent**: no hidden state, no side effects in the spec.
//! - **OpenAPI-ready**: `Filter<T>` is split into `RuntimeFilter` (opaque closures,
//!   used for in-memory impls) and a `TypedFilter` stub (empty enum for now, will
//!   be extended with field-accessor predicates when the sqlx adapter lands —
//!   typed predicates can be both executed as SQL and described as OpenAPI query
//!   parameters).
//! - **Zero boilerplate for prototyping**: `InMemoryRepository<T, Id>` handles all
//!   Mutex/HashMap/AtomicU64 internals. Users implement `HasId` (2 methods) and
//!   call `InMemoryRepository::new()`.
//!
//! # Example
//!
//! ```ignore
//! use rust_api::prelude::*;
//!
//! // 1. Implement HasId on your domain type (the only user-space wiring).
//! impl HasId<u64> for User {
//!     fn get_id(&self) -> u64 { self.id }
//!     fn set_id(&mut self, id: u64) { self.id = id; }
//! }
//!
//! // 2. Spin up the repo — no Mutex, no HashMap, no AtomicU64 in sight.
//! let repo = Arc::new(InMemoryRepository::<User, u64>::new());
//!
//! // 3. Query with a composable spec.
//! let spec = QuerySpec::new().limit(20).offset(0);
//! let users = repo.find(spec).await?;
//! ```

use std::collections::HashMap;
use std::hash::Hash;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::error::Result;

// ---------------------------------------------------------------------------
// HasId<Id> — the only trait users implement
// ---------------------------------------------------------------------------

/// Gives the repository access to an entity's identity field.
///
/// Implement this on your domain aggregate root. Two methods, no logic:
///
/// ```rust,ignore
/// impl HasId<u64> for User {
///     fn get_id(&self) -> u64 { self.id }
///     fn set_id(&mut self, id: u64) { self.id = id; }
/// }
/// ```
///
/// The repository treats an entity as **new** (needing ID assignment) when
/// `get_id()` returns `Id::default()` — for `u64` that is `0`, which is the
/// natural sentinel value.
pub trait HasId<Id: Clone + Default + PartialEq>: Sized {
    fn get_id(&self) -> Id;
    fn set_id(&mut self, id: Id);

    /// Returns `true` when the entity has not yet been assigned an ID.
    fn is_new(&self) -> bool {
        self.get_id() == Id::default()
    }
}

// ---------------------------------------------------------------------------
// Filter<T> — runtime-only for now, typed stub preserved for future
// ---------------------------------------------------------------------------

/// A single filter predicate on entity `T`.
///
/// - `Runtime` — a type-erased closure. Works for in-memory repositories.
///   Opaque to codegen; cannot be translated to SQL or described as an
///   OpenAPI query parameter.
///
/// - `Typed` — an empty stub. Will hold field-accessor-based predicates when
///   the sqlx adapter arrives. Typed predicates can be rendered to SQL and
///   inspected by the OpenAPI generator.
pub enum Filter<T> {
    Runtime(Box<dyn Fn(&T) -> bool + Send + Sync + 'static>),
    Typed(TypedFilter<T>),
}

/// Placeholder for future typed predicates (e.g. `User::email.eq("foo")`).
///
/// Empty for now — no variants means it can never be constructed, which is a
/// compile-time guarantee that no typed-filter code paths exist yet.
pub enum TypedFilter<T> {
    _Phantom(PhantomData<T>, std::convert::Infallible),
}

// ---------------------------------------------------------------------------
// QuerySpec<T> — composable, immutable query description
// ---------------------------------------------------------------------------

/// A composable description of a query against a `Repository<T, Id>`.
///
/// Pure data — no I/O, no side effects. Accumulate filters and options via
/// builder methods, then pass to `repo.find(spec)`.
pub struct QuerySpec<T> {
    pub filters: Vec<Filter<T>>,
    pub order_by: Option<fn(&T, &T) -> std::cmp::Ordering>,
    pub limit: Option<usize>,
    pub offset: usize,
}

impl<T> Default for QuerySpec<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> QuerySpec<T> {
    pub fn new() -> Self {
        Self {
            filters: Vec::new(),
            order_by: None,
            limit: None,
            offset: 0,
        }
    }

    /// Add a runtime filter closure. Opaque to codegen.
    pub fn filter_runtime(mut self, f: impl Fn(&T) -> bool + Send + Sync + 'static) -> Self {
        self.filters.push(Filter::Runtime(Box::new(f)));
        self
    }

    pub fn order_by(mut self, cmp: fn(&T, &T) -> std::cmp::Ordering) -> Self {
        self.order_by = Some(cmp);
        self
    }

    pub fn limit(mut self, n: usize) -> Self {
        self.limit = Some(n);
        self
    }

    pub fn offset(mut self, n: usize) -> Self {
        self.offset = n;
        self
    }

    /// Returns `true` if the entity passes every filter in this spec.
    pub fn matches(&self, entity: &T) -> bool {
        self.filters.iter().all(|f| match f {
            Filter::Runtime(pred) => pred(entity),
            Filter::Typed(_) => unreachable!("TypedFilter has no variants"),
        })
    }
}

// ---------------------------------------------------------------------------
// Repository<T, Id> — async trait
// ---------------------------------------------------------------------------

/// Async data-access trait. Implement this for any backing store.
///
/// For prototyping and tests use `InMemoryRepository<T, Id>` — it is provided
/// by the framework and requires zero user-space boilerplate. Only write a
/// custom impl when targeting a real database.
pub trait Repository<T: Send + Sync + 'static, Id: Send + Sync>: Send + Sync {
    fn find_by_id(
        &self,
        id: &Id,
    ) -> impl std::future::Future<Output = Result<Option<T>>> + Send;

    fn find(
        &self,
        spec: QuerySpec<T>,
    ) -> impl std::future::Future<Output = Result<Vec<T>>> + Send;

    fn save(&self, entity: T) -> impl std::future::Future<Output = Result<T>> + Send;

    fn delete(&self, id: &Id) -> impl std::future::Future<Output = Result<bool>> + Send;
}

// ---------------------------------------------------------------------------
// InMemoryRepository<T, Id> — framework-provided implementation
// ---------------------------------------------------------------------------

/// A ready-to-use in-memory `Repository<T, Id>`.
///
/// All concurrency primitives (Mutex, HashMap, AtomicU64) are hidden inside
/// the framework. User code never touches them.
///
/// # Usage
///
/// Implement `HasId` on your domain type, then call `InMemoryRepository::new()`
/// for `u64` IDs or `InMemoryRepository::with_id_gen(|| ...)` for any other
/// ID type:
///
/// ```rust,ignore
/// // u64 IDs — most common case, single call, zero boilerplate.
/// let repo = Arc::new(InMemoryRepository::<User, u64>::new());
///
/// // Custom ID type (e.g. uuid).
/// let repo = Arc::new(InMemoryRepository::with_id_gen(|| uuid::Uuid::new_v4()));
/// ```
pub struct InMemoryRepository<T, Id> {
    store: Arc<Mutex<HashMap<Id, T>>>,
    next_id: Arc<dyn Fn() -> Id + Send + Sync + 'static>,
}

// Convenience constructor: hides AtomicU64 for the ubiquitous u64 ID case.
impl<T> InMemoryRepository<T, u64>
where
    T: HasId<u64> + Clone + Send + Sync + 'static,
{
    pub fn new() -> Self {
        let counter = Arc::new(AtomicU64::new(1));
        Self::with_id_gen(move || counter.fetch_add(1, Ordering::Relaxed))
    }
}

impl<T> Default for InMemoryRepository<T, u64>
where
    T: HasId<u64> + Clone + Send + Sync + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

// Generic constructor: user supplies an ID generator closure.
impl<T, Id> InMemoryRepository<T, Id>
where
    T: HasId<Id> + Clone + Send + Sync + 'static,
    Id: Eq + Hash + Clone + Default + PartialEq + Send + Sync + 'static,
{
    pub fn with_id_gen(next_id: impl Fn() -> Id + Send + Sync + 'static) -> Self {
        Self {
            store: Arc::new(Mutex::new(HashMap::new())),
            next_id: Arc::new(next_id),
        }
    }
}

impl<T, Id> Repository<T, Id> for InMemoryRepository<T, Id>
where
    T: HasId<Id> + Clone + Send + Sync + 'static,
    Id: Eq + Hash + Clone + Default + PartialEq + Send + Sync + 'static,
{
    async fn find_by_id(&self, id: &Id) -> Result<Option<T>> {
        let guard = self.store.lock().unwrap();
        Ok(guard.get(id).cloned())
    }

    async fn find(&self, spec: QuerySpec<T>) -> Result<Vec<T>> {
        let guard = self.store.lock().unwrap();
        let mut results: Vec<T> = guard.values().filter(|e| spec.matches(e)).cloned().collect();
        if let Some(cmp) = spec.order_by {
            results.sort_by(cmp);
        }
        let results = results.into_iter().skip(spec.offset);
        let results: Vec<T> = match spec.limit {
            Some(n) => results.take(n).collect(),
            None => results.collect(),
        };
        Ok(results)
    }

    async fn save(&self, mut entity: T) -> Result<T> {
        let mut guard = self.store.lock().unwrap();
        if entity.is_new() {
            entity.set_id((self.next_id)());
        }
        guard.insert(entity.get_id(), entity.clone());
        Ok(entity)
    }

    async fn delete(&self, id: &Id) -> Result<bool> {
        let mut guard = self.store.lock().unwrap();
        Ok(guard.remove(id).is_some())
    }
}
