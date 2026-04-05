use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use uuid::Uuid;

use crate::app::AppState;
use crate::common::{db_err, require_write_role, slog};
use crate::error::AppError;
use crate::extractors::SessionUser;
use crate::middleware::trace_id::TraceId;
use fieldtrace_shared::*;

const VALID_TRANSITIONS: &[(&str, &str)] = &[
    ("received", "in_care"), ("received", "in_stock"),
    ("in_care", "adopted"), ("in_care", "transferred"), ("in_care", "disposed"),
    ("in_stock", "transferred"), ("in_stock", "disposed"),
];

pub async fn list(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
) -> Result<Json<Vec<IntakeResponse>>, AppError> {
    let t = &tid.0;
    let rows = sqlx::query_as::<_, IntakeRow>(
        "SELECT id, facility_id, intake_type, status, details, created_by, created_at, region, tags FROM intake_records ORDER BY created_at DESC",
    )
    .fetch_all(&state.db).await
    .map_err(db_err(t))?;
    Ok(Json(rows.into_iter().map(row_to_resp).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Json(body): Json<IntakeRequest>,
) -> Result<(StatusCode, Json<IntakeResponse>), AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;
    if !["animal", "supply", "donation"].contains(&body.intake_type.as_str()) {
        return Err(AppError::validation("intake_type must be animal, supply, or donation", t));
    }
    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO intake_records (id, intake_type, details, created_by, region, tags) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&body.intake_type)
    .bind(&body.details)
    .bind(&user.user_id)
    .bind(&body.region)
    .bind(&body.tags)
    .execute(&state.db)
    .await
    .map_err(db_err(t))?;

    crate::modules::audit::write(
        &state.db, &user.user_id, "intake.create", "intake", &id, t,
    ).await;
    slog(&state.db, "info",
        &format!("intake.create type={} id={}", body.intake_type, id), t).await;

    Ok((StatusCode::CREATED, Json(IntakeResponse {
        id,
        facility_id: "default".into(),
        intake_type: body.intake_type,
        status: "received".into(),
        details: body.details,
        created_by: user.user_id,
        created_at: String::new(),
        region: body.region,
        tags: body.tags,
    })))
}

pub async fn get_one(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Path(id): Path<String>,
) -> Result<Json<IntakeResponse>, AppError> {
    let t = &tid.0;
    let r = sqlx::query_as::<_, IntakeRow>(
        "SELECT id, facility_id, intake_type, status, details, created_by, created_at, region, tags FROM intake_records WHERE id = ?",
    ).bind(&id).fetch_optional(&state.db).await
    .map_err(db_err(t))?
    .ok_or_else(|| AppError::not_found("Intake record not found", t))?;

    Ok(Json(row_to_resp(r)))
}

fn row_to_resp(r: IntakeRow) -> IntakeResponse {
    IntakeResponse {
        id: r.id,
        facility_id: r.facility_id,
        intake_type: r.intake_type,
        status: r.status,
        details: r.details,
        created_by: r.created_by,
        created_at: r.created_at,
        region: r.region,
        tags: r.tags,
    }
}

pub async fn update_status(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Path(id): Path<String>,
    Json(body): Json<StatusUpdateRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;

    let current: (String,) = sqlx::query_as("SELECT status FROM intake_records WHERE id = ?")
        .bind(&id).fetch_optional(&state.db).await
        .map_err(db_err(t))?
        .ok_or_else(|| AppError::not_found("Intake record not found", t))?;

    let valid = VALID_TRANSITIONS.iter().any(|(from, to)| *from == current.0 && *to == body.status);
    if !valid {
        return Err(AppError::conflict(
            format!("Invalid transition from '{}' to '{}'", current.0, body.status), t,
        ));
    }

    sqlx::query("UPDATE intake_records SET status = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&body.status).bind(&id).execute(&state.db).await
        .map_err(db_err(t))?;

    crate::modules::audit::write(
        &state.db, &user.user_id, "intake.status_update", "intake", &id, t,
    ).await;
    slog(&state.db, "info",
        &format!("intake.status_update id={} status={}", id, body.status), t).await;

    Ok(Json(serde_json::json!({"message": "Status updated", "status": body.status})))
}

#[derive(sqlx::FromRow)]
struct IntakeRow {
    id: String, facility_id: String, intake_type: String,
    status: String, details: String, created_by: String, created_at: String,
    region: String, tags: String,
}
