//! Shared UI formatting helpers — the tiny library every page + Nav
//! reaches for when it needs to render a display value (user-facing
//! phone mask, humanised role label, truncated ISO timestamp).
//!
//! These helpers live here so the Nav component, dashboard widgets,
//! and the evidence-card renderer all produce identical strings.
//! `mask_phone` and `short_timestamp` are consumed today only by the
//! `fieldtrace-frontend-tests` crate (via `#[path]` include) — the
//! Leptos widgets that render address-book rows and audit-log tables
//! will migrate onto them in follow-up work.

#![allow(dead_code)]

/// Render a user role for the Nav header. Unknown values passthrough
/// so an operator doesn't see "???" if the backend adds a new role
/// before the frontend catches up.
pub fn role_label(role: &str) -> String {
    match role {
        "administrator" => "Administrator".to_string(),
        "operations_staff" => "Operations staff".to_string(),
        "auditor" => "Auditor".to_string(),
        other => other.to_string(),
    }
}

/// Keep the first two characters of a phone number and replace the
/// rest with `•`. Matches the backend's address-book masking policy
/// so the UI shows the same redaction the API returns.
pub fn mask_phone(phone: &str) -> String {
    let digits: Vec<char> = phone.chars().filter(|c| c.is_ascii_digit()).collect();
    if digits.len() < 4 {
        return "•".repeat(digits.len().max(1));
    }
    let keep = &digits[digits.len() - 4..];
    let mut out = String::new();
    for _ in 0..(digits.len() - 4) {
        out.push('•');
    }
    for c in keep {
        out.push(*c);
    }
    out
}

/// Trim an ISO-8601 timestamp down to `YYYY-MM-DD HH:MM` for compact
/// table rows. If the input is malformed, return it unchanged so we
/// never silently drop debug info.
pub fn short_timestamp(iso: &str) -> String {
    // Expected shape: "2026-04-17T12:34:56.789Z" — take "YYYY-MM-DD HH:MM".
    if iso.len() < 16 || iso.as_bytes().get(10) != Some(&b'T') {
        return iso.to_string();
    }
    let mut s = String::with_capacity(16);
    s.push_str(&iso[..10]);
    s.push(' ');
    s.push_str(&iso[11..16]);
    s
}
