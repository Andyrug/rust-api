//! Domain model types.
//!
//! These represent validated, trusted values inside the application boundary.
//! All construction goes through smart constructors that enforce invariants;
//! you can never hold an `Email` or `Username` that violated its rules.
//!
//! ## Identity
//!
//! Entity IDs use `Id<T>` — a UUID branded with the type it identifies.
//! `Id<User>` and `Id<Post>` are distinct compile-time types; passing one
//! where the other is expected is a type error, not a runtime bug.
//!
//! `User::new()` generates its own `Id<User>` — the domain owns its identity
//! from the moment the aggregate is constructed.  The repository is a store,
//! not an ID authority.
//!
//! ## Invariants
//!
//! - `#[derive(NewType)]` generates the smart constructor, Deref, Display,
//!   Serialize, and a validating Deserialize impl.
//! - No HTTP types (no StatusCode, no Json extractor, no headers).
//! - `User` is the aggregate root; its fields are domain newtypes.
//! - Cross-field rules that cannot be expressed with attributes are handled
//!   with a manual `Validatable` impl on the aggregate.

use rust_api::prelude::*;

// ---------------------------------------------------------------------------
// Value objects (single-field newtypes with validation)
// ---------------------------------------------------------------------------

/// A validated e-mail address.
#[derive(Debug, Clone, NewType)]
pub struct Email(
    #[validate(email)]
    String,
);

/// A username: 3–30 lowercase alphanumeric characters and underscores.
#[derive(Debug, Clone, NewType)]
pub struct Username(
    #[validate(min_length = 3)]
    #[validate(max_length = 30)]
    #[validate(matches = "^[a-z0-9_]+$")]
    String,
);

// ---------------------------------------------------------------------------
// User aggregate root
// ---------------------------------------------------------------------------

/// The canonical in-memory representation of a user.
///
/// `id` is `Id<User>` — a phantom-typed UUID.  The compiler refuses to
/// substitute an `Id<Post>` here.  `new()` mints a fresh UUID; the aggregate
/// is valid and uniquely identified from the moment it exists, independent of
/// any database or repository.
#[derive(Debug, Clone)]
pub struct User {
    pub id: Id<User>,
    pub username: Username,
    pub email: Email,
}

impl User {
    /// Constructs a new `User` and generates its own `Id<User>`.
    /// No `id: 0` sentinel, no repo round-trip required to have an identity.
    pub fn new(username: Username, email: Email) -> Self {
        Self { id: Id::new(), username, email }
    }
}

/// Cross-field aggregate invariant: username must not equal the local-part
/// of the email address (the portion before `@`).
///
/// Single-field rules are already enforced by the `Username` and `Email`
/// smart constructors.  `User::validate()` only expresses constraints that
/// span *both* fields.
impl Validatable for User {
    fn validate(self) -> ValidationResult<Self> {
        let local = self.email.split('@').next().unwrap_or("");
        if self.username.as_str() == local {
            Err(vec![FieldError::new(
                "username",
                "username must not match the local part of the email address",
            )])
        } else {
            Ok(self)
        }
    }
}

impl HasId<Id<User>> for User {
    fn get_id(&self) -> Id<User> { self.id }
    fn set_id(&mut self, id: Id<User>) { self.id = id; }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn alice() -> User {
        User::new(
            Username::new("alice_99").unwrap(),
            Email::new("alice@example.com").unwrap(),
        )
    }

    #[test]
    fn email_valid() {
        assert!(Email::new("alice@example.com").is_ok());
    }

    #[test]
    fn email_invalid() {
        assert!(Email::new("not-an-email").is_err());
    }

    #[test]
    fn username_valid() {
        assert!(Username::new("alice_99").is_ok());
    }

    #[test]
    fn username_too_short() {
        let errs = Username::new("ab").unwrap_err();
        assert!(errs.iter().any(|e| e.field == "value"));
    }

    #[test]
    fn username_uppercase_fails() {
        let errs = Username::new("Alice").unwrap_err();
        assert!(errs.iter().any(|e| e.field == "value"));
    }

    #[test]
    fn user_id_is_phantom_typed() {
        // Id<User> and Id<Email> are different types at compile time.
        // This test documents the intent; the real proof is that
        // `let _: Id<Email> = alice().id;` would be E0308.
        let u = alice();
        let _: Id<User> = u.id;
    }

    #[test]
    fn user_new_generates_unique_ids() {
        let a = alice();
        let b = alice();
        assert_ne!(a.id, b.id, "every User::new() call mints a fresh UUID");
    }

    #[test]
    fn user_validate_passes_when_username_differs_from_email_local() {
        assert!(alice().validate().is_ok());
    }

    #[test]
    fn user_validate_fails_when_username_matches_email_local() {
        let user = User::new(
            Username::new("alice").unwrap(),
            Email::new("alice@example.com").unwrap(),
        );
        let errs = user.validate().unwrap_err();
        assert!(errs.iter().any(|e| e.field == "username"));
    }
}
