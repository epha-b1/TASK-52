//! Stock movements ledger — the canonical source of truth for
//! `inventory_on_hand`. Replaces the dashboard's previous
//! `COUNT(*) FROM supply_entries` shortcut.
//!
//! Movement semantics:
//!
//! - `receipt`: positive delta, inventory goes up (goods received)
//! - `allocation`: negative delta, inventory goes down (goods dispatched)
//! - `adjustment`: signed, reconciliation after a count
//! - `return`: positive delta (goods returned to stock)
//! - `loss`: negative delta (damage, theft, expiry)
//!
//! The ledger is append-only. Corrections go through a new movement,
//! never an UPDATE/DELETE. This gives a full audit trail that the
//! retention + diagnostics flows can rely on.

use axum::extract::{Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use std::collections::HashMap;
use uuid::Uuid;

use crate::app::AppState;
use crate::common::{db_err, require_write_role, slog};
use crate::error::AppError;
use crate::extractors::SessionUser;
use crate::middleware::trace_id::TraceId;
use fieldtrace_shared::*;

const VALID_REASONS: &[&str] = &["receipt", "allocation", "adjustment", "return", "loss"];

pub async fn list(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Vec<StockMovementResponse>>, AppError> {
    let t = &tid.0;

    let mut sql = String::from(
        "SELECT id, supply_id, quantity_delta, reason, notes, actor_id, created_at \
         FROM stock_movements WHERE 1=1",
    );
    let mut binds: Vec<String> = Vec::new();
    if let Some(s) = q.get("supply_id").filter(|v| !v.is_empty()) {
        sql.push_str(" AND supply_id = ?");
        binds.push(s.clone());
    }
    if let Some(r) = q.get("reason").filter(|v| !v.is_empty()) {
        sql.push_str(" AND reason = ?");
        binds.push(r.clone());
    }
    sql.push_str(" ORDER BY created_at DESC");

    let mut query = sqlx::query_as::<_, MovementRow>(&sql);
    for b in &binds { query = query.bind(b); }
    let rows = query.fetch_all(&state.db).await.map_err(db_err(t))?;

    Ok(Json(rows.into_iter().map(|r| StockMovementResponse {
        id: r.id,
        supply_id: r.supply_id,
        quantity_delta: r.quantity_delta,
        reason: r.reason,
        notes: r.notes,
        actor_id: r.actor_id,
        created_at: r.created_at,
    }).collect()))
}

pub async fn create(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Json(body): Json<StockMovementRequest>,
) -> Result<(StatusCode, Json<StockMovementResponse>), AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;

    if !VALID_REASONS.contains(&body.reason.as_str()) {
        return Err(AppError::validation(
            "reason must be one of receipt, allocation, adjustment, return, loss",
            t,
        ));
    }
    if body.quantity_delta == 0 {
        return Err(AppError::validation("quantity_delta must not be zero", t));
    }
    // Sign sanity: receipt + return must be positive; allocation + loss must be negative.
    // Adjustment may be either sign (reconciliation).
    let sign_valid = match body.reason.as_str() {
        "receipt" | "return" => body.quantity_delta > 0,
        "allocation" | "loss" => body.quantity_delta < 0,
        _ => true,
    };
    if !sign_valid {
        return Err(AppError::validation(
            format!("quantity_delta sign does not match reason '{}'", body.reason),
            t,
        ));
    }

    let id = Uuid::new_v4().to_string();
    sqlx::query(
        "INSERT INTO stock_movements (id, supply_id, quantity_delta, reason, notes, actor_id) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&id)
    .bind(&body.supply_id)
    .bind(body.quantity_delta)
    .bind(&body.reason)
    .bind(&body.notes)
    .bind(&user.user_id)
    .execute(&state.db)
    .await
    .map_err(db_err(t))?;

    crate::modules::audit::write(
        &state.db, &user.user_id, "stock.movement", "stock", &id, t,
    ).await;
    slog(&state.db, "info",
        &format!("stock.movement id={} reason={} delta={}", id, body.reason, body.quantity_delta), t).await;

    Ok((StatusCode::CREATED, Json(StockMovementResponse {
        id,
        supply_id: body.supply_id,
        quantity_delta: body.quantity_delta,
        reason: body.reason,
        notes: body.notes,
        actor_id: user.user_id,
        created_at: String::new(),
    })))
}

/// GET /stock/inventory — returns the current total on-hand (sum of
/// all quantity_delta rows) and per-supply breakdown.
pub async fn inventory(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
) -> Result<Json<InventorySnapshot>, AppError> {
    let t = &tid.0;

    let total: (Option<i64>,) =
        sqlx::query_as("SELECT SUM(quantity_delta) FROM stock_movements")
            .fetch_one(&state.db)
            .await
            .map_err(db_err(t))?;
    let on_hand = total.0.unwrap_or(0);

    let per_supply: Vec<(Option<String>, Option<i64>)> = sqlx::query_as(
        "SELECT supply_id, SUM(quantity_delta) FROM stock_movements GROUP BY supply_id",
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_err(t))?;

    let by_supply: Vec<InventoryLine> = per_supply
        .into_iter()
        .map(|(sid, q)| InventoryLine {
            supply_id: sid,
            quantity: q.unwrap_or(0),
        })
        .collect();

    Ok(Json(InventorySnapshot { total_on_hand: on_hand, by_supply }))
}

/// Shared helper used by the dashboard to compute the canonical
/// `inventory_on_hand` metric. Lives here so there's exactly one
/// source of truth.
pub async fn sum_on_hand(db: &sqlx::SqlitePool) -> Result<i64, sqlx::Error> {
    let (v,): (Option<i64>,) =
        sqlx::query_as("SELECT SUM(quantity_delta) FROM stock_movements")
            .fetch_one(db)
            .await?;
    Ok(v.unwrap_or(0))
}

#[derive(sqlx::FromRow)]
struct MovementRow {
    id: String,
    supply_id: Option<String>,
    quantity_delta: i64,
    reason: String,
    notes: String,
    actor_id: String,
    created_at: String,
}
