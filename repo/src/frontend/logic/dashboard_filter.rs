//! Dashboard / reports filter serialization.
//!
//! The Leptos `DashboardPage` binds an interactive filter row (region,
//! tags, free-text query, status, intake type). This module owns the
//! pure serialization: turn the in-memory filter struct into the exact
//! query-string shape the `/reports/summary` and `/reports/export`
//! endpoints accept. The test crate locks the shape so a silent drift
//! can't break the dashboard round-trip.

/// Dashboard filter state as edited on the client.
/// Empty strings are treated identically to `None` — the server echoes
/// an unfiltered summary in that case.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DashboardFilter {
    pub region: String,
    pub tags: String,
    pub q: String,
    pub status: String,
    pub intake_type: String,
}

impl DashboardFilter {
    /// Convert the filter into a URL query string (without the leading
    /// `?`). Keys are emitted in a stable order; empty values are
    /// dropped so the server doesn't have to differentiate
    /// `?region=` from `?region` from absence.
    pub fn to_query_string(&self) -> String {
        let pairs = [
            ("region", self.region.as_str()),
            ("tags", self.tags.as_str()),
            ("q", self.q.as_str()),
            ("status", self.status.as_str()),
            ("intake_type", self.intake_type.as_str()),
        ];
        let mut out = String::new();
        for (k, v) in pairs {
            let v = v.trim();
            if v.is_empty() {
                continue;
            }
            if !out.is_empty() {
                out.push('&');
            }
            out.push_str(k);
            out.push('=');
            out.push_str(&percent_encode(v));
        }
        out
    }

    /// Whether any filter is currently active. Drives a "Reset"-button
    /// visibility toggle in the dashboard UI.
    pub fn is_active(&self) -> bool {
        !self.region.trim().is_empty()
            || !self.tags.trim().is_empty()
            || !self.q.trim().is_empty()
            || !self.status.trim().is_empty()
            || !self.intake_type.trim().is_empty()
    }
}

/// Minimal, dependency-free URL percent encoder for the characters we
/// actually put into filter values (letters, digits, spaces, commas,
/// hyphens). The frontend bin imports this same helper so behavior is
/// identical between runtime and test.
pub fn percent_encode(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    for b in raw.as_bytes() {
        let c = *b;
        let is_unreserved = c.is_ascii_alphanumeric()
            || c == b'-'
            || c == b'_'
            || c == b'.'
            || c == b'~';
        if is_unreserved {
            out.push(c as char);
        } else {
            out.push_str(&format!("%{:02X}", c));
        }
    }
    out
}
