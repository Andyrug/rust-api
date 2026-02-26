//! Applicative validation primitives.
//!
//! A single validation story — accumulating, never short-circuiting — applied
//! identically at the HTTP boundary and inside services.
//!
//! # The one primitive
//!
//! ```ignore
//! pub trait Validatable: Sized {
//!     fn validate(self) -> Result<Self, Vec<FieldError>>;
//! }
//! ```
//!
//! # Two ways to satisfy it
//!
//! **Derive** (typical — field attributes generate the composition chain):
//!
//! ```ignore
//! #[derive(Deserialize, Validatable)]
//! pub struct CreateUserRequest {
//!     #[validate(email)]
//!     pub email: String,
//!     #[validate(min_length = 3, max_length = 50)]
//!     pub username: String,
//! }
//! ```
//!
//! **Manual impl** (cross-field or conditional logic — uses `IntoValidated`
//! from frunk to compose independent checks applicatively):
//!
//! ```ignore
//! use rust_api::prelude::*;
//!
//! impl Validatable for DateRange {
//!     fn validate(self) -> Result<Self, Vec<FieldError>> {
//!         (validate_non_empty("start", &self.start).into_validated()
//!             + validate_non_empty("end", &self.end)
//!             + if self.end > self.start {
//!                 Ok(())
//!             } else {
//!                 Err(FieldError::new("end", "must be after start"))
//!             })
//!         .into_result()
//!         .map(|_| self)
//!     }
//! }
//! ```

use axum::{
    body::Body,
    extract::{FromRequest, FromRequestParts, Query, Request},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
pub use frunk::validated::IntoValidated;
use serde::{de::DeserializeOwned, Serialize};

// ---------------------------------------------------------------------------
// FieldError
// ---------------------------------------------------------------------------

/// A single validation failure for a named field.
#[derive(Debug, Clone, Serialize)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl FieldError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            message: message.into(),
        }
    }
}

// ---------------------------------------------------------------------------
// Validatable trait
// ---------------------------------------------------------------------------

/// Convenience alias for the return type of `Validatable::validate`.
///
/// Return type for service methods that perform validation-style fallibility.
///
/// `Result<T, Vec<FieldError>>` spelled out explicitly — avoids the 1-arg
/// `rust_api::Result<T>` alias that shadows `std::result::Result` in the
/// prelude.  Use this in `impl Validatable` blocks and service methods.
pub type ValidationResult<T> = std::result::Result<T, Vec<FieldError>>;

/// Return type for Axum handler functions that can produce validation errors.
///
/// ```ignore
/// #[post("/users")]
/// pub async fn create_user(
///     State(svc): State<Arc<UserService>>,
///     ValidatedJson(req): ValidatedJson<CreateUserRequest>,
/// ) -> HandlerResult<Json<UserResponse>> {
///     svc.create(req).await.map(Json).map_err(ValidationRejection)
/// }
/// ```
///
/// Axum renders `HandlerResult<T>` automatically:
/// - `Ok(T)`  → T's `IntoResponse` (typically `200 OK`)
/// - `Err(ValidationRejection)` → `422 Unprocessable Entity` with JSON errors
///
/// The explicit concrete type (vs `impl IntoResponse`) keeps the signature
/// readable, lets the compiler verify the branches, and gives future tooling
/// (OpenAPI generation) a hook to inspect the response shape.
pub type HandlerResult<T> = std::result::Result<T, ValidationRejection>;

/// Types that can validate themselves, returning **all** errors at once.
///
/// Returns `Ok(self)` when every check passes, or `Err(Vec<FieldError>)` with
/// every accumulated failure.  Nothing short-circuits.
///
/// Implement via `#[derive(Validatable)]` for attribute-driven field validation,
/// or manually for cross-field / conditional rules.
pub trait Validatable: Sized {
    fn validate(self) -> ValidationResult<Self>;
}

// ---------------------------------------------------------------------------
// ValidationRejection — unified error response
// ---------------------------------------------------------------------------

/// HTTP rejection produced when a `ValidatedJson` or `ValidatedQuery` extractor
/// finds validation errors.  Serialises as:
///
/// ```json
/// { "errors": [{ "field": "email", "message": "invalid email format" }] }
/// ```
#[derive(Debug)]
pub struct ValidationRejection(pub Vec<FieldError>);

impl IntoResponse for ValidationRejection {
    fn into_response(self) -> Response {
        #[derive(Serialize)]
        struct Body {
            errors: Vec<FieldError>,
        }
        let body = Body { errors: self.0 };
        (StatusCode::UNPROCESSABLE_ENTITY, Json(body)).into_response()
    }
}

// ---------------------------------------------------------------------------
// ValidatedJson extractor
// ---------------------------------------------------------------------------

/// Axum extractor that deserialises a JSON body then runs `T::validate()`.
///
/// Returns `422 Unprocessable Entity` with all accumulated `FieldError`s if
/// validation fails.  The handler only runs when validation succeeds.
///
/// # Example
///
/// ```ignore
/// #[post("/users")]
/// async fn create_user(
///     State(svc): State<Arc<UserService>>,
///     ValidatedJson(req): ValidatedJson<CreateUserRequest>,
/// ) -> impl IntoResponse { ... }
/// ```
pub struct ValidatedJson<T>(pub T);

impl<T, S> FromRequest<S> for ValidatedJson<T>
where
    T: DeserializeOwned + Validatable,
    S: Send + Sync,
{
    type Rejection = ValidationRejection;

    async fn from_request(req: Request<Body>, state: &S) -> Result<Self, Self::Rejection> {
        let Json(value) = Json::<T>::from_request(req, state)
            .await
            .map_err(|e| ValidationRejection(vec![FieldError::new("body", e.to_string())]))?;

        value
            .validate()
            .map(ValidatedJson)
            .map_err(|errs| ValidationRejection(errs))
    }
}

// ---------------------------------------------------------------------------
// ValidatedQuery extractor
// ---------------------------------------------------------------------------

/// Axum extractor that parses query parameters then runs `T::validate()`.
///
/// Returns `422 Unprocessable Entity` with all accumulated `FieldError`s if
/// validation fails.
///
/// # Example
///
/// ```ignore
/// #[get("/users")]
/// async fn list_users(
///     ValidatedQuery(params): ValidatedQuery<ListUsersQuery>,
/// ) -> impl IntoResponse { ... }
/// ```
pub struct ValidatedQuery<T>(pub T);

impl<T, S> FromRequestParts<S> for ValidatedQuery<T>
where
    T: DeserializeOwned + Validatable,
    S: Send + Sync,
{
    type Rejection = ValidationRejection;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &S,
    ) -> Result<Self, Self::Rejection> {
        let Query(value) = Query::<T>::from_request_parts(parts, state)
            .await
            .map_err(|e| {
                ValidationRejection(vec![FieldError::new("query", e.to_string())])
            })?;

        value
            .validate()
            .map(ValidatedQuery)
            .map_err(|errs| ValidationRejection(errs))
    }
}
