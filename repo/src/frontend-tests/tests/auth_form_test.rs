//! Login/register form validation tests.
//!
//! Imports the real `src/frontend/logic/auth_form.rs` — the same file
//! the `LoginPage` Leptos component uses to drive its submit-button
//! `disabled=…` binding — and locks every branch of the client-side
//! validator.

use fieldtrace_frontend_tests::frontend_auth_form::{
    can_submit, validate_credentials, CredentialError, MAX_PASSWORD_LEN, MIN_PASSWORD_LEN,
};

#[test]
fn length_constants_match_backend_policy() {
    // Backend /auth/register rejects passwords < 12 characters; the
    // client has to keep pace so the UI never allows a request that
    // the server will reject with 400.
    assert_eq!(MIN_PASSWORD_LEN, 12);
    assert!(MAX_PASSWORD_LEN > MIN_PASSWORD_LEN);
}

#[test]
fn empty_username_is_rejected_with_specific_error() {
    assert_eq!(
        validate_credentials("", "password1234"),
        Err(CredentialError::UsernameEmpty)
    );
    assert_eq!(
        validate_credentials("   ", "password1234"),
        Err(CredentialError::UsernameEmpty),
        "whitespace-only username must be rejected as empty after trim"
    );
}

#[test]
fn too_short_username_is_rejected() {
    assert_eq!(
        validate_credentials("ab", "password1234"),
        Err(CredentialError::UsernameTooShort)
    );
}

#[test]
fn username_with_whitespace_is_rejected() {
    assert_eq!(
        validate_credentials("bad name", "password1234"),
        Err(CredentialError::UsernameHasWhitespace)
    );
}

#[test]
fn short_password_triggers_password_error() {
    let err = validate_credentials("alice", "short").unwrap_err();
    assert_eq!(err, CredentialError::PasswordTooShort);
    assert_eq!(err.message(), "Password must be at least 12 characters.");
}

#[test]
fn password_exactly_at_boundary_is_accepted() {
    assert!(validate_credentials("alice", "a".repeat(MIN_PASSWORD_LEN).as_str()).is_ok());
}

#[test]
fn password_one_below_boundary_is_rejected() {
    assert_eq!(
        validate_credentials("alice", "a".repeat(MIN_PASSWORD_LEN - 1).as_str()),
        Err(CredentialError::PasswordTooShort)
    );
}

#[test]
fn oversize_password_is_rejected() {
    let huge = "a".repeat(MAX_PASSWORD_LEN + 1);
    assert_eq!(
        validate_credentials("alice", &huge),
        Err(CredentialError::PasswordTooLong)
    );
}

#[test]
fn can_submit_matches_validator() {
    assert!(can_submit("alice", "MySecurePass12"));
    assert!(!can_submit("", "MySecurePass12"));
    assert!(!can_submit("alice", "short"));
}

#[test]
fn every_error_variant_has_a_user_facing_message() {
    // If anyone adds a new variant they're forced to wire a message
    // here or the exhaustive match under `impl CredentialError`
    // won't compile, which is exactly the contract we want.
    for e in [
        CredentialError::UsernameEmpty,
        CredentialError::UsernameTooShort,
        CredentialError::UsernameHasWhitespace,
        CredentialError::PasswordTooShort,
        CredentialError::PasswordTooLong,
    ] {
        let m = e.message();
        assert!(!m.is_empty(), "error {:?} exposes empty message", e);
        // Messages must be plain text, not debug-form `Foo { .. }`.
        assert!(!m.contains('{'), "error {:?} looks like debug output", e);
    }
}
