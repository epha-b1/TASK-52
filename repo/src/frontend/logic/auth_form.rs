//! Login/register form validation used by `pages/login.rs` and
//! `pages/register.rs`. Rules mirror the backend `/auth/register` and
//! `/auth/login` validators, but living on the client means we can
//! surface errors before the network round-trip.

#![allow(dead_code)]

/// Minimum password length enforced on both client and server.
pub const MIN_PASSWORD_LEN: usize = 12;

/// Maximum password length — matches the backend's guard against
/// unbounded Argon2 cost.
pub const MAX_PASSWORD_LEN: usize = 128;

/// All client-side reasons a credentials form can fail validation.
/// The backend emits a different error envelope on authentication
/// failure; this enum is purely about whether the form is ready to
/// submit.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredentialError {
    UsernameEmpty,
    UsernameTooShort,
    UsernameHasWhitespace,
    PasswordTooShort,
    PasswordTooLong,
}

impl CredentialError {
    pub fn message(&self) -> &'static str {
        match self {
            CredentialError::UsernameEmpty => "Username is required.",
            CredentialError::UsernameTooShort => "Username must be at least 3 characters.",
            CredentialError::UsernameHasWhitespace => "Username cannot contain spaces.",
            CredentialError::PasswordTooShort => "Password must be at least 12 characters.",
            CredentialError::PasswordTooLong => "Password cannot exceed 128 characters.",
        }
    }
}

/// Validate a `(username, password)` pair for submission.
/// Returns `Ok(())` only when every client-side rule passes.
pub fn validate_credentials(username: &str, password: &str) -> Result<(), CredentialError> {
    let u = username.trim();
    if u.is_empty() {
        return Err(CredentialError::UsernameEmpty);
    }
    if u.len() < 3 {
        return Err(CredentialError::UsernameTooShort);
    }
    if u.chars().any(|c| c.is_whitespace()) {
        return Err(CredentialError::UsernameHasWhitespace);
    }
    if password.len() < MIN_PASSWORD_LEN {
        return Err(CredentialError::PasswordTooShort);
    }
    if password.len() > MAX_PASSWORD_LEN {
        return Err(CredentialError::PasswordTooLong);
    }
    Ok(())
}

/// Whether the submit button should be enabled for the given inputs.
/// Pure wrapper used by the Leptos component to drive `disabled=…`.
pub fn can_submit(username: &str, password: &str) -> bool {
    validate_credentials(username, password).is_ok()
}
