//! Transfers — first-class operational queue replacing the legacy
//! "intake-status filter" workspace workaround.
//!
//! Lifecycle:
//!
//! ```text
//!   queued ──► approved ──► in_transit ──► received
//!     │           │              │
//!     └───────────┴──────────────┴────► canceled
//! ```
//!
//! Any transition not in `VALID_TRANSITIONS` returns 409 CONFLICT with a
//! human-readable message. Handlers enforce `require_write_role`
//! (admin/staff only — auditors get 403).

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

/// (from, to) pairs that are allowed. Everything else is a 409.
const VALID_TRANSITIONS: &[(&str, &str)] = &[
    ("queued", "approved"),
    ("queued", "canceled"),
    ("approved", "in_transit"),
    ("approved", "canceled"),
    ("in_transit", "received"),
    ("in_transit", "canceled"),
];

pub async fn list(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
) -> Result<Json<Vec<TransferResponse>>, AppError> {
    let t = &tid.0;
    let rows = sqlx::query_as::<_, TransferRow>(
        "SELECT id, intake_id, origin_facility_id, destination, reason, status, notes, created_by, created_at \
         FROM transfers ORDER BY created_at DESC",
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_err(t))?;

    Ok(Json(rows.into_iter().map(to_resp).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Json(body): Json<TransferRequest>,
) -> Result<(StatusCode, Json<TransferResponse>), AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;

    if body.destination.trim().is_empty() {
        return Err(AppError::validation("destination is required", t));
    }

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO transfers (id, intake_id, destination, reason, notes, created_by) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&body.intake_id)
    .bind(&body.destination)
    .bind(&body.reason)
    .bind(&body.notes)
    .bind(&user.user_id)
    .execute(&state.db)
    .await
    .map_err(db_err(t))?;

    crate::modules::audit::write(
        &state.db, &user.user_id, "transfer.create", "transfer", &id, t,
    ).await;
    slog(&state.db, "info",
        &format!("transfer.create id={} destination={}", id, body.destination), t).await;

    Ok((StatusCode::CREATED, Json(TransferResponse {
        id,
        intake_id: body.intake_id,
        origin_facility_id: "default".into(),
        destination: body.destination,
        reason: body.reason,
        status: "queued".into(),
        notes: body.notes,
        created_by: user.user_id,
        created_at: String::new(),
    })))
}

pub async fn get_one(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Path(id): Path<String>,
) -> Result<Json<TransferResponse>, AppError> {
    let t = &tid.0;
    let row = sqlx::query_as::<_, TransferRow>(
        "SELECT id, intake_id, origin_facility_id, destination, reason, status, notes, created_by, created_at \
         FROM transfers WHERE id = ?",
    )
    .bind(&id)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err(t))?
    .ok_or_else(|| AppError::not_found("Transfer not found", t))?;

    Ok(Json(to_resp(row)))
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

    let current: (String,) = sqlx::query_as("SELECT status FROM transfers WHERE id = ?")
        .bind(&id)
        .fetch_optional(&state.db)
        .await
        .map_err(db_err(t))?
        .ok_or_else(|| AppError::not_found("Transfer not found", t))?;

    let valid = VALID_TRANSITIONS.iter().any(|(f, to)| *f == current.0 && *to == body.status);
    if !valid {
        return Err(AppError::conflict(
            format!("Invalid transfer transition from '{}' to '{}'", current.0, body.status),
            t,
        ));
    }

    sqlx::query("UPDATE transfers SET status = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&body.status)
        .bind(&id)
        .execute(&state.db)
        .await
        .map_err(db_err(t))?;

    crate::modules::audit::write(
        &state.db, &user.user_id, "transfer.status_update", "transfer", &id, t,
    ).await;
    slog(&state.db, "info",
        &format!("transfer.status_update id={} status={}", id, body.status), t).await;

    Ok(Json(serde_json::json!({
        "message": "Transfer status updated",
        "id": id,
        "status": body.status,
    })))
}

fn to_resp(r: TransferRow) -> TransferResponse {
    TransferResponse {
        id: r.id,
        intake_id: r.intake_id,
        origin_facility_id: r.origin_facility_id,
        destination: r.destination,
        reason: r.reason,
        status: r.status,
        notes: r.notes,
        created_by: r.created_by,
        created_at: r.created_at,
    }
}

#[derive(sqlx::FromRow)]
struct TransferRow {
    id: String,
    intake_id: Option<String>,
    origin_facility_id: String,
    destination: String,
    reason: String,
    status: String,
    notes: String,
    created_by: String,
    created_at: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn valid_transitions_cover_full_lifecycle() {
        let happy: Vec<(&str, &str)> = vec![
            ("queued", "approved"),
            ("approved", "in_transit"),
            ("in_transit", "received"),
        ];
        for t in &happy {
            assert!(
                VALID_TRANSITIONS.iter().any(|x| x == t),
                "missing happy path transition {:?}",
                t
            );
        }
    }

    #[test]
    fn cancel_from_any_non_terminal_state_is_allowed() {
        for from in ["queued", "approved", "in_transit"] {
            assert!(
                VALID_TRANSITIONS.iter().any(|(f, to)| *f == from && *to == "canceled"),
                "{}→canceled missing",
                from
            );
        }
    }

    #[test]
    fn backwards_transitions_not_allowed() {
        let bad: Vec<(&str, &str)> = vec![
            ("received", "queued"),
            ("received", "approved"),
            ("received", "in_transit"),
            ("canceled", "approved"),
            ("in_transit", "queued"),
            ("approved", "queued"),
        ];
        for t in bad {
            assert!(
                !VALID_TRANSITIONS.iter().any(|x| *x == t),
                "disallowed transition {:?} should not be in VALID_TRANSITIONS",
                t
            );
        }
    }
}
