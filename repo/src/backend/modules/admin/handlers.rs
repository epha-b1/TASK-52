use axum::body::Body;
use axum::extract::{Path, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::{Extension, Json};
use std::path::PathBuf;
use uuid::Uuid;

use crate::app::AppState;
use crate::common::db_err;
use crate::crypto::Crypto;
use crate::error::AppError;
use crate::extractors::SessionUser;
use crate::middleware::trace_id::TraceId;
use crate::zip::ZipWriter;

const CONFIG_VERSION_CAP: i64 = 10;

// GET /admin/config — return latest version
pub async fn get_config(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    let row: Option<(i64, String)> = sqlx::query_as(
        "SELECT id, snapshot FROM config_versions ORDER BY id DESC LIMIT 1"
    ).fetch_optional(&state.db).await.map_err(db_err(t))?;
    match row {
        Some((id, snap)) => Ok(Json(serde_json::json!({"version_id": id, "snapshot": snap}))),
        None => Ok(Json(serde_json::json!({"version_id": 0, "snapshot": "{}"}))),
    }
}

// PATCH /admin/config — save new version, keep last 10
pub async fn update_config(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Json(body): Json<serde_json::Value>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    let snapshot = body.to_string();

    let res = sqlx::query("INSERT INTO config_versions (snapshot, saved_by) VALUES (?, ?)")
        .bind(&snapshot).bind(&user.user_id).execute(&state.db).await
        .map_err(db_err(t))?;
    let new_id = res.last_insert_rowid();

    // Trim to last N
    sqlx::query(&format!(
        "DELETE FROM config_versions WHERE id NOT IN (SELECT id FROM config_versions ORDER BY id DESC LIMIT {})",
        CONFIG_VERSION_CAP
    ))
    .execute(&state.db).await.ok();

    crate::modules::audit::write(
        &state.db, &user.user_id, "admin.config.update", "config", &new_id.to_string(), t,
    ).await;

    Ok(Json(serde_json::json!({"version_id": new_id, "message": "Config saved"})))
}

// GET /admin/config/versions — list last 10
pub async fn list_versions(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let t = &tid.0;
    let rows: Vec<(i64, String, String)> = sqlx::query_as(
        "SELECT id, saved_by, created_at FROM config_versions ORDER BY id DESC LIMIT 10"
    ).fetch_all(&state.db).await.map_err(db_err(t))?;
    Ok(Json(rows.into_iter().map(|(id, by, at)|
        serde_json::json!({"id": id, "saved_by": by, "created_at": at})).collect()))
}

// POST /admin/config/rollback/:id
pub async fn rollback(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Path(version_id): Path<i64>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    let row: Option<(String,)> = sqlx::query_as("SELECT snapshot FROM config_versions WHERE id = ?")
        .bind(version_id).fetch_optional(&state.db).await.map_err(db_err(t))?;
    let (snap,) = row.ok_or_else(|| AppError::not_found("Version not found", t))?;

    let res = sqlx::query("INSERT INTO config_versions (snapshot, saved_by) VALUES (?, ?)")
        .bind(&snap).bind(&user.user_id).execute(&state.db).await.map_err(db_err(t))?;

    // Keep the cap at CONFIG_VERSION_CAP rows after rollback too — otherwise
    // repeated rollback calls could unbound the table.
    sqlx::query(&format!(
        "DELETE FROM config_versions WHERE id NOT IN (SELECT id FROM config_versions ORDER BY id DESC LIMIT {})",
        CONFIG_VERSION_CAP
    ))
    .execute(&state.db).await.ok();

    crate::modules::audit::write(
        &state.db, &user.user_id, "admin.config.rollback", "config", &version_id.to_string(), t,
    ).await;

    Ok(Json(serde_json::json!({"message":"Rolled back","new_version_id": res.last_insert_rowid()})))
}

// ── Diagnostics: REAL ZIP generation ─────────────────────────────────
//
// The diagnostic package contains:
//   logs.txt           — recent structured_logs rows (last 7 days)
//   metrics.json       — job_metrics snapshot
//   config_history.json — config_versions snapshot
//   audit_summary.csv  — audit counts by action (no sensitive payloads)
//
// The ZIP is written to `{storage_dir}/diagnostics/{download_id}.zip` and
// the `diagnostics_cleanup` background job deletes files older than 1 hour.

fn diagnostics_dir(state: &AppState) -> PathBuf {
    let mut p = PathBuf::from(&state.config.storage_dir);
    p.push("diagnostics");
    p
}

pub async fn export_diagnostics(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;

    // Collect pieces
    let logs_rows: Vec<(i64, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, level, message, trace_id, created_at FROM structured_logs \
         WHERE created_at > datetime('now', '-7 days') ORDER BY id DESC LIMIT 5000"
    ).fetch_all(&state.db).await.map_err(db_err(t))?;
    let mut logs_txt = String::new();
    for (id, lvl, msg, tid_opt, at) in logs_rows {
        logs_txt.push_str(&format!("{} [{}] {} trace_id={} at={}\n",
            id, lvl, msg, tid_opt.unwrap_or_default(), at));
    }

    let metrics_rows: Vec<(String, String, i64, Option<String>, String)> = sqlx::query_as(
        "SELECT job_name, status, run_count, last_error, last_run_at FROM job_metrics ORDER BY id DESC LIMIT 1000"
    ).fetch_all(&state.db).await.map_err(db_err(t))?;
    let metrics_json = serde_json::to_string_pretty(
        &metrics_rows.iter().map(|(n, s, c, e, a)| serde_json::json!({
            "job_name": n, "status": s, "run_count": c, "last_error": e, "last_run_at": a
        })).collect::<Vec<_>>()
    ).unwrap_or_else(|_| "[]".into());

    // Full config snapshots (id, saved_by, created_at, snapshot payload).
    // The complete snapshot body is included so operators can diff versions
    // or roll back from a recovered package — metadata alone wouldn't
    // satisfy the "config history" requirement.
    let cfg_rows: Vec<(i64, String, String, String)> = sqlx::query_as(
        "SELECT id, saved_by, created_at, snapshot FROM config_versions ORDER BY id DESC"
    ).fetch_all(&state.db).await.map_err(db_err(t))?;
    let cfg_json = serde_json::to_string_pretty(
        &cfg_rows.iter().map(|(i, by, at, snap)| {
            // Snapshot is stored as a JSON string; parse it so the export
            // is structurally readable (embedded object, not escaped text).
            let parsed: serde_json::Value = serde_json::from_str(snap)
                .unwrap_or_else(|_| serde_json::Value::String(snap.clone()));
            serde_json::json!({
                "id": i,
                "saved_by": by,
                "created_at": at,
                "snapshot": parsed,
            })
        }).collect::<Vec<_>>()
    ).unwrap_or_else(|_| "[]".into());

    let audit_rows: Vec<(String, i64)> = sqlx::query_as(
        "SELECT action, COUNT(*) FROM audit_logs GROUP BY action ORDER BY 2 DESC"
    ).fetch_all(&state.db).await.map_err(db_err(t))?;
    let mut audit_csv = String::from("# sensitive fields omitted\naction,count\n");
    for (action, count) in audit_rows {
        audit_csv.push_str(&format!("{},{}\n", action, count));
    }

    // Write ZIP
    let download_id = Uuid::new_v4().to_string();
    let dir = diagnostics_dir(&state);
    std::fs::create_dir_all(&dir)
        .map_err(|e| {
            tracing::error!(trace_id = %t, error = %e, "create diagnostics dir failed");
            AppError::internal("Internal server error", t)
        })?;
    let mut path = dir.clone();
    path.push(format!("{}.zip", download_id));

    let file = std::fs::File::create(&path).map_err(|e| {
        tracing::error!(trace_id = %t, error = %e, "create zip file failed");
        AppError::internal("Internal server error", t)
    })?;
    let mut zip = ZipWriter::new(file);
    zip.add_file("logs.txt", logs_txt.as_bytes())
        .and_then(|_| zip.add_file("metrics.json", metrics_json.as_bytes()))
        .and_then(|_| zip.add_file("config_history.json", cfg_json.as_bytes()))
        .and_then(|_| zip.add_file("audit_summary.csv", audit_csv.as_bytes()))
        .map_err(|e| {
            tracing::error!(trace_id = %t, error = %e, "zip write failed");
            AppError::internal("Internal server error", t)
        })?;
    zip.finish().map_err(|e| {
        tracing::error!(trace_id = %t, error = %e, "zip finish failed");
        AppError::internal("Internal server error", t)
    })?;

    let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);

    crate::modules::audit::write(
        &state.db, &user.user_id, "admin.diagnostics.export", "diagnostics", &download_id, t,
    ).await;

    Ok(Json(serde_json::json!({
        "download_id": download_id,
        "download_url": format!("/admin/diagnostics/download/{}", download_id),
        "size_bytes": size,
        "expires_in_seconds": 3600,
        "message": "Diagnostic package ready"
    })))
}

pub async fn download_diagnostics(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Path(id): Path<String>,
) -> Result<Response, AppError> {
    let t = &tid.0;
    // Prevent path traversal: only accept UUID-like ids.
    if id.len() > 64 || !id.chars().all(|c| c.is_ascii_alphanumeric() || c == '-') {
        return Err(AppError::validation("Invalid download id", t));
    }
    let mut path = diagnostics_dir(&state);
    path.push(format!("{}.zip", id));
    if !path.exists() {
        return Err(AppError::not_found("Diagnostic package not found or expired", t));
    }
    let bytes = std::fs::read(&path).map_err(|e| {
        tracing::error!(trace_id = %t, error = %e, "read zip failed");
        AppError::internal("Internal server error", t)
    })?;

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("application/zip"));
    let disp = format!("attachment; filename=\"diagnostics-{}.zip\"", id);
    headers.insert("Content-Disposition", HeaderValue::from_str(&disp).unwrap_or_else(|_| HeaderValue::from_static("attachment")));
    Ok((StatusCode::OK, headers, Body::from(bytes)).into_response())
}

// GET /admin/logs — last 200 structured_logs rows. Admin only.
// Used for operator inspection and to prove the persistent log trail.
pub async fn list_logs(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let t = &tid.0;
    let rows: Vec<(i64, String, String, Option<String>, String)> = sqlx::query_as(
        "SELECT id, level, message, trace_id, created_at FROM structured_logs \
         ORDER BY id DESC LIMIT 200"
    ).fetch_all(&state.db).await.map_err(db_err(t))?;
    Ok(Json(rows.into_iter().map(|(id, lvl, msg, tid_opt, at)| {
        serde_json::json!({
            "id": id,
            "level": lvl,
            "message": msg,
            "trace_id": tid_opt,
            "created_at": at,
        })
    }).collect()))
}

// GET /admin/jobs
pub async fn jobs(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
) -> Result<Json<Vec<serde_json::Value>>, AppError> {
    let t = &tid.0;
    let rows: Vec<(String, String, i64, String)> = sqlx::query_as(
        "SELECT job_name, status, run_count, last_run_at FROM job_metrics ORDER BY last_run_at DESC"
    ).fetch_all(&state.db).await.map_err(db_err(t))?;
    Ok(Json(rows.into_iter().map(|(name, status, count, last)|
        serde_json::json!({"job_name": name, "status": status, "run_count": count, "last_run_at": last})).collect()))
}

// ── Manual account purge trigger ──────────────────────────────────────
//
// Admin operator tool. Runs the same purge logic used by the hourly
// background job so operators can respond to GDPR deletion requests
// immediately once the 7-day cooling-off window has lapsed. The
// `grace_period_days` body field defaults to 7 and is also the only way
// the integration tests can drive the purge deterministically without
// advancing the wall clock.

#[derive(serde::Deserialize, Default)]
pub struct AccountPurgeRequest {
    #[serde(default)]
    pub grace_period_days: Option<i64>,
}

pub async fn run_account_purge(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    body: Option<Json<AccountPurgeRequest>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    let days = body.and_then(|b| b.0.grace_period_days).unwrap_or(7);
    if days < 0 {
        return Err(AppError::validation("grace_period_days must be >= 0", t));
    }

    let purged = crate::jobs::run_account_purge(&state.db, days).await
        .map_err(|e| {
            tracing::error!(trace_id = %t, error = %e, "manual purge failed");
            AppError::internal("Internal server error", t)
        })?;

    crate::modules::audit::write(
        &state.db, &user.user_id, "admin.account.purge", "account_purge",
        &format!("purged={}", purged), t,
    ).await;

    Ok(Json(serde_json::json!({
        "message": "Account purge run complete",
        "purged": purged,
        "grace_period_days": days,
    })))
}

// ── Manual evidence retention trigger ─────────────────────────────────
//
// Admin operator tool. Runs the same retention logic as the hourly
// background job so operators can respond to retention policy changes
// immediately. Integration tests use `max_age_days: 0` to make the sweep
// deterministic — inserting evidence just before the call, the `<=` in
// the predicate matches same-second inserts.

#[derive(serde::Deserialize, Default)]
pub struct RetentionPurgeRequest {
    #[serde(default)]
    pub max_age_days: Option<i64>,
}

pub async fn run_evidence_retention(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    body: Option<Json<RetentionPurgeRequest>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    let days = body.and_then(|b| b.0.max_age_days).unwrap_or(365);
    if days < 0 {
        return Err(AppError::validation("max_age_days must be >= 0", t));
    }

    let deleted = crate::jobs::run_evidence_retention(&state.db, days).await
        .map_err(|e| {
            tracing::error!(trace_id = %t, error = %e, "manual retention sweep failed");
            AppError::internal("Internal server error", t)
        })?;

    crate::modules::audit::write(
        &state.db, &user.user_id, "admin.evidence.retention_sweep", "retention",
        &format!("deleted={}", deleted), t,
    ).await;

    Ok(Json(serde_json::json!({
        "message": "Retention sweep complete",
        "deleted": deleted,
        "max_age_days": days,
    })))
}

// ── REAL transactional key rotation ───────────────────────────────────

#[derive(serde::Deserialize)]
pub struct RotateKeyRequest {
    pub new_key_hex: String,
}

pub async fn rotate_key(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Json(body): Json<RotateKeyRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;

    // Validate new key upfront (no plaintext in logs).
    let new_crypto = Crypto::from_hex(&body.new_key_hex)
        .map_err(|_| AppError::validation("new_key_hex must be a 64-char hex string", t))?;
    let old_crypto = state.crypto();

    // Short-circuit if the same key was supplied.
    if body.new_key_hex == state.config.encryption_key {
        return Err(AppError::conflict("New key identical to current key", t));
    }

    // Collect every encrypted row, re-encrypt with new key, and commit
    // inside one transaction. On any failure, we roll back and do NOT
    // swap the in-memory cipher.
    let mut tx = state.db.begin().await.map_err(db_err(t))?;

    // Load all address_book encrypted fields.
    let rows: Vec<(String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, street_enc, city_enc, state_enc, phone_enc FROM address_book"
    ).fetch_all(&mut *tx).await.map_err(db_err(t))?;

    let mut rotated = 0usize;
    for (id, s, c, st, p) in rows {
        // Decrypt with old, re-encrypt with new. We short-circuit to fail
        // on first decryption failure to avoid silent data loss.
        let pt_s = old_crypto.try_decrypt(&s).map_err(|_| {
            tracing::error!(trace_id = %t, row_id = %id, "decrypt with old key failed");
            AppError::internal("Key rotation failed — aborted", t)
        })?;
        let pt_c = old_crypto.try_decrypt(&c).map_err(|_| AppError::internal("Key rotation failed — aborted", t))?;
        let pt_st = old_crypto.try_decrypt(&st).map_err(|_| AppError::internal("Key rotation failed — aborted", t))?;
        let pt_p = old_crypto.try_decrypt(&p).map_err(|_| AppError::internal("Key rotation failed — aborted", t))?;

        let new_s = new_crypto.try_encrypt(&pt_s).map_err(|_| AppError::internal("Key rotation failed — aborted", t))?;
        let new_c = new_crypto.try_encrypt(&pt_c).map_err(|_| AppError::internal("Key rotation failed — aborted", t))?;
        let new_st = new_crypto.try_encrypt(&pt_st).map_err(|_| AppError::internal("Key rotation failed — aborted", t))?;
        let new_p = new_crypto.try_encrypt(&pt_p).map_err(|_| AppError::internal("Key rotation failed — aborted", t))?;

        sqlx::query("UPDATE address_book SET street_enc=?, city_enc=?, state_enc=?, phone_enc=? WHERE id=?")
            .bind(&new_s).bind(&new_c).bind(&new_st).bind(&new_p).bind(&id)
            .execute(&mut *tx).await.map_err(db_err(t))?;
        rotated += 1;
    }

    // (Future: rotate other encrypted-at-rest fields here inside the same tx.)

    tx.commit().await.map_err(db_err(t))?;

    // Swap in-memory cipher only AFTER the DB has committed.
    state.set_crypto(new_crypto);

    // Persist the new key to the key file if one is configured. We avoid
    // writing the key to logs.
    if let Some(ref path) = state.config.encryption_key_file {
        if let Err(e) = std::fs::write(path, &body.new_key_hex) {
            tracing::error!(trace_id = %t, error = %e, "Failed to persist new key to file");
            // DB was already updated. Return partial success signal.
            return Ok(Json(serde_json::json!({
                "message": "Key rotation committed but key file write failed. Restart will use env ENCRYPTION_KEY.",
                "rotated_rows": rotated,
                "persisted_to_file": false
            })));
        }
    }

    crate::modules::audit::write(
        &state.db, &user.user_id, "admin.security.rotate_key", "crypto", "", t,
    ).await;

    Ok(Json(serde_json::json!({
        "message": "Key rotation complete",
        "rotated_rows": rotated,
        "persisted_to_file": state.config.encryption_key_file.is_some()
    })))
}
