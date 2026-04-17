//! Intake form — the form-heavy module required by the audit.
//!
//! Imports the real `src/frontend/logic/intake_form.rs` and locks the
//! submittability rules, the exact JSON body POSTed to `/intake`, and
//! the reset-on-submit behaviour the draft autosave machinery relies
//! on.

use fieldtrace_frontend_tests::frontend_intake_form::{
    is_known_intake_type, IntakeForm, INTAKE_TYPES,
};

fn valid_form() -> IntakeForm {
    IntakeForm {
        intake_type: "animal".into(),
        tag: "A-42".into(),
        region: "north".into(),
        notes: "hind leg limp".into(),
    }
}

#[test]
fn intake_types_match_backend_contract() {
    // These are the exact three values the backend `/intake` handler
    // recognises. Adding or removing a type must be a deliberate
    // cross-stack change, so we lock the full set here.
    assert_eq!(INTAKE_TYPES, ["animal", "supply", "donation"]);
    for t in INTAKE_TYPES {
        assert!(is_known_intake_type(t), "{} should be accepted", t);
    }
    assert!(!is_known_intake_type("unknown"));
    assert!(!is_known_intake_type(""));
}

#[test]
fn default_form_is_not_submittable() {
    let f = IntakeForm::default();
    assert!(!f.is_submittable(), "empty form must not be submittable");
}

#[test]
fn form_needs_both_intake_type_and_region() {
    let mut f = valid_form();
    assert!(f.is_submittable());

    f.region.clear();
    assert!(!f.is_submittable(), "region is required");

    f.region = "east".into();
    f.intake_type = "bogus".into();
    assert!(!f.is_submittable(), "intake_type must be a known kind");
}

#[test]
fn whitespace_region_is_not_a_region() {
    let mut f = valid_form();
    f.region = "   ".into();
    assert!(!f.is_submittable());
}

#[test]
fn create_body_matches_backend_schema() {
    let f = valid_form();
    let body = f.to_create_body();
    // Top-level shape: { intake_type, details } — details is a string
    // blob because the backend's column type is TEXT.
    assert_eq!(body["intake_type"], "animal");
    let details = body["details"].as_str().expect("details is a string");
    // Details payload must include every user-entered field verbatim.
    let parsed: serde_json::Value = serde_json::from_str(details).unwrap();
    assert_eq!(parsed["tag"], "A-42");
    assert_eq!(parsed["region"], "north");
    assert_eq!(parsed["notes"], "hind leg limp");
}

#[test]
fn reset_clears_transient_fields_but_keeps_sticky_ones() {
    let mut f = valid_form();
    f.reset();
    // Transient fields wiped so the next entry starts fresh.
    assert_eq!(f.tag, "");
    assert_eq!(f.notes, "");
    // intake_type + region stay so batch entries don't have to
    // re-select them every time.
    assert_eq!(f.intake_type, "animal");
    assert_eq!(f.region, "north");
}

#[test]
fn form_round_trips_through_serde_json() {
    // Draft autosave stores the form as JSON in localStorage; the
    // round-trip must be lossless or a preserved draft won't restore.
    let f = valid_form();
    let json = serde_json::to_string(&f).unwrap();
    let back: IntakeForm = serde_json::from_str(&json).unwrap();
    assert_eq!(back, f);
}
