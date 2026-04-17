//! App shell / session behavior tests.
//!
//! Exercises the real `src/frontend/draft.rs` (via the lib crate's
//! `#[path]` import) and the `src/frontend/logic/session.rs` state
//! machine. Asserts the actual draft autosave constants, the pending-
//! route preserve → consume round-trip, session-flash lifecycle, and
//! every legal page transition of the app shell reducer.

use fieldtrace_frontend_tests::frontend_draft;
use fieldtrace_frontend_tests::frontend_session::{next_page, Page, SessionEvent};
use fieldtrace_shared::{
    clear_draft_from, consume_pending_route_from, consume_session_flash_from,
    flash_session_expired_to, load_draft_from, preserve_route_to, save_draft_to,
    DraftStore, DRAFT_KEY_PREFIX, PENDING_ROUTE_KEY, SESSION_MSG_KEY,
};
use std::cell::RefCell;
use std::collections::HashMap;

/// Minimal in-memory stand-in for `web_sys::Storage` used only to
/// drive the shared `DraftStore` trait from a host-target test.
struct MemStore(RefCell<HashMap<String, String>>);
impl MemStore {
    fn new() -> Self {
        MemStore(RefCell::new(HashMap::new()))
    }
}
impl DraftStore for MemStore {
    fn get(&self, k: &str) -> Option<String> {
        self.0.borrow().get(k).cloned()
    }
    fn set(&self, k: &str, v: &str) {
        self.0.borrow_mut().insert(k.to_string(), v.to_string());
    }
    fn remove(&self, k: &str) {
        self.0.borrow_mut().remove(k);
    }
}

// ── Frontend module-level contract ────────────────────────────────────

#[test]
fn frontend_draft_exposes_restore_banner_prefix() {
    // This const is what the integration-test suite greps for inside
    // the WASM bundle to prove the restore path hasn't been DCE'd.
    assert_eq!(
        frontend_draft::RESTORE_BANNER_PREFIX,
        "fieldtrace.session_restored_from:"
    );
}

#[test]
fn frontend_draft_non_wasm_shims_are_inert() {
    // On the host target the frontend crate's shims return None /
    // no-op, which is the contract tested here. If anyone ever
    // removed the `#[cfg(not(target_arch = "wasm32"))]` shim the
    // test crate would stop compiling.
    assert!(frontend_draft::load_draft("intake-form").is_none());
    assert!(frontend_draft::consume_pending_route().is_none());
    assert!(frontend_draft::consume_session_flash().is_none());
    // save/clear/preserve are no-ops; calling them must not panic.
    frontend_draft::save_draft("intake-form", serde_json::json!({"a": 1}));
    frontend_draft::clear_draft("intake-form");
    frontend_draft::preserve_route("/dashboard");
    frontend_draft::flash_session_expired();
    frontend_draft::restore_browser_url("/dashboard");
}

// ── Draft store round-trip (what the frontend saves/restores) ────────

#[test]
fn save_and_load_preserves_full_form_state() {
    let s = MemStore::new();
    let payload = serde_json::json!({
        "intake_type": "animal",
        "tag": "A-42",
        "region": "north",
        "notes": "found at east fence line",
    });
    save_draft_to(&s, "intake-form", payload.clone());
    let loaded = load_draft_from(&s, "intake-form");
    assert_eq!(loaded, Some(payload));
}

#[test]
fn draft_key_uses_the_documented_prefix() {
    let s = MemStore::new();
    save_draft_to(&s, "address-form", serde_json::json!({"street":"1 Main"}));
    // The internal key must be exactly `<prefix><form_id>`.
    let k = format!("{}{}", DRAFT_KEY_PREFIX, "address-form");
    assert!(s.get(&k).is_some(), "expected storage key {} to exist", k);
}

#[test]
fn clear_draft_removes_the_entry() {
    let s = MemStore::new();
    save_draft_to(&s, "supply-form", serde_json::json!({"name":"Can"}));
    clear_draft_from(&s, "supply-form");
    assert!(load_draft_from(&s, "supply-form").is_none());
}

#[test]
fn preserve_and_consume_pending_route_is_single_shot() {
    let s = MemStore::new();
    preserve_route_to(&s, "/dashboard");
    assert_eq!(consume_pending_route_from(&s), Some("/dashboard".to_string()));
    // Consumed — second call sees nothing.
    assert_eq!(consume_pending_route_from(&s), None);
    // And the raw key is gone from storage.
    assert!(s.get(PENDING_ROUTE_KEY).is_none());
}

#[test]
fn flash_session_expired_round_trip() {
    let s = MemStore::new();
    flash_session_expired_to(&s, "Your session expired.");
    assert_eq!(
        consume_session_flash_from(&s),
        Some("Your session expired.".to_string()),
    );
    assert!(s.get(SESSION_MSG_KEY).is_none(), "flash must be consumed");
}

// ── App-shell reducer: every legal transition ────────────────────────

#[test]
fn initial_loading_transitions_to_dashboard_when_auth_ok() {
    assert_eq!(next_page(&Page::Loading, &SessionEvent::AuthCheckOk), Page::Dashboard);
}

#[test]
fn initial_loading_falls_back_to_login_on_failed_auth_check() {
    assert_eq!(next_page(&Page::Loading, &SessionEvent::AuthCheckFailed), Page::Login);
}

#[test]
fn login_success_on_login_page_lands_on_dashboard() {
    assert_eq!(next_page(&Page::Login, &SessionEvent::LoginSucceeded), Page::Dashboard);
}

#[test]
fn login_page_can_navigate_to_register() {
    assert_eq!(next_page(&Page::Login, &SessionEvent::NavigateToRegister), Page::Register);
}

#[test]
fn register_page_back_to_login() {
    assert_eq!(next_page(&Page::Register, &SessionEvent::NavigateToLogin), Page::Login);
}

#[test]
fn logout_from_dashboard_returns_to_login() {
    assert_eq!(next_page(&Page::Dashboard, &SessionEvent::LogoutRequested), Page::Login);
}

#[test]
fn login_failure_always_routes_back_to_login() {
    for start in [Page::Loading, Page::Login, Page::Register, Page::Dashboard] {
        assert_eq!(
            next_page(&start, &SessionEvent::LoginFailed),
            Page::Login,
            "starting from {:?}",
            start
        );
    }
}

#[test]
fn stale_events_are_no_ops() {
    // Logging out while on the Login page is a stale event from a
    // late-arriving logout callback; it must leave the page unchanged.
    assert_eq!(next_page(&Page::Login, &SessionEvent::LogoutRequested), Page::Login);
    // Auth-check OK arriving after we already bailed to Login shouldn't
    // silently drag the user into Dashboard.
    assert_eq!(next_page(&Page::Login, &SessionEvent::AuthCheckOk), Page::Login);
}
