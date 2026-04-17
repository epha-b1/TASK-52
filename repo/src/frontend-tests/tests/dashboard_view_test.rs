//! Dashboard / reporting view tests.
//!
//! Exercises `src/frontend/logic/dashboard_filter.rs` — the same
//! filter→query-string serialiser that `pages/dashboard.rs` and
//! `pages/reports.rs` use when calling `/reports/summary` and
//! `/reports/export`. Lockdown matters because the backend tests
//! assert the exact same query keys (`region`, `tags`, `q`).

use fieldtrace_frontend_tests::frontend_dashboard_filter::{percent_encode, DashboardFilter};

fn filter(region: &str, tags: &str, q: &str) -> DashboardFilter {
    DashboardFilter {
        region: region.into(),
        tags: tags.into(),
        q: q.into(),
        status: String::new(),
        intake_type: String::new(),
    }
}

#[test]
fn empty_filter_serialises_to_empty_query() {
    let f = DashboardFilter::default();
    assert_eq!(f.to_query_string(), "");
    assert!(!f.is_active());
}

#[test]
fn single_key_serialises_with_that_key_only() {
    let f = filter("north", "", "");
    assert_eq!(f.to_query_string(), "region=north");
    assert!(f.is_active());
}

#[test]
fn multiple_keys_join_with_ampersand_in_stable_order() {
    let f = filter("north", "urgent", "alpha");
    // Order must match the hard-coded `pairs` array in
    // DashboardFilter::to_query_string so the test crate catches any
    // reshuffle that would break the dashboard round-trip.
    assert_eq!(f.to_query_string(), "region=north&tags=urgent&q=alpha");
}

#[test]
fn empty_values_are_dropped_not_emitted_as_bare_keys() {
    let f = filter("", "urgent", "");
    assert_eq!(f.to_query_string(), "tags=urgent");
    // Regression: we must not emit `region=&tags=urgent&q=`.
    assert!(!f.to_query_string().contains("region="));
    assert!(!f.to_query_string().contains("q="));
}

#[test]
fn whitespace_only_values_are_treated_as_empty() {
    let f = filter("   ", "\t", "  ");
    assert_eq!(f.to_query_string(), "");
    assert!(!f.is_active());
}

#[test]
fn special_characters_are_percent_encoded() {
    let f = filter("north west", "tag&x", "alpha=beta");
    let qs = f.to_query_string();
    // Spaces → %20, '&' → %26, '=' → %3D
    assert!(qs.contains("region=north%20west"), "qs={}", qs);
    assert!(qs.contains("tags=tag%26x"), "qs={}", qs);
    assert!(qs.contains("q=alpha%3Dbeta"), "qs={}", qs);
}

#[test]
fn percent_encode_preserves_unreserved_chars() {
    assert_eq!(percent_encode("abcXYZ0_9.-~"), "abcXYZ0_9.-~");
}

#[test]
fn percent_encode_uses_uppercase_hex() {
    // URL standard calls for uppercase hex digits in percent-encoding.
    assert_eq!(percent_encode(" "), "%20");
    assert_eq!(percent_encode("/"), "%2F");
    assert_eq!(percent_encode(":"), "%3A");
}

#[test]
fn is_active_surfaces_any_non_empty_field() {
    let mut f = DashboardFilter::default();
    assert!(!f.is_active());
    f.status = "received".into();
    assert!(f.is_active());
    f.status.clear();
    f.intake_type = "animal".into();
    assert!(f.is_active());
}
