//! HTTP request DTOs.
//!
//! These types live at the HTTP boundary — they model what comes in over the
//! wire.  They are deliberately separate from domain types: a DTO is just a
//! validated bag of primitives; the service layer converts them into domain
//! values.
//!
//! Invariants:
//!   - All fields are plain `String` / primitives (no domain newtypes here).
//!   - `#[derive(Validatable)]` enforces structural constraints at the HTTP
//!     boundary before the request reaches the service.
//!   - No business logic, no service calls.

use rust_api::prelude::*;

// ---------------------------------------------------------------------------
// POST /users
// ---------------------------------------------------------------------------


#[derive(Debug, Clone, Deserialize, Serialize, Validatable)]
pub struct CreateUserRequest {
    /// 3–30 lowercase alphanumeric characters plus underscore.
    #[validate(min_length = 3)]
    #[validate(max_length = 30)]
    #[validate(matches = "^[a-z0-9_]+$")]
    pub username: String,

    #[validate(email)]
    pub email: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> CreateUserRequest {
        CreateUserRequest {
            username: "alice_99".into(),
            email: "alice@example.com".into(),
        }
    }

    #[test]
    fn valid_request_passes() {
        assert!(valid().validate().is_ok());
    }

    #[test]
    fn short_username_fails() {
        let req = CreateUserRequest { username: "ab".into(), ..valid() };
        let errs = req.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.field == "username"));
    }

    #[test]
    fn invalid_email_fails() {
        let req = CreateUserRequest { email: "not-an-email".into(), ..valid() };
        let errs = req.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.field == "email"));
    }

    #[test]
    fn multiple_failures_all_collected() {
        let req = CreateUserRequest { username: "ab".into(), email: "bad".into() };
        let errs = req.validate().unwrap_err();
        assert!(errs.len() >= 2, "expected at least 2 errors, got {}", errs.len());
    }

    #[test]
    fn uppercase_username_fails_pattern() {
        let req = CreateUserRequest { username: "Alice".into(), ..valid() };
        let errs = req.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.field == "username"));
    }
}
