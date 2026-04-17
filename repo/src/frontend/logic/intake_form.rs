//! Intake form — the primary form-heavy module on the client side.
//! Centralises the submittability rules and the JSON shape that gets
//! POSTed to `/intake` so both `pages/intake.rs` and the draft/autosave
//! machinery agree on what the form state looks like.

use serde::{Deserialize, Serialize};

/// The three intake kinds the backend accepts.
pub const INTAKE_TYPES: [&str; 3] = ["animal", "supply", "donation"];

/// User-editable intake form state. Kept as a plain struct so it can be
/// trivially JSON-serialised for draft autosave AND for the POST body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct IntakeForm {
    pub intake_type: String,
    pub tag: String,
    pub region: String,
    pub notes: String,
}

/// Whether this intake type is one the backend will accept.
pub fn is_known_intake_type(kind: &str) -> bool {
    INTAKE_TYPES.contains(&kind)
}

impl IntakeForm {
    /// Form is submittable when a valid intake_type is selected and the
    /// region is not whitespace. Notes/tag are optional.
    pub fn is_submittable(&self) -> bool {
        is_known_intake_type(&self.intake_type) && !self.region.trim().is_empty()
    }

    /// Produce the JSON body POSTed to `/intake`. The backend expects
    /// `intake_type` at the top level and a `details` string carrying the
    /// arbitrary metadata block; we bundle tag/region/notes into that
    /// details blob.
    pub fn to_create_body(&self) -> serde_json::Value {
        let details = serde_json::json!({
            "tag": self.tag,
            "region": self.region,
            "notes": self.notes,
        });
        serde_json::json!({
            "intake_type": self.intake_type,
            "details": details.to_string(),
        })
    }

    /// Clear transient fields after a successful submission so the form
    /// is ready for the next entry — kept deterministic so the draft
    /// autosave layer can re-use it.
    pub fn reset(&mut self) {
        self.tag.clear();
        self.notes.clear();
        // intake_type + region intentionally sticky across entries.
    }
}
