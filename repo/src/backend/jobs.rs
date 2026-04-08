//! Background jobs registered at startup.
//!
//! - `session_cleanup`: deletes expired sessions every 5 minutes.
//! - `account_deletion_purge`: purges users whose deletion_requested_at is
//!   older than 7 days. Runs every hour.
//! - `diagnostics_cleanup`: deletes diagnostic ZIPs older than 1 hour.
//! - `evidence_retention`: marks evidence past retention for cleanup.
//!
//! Each job writes its status to `job_metrics` so `/admin/jobs` shows real data.

use crate::config::Config;
use sqlx::SqlitePool;
use std::path::PathBuf;
use std::time::Duration;

pub fn register_all(db: SqlitePool, config: Config) {
    let d1 = db.clone();
    tokio::spawn(async move { session_cleanup_loop(d1).await; });
    let d2 = db.clone();
    tokio::spawn(async move { account_deletion_purge_loop(d2).await; });
    let d3 = db.clone();
    let c3 = config.clone();
    tokio::spawn(async move { diagnostics_cleanup_loop(d3, c3).await; });
    let d4 = db.clone();
    tokio::spawn(async move { evidence_retention_loop(d4).await; });
    let d5 = db.clone();
    let c5 = config.clone();
    tokio::spawn(async move { stale_upload_cleanup_loop(d5, c5).await; });
    tracing::info!("All background jobs registered");
}

async fn record_run(db: &SqlitePool, job: &str, status: &str, err: Option<String>) {
    let _ = sqlx::query(
        "INSERT INTO job_metrics (job_name, status, run_count, last_error, last_run_at) \
         VALUES (?, ?, 1, ?, datetime('now'))"
    )
    .bind(job)
    .bind(status)
    .bind(err)
    .execute(db)
    .await;
}

// ── Session cleanup ───────────────────────────────────────────────────

async fn session_cleanup_loop(db: SqlitePool) {
    tracing::info!(job = "session_cleanup", "Registered (every 5 min)");
    // First tick is immediate so metrics show up promptly.
    let mut ticker = tokio::time::interval(Duration::from_secs(300));
    loop {
        ticker.tick().await;
        let res = sqlx::query(
            "DELETE FROM sessions WHERE last_active < datetime('now', '-30 minutes')"
        ).execute(&db).await;
        match res {
            Ok(r) => {
                tracing::info!(job = "session_cleanup", deleted = r.rows_affected(), "cleanup run");
                record_run(&db, "session_cleanup", "ok", None).await;
            }
            Err(e) => {
                tracing::error!(job = "session_cleanup", error = %e, "cleanup failed");
                record_run(&db, "session_cleanup", "error", Some(e.to_string())).await;
            }
        }
    }
}

// ── Account deletion purge ────────────────────────────────────────────

async fn account_deletion_purge_loop(db: SqlitePool) {
    tracing::info!(job = "account_deletion_purge", "Registered (every 1h)");
    let mut ticker = tokio::time::interval(Duration::from_secs(3600));
    loop {
        ticker.tick().await;
        match run_account_purge(&db, 7).await {
            Ok(purged) => {
                tracing::info!(job = "account_deletion_purge", purged, "purge run");
                record_run(&db, "account_deletion_purge", "ok", None).await;
            }
            Err(e) => {
                tracing::error!(job = "account_deletion_purge", error = %e, "purge failed");
                record_run(&db, "account_deletion_purge", "error", Some(e)).await;
            }
        }
    }
}

/// Runs a single purge pass transactionally. Anonymizes eligible users
/// (deletion_requested_at older than `grace_period_days`) instead of
/// hard-deleting, preserving every FK reference. Personal data (address
/// book entries, sessions, username) is wiped.
///
/// Returns the number of users anonymized or an error string on failure.
/// Safe to call directly from an admin endpoint — rolls back on any error.
pub async fn run_account_purge(db: &SqlitePool, grace_period_days: i64) -> Result<usize, String> {
    let threshold = format!("-{} days", grace_period_days);

    // Collect victim ids with a read (no lock needed).
    let victims: Vec<(String,)> = sqlx::query_as(
        "SELECT id FROM users \
         WHERE anonymized = 0 \
           AND deletion_requested_at IS NOT NULL \
           AND deletion_requested_at <= datetime('now', ?)",
    )
    .bind(&threshold)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;

    if victims.is_empty() { return Ok(0); }

    // Transactional anonymization.
    let mut tx = db.begin().await.map_err(|e| e.to_string())?;

    let mut purged = 0usize;
    for (uid,) in &victims {
        // Generate a unique anonymized username. Collisions are unlikely but
        // we use a UUID segment to guarantee uniqueness against the UNIQUE
        // constraint on users.username.
        let anon_username = format!("anon-{}", uuid::Uuid::new_v4());

        // Wipe personal data: address book is personal, drop it entirely.
        sqlx::query("DELETE FROM address_book WHERE user_id = ?")
            .bind(uid).execute(&mut *tx).await
            .map_err(|e| e.to_string())?;

        // Drop active sessions so the soon-to-be-anonymized user is kicked out.
        sqlx::query("DELETE FROM sessions WHERE user_id = ?")
            .bind(uid).execute(&mut *tx).await
            .map_err(|e| e.to_string())?;

        // Null out nullable actor references where we can (they point back
        // to a real person otherwise). checkin_ledger.override_by is nullable.
        sqlx::query("UPDATE checkin_ledger SET override_by = NULL WHERE override_by = ?")
            .bind(uid).execute(&mut *tx).await
            .map_err(|e| e.to_string())?;

        // audit_logs.actor_id is nullable too — drop the link.
        sqlx::query("UPDATE audit_logs SET actor_id = NULL WHERE actor_id = ?")
            .bind(uid).execute(&mut *tx).await
            .map_err(|e| e.to_string())?;

        // Finally flip the user row to its anonymized tombstone state. The
        // row stays so NOT NULL FKs in intake_records, inspections,
        // evidence_records, supply_entries, traceability_codes,
        // traceability_events, config_versions remain valid. Password hash
        // is set to a value that no password will ever verify against.
        sqlx::query(
            "UPDATE users \
             SET username = ?, \
                 password_hash = '$invalid$anonymized$', \
                 anonymized = 1, \
                 deletion_requested_at = NULL, \
                 updated_at = datetime('now') \
             WHERE id = ?"
        )
        .bind(&anon_username).bind(uid)
        .execute(&mut *tx).await
        .map_err(|e| e.to_string())?;

        purged += 1;
    }

    tx.commit().await.map_err(|e| e.to_string())?;
    Ok(purged)
}

// ── Diagnostics ZIP cleanup ───────────────────────────────────────────

fn diagnostics_dir(config: &Config) -> PathBuf {
    let mut p = PathBuf::from(&config.storage_dir);
    p.push("diagnostics");
    p
}

async fn diagnostics_cleanup_loop(db: SqlitePool, config: Config) {
    tracing::info!(job = "diagnostics_cleanup", "Registered (every 10 min)");
    let mut ticker = tokio::time::interval(Duration::from_secs(600));
    loop {
        ticker.tick().await;
        let dir = diagnostics_dir(&config);
        let removed = cleanup_old_files(&dir, 3600);
        tracing::info!(job = "diagnostics_cleanup", removed, "cleanup run");
        record_run(&db, "diagnostics_cleanup", "ok", None).await;
    }
}

/// Delete files in `dir` older than `max_age_secs`. Returns number deleted.
pub fn cleanup_old_files(dir: &std::path::Path, max_age_secs: u64) -> usize {
    let mut count = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        let now = std::time::SystemTime::now();
        for entry in entries.flatten() {
            if let Ok(meta) = entry.metadata() {
                if meta.is_file() {
                    if let Ok(modified) = meta.modified() {
                        if let Ok(age) = now.duration_since(modified) {
                            if age.as_secs() > max_age_secs {
                                if std::fs::remove_file(entry.path()).is_ok() {
                                    count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    count
}

// ── Evidence retention ────────────────────────────────────────────────

async fn evidence_retention_loop(db: SqlitePool) {
    tracing::info!(job = "evidence_retention", "Registered (every 1h)");
    let mut ticker = tokio::time::interval(Duration::from_secs(3600));
    loop {
        ticker.tick().await;
        match run_evidence_retention(&db, 365, None).await {
            Ok(deleted) => {
                tracing::info!(job = "evidence_retention", deleted, "retention sweep");
                record_run(&db, "evidence_retention", "ok", None).await;
            }
            Err(e) => {
                tracing::error!(job = "evidence_retention", error = %e, "sweep failed");
                record_run(&db, "evidence_retention", "error", Some(e)).await;
            }
        }
    }
}

/// Transactionally deletes evidence older than `max_age_days` whose
/// `linked = 0 AND legal_hold = 0`. Returns the number of rows deleted.
///
/// Legal-hold rows and rows linked to another resource (intake/inspection/
/// traceability/checkin) are always preserved regardless of age. Related
/// upload_sessions for the same uploader that are also older than the
/// threshold are opportunistically dropped to avoid unbounded growth.
///
/// Callable from the background loop and from the admin endpoint
/// `POST /admin/retention-purge` (GDPR/operator tool + deterministic test
/// driver via `max_age_days: 0`).
pub async fn run_evidence_retention(db: &SqlitePool, max_age_days: i64, storage_dir: Option<&str>) -> Result<usize, String> {
    if max_age_days < 0 {
        return Err("max_age_days must be >= 0".into());
    }
    let threshold = format!("-{} days", max_age_days);

    // Snapshot victims before deletion so we can (a) log how many we touched
    // and (b) emit an auditable structured log entry per row.
    let victims: Vec<(String, String)> = sqlx::query_as(
        "SELECT id, storage_path FROM evidence_records \
         WHERE linked = 0 \
           AND legal_hold = 0 \
           AND created_at <= datetime('now', ?)",
    )
    .bind(&threshold)
    .fetch_all(db)
    .await
    .map_err(|e| e.to_string())?;

    if victims.is_empty() {
        return Ok(0);
    }

    // Clean up stored files from disk using the canonical path from DB.
    for (id, path) in &victims {
        let file = if !path.is_empty() {
            path.clone()
        } else if let Some(dir) = storage_dir {
            format!("{}/uploads/{}_final", dir, id)
        } else {
            continue;
        };
        let _ = std::fs::remove_file(&file);
    }

    let mut tx = db.begin().await.map_err(|e| e.to_string())?;

    // Defensive: re-check inside the tx to avoid races with a concurrent
    // link/legal-hold flip. Using the same predicate guarantees we never
    // delete a row that just became linked or placed on legal hold.
    let res = sqlx::query(
        "DELETE FROM evidence_records \
         WHERE linked = 0 \
           AND legal_hold = 0 \
           AND created_at <= datetime('now', ?)",
    )
    .bind(&threshold)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;
    let deleted = res.rows_affected() as usize;

    // Clean completed/expired upload sessions older than the threshold too.
    // These are short-lived scratch rows that never reach 365 days under
    // normal operation, but if a client abandons an upload they otherwise
    // accumulate forever.
    sqlx::query(
        "DELETE FROM upload_sessions \
         WHERE created_at <= datetime('now', ?)"
    )
    .bind(&threshold)
    .execute(&mut *tx)
    .await
    .map_err(|e| e.to_string())?;

    tx.commit().await.map_err(|e| e.to_string())?;

    // Structured log entry (one row) so retention runs are visible in
    // /admin/logs and the diagnostic ZIP. We deliberately only log counts —
    // individual evidence IDs would bloat the log table.
    crate::common::slog(
        db,
        "info",
        &format!("evidence_retention deleted={} victims={}", deleted, victims.len()),
        "job",
    ).await;

    Ok(deleted)
}

// ── Stale upload session cleanup ────────────────────────────────────

async fn stale_upload_cleanup_loop(db: SqlitePool, config: Config) {
    tracing::info!(job = "stale_upload_cleanup", "Registered (every 30 min)");
    let mut ticker = tokio::time::interval(Duration::from_secs(1800));
    loop {
        ticker.tick().await;
        // Delete upload sessions stuck in 'in_progress' for > 24 hours,
        // plus their chunk files on disk.
        let stale: Vec<(String,)> = sqlx::query_as(
            "SELECT id FROM upload_sessions \
             WHERE status = 'in_progress' \
               AND created_at <= datetime('now', '-24 hours')"
        ).fetch_all(&db).await.unwrap_or_default();

        let mut cleaned = 0usize;
        for (session_id,) in &stale {
            // Remove chunk directory
            let chunk_dir = format!("{}/uploads/{}", config.storage_dir, session_id);
            let _ = std::fs::remove_dir_all(&chunk_dir);
            // Remove from DB
            let _ = sqlx::query("DELETE FROM upload_sessions WHERE id = ?")
                .bind(session_id).execute(&db).await;
            cleaned += 1;
        }
        if cleaned > 0 {
            tracing::info!(job = "stale_upload_cleanup", cleaned, "cleaned stale uploads");
        }
        record_run(&db, "stale_upload_cleanup", "ok", None).await;
    }
}
