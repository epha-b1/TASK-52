//! Cross-cutting utilities: error sanitization, date formatting, role helpers.
//!
//! ## Error sanitization
//!
//! DB and system errors must NEVER propagate their raw `Display` output to
//! clients. This module wraps the common "map_err → AppError::internal"
//! pattern into `db_err(trace_id)` / `system_err(trace_id)` helpers that log
//! full detail via `tracing::error!` and return a generic user-facing message.
//!
//! ## Local date/time formatting
//!
//! Evidence watermark and traceability codes need real local dates without a
//! chrono dependency. We compute civil date/time fields directly from
//! `SystemTime` using the standard proleptic Gregorian calendar.

use crate::error::AppError;
use crate::extractors::SessionUser;
use sqlx::SqlitePool;
use std::time::{SystemTime, UNIX_EPOCH};

// ── Structured log persistence ────────────────────────────────────────
//
// Writes a row into the `structured_logs` table. This is on top of the
// `tracing` subscriber which emits JSON to stdout — the DB copy lets the
// diagnostics ZIP bundle the last 7 days of local activity even when the
// container's stdout history has been rotated.
//
// The caller is responsible for keeping `message` free of secrets.
// `sanitize_message` below blocks obviously sensitive substrings as a
// defense in depth.

/// Substrings we never allow inside a structured_logs `message` field.
const SENSITIVE_MARKERS: &[&str] = &[
    "password", "Password", "PASSWORD",
    "session_id=", "session=", "bearer ", "Bearer ",
    "Authorization:", "authorization:",
    "token=", "api_key=", "apikey=",
    "$argon2", "secret=",
];

/// Drops or masks sensitive substrings. We refuse to log anything that
/// looks like a credential — the message is replaced with a safe marker.
pub fn sanitize_log_message(msg: &str) -> String {
    for marker in SENSITIVE_MARKERS {
        if msg.contains(marker) {
            return "[REDACTED: sensitive content blocked]".into();
        }
    }
    // Also cap length to prevent log poisoning.
    if msg.len() > 2000 { msg.chars().take(2000).collect() } else { msg.to_string() }
}

/// Fire-and-forget write to structured_logs. Silently swallows errors
/// because logging failures must never break the business flow.
pub async fn slog(db: &SqlitePool, level: &str, message: &str, trace_id: &str) {
    let safe = sanitize_log_message(message);
    let _ = sqlx::query(
        "INSERT INTO structured_logs (level, message, trace_id) VALUES (?, ?, ?)"
    )
    .bind(level)
    .bind(&safe)
    .bind(trace_id)
    .execute(db)
    .await;
}

// ── Error sanitization ────────────────────────────────────────────────

/// Returns a mapping closure that logs the full DB error and yields a
/// sanitized AppError::internal response (no raw `Display` leaked).
pub fn db_err<E: std::fmt::Display>(trace_id: &str) -> impl Fn(E) -> AppError + '_ {
    let tid = trace_id.to_string();
    move |e| {
        tracing::error!(trace_id = %tid, error = %e, "Database error");
        AppError::internal("Internal server error", tid.clone())
    }
}

/// System-level (I/O, crypto, other) error sanitizer.
pub fn system_err<'a, E: std::fmt::Display>(
    trace_id: &'a str,
    context: &'static str,
) -> impl Fn(E) -> AppError + 'a {
    let tid = trace_id.to_string();
    move |e| {
        tracing::error!(trace_id = %tid, context = %context, error = %e, "System error");
        AppError::internal("Internal server error", tid.clone())
    }
}

// ── Role authorization helpers ────────────────────────────────────────

pub fn is_admin(u: &SessionUser) -> bool { u.role == "administrator" }
pub fn is_staff(u: &SessionUser) -> bool { u.role == "operations_staff" }
pub fn is_auditor(u: &SessionUser) -> bool { u.role == "auditor" }

/// Reject auditors from mutating endpoints. Returns 403 if role == auditor.
pub fn require_write_role(u: &SessionUser, trace_id: &str) -> Result<(), AppError> {
    if is_auditor(u) {
        return Err(AppError::forbidden(
            "Auditors have read-only access and cannot perform this action",
            trace_id,
        ));
    }
    Ok(())
}

/// Admin or auditor only (used for traceability publish/retract, exports).
pub fn require_admin_or_auditor(u: &SessionUser, trace_id: &str) -> Result<(), AppError> {
    if !is_admin(u) && !is_auditor(u) {
        return Err(AppError::forbidden("Administrator or Auditor role required", trace_id));
    }
    Ok(())
}

// ── Local date/time formatting (no chrono) ────────────────────────────

/// Civil date computed from a Unix timestamp, adjusted by the facility's
/// configured timezone offset (`FACILITY_TZ_OFFSET_HOURS`). This ensures
/// watermarks and traceability codes reflect local facility time.
#[derive(Debug, Clone, Copy)]
pub struct CivilDateTime {
    pub year: i32,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl CivilDateTime {
    pub fn from_unix(ts: i64) -> Self {
        // Days since Unix epoch (1970-01-01).
        let secs_per_day = 86_400i64;
        let mut days = ts.div_euclid(secs_per_day);
        let mut secs_of_day = ts.rem_euclid(secs_per_day) as i32;
        if secs_of_day < 0 { secs_of_day += secs_per_day as i32; days -= 1; }

        // Civil from days algorithm by Howard Hinnant.
        let z = days + 719_468; // shift epoch to 0000-03-01
        let era = if z >= 0 { z } else { z - 146096 } / 146097;
        let doe = (z - era * 146097) as u32; // [0, 146096]
        let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365; // [0, 399]
        let y = yoe as i32 + era as i32 * 400;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100); // [0, 365]
        let mp = (5 * doy + 2) / 153; // [0, 11]
        let d = (doy - (153 * mp + 2) / 5 + 1) as u8; // [1, 31]
        let m = (if mp < 10 { mp + 3 } else { mp - 9 }) as u8; // [1, 12]
        let year = if m <= 2 { y + 1 } else { y };

        let hour = (secs_of_day / 3600) as u8;
        let minute = ((secs_of_day % 3600) / 60) as u8;
        let second = (secs_of_day % 60) as u8;

        CivilDateTime { year, month: m, day: d, hour, minute, second }
    }

    /// Return the current civil date/time adjusted by the facility's
    /// timezone offset. The offset is read from `FACILITY_TZ_OFFSET_HOURS`
    /// (e.g. `-5` for EST, `-8` for PST, `0` for UTC). Defaults to 0.
    pub fn now() -> Self {
        let offset_hours: i64 = std::env::var("FACILITY_TZ_OFFSET_HOURS")
            .ok()
            .and_then(|v| v.parse().ok())
            .unwrap_or(0);
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs() as i64)
            .unwrap_or(0);
        Self::from_unix(ts + offset_hours * 3600)
    }

    /// `YYYYMMDD`
    pub fn yyyymmdd(&self) -> String {
        format!("{:04}{:02}{:02}", self.year, self.month, self.day)
    }

    /// `MM/DD/YYYY hh:mm AM/PM` (12-hour clock)
    pub fn us_12h(&self) -> String {
        let (h12, ampm) = match self.hour {
            0 => (12u8, "AM"),
            h if h < 12 => (h, "AM"),
            12 => (12, "PM"),
            h => (h - 12, "PM"),
        };
        format!("{:02}/{:02}/{:04} {:02}:{:02} {}",
            self.month, self.day, self.year, h12, self.minute, ampm)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sanitize_blocks_password_word() {
        assert_eq!(
            sanitize_log_message("user submitted password: hunter2"),
            "[REDACTED: sensitive content blocked]"
        );
    }

    #[test]
    fn sanitize_blocks_argon_hash() {
        assert_eq!(
            sanitize_log_message("stored $argon2id$v=19$..."),
            "[REDACTED: sensitive content blocked]"
        );
    }

    #[test]
    fn sanitize_blocks_bearer_token() {
        assert_eq!(
            sanitize_log_message("Authorization: Bearer abc.def"),
            "[REDACTED: sensitive content blocked]"
        );
    }

    #[test]
    fn sanitize_passes_clean_messages() {
        assert_eq!(
            sanitize_log_message("intake.create id=abc123"),
            "intake.create id=abc123"
        );
    }

    #[test]
    fn sanitize_caps_very_long_messages() {
        let long = "a".repeat(5000);
        let result = sanitize_log_message(&long);
        assert_eq!(result.len(), 2000);
    }

    #[test]
    fn unix_epoch_is_1970_01_01() {
        let d = CivilDateTime::from_unix(0);
        assert_eq!((d.year, d.month, d.day), (1970, 1, 1));
        assert_eq!((d.hour, d.minute, d.second), (0, 0, 0));
    }

    #[test]
    fn mid_2020_date() {
        // 2020-07-04 00:00:00 UTC = 1593820800
        let d = CivilDateTime::from_unix(1_593_820_800);
        assert_eq!((d.year, d.month, d.day), (2020, 7, 4));
    }

    #[test]
    fn yyyymmdd_format() {
        let d = CivilDateTime { year: 2026, month: 4, day: 5, hour: 0, minute: 0, second: 0 };
        assert_eq!(d.yyyymmdd(), "20260405");
    }

    #[test]
    fn us_12h_morning() {
        let d = CivilDateTime { year: 2026, month: 4, day: 5, hour: 9, minute: 30, second: 0 };
        assert_eq!(d.us_12h(), "04/05/2026 09:30 AM");
    }

    #[test]
    fn us_12h_noon() {
        let d = CivilDateTime { year: 2026, month: 4, day: 5, hour: 12, minute: 0, second: 0 };
        assert_eq!(d.us_12h(), "04/05/2026 12:00 PM");
    }

    #[test]
    fn us_12h_midnight() {
        let d = CivilDateTime { year: 2026, month: 4, day: 5, hour: 0, minute: 0, second: 0 };
        assert_eq!(d.us_12h(), "04/05/2026 12:00 AM");
    }

    #[test]
    fn us_12h_evening() {
        let d = CivilDateTime { year: 2026, month: 4, day: 5, hour: 15, minute: 45, second: 0 };
        assert_eq!(d.us_12h(), "04/05/2026 03:45 PM");
    }
}
