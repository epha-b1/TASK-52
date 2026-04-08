use serde::{Deserialize, Serialize};

// ── Draft autosave / session-expiry restore (pure helpers) ────────────
//
// Frontend-only feature, but the pure key/serialization logic lives here
// in shared so it can be exercised by `cargo test` on the host target
// (the frontend crate itself only compiles for wasm32 via trunk).

pub const DRAFT_KEY_PREFIX: &str = "fieldtrace.draft.";
pub const PENDING_ROUTE_KEY: &str = "fieldtrace.pending_route";
pub const SESSION_MSG_KEY: &str = "fieldtrace.session_msg";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FormDraft {
    pub form_id: String,
    pub fields: serde_json::Value,
}

pub fn draft_key(form_id: &str) -> String {
    format!("{}{}", DRAFT_KEY_PREFIX, form_id)
}

pub fn serialize_draft(form_id: &str, fields: serde_json::Value) -> String {
    let d = FormDraft { form_id: form_id.to_string(), fields };
    serde_json::to_string(&d).unwrap_or_default()
}

pub fn deserialize_draft(json: &str) -> Option<FormDraft> {
    serde_json::from_str(json).ok()
}

// ── Storage abstraction so the save/load/preserve/consume round-trip
//    can be exercised on the host target (pure cargo test) AND on wasm32
//    (web_sys::Storage wrapper in the frontend crate) with the same
//    logic path. ────────────────────────────────────────────────────────

pub trait DraftStore {
    fn get(&self, key: &str) -> Option<String>;
    fn set(&self, key: &str, value: &str);
    fn remove(&self, key: &str);
}

/// Persist a form draft snapshot into the given store.
pub fn save_draft_to<S: DraftStore>(store: &S, form_id: &str, fields: serde_json::Value) {
    store.set(&draft_key(form_id), &serialize_draft(form_id, fields));
}

/// Read back the previously-saved draft snapshot (fields only).
pub fn load_draft_from<S: DraftStore>(store: &S, form_id: &str) -> Option<serde_json::Value> {
    let json = store.get(&draft_key(form_id))?;
    deserialize_draft(&json).map(|d| d.fields)
}

/// Remove a draft snapshot (called on successful submit).
pub fn clear_draft_from<S: DraftStore>(store: &S, form_id: &str) {
    store.remove(&draft_key(form_id));
}

/// Record the route the user was on when their session expired. The app
/// shell re-reads this after login to navigate back.
pub fn preserve_route_to<S: DraftStore>(store: &S, route: &str) {
    store.set(PENDING_ROUTE_KEY, route);
}

/// Read-and-clear the preserved route. Returns None if no route was
/// pending — this is how the app shell knows "nothing to restore".
pub fn consume_pending_route_from<S: DraftStore>(store: &S) -> Option<String> {
    let v = store.get(PENDING_ROUTE_KEY);
    store.remove(PENDING_ROUTE_KEY);
    v
}

/// Flash a user-visible message that's rendered on the next app mount.
pub fn flash_session_expired_to<S: DraftStore>(store: &S, msg: &str) {
    store.set(SESSION_MSG_KEY, msg);
}

/// Read-and-clear the session-expired flash message.
pub fn consume_session_flash_from<S: DraftStore>(store: &S) -> Option<String> {
    let v = store.get(SESSION_MSG_KEY);
    store.remove(SESSION_MSG_KEY);
    v
}

#[cfg(test)]
mod draft_tests {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    /// In-memory DraftStore for unit tests. Interior mutability mirrors
    /// the real `web_sys::Storage` shape where methods take `&self`.
    struct MockStore(RefCell<HashMap<String, String>>);
    impl MockStore {
        fn new() -> Self { MockStore(RefCell::new(HashMap::new())) }
        fn len(&self) -> usize { self.0.borrow().len() }
    }
    impl DraftStore for MockStore {
        fn get(&self, key: &str) -> Option<String> {
            self.0.borrow().get(key).cloned()
        }
        fn set(&self, key: &str, value: &str) {
            self.0.borrow_mut().insert(key.to_string(), value.to_string());
        }
        fn remove(&self, key: &str) {
            self.0.borrow_mut().remove(key);
        }
    }

    #[test]
    fn draft_key_has_prefix() {
        assert_eq!(draft_key("intake-form"), "fieldtrace.draft.intake-form");
        assert_eq!(draft_key("address-new"), "fieldtrace.draft.address-new");
    }

    #[test]
    fn round_trip_preserves_fields() {
        let fields = serde_json::json!({
            "intake_type": "animal",
            "details": "rescue pup"
        });
        let s = serialize_draft("intake-form", fields.clone());
        let d = deserialize_draft(&s).unwrap();
        assert_eq!(d.form_id, "intake-form");
        assert_eq!(d.fields, fields);
    }

    #[test]
    fn deserialize_rejects_garbage() {
        assert!(deserialize_draft("not json").is_none());
        assert!(deserialize_draft("").is_none());
    }

    #[test]
    fn constants_are_stable() {
        // These constants are greppable from the built wasm bundle —
        // the integration test relies on them being present verbatim.
        assert_eq!(DRAFT_KEY_PREFIX, "fieldtrace.draft.");
        assert_eq!(PENDING_ROUTE_KEY, "fieldtrace.pending_route");
        assert_eq!(SESSION_MSG_KEY, "fieldtrace.session_msg");
    }

    // ── Real round-trip tests over the DraftStore trait ───────────────

    #[test]
    fn save_and_load_full_round_trip() {
        let store = MockStore::new();
        let fields = serde_json::json!({
            "label": "Home",
            "street": "1 Main",
            "zip_plus4": "90210-1234"
        });
        save_draft_to(&store, "address-form", fields.clone());
        let loaded = load_draft_from(&store, "address-form").expect("draft present");
        assert_eq!(loaded, fields);
    }

    #[test]
    fn load_returns_none_for_unknown_form() {
        let store = MockStore::new();
        assert!(load_draft_from(&store, "nonexistent").is_none());
    }

    #[test]
    fn clear_draft_removes_the_entry() {
        let store = MockStore::new();
        save_draft_to(&store, "intake-form",
            serde_json::json!({"intake_type": "animal"}));
        assert!(load_draft_from(&store, "intake-form").is_some());
        clear_draft_from(&store, "intake-form");
        assert!(load_draft_from(&store, "intake-form").is_none());
    }

    #[test]
    fn preserve_and_consume_route_round_trip() {
        let store = MockStore::new();
        // Nothing to consume initially.
        assert!(consume_pending_route_from(&store).is_none());

        preserve_route_to(&store, "/dashboard/intake");
        // Consume returns the preserved value …
        assert_eq!(
            consume_pending_route_from(&store),
            Some("/dashboard/intake".to_string())
        );
        // … AND clears the key so a second consume returns None.
        assert!(consume_pending_route_from(&store).is_none());
    }

    #[test]
    fn consume_pending_route_is_idempotent_per_preserve() {
        let store = MockStore::new();
        preserve_route_to(&store, "/a");
        preserve_route_to(&store, "/b"); // second call overwrites
        assert_eq!(consume_pending_route_from(&store), Some("/b".into()));
        // After consume, store has no draft + no route.
        assert_eq!(store.len(), 0);
    }

    #[test]
    fn session_flash_round_trip() {
        let store = MockStore::new();
        flash_session_expired_to(&store, "Your session expired");
        assert_eq!(
            consume_session_flash_from(&store),
            Some("Your session expired".to_string())
        );
        assert!(consume_session_flash_from(&store).is_none());
    }

    #[test]
    fn draft_and_route_are_stored_under_different_keys() {
        let store = MockStore::new();
        save_draft_to(&store, "intake-form",
            serde_json::json!({"intake_type": "animal"}));
        preserve_route_to(&store, "/dashboard");
        // Both present simultaneously.
        assert!(load_draft_from(&store, "intake-form").is_some());
        assert_eq!(consume_pending_route_from(&store), Some("/dashboard".into()));
        // Consuming the route does NOT touch the draft.
        assert!(load_draft_from(&store, "intake-form").is_some());
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthResponse { pub status: String }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    pub status: u16,
    pub code: String,
    pub message: String,
    pub trace_id: String,
}

// ── Auth ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegisterRequest { pub username: String, pub password: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LoginRequest { pub username: String, pub password: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChangePasswordRequest { pub current_password: String, pub new_password: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UserResponse { pub id: String, pub username: String, pub role: String, pub created_at: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthResponse { pub user: UserResponse, pub message: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CreateUserRequest { pub username: String, pub password: String, pub role: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UpdateUserRequest { pub role: Option<String> }

// ── Address Book ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressRequest {
    pub label: String, pub street: String, pub city: String,
    pub state: String, pub zip_plus4: String, pub phone: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AddressResponse {
    pub id: String, pub label: String,
    /// Masked street (house number + "***") to reduce incidental exposure.
    pub street_masked: String,
    /// Masked city (first 2 chars + "***").
    pub city_masked: String,
    /// State abbreviation (not masked — 2 chars not sensitive alone).
    pub state: String,
    pub zip_plus4: String,
    pub phone_masked: String,
    pub created_at: String,
}

// ── Intake ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntakeRequest {
    pub intake_type: String,
    pub details: String,
    /// Optional region tag (e.g. "north", "warehouse-2"). Used by the
    /// dashboard region filter. Empty string when not specified.
    #[serde(default)]
    pub region: String,
    /// Comma-separated tag list. Used by the dashboard tag filter.
    #[serde(default)]
    pub tags: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntakeResponse {
    pub id: String, pub facility_id: String, pub intake_type: String,
    pub status: String, pub details: String, pub created_by: String, pub created_at: String,
    #[serde(default)]
    pub region: String,
    #[serde(default)]
    pub tags: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusUpdateRequest { pub status: String }

// ── Transfers ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRequest {
    pub intake_id: Option<String>,
    pub destination: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub notes: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferResponse {
    pub id: String,
    pub intake_id: Option<String>,
    pub origin_facility_id: String,
    pub destination: String,
    pub reason: String,
    pub status: String,
    pub notes: String,
    pub created_by: String,
    pub created_at: String,
}

// ── Stock movements ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockMovementRequest {
    pub supply_id: Option<String>,
    pub quantity_delta: i64,
    pub reason: String,
    #[serde(default)]
    pub notes: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StockMovementResponse {
    pub id: String,
    pub supply_id: Option<String>,
    pub quantity_delta: i64,
    pub reason: String,
    pub notes: String,
    pub actor_id: String,
    pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventoryLine {
    pub supply_id: Option<String>,
    pub quantity: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InventorySnapshot {
    pub total_on_hand: i64,
    pub by_supply: Vec<InventoryLine>,
}

// ── Traceability steps ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceStepResponse {
    pub id: String,
    pub code_id: String,
    pub step_type: String,
    pub step_label: String,
    pub details: String,
    pub occurred_at: String,
}

// ── Inspections ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectionRequest { pub intake_id: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InspectionResponse {
    pub id: String, pub intake_id: String, pub inspector_id: String,
    pub status: String, pub outcome_notes: String, pub created_at: String, pub resolved_at: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolveInspectionRequest { pub status: String, pub outcome_notes: String }

// ── Evidence ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadStartRequest {
    pub filename: String, pub media_type: String,
    pub total_size: i64, pub duration_seconds: i64,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadStartResponse { pub upload_id: String, pub chunk_size_bytes: i64, pub total_chunks: i64 }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadChunkRequest {
    pub upload_id: String,
    pub chunk_index: i64,
    /// Base64-encoded chunk payload. The backend decodes and persists the
    /// raw bytes to `storage/uploads/<upload_id>/chunk_<index>`.
    #[serde(default)]
    pub data: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UploadCompleteRequest {
    pub upload_id: String, pub fingerprint: String, pub total_size: i64,
    pub exif_capture_time: Option<String>, pub tags: Option<String>, pub keyword: Option<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceResponse {
    pub id: String, pub filename: String, pub media_type: String,
    pub watermark_text: String, pub missing_exif: bool,
    pub linked: bool, pub legal_hold: bool, pub created_at: String,
    /// Actual stored file size on disk. Currently equals the original size
    /// (no in-process transcoding). Reserved for future offline pipeline.
    pub compressed_bytes: i64,
    /// 1.0 when no transcoding was performed (current behavior).
    pub compression_ratio: f64,
    /// False when the file was stored unchanged (current behavior).
    pub compression_applied: bool,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvidenceLinkRequest { pub target_type: String, pub target_id: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LegalHoldRequest { pub legal_hold: bool }

// ── Supply ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyRequest {
    pub name: String, pub sku: Option<String>, pub size: String, pub color: String,
    pub price_cents: Option<i64>, pub discount_cents: Option<i64>, pub notes: String,
    /// Current stock status: "in_stock", "low_stock", "out_of_stock", or "unknown".
    #[serde(default = "default_stock_status")]
    pub stock_status: String,
    /// Comma-separated media reference IDs (evidence links).
    #[serde(default)]
    pub media_references: String,
    /// Short review summary for quick audit scan.
    #[serde(default)]
    pub review_summary: String,
}

fn default_stock_status() -> String { "unknown".into() }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyResponse {
    pub id: String, pub name: String, pub sku: Option<String>,
    pub canonical_size: Option<String>, pub canonical_color: Option<String>,
    pub price_cents: Option<i64>, pub parse_status: String, pub parse_conflicts: String, pub created_at: String,
    #[serde(default = "default_stock_status")]
    pub stock_status: String,
    #[serde(default)]
    pub media_references: String,
    #[serde(default)]
    pub review_summary: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupplyResolveRequest {
    pub canonical_color: Option<String>, pub canonical_size: Option<String>,
}

// ── Traceability ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceCodeRequest { pub intake_id: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TraceCodeResponse {
    pub id: String, pub code: String, pub intake_id: Option<String>,
    pub status: String, pub version: i64, pub created_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TracePublishRequest { pub comment: String }

// ── Privacy Preferences ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyPreferencesResponse {
    pub show_email: bool,
    pub show_phone: bool,
    pub allow_audit_log_export: bool,
    pub allow_data_sharing: bool,
    pub updated_at: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrivacyPreferencesUpdate {
    pub show_email: Option<bool>,
    pub show_phone: Option<bool>,
    pub allow_audit_log_export: Option<bool>,
    pub allow_data_sharing: Option<bool>,
}

// ── Check-In ──
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberRequest { pub member_id: String, pub name: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemberResponse { pub id: String, pub member_id: String, pub name: String, pub created_at: String }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckinRequest { pub member_id: String, pub override_reason: Option<String> }
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckinResponse {
    pub id: String, pub member_id: String, pub checked_in_at: String, pub was_override: bool,
}
