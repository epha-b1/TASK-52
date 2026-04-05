//! Draft autosave + session-expiry restore (WASM side).
//!
//! All business logic lives in `fieldtrace-shared` behind a `DraftStore`
//! trait so `cargo test -p fieldtrace-shared` exercises the full
//! preserve/consume round-trip with an in-memory mock store on the host
//! target. The only thing this file does is wrap `web_sys::Storage`
//! with an impl of that trait and expose the thin `save_draft`/
//! `load_draft`/`preserve_route`/`consume_pending_route` helpers that
//! the intake/address forms and the app shell import.

#[allow(unused_imports)]
use fieldtrace_shared::{
    clear_draft_from, consume_pending_route_from, consume_session_flash_from,
    flash_session_expired_to, load_draft_from, preserve_route_to, save_draft_to,
    DraftStore, DRAFT_KEY_PREFIX, PENDING_ROUTE_KEY, SESSION_MSG_KEY,
};

/// Distinct literal embedded in the WASM bundle so the integration test
/// can prove the restore call site was not dead-code-eliminated. The
/// banner shown to the user after a successful re-login uses this
/// exact string prefix.
pub const RESTORE_BANNER_PREFIX: &str = "fieldtrace.session_restored_from:";

#[cfg(target_arch = "wasm32")]
struct WebStorage(web_sys::Storage);

#[cfg(target_arch = "wasm32")]
impl DraftStore for WebStorage {
    fn get(&self, key: &str) -> Option<String> {
        self.0.get_item(key).ok().flatten()
    }
    fn set(&self, key: &str, value: &str) {
        let _ = self.0.set_item(key, value);
    }
    fn remove(&self, key: &str) {
        let _ = self.0.remove_item(key);
    }
}

#[cfg(target_arch = "wasm32")]
fn storage() -> Option<WebStorage> {
    web_sys::window()?
        .local_storage()
        .ok()
        .flatten()
        .map(WebStorage)
}

// ── WASM helpers: thin wrappers over the shared DraftStore API ────────

#[cfg(target_arch = "wasm32")]
pub fn save_draft(form_id: &str, fields: serde_json::Value) {
    if let Some(s) = storage() {
        save_draft_to(&s, form_id, fields);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn load_draft(form_id: &str) -> Option<serde_json::Value> {
    storage().and_then(|s| load_draft_from(&s, form_id))
}

#[cfg(target_arch = "wasm32")]
pub fn clear_draft(form_id: &str) {
    if let Some(s) = storage() {
        clear_draft_from(&s, form_id);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn preserve_route(route: &str) {
    if let Some(s) = storage() {
        preserve_route_to(&s, route);
    }
}

#[cfg(target_arch = "wasm32")]
pub fn consume_pending_route() -> Option<String> {
    storage().and_then(|s| consume_pending_route_from(&s))
}

#[cfg(target_arch = "wasm32")]
pub fn flash_session_expired() {
    if let Some(s) = storage() {
        flash_session_expired_to(
            &s,
            "Your session expired. Your draft has been preserved — please sign in to continue.",
        );
    }
}

#[cfg(target_arch = "wasm32")]
pub fn consume_session_flash() -> Option<String> {
    storage().and_then(|s| consume_session_flash_from(&s))
}

/// Update the browser URL bar to `route` without triggering a
/// navigation. Called by the app shell after `consume_pending_route`
/// returns Some.
#[cfg(target_arch = "wasm32")]
pub fn restore_browser_url(route: &str) {
    if let Some(w) = web_sys::window() {
        if let Ok(h) = w.history() {
            let _ = h.replace_state_with_url(
                &wasm_bindgen::JsValue::NULL,
                "",
                Some(route),
            );
        }
    }
}

// ── Non-wasm no-op shims so host-target callers still compile ─────────
// (The frontend crate is only compiled for wasm32 by trunk, so these are
// never exercised at runtime — they exist only so `cargo check` against
// the host target doesn't break if anyone ever tries.)

#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn save_draft(_form_id: &str, _fields: serde_json::Value) {}
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn load_draft(_form_id: &str) -> Option<serde_json::Value> { None }
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn clear_draft(_form_id: &str) {}
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn preserve_route(_route: &str) {}
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn consume_pending_route() -> Option<String> { None }
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn flash_session_expired() {}
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn consume_session_flash() -> Option<String> { None }
#[cfg(not(target_arch = "wasm32"))]
#[allow(dead_code)]
pub fn restore_browser_url(_route: &str) {}
