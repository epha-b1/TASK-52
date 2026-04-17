//! Shared UI formatting tests — the helpers `components/nav.rs` and
//! every table-rendering page import when turning backend values into
//! display strings. Imports the real `src/frontend/logic/ui_format.rs`.

use fieldtrace_frontend_tests::frontend_ui_format::{mask_phone, role_label, short_timestamp};

// ── role_label: drives the Nav chip ──────────────────────────────────

#[test]
fn role_label_maps_every_known_backend_role() {
    assert_eq!(role_label("administrator"), "Administrator");
    assert_eq!(role_label("operations_staff"), "Operations staff");
    assert_eq!(role_label("auditor"), "Auditor");
}

#[test]
fn unknown_role_passes_through_not_replaced_with_question_marks() {
    // Policy: if the backend ships a new role before the frontend
    // catches up, the Nav shows the raw value rather than "???".
    assert_eq!(role_label("super_operator"), "super_operator");
    assert_eq!(role_label(""), "");
}

// ── mask_phone: matches the address-book response masking ────────────

#[test]
fn phone_with_fewer_than_four_digits_is_fully_masked() {
    // Never leak partial digits when the number is too short.
    assert_eq!(mask_phone("12"), "••");
    assert_eq!(mask_phone("1"), "•");
}

#[test]
fn phone_shows_only_last_four_digits() {
    assert_eq!(mask_phone("5551234567"), "••••••4567");
}

#[test]
fn phone_strips_non_digits_before_masking() {
    // The UI always masks based on digit count; punctuation doesn't
    // add or remove dots.
    assert_eq!(mask_phone("(555) 123-4567"), "••••••4567");
    assert_eq!(mask_phone("+1 555 123 4567"), "•••••••4567");
}

#[test]
fn empty_phone_does_not_panic() {
    // Defensive: an empty string must degrade gracefully, not crash
    // a table row.
    let out = mask_phone("");
    assert!(out == "•" || out == "", "got {:?}", out);
}

// ── short_timestamp: compact table format ───────────────────────────

#[test]
fn short_timestamp_extracts_date_and_hour_minute() {
    assert_eq!(
        short_timestamp("2026-04-17T12:34:56.789Z"),
        "2026-04-17 12:34"
    );
}

#[test]
fn short_timestamp_handles_seconds_without_fractionals() {
    assert_eq!(
        short_timestamp("2026-04-17T09:05:00Z"),
        "2026-04-17 09:05"
    );
}

#[test]
fn malformed_timestamp_returns_input_unchanged() {
    assert_eq!(short_timestamp("not-a-timestamp"), "not-a-timestamp");
    assert_eq!(short_timestamp("2026/04/17 12:34"), "2026/04/17 12:34");
    assert_eq!(short_timestamp(""), "");
}
