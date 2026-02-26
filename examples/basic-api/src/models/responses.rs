//! HTTP response types.
//!
//! These are serialisation-only shapes that the controller maps domain values
//! into before handing them to Axum. Keeping them separate from domain models
//! lets the API contract evolve independently of the domain.
//!
//! Invariants:
//!   - `#[derive(Serialize)]` only — these are never deserialised from a
//!     request body.
//!   - No validation attributes — responses are produced by trusted code.
//!   - `id` is serialised as a UUID string (e.g. `"550e8400-e29b-41d4-…"`).

use rust_api::prelude::*;

#[derive(Debug, Serialize)]
pub struct UserResponse {
    pub id: String,
    pub username: String,
    pub email: String,
}
