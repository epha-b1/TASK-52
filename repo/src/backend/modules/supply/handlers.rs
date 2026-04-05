use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use std::collections::HashMap;
use uuid::Uuid;

use crate::app::AppState;
use crate::common::{db_err, require_write_role, slog};
use crate::error::AppError;
use crate::extractors::SessionUser;
use crate::middleware::trace_id::TraceId;
use crate::modules::supply::parser;
use fieldtrace_shared::*;

pub async fn list(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
) -> Result<Json<Vec<SupplyResponse>>, AppError> {
    let rows = sqlx::query_as::<_, SupRow>(
        "SELECT id, name, sku, canonical_size, canonical_color, price_cents, parse_status, parse_conflicts, created_at \
         FROM supply_entries ORDER BY created_at DESC"
    ).fetch_all(&state.db).await
    .map_err(db_err(&tid.0))?;
    Ok(Json(rows.into_iter().map(|r| SupplyResponse {
        id: r.id, name: r.name, sku: r.sku,
        canonical_size: r.canonical_size, canonical_color: r.canonical_color,
        price_cents: r.price_cents, parse_status: r.parse_status,
        parse_conflicts: r.parse_conflicts, created_at: r.created_at,
    }).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Json(body): Json<SupplyRequest>,
) -> Result<(StatusCode, Json<SupplyResponse>), AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;

    let (canonical_color, color_conflict) = match parser::normalize_color(&body.color) {
        Some(c) => (Some(c), None),
        None => (None, Some("color")),
    };
    let (canonical_size, size_conflict) = match parser::normalize_size(&body.size) {
        Some(s) => (Some(s), None),
        None => (None, Some("size")),
    };

    let mut conflicts: HashMap<String, String> = HashMap::new();
    if let Some(f) = color_conflict { conflicts.insert(f.into(), format!("Unknown color: {}", body.color)); }
    if let Some(f) = size_conflict { conflicts.insert(f.into(), format!("Cannot parse size: {}", body.size)); }

    let parse_status = if conflicts.is_empty() { "ok" } else { "needs_review" };
    let conflicts_json = serde_json::to_string(&conflicts).unwrap_or("{}".into());

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO supply_entries (id, name, sku, raw_size, canonical_size, raw_color, canonical_color, price_cents, discount_cents, notes, parse_status, parse_conflicts, created_by) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?)"
    )
    .bind(&id).bind(&body.name).bind(&body.sku).bind(&body.size).bind(&canonical_size)
    .bind(&body.color).bind(&canonical_color).bind(body.price_cents).bind(body.discount_cents)
    .bind(&body.notes).bind(parse_status).bind(&conflicts_json).bind(&user.user_id)
    .execute(&state.db).await
    .map_err(db_err(t))?;

    slog(&state.db, "info",
        &format!("supply.create id={} parse_status={}", id, parse_status), t).await;

    Ok((StatusCode::CREATED, Json(SupplyResponse {
        id, name: body.name, sku: body.sku,
        canonical_size, canonical_color, price_cents: body.price_cents,
        parse_status: parse_status.into(), parse_conflicts: conflicts_json, created_at: String::new(),
    })))
}

pub async fn resolve(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Path(id): Path<String>,
    Json(body): Json<SupplyResolveRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;
    sqlx::query(
        "UPDATE supply_entries SET canonical_color = COALESCE(?, canonical_color), canonical_size = COALESCE(?, canonical_size), parse_status = 'ok', parse_conflicts = '{}' WHERE id = ?"
    )
    .bind(&body.canonical_color).bind(&body.canonical_size).bind(&id)
    .execute(&state.db).await
    .map_err(db_err(t))?;
    Ok(Json(serde_json::json!({"message":"Resolved"})))
}

#[derive(sqlx::FromRow)]
struct SupRow {
    id: String, name: String, sku: Option<String>,
    canonical_size: Option<String>, canonical_color: Option<String>,
    price_cents: Option<i64>, parse_status: String, parse_conflicts: String, created_at: String,
}
