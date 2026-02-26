//! Standard library of intrinsic validation functions.
//!
//! Every function here is a **pure function of its inputs** — no I/O, no async,
//! no external state.  They return `Result<(), FieldError>` and compose directly
//! into a `frunk::Validated` accumulation chain via `into_validated() + ...`.
//!
//! # Intrinsic vs Contextual
//!
//! These validators check properties of the *value itself* (format, length, range).
//! Contextual checks that require external state (e.g. uniqueness in a database)
//! belong in the service layer as explicit `async` operations that happen to return
//! the same `FieldError` type.
//!
//! # Example
//!
//! ```ignore
//! use rust_api::prelude::*;
//!
//! fn validate_request(email: &str, age: u32) -> Result<(), Vec<FieldError>> {
//!     (validate_email("email", email).into_validated()
//!         + validate_range("age", age, 18u32, 120u32))
//!     .into_result()
//! }
//! ```

use once_cell::sync::Lazy;
use regex::Regex;

use crate::validation::FieldError;

// ---------------------------------------------------------------------------
// Pre-compiled regex patterns
// ---------------------------------------------------------------------------

static RE_EMAIL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^[a-z0-9._%+\-]+@[a-z0-9.\-]+\.[a-z]{2,}$").unwrap()
});

static RE_URL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^https?://[^\s/$.?#].[^\s]*$").unwrap()
});

static RE_UUID: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$")
        .unwrap()
});

static RE_US_ZIP: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\d{5}(-\d{4})?$").unwrap());

static RE_CA_POSTAL: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"(?i)^[ABCEGHJ-NPRSTVXY]\d[ABCEGHJ-NPRSTV-Z] ?\d[ABCEGHJ-NPRSTV-Z]\d$").unwrap()
});

static RE_PHONE_E164: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\+[1-9]\d{6,14}$").unwrap());

// ---------------------------------------------------------------------------
// Validators
// ---------------------------------------------------------------------------

/// Validates that `value` looks like an RFC 5322 email address.
pub fn validate_email(field: &str, value: &str) -> Result<(), FieldError> {
    if RE_EMAIL.is_match(value) {
        Ok(())
    } else {
        Err(FieldError::new(field, "invalid email address"))
    }
}

/// Validates that `value` is a well-formed HTTP/HTTPS URL.
pub fn validate_url(field: &str, value: &str) -> Result<(), FieldError> {
    if RE_URL.is_match(value) {
        Ok(())
    } else {
        Err(FieldError::new(field, "invalid URL"))
    }
}

/// Validates that `value` is a UUID v4 string.
pub fn validate_uuid(field: &str, value: &str) -> Result<(), FieldError> {
    if RE_UUID.is_match(value) {
        Ok(())
    } else {
        Err(FieldError::new(field, "invalid UUID v4"))
    }
}

/// Validates that `value` is not empty or whitespace-only.
pub fn validate_non_empty(field: &str, value: &str) -> Result<(), FieldError> {
    if !value.trim().is_empty() {
        Ok(())
    } else {
        Err(FieldError::new(field, "must not be empty"))
    }
}

/// Validates that `value` has at least `min` Unicode characters.
pub fn validate_min_length(field: &str, value: &str, min: usize) -> Result<(), FieldError> {
    if value.chars().count() >= min {
        Ok(())
    } else {
        Err(FieldError::new(
            field,
            format!("must be at least {min} characters"),
        ))
    }
}

/// Validates that `value` has at most `max` Unicode characters.
pub fn validate_max_length(field: &str, value: &str, max: usize) -> Result<(), FieldError> {
    if value.chars().count() <= max {
        Ok(())
    } else {
        Err(FieldError::new(
            field,
            format!("must be at most {max} characters"),
        ))
    }
}

/// Validates that `value` matches `pattern`.
///
/// The pattern is compiled on first use and cached.
/// Returns an error if the pattern is invalid (treated as a validation failure).
pub fn validate_matches(field: &str, value: &str, pattern: &str) -> Result<(), FieldError> {
    match Regex::new(pattern) {
        Ok(re) if re.is_match(value) => Ok(()),
        Ok(_) => Err(FieldError::new(field, "does not match required pattern")),
        Err(_) => Err(FieldError::new(field, "invalid validation pattern (framework error)")),
    }
}

/// Validates that `value` is between `min` and `max` (inclusive).
///
/// Works for any type implementing `PartialOrd + std::fmt::Display`.
pub fn validate_range<T>(field: &str, value: T, min: T, max: T) -> Result<(), FieldError>
where
    T: PartialOrd + std::fmt::Display,
{
    if value >= min && value <= max {
        Ok(())
    } else {
        Err(FieldError::new(
            field,
            format!("must be between {min} and {max}"),
        ))
    }
}

/// Validates that `value` is a US ZIP code (5-digit or ZIP+4).
pub fn validate_us_zip(field: &str, value: &str) -> Result<(), FieldError> {
    if RE_US_ZIP.is_match(value) {
        Ok(())
    } else {
        Err(FieldError::new(field, "invalid US ZIP code"))
    }
}

/// Validates that `value` is a Canadian postal code (e.g. `A1A 1A1`).
pub fn validate_ca_postal(field: &str, value: &str) -> Result<(), FieldError> {
    if RE_CA_POSTAL.is_match(value) {
        Ok(())
    } else {
        Err(FieldError::new(field, "invalid Canadian postal code"))
    }
}

/// Validates that `value` is an E.164 international phone number (e.g. `+12125551234`).
pub fn validate_phone_e164(field: &str, value: &str) -> Result<(), FieldError> {
    if RE_PHONE_E164.is_match(value) {
        Ok(())
    } else {
        Err(FieldError::new(field, "invalid E.164 phone number"))
    }
}

/// Validates `value` against a caller-supplied pure predicate.
///
/// The predicate **must** be a pure function of the value — no I/O, no async,
/// no external state.  The `Fn(&T) -> bool` signature enforces this boundary.
///
/// # Example
///
/// ```ignore
/// validate_custom("username", &req.username, |u| !RESERVED.contains(u), "reserved word")
/// ```
pub fn validate_custom<T>(
    field: &str,
    value: &T,
    predicate: impl Fn(&T) -> bool,
    message: &str,
) -> Result<(), FieldError> {
    if predicate(value) {
        Ok(())
    } else {
        Err(FieldError::new(field, message))
    }
}

// ---------------------------------------------------------------------------
// Unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn email_accepts_valid() {
        assert!(validate_email("e", "user@example.com").is_ok());
        assert!(validate_email("e", "a.b+c@sub.domain.io").is_ok());
    }

    #[test]
    fn email_rejects_invalid() {
        assert!(validate_email("e", "notanemail").is_err());
        assert!(validate_email("e", "@missing.local").is_err());
        assert!(validate_email("e", "missing@").is_err());
    }

    #[test]
    fn url_accepts_valid() {
        assert!(validate_url("u", "https://example.com").is_ok());
        assert!(validate_url("u", "http://foo.bar/baz?q=1").is_ok());
    }

    #[test]
    fn url_rejects_invalid() {
        assert!(validate_url("u", "ftp://not-http.com").is_err());
        assert!(validate_url("u", "not a url").is_err());
    }

    #[test]
    fn uuid_accepts_valid_v4() {
        assert!(validate_uuid("id", "550e8400-e29b-41d4-a716-446655440000").is_ok());
        assert!(validate_uuid("id", "f47ac10b-58cc-4372-a567-0e02b2c3d479").is_ok());
        // v1 UUID (version byte is 1, not 4) — should fail
        assert!(validate_uuid("id", "550e8400-e29b-11d4-a716-446655440000").is_err());
    }

    #[test]
    fn uuid_rejects_non_uuid() {
        assert!(validate_uuid("id", "not-a-uuid").is_err());
    }

    #[test]
    fn non_empty_accepts_content() {
        assert!(validate_non_empty("f", "hello").is_ok());
    }

    #[test]
    fn non_empty_rejects_blank() {
        assert!(validate_non_empty("f", "").is_err());
        assert!(validate_non_empty("f", "   ").is_err());
    }

    #[test]
    fn min_length_boundary() {
        assert!(validate_min_length("f", "abc", 3).is_ok());
        assert!(validate_min_length("f", "ab", 3).is_err());
    }

    #[test]
    fn max_length_boundary() {
        assert!(validate_max_length("f", "abc", 3).is_ok());
        assert!(validate_max_length("f", "abcd", 3).is_err());
    }

    #[test]
    fn range_inclusive_boundaries() {
        assert!(validate_range("age", 18u32, 18, 120).is_ok());
        assert!(validate_range("age", 120u32, 18, 120).is_ok());
        assert!(validate_range("age", 17u32, 18, 120).is_err());
        assert!(validate_range("age", 121u32, 18, 120).is_err());
    }

    #[test]
    fn us_zip_accepts_valid() {
        assert!(validate_us_zip("z", "12345").is_ok());
        assert!(validate_us_zip("z", "12345-6789").is_ok());
    }

    #[test]
    fn us_zip_rejects_invalid() {
        assert!(validate_us_zip("z", "1234").is_err());
        assert!(validate_us_zip("z", "ABCDE").is_err());
    }

    #[test]
    fn ca_postal_accepts_valid() {
        assert!(validate_ca_postal("p", "K1A 0A9").is_ok());
        assert!(validate_ca_postal("p", "M5V3L9").is_ok());
    }

    #[test]
    fn ca_postal_rejects_invalid() {
        assert!(validate_ca_postal("p", "12345").is_err());
        assert!(validate_ca_postal("p", "Z9Z 9Z9").is_err()); // Z not valid first char
    }

    #[test]
    fn phone_e164_accepts_valid() {
        assert!(validate_phone_e164("p", "+12125551234").is_ok());
        assert!(validate_phone_e164("p", "+447911123456").is_ok());
    }

    #[test]
    fn phone_e164_rejects_invalid() {
        assert!(validate_phone_e164("p", "2125551234").is_err()); // missing +
        assert!(validate_phone_e164("p", "+1").is_err()); // too short
    }

    #[test]
    fn validate_custom_uses_predicate() {
        let reserved = ["admin", "root"];
        assert!(
            validate_custom("username", &"alice", |u| !reserved.contains(u), "reserved").is_ok()
        );
        assert!(
            validate_custom("username", &"admin", |u| !reserved.contains(u), "reserved").is_err()
        );
    }

    #[test]
    fn validate_matches_pattern() {
        assert!(validate_matches("f", "abc123", r"^[a-z0-9]+$").is_ok());
        assert!(validate_matches("f", "ABC", r"^[a-z0-9]+$").is_err());
    }
}
