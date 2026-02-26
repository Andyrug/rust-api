//! Phantom-typed entity identity.
//!
//! `Id<T>` is a `Uuid` branded with the type it identifies.  At compile time
//! `Id<User>` and `Id<Post>` are distinct types; the compiler refuses to mix
//! them.  At runtime the phantom marker disappears entirely — no allocation,
//! no indirection, same cost as a bare `Uuid`.
//!
//! # Why phantom types for IDs?
//!
//! Without branding, this compiles silently and corrupts data:
//!
//! ```ignore
//! fn transfer(from: Uuid, to: Uuid) { ... }
//! transfer(account_id, user_id);  // swapped — compiler accepts it
//! ```
//!
//! With `Id<T>`:
//!
//! ```ignore
//! fn transfer(from: Id<Account>, to: Id<Account>) { ... }
//! transfer(account_id, user_id);  // E0308 — caught at compile time
//! ```
//!
//! # Usage
//!
//! ```ignore
//! use rust_api::prelude::*;
//!
//! pub struct User { pub id: Id<User>, ... }
//!
//! impl User {
//!     pub fn new(username: Username, email: Email) -> Self {
//!         Self { id: Id::new(), username, email }
//!     }
//! }
//!
//! impl HasId<Id<User>> for User {
//!     fn get_id(&self) -> Id<User> { self.id }
//!     fn set_id(&mut self, id: Id<User>) { self.id = id; }
//! }
//! ```

use std::fmt;
use std::marker::PhantomData;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use uuid::Uuid;

/// A UUID branded with the type `T` it identifies.
///
/// - Zero runtime cost: `PhantomData<fn() -> T>` is zero-sized.
/// - `Id<User>` ≠ `Id<Post>` at the type level; the compiler enforces this.
/// - `Copy`: passing IDs around never requires `.clone()`.
/// - Serialises as a plain UUID string (e.g. `"550e8400-e29b-41d4-a716-446655440000"`).
/// - `Default` is the nil UUID — useful as a "not yet assigned" sentinel for
///   repo-assigned IDs, but domain-owned construction via `Id::new()` never
///   produces nil.
pub struct Id<T> {
    value: Uuid,
    // `fn() -> T` is covariant in T and always Send + Sync regardless of T.
    _marker: PhantomData<fn() -> T>,
}

impl<T> Id<T> {
    /// Generate a fresh random (v4) ID.
    pub fn new() -> Self {
        Self { value: Uuid::new_v4(), _marker: PhantomData }
    }

    /// The nil UUID — all zeros.  Used as the "not yet assigned" sentinel.
    pub fn nil() -> Self {
        Self { value: Uuid::nil(), _marker: PhantomData }
    }

    /// The underlying `Uuid` value.
    pub fn value(&self) -> Uuid {
        self.value
    }
}

// ---------------------------------------------------------------------------
// Std trait impls
// ---------------------------------------------------------------------------

// Manually implemented so T does not need to satisfy these bounds.
impl<T> Clone for Id<T> {
    fn clone(&self) -> Self {
        *self
    }
}
impl<T> Copy for Id<T> {}

impl<T> PartialEq for Id<T> {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}
impl<T> Eq for Id<T> {}

impl<T> std::hash::Hash for Id<T> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.value.hash(state);
    }
}

impl<T> Default for Id<T> {
    fn default() -> Self {
        Self::nil()
    }
}

impl<T> fmt::Debug for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Id({})", self.value)
    }
}

impl<T> fmt::Display for Id<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.value)
    }
}

// ---------------------------------------------------------------------------
// Serde: serialise as a plain UUID string, not as a struct
// ---------------------------------------------------------------------------

impl<T> Serialize for Id<T> {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        self.value.serialize(s)
    }
}

impl<'de, T> Deserialize<'de> for Id<T> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let value = Uuid::deserialize(d)?;
        Ok(Self { value, _marker: PhantomData })
    }
}

// ---------------------------------------------------------------------------
// Send + Sync
// ---------------------------------------------------------------------------

// SAFETY: `Id<T>` holds only a `Uuid` (which is Send+Sync) and a zero-sized
// `PhantomData<fn() -> T>` (which is always Send+Sync regardless of T).
unsafe impl<T> Send for Id<T> {}
unsafe impl<T> Sync for Id<T> {}
