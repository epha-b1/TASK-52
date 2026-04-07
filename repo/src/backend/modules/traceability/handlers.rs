use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use uuid::Uuid;

use crate::app::AppState;
use crate::common::{db_err, require_admin_or_auditor, require_write_role, slog, CivilDateTime};
use crate::error::AppError;
use crate::extractors::SessionUser;
use crate::middleware::trace_id::TraceId;
use crate::modules::traceability::code;
use fieldtrace_shared::*;

/// Append an immutable timeline step for a traceability code. Called
/// on create / publish / retract / inspection-outcome / manual-note so
/// the history is auditable without reading `traceability_events`
/// (which is actor-scoped and comment-bearing).
pub(crate) async fn append_step(
    db: &sqlx::SqlitePool,
    code_id: &str,
    step_type: &str,
    step_label: &str,
    details: &str,
) {
    let id = Uuid::new_v4().to_string();
    let _ = sqlx::query(
        "INSERT INTO traceability_steps (id, code_id, step_type, step_label, details) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(code_id)
    .bind(step_type)
    .bind(step_label)
    .bind(details)
    .execute(db)
    .await;
}

pub async fn list(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
) -> Result<Json<Vec<TraceCodeResponse>>, AppError> {
    let t = &tid.0;
    // Auditors only see published; Admin/Staff see all
    let query = if user.role == "auditor" {
        "SELECT id, code, intake_id, status, version, created_at FROM traceability_codes WHERE status = 'published' ORDER BY created_at DESC"
    } else {
        "SELECT id, code, intake_id, status, version, created_at FROM traceability_codes ORDER BY created_at DESC"
    };
    let rows = sqlx::query_as::<_, TraceRow>(query)
        .fetch_all(&state.db).await
        .map_err(db_err(t))?;
    Ok(Json(rows.into_iter().map(|r| TraceCodeResponse {
        id: r.id, code: r.code, intake_id: r.intake_id,
        status: r.status, version: r.version, created_at: r.created_at,
    }).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Json(body): Json<TraceCodeRequest>,
) -> Result<(StatusCode, Json<TraceCodeResponse>), AppError> {
    let t = &tid.0;
    // Role policy per README matrix:
    //   administrator / operations_staff → may create traceability codes
    //   auditor → read-only, BUT may publish/retract (handled in those endpoints)
    // So create must use require_write_role (blocks auditor with 403), not the
    // admin-or-auditor guard used by publish/retract.
    require_write_role(&user, t)?;

    // Get next sequence
    let seq: (i64,) = sqlx::query_as("SELECT COUNT(*) + 1 FROM traceability_codes")
        .fetch_one(&state.db).await
        .map_err(db_err(t))?;

    let date = CivilDateTime::now().yyyymmdd();
    let generated = code::generate(&state.config.facility_code, &date, seq.0 as u32);
    let id = Uuid::new_v4().to_string();

    sqlx::query("INSERT INTO traceability_codes (id, code, intake_id, created_by) VALUES (?,?,?,?)")
        .bind(&id).bind(&generated).bind(&body.intake_id).bind(&user.user_id)
        .execute(&state.db).await
        .map_err(db_err(t))?;

    crate::modules::audit::write(
        &state.db, &user.user_id, "traceability.create", "traceability", &id, t,
    ).await;

    // Append immutable step #1 to the timeline.
    append_step(
        &state.db,
        &id,
        "create",
        "Code generated",
        &format!("code={}", generated),
    ).await;

    Ok((StatusCode::CREATED, Json(TraceCodeResponse {
        id, code: generated, intake_id: body.intake_id,
        status: "draft".into(), version: 1, created_at: String::new(),
    })))
}

pub async fn publish(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Path(id): Path<String>,
    Json(body): Json<TracePublishRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    require_admin_or_auditor(&user, t)?;

    if body.comment.trim().is_empty() {
        return Err(AppError::validation("Comment is required", t));
    }

    let row: Option<(String, i64)> = sqlx::query_as("SELECT status, version FROM traceability_codes WHERE id = ?")
        .bind(&id).fetch_optional(&state.db).await
        .map_err(db_err(t))?;
    let (status, version) = row.ok_or_else(|| AppError::not_found("Code not found", t))?;
    if status == "published" {
        return Err(AppError::conflict("Already published", t));
    }

    let new_version = version + 1;
    sqlx::query("UPDATE traceability_codes SET status = 'published', version = ? WHERE id = ?")
        .bind(new_version).bind(&id)
        .execute(&state.db).await
        .map_err(db_err(t))?;

    let event_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO traceability_events (id, code_id, event_type, comment, actor_id, version) VALUES (?,?,?,?,?,?)")
        .bind(&event_id).bind(&id).bind("publish").bind(&body.comment).bind(&user.user_id).bind(new_version)
        .execute(&state.db).await.ok();

    crate::modules::audit::write(
        &state.db, &user.user_id, "traceability.publish", "traceability", &id, t,
    ).await;
    slog(&state.db, "info",
        &format!("traceability.publish id={} version={}", id, new_version), t).await;
    append_step(
        &state.db,
        &id,
        "publish",
        "Published",
        &format!("version={} comment={}", new_version, body.comment),
    ).await;

    Ok(Json(serde_json::json!({"message":"Published","version":new_version})))
}

pub async fn retract(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Path(id): Path<String>,
    Json(body): Json<TracePublishRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    require_admin_or_auditor(&user, t)?;

    if body.comment.trim().is_empty() {
        return Err(AppError::validation("Comment is required", t));
    }

    let row: Option<(String, i64)> = sqlx::query_as("SELECT status, version FROM traceability_codes WHERE id = ?")
        .bind(&id).fetch_optional(&state.db).await
        .map_err(db_err(t))?;
    let (status, version) = row.ok_or_else(|| AppError::not_found("Code not found", t))?;
    if status != "published" {
        return Err(AppError::conflict("Can only retract published codes", t));
    }

    let new_version = version + 1;
    sqlx::query("UPDATE traceability_codes SET status = 'retracted', version = ? WHERE id = ?")
        .bind(new_version).bind(&id)
        .execute(&state.db).await
        .map_err(db_err(t))?;

    let event_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO traceability_events (id, code_id, event_type, comment, actor_id, version) VALUES (?,?,?,?,?,?)")
        .bind(&event_id).bind(&id).bind("retract").bind(&body.comment).bind(&user.user_id).bind(new_version)
        .execute(&state.db).await.ok();

    crate::modules::audit::write(
        &state.db, &user.user_id, "traceability.retract", "traceability", &id, t,
    ).await;
    slog(&state.db, "warn",
        &format!("traceability.retract id={} version={}", id, new_version), t).await;
    append_step(
        &state.db,
        &id,
        "retract",
        "Retracted",
        &format!("version={} comment={}", new_version, body.comment),
    ).await;

    Ok(Json(serde_json::json!({"message":"Retracted","version":new_version})))
}

// Public offline verify
pub async fn verify_code(Path(code_str): Path<String>) -> Json<serde_json::Value> {
    let valid = code::verify(&code_str);
    Json(serde_json::json!({"code": code_str, "valid": valid}))
}

// GET /traceability/:id/steps — ordered append-only timeline for a code.
pub async fn list_steps(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Path(code_id): Path<String>,
) -> Result<Json<Vec<TraceStepResponse>>, AppError> {
    let t = &tid.0;
    // Verify the code exists first so we return 404 cleanly instead of
    // silently returning an empty timeline.
    let exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM traceability_codes WHERE id = ?")
            .bind(&code_id)
            .fetch_optional(&state.db)
            .await
            .map_err(db_err(t))?;
    if exists.is_none() {
        return Err(AppError::not_found("Traceability code not found", t));
    }

    let rows: Vec<(String, String, String, String, String, String)> = sqlx::query_as(
        "SELECT id, code_id, step_type, step_label, details, occurred_at \
         FROM traceability_steps \
         WHERE code_id = ? \
         ORDER BY occurred_at ASC, id ASC",
    )
    .bind(&code_id)
    .fetch_all(&state.db)
    .await
    .map_err(db_err(t))?;

    Ok(Json(
        rows.into_iter()
            .map(|(id, cid, st, lbl, det, at)| TraceStepResponse {
                id,
                code_id: cid,
                step_type: st,
                step_label: lbl,
                details: det,
                occurred_at: at,
            })
            .collect(),
    ))
}

// POST /traceability/:id/steps — append a manual note to the timeline.
// Admin/staff only (writes). Auditors cannot mutate the history.
pub async fn append_manual_step(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Path(code_id): Path<String>,
    Json(body): Json<serde_json::Value>,
) -> Result<(StatusCode, Json<serde_json::Value>), AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;

    let label = body.get("label").and_then(|v| v.as_str()).unwrap_or("").trim().to_string();
    let details = body.get("details").and_then(|v| v.as_str()).unwrap_or("").to_string();
    if label.is_empty() {
        return Err(AppError::validation("label is required", t));
    }

    let exists: Option<(String,)> =
        sqlx::query_as("SELECT id FROM traceability_codes WHERE id = ?")
            .bind(&code_id)
            .fetch_optional(&state.db)
            .await
            .map_err(db_err(t))?;
    if exists.is_none() {
        return Err(AppError::not_found("Traceability code not found", t));
    }

    append_step(&state.db, &code_id, "note", &label, &details).await;

    crate::modules::audit::write(
        &state.db, &user.user_id, "traceability.note", "traceability", &code_id, t,
    ).await;

    Ok((StatusCode::CREATED, Json(serde_json::json!({
        "message": "Step appended",
        "code_id": code_id,
    }))))
}

#[derive(sqlx::FromRow)]
struct TraceRow {
    id: String, code: String, intake_id: Option<String>,
    status: String, version: i64, created_at: String,
}
