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

pub async fn list(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
) -> Result<Json<Vec<InspectionResponse>>, AppError> {
    let t = &tid.0;
    let rows = sqlx::query_as::<_, InspRow>(
        "SELECT id, intake_id, inspector_id, status, outcome_notes, created_at, resolved_at FROM inspections ORDER BY created_at DESC",
    ).fetch_all(&state.db).await
    .map_err(db_err(t))?;
    Ok(Json(rows.into_iter().map(to_resp).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Json(body): Json<InspectionRequest>,
) -> Result<(StatusCode, Json<InspectionResponse>), AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;

    // Verify intake exists
    let exists: Option<(String,)> = sqlx::query_as("SELECT id FROM intake_records WHERE id = ?")
        .bind(&body.intake_id).fetch_optional(&state.db).await
        .map_err(db_err(t))?;
    if exists.is_none() {
        return Err(AppError::not_found("Intake record not found", t));
    }

    let id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO inspections (id, intake_id, inspector_id) VALUES (?,?,?)")
        .bind(&id).bind(&body.intake_id).bind(&user.user_id)
        .execute(&state.db).await.map_err(db_err(t))?;

    crate::modules::audit::write(
        &state.db, &user.user_id, "inspection.create", "inspection", &id, t,
    ).await;

    Ok((StatusCode::CREATED, Json(InspectionResponse {
        id, intake_id: body.intake_id, inspector_id: user.user_id,
        status: "pending".into(), outcome_notes: String::new(),
        created_at: String::new(), resolved_at: None,
    })))
}

pub async fn resolve(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Path(id): Path<String>,
    Json(body): Json<ResolveInspectionRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;

    if !["passed", "failed"].contains(&body.status.as_str()) {
        return Err(AppError::validation("Status must be passed or failed", t));
    }

    let current: Option<(String,)> = sqlx::query_as("SELECT status FROM inspections WHERE id = ?")
        .bind(&id).fetch_optional(&state.db).await
        .map_err(db_err(t))?;

    match current {
        None => return Err(AppError::not_found("Inspection not found", t)),
        Some((s,)) if s != "pending" => return Err(AppError::conflict(
            format!("Inspection already resolved as '{}'", s), t,
        )),
        _ => {}
    }

    sqlx::query("UPDATE inspections SET status = ?, outcome_notes = ?, resolved_at = datetime('now') WHERE id = ?")
        .bind(&body.status).bind(&body.outcome_notes).bind(&id)
        .execute(&state.db).await.map_err(db_err(t))?;

    crate::modules::audit::write(
        &state.db, &user.user_id, "inspection.resolve", "inspection", &id, t,
    ).await;
    slog(&state.db, "info",
        &format!("inspection.resolve id={} status={}", id, body.status), t).await;

    // If this inspection is linked (via an evidence_link → intake/
    // traceability chain) to a traceability code, append a step to its
    // timeline. We do a best-effort lookup: any traceability_codes row
    // whose intake_id matches this inspection's intake_id gets a step.
    let intake_id: Option<(String,)> =
        sqlx::query_as("SELECT intake_id FROM inspections WHERE id = ?")
            .bind(&id).fetch_optional(&state.db).await.ok().flatten();
    if let Some((iid,)) = intake_id {
        let codes: Vec<(String,)> =
            sqlx::query_as("SELECT id FROM traceability_codes WHERE intake_id = ?")
                .bind(&iid).fetch_all(&state.db).await.unwrap_or_default();
        for (code_id,) in codes {
            crate::modules::traceability::handlers::append_step(
                &state.db,
                &code_id,
                "inspection",
                &format!("Inspection {}", body.status),
                &format!("inspection_id={} notes={}", id, body.outcome_notes),
            ).await;
        }
    }

    Ok(Json(serde_json::json!({"message": "Inspection resolved"})))
}

fn to_resp(r: InspRow) -> InspectionResponse {
    InspectionResponse {
        id: r.id, intake_id: r.intake_id, inspector_id: r.inspector_id,
        status: r.status, outcome_notes: r.outcome_notes,
        created_at: r.created_at, resolved_at: r.resolved_at,
    }
}

#[derive(sqlx::FromRow)]
struct InspRow {
    id: String, intake_id: String, inspector_id: String,
    status: String, outcome_notes: String, created_at: String,
    resolved_at: Option<String>,
}
