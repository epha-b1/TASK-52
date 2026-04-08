//! Dashboard / reports.
//!
//! Filter surface (all optional, combine freely):
//!
//! - `from`, `to`           — ISO date range on `intake_records.created_at`
//! - `status`               — intake status
//! - `intake_type`          — animal | supply | donation
//! - `region`               — exact region match
//! - `tags`                 — substring match against CSV tag column
//! - `q`                    — full-text substring search against details/region/tags
//!
//! `inventory_on_hand` is sourced from the `stock_movements` ledger
//! (SUM of quantity_delta), NOT `COUNT(supply_entries)`.

use axum::extract::{Query, State};
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::{Extension, Json};
use std::collections::HashMap;

use crate::app::AppState;
use crate::common::{db_err, require_admin_or_auditor};
use crate::error::AppError;
use crate::extractors::SessionUser;
use crate::middleware::trace_id::TraceId;

#[derive(Default, Clone)]
struct DashboardFilters {
    from: String,
    to: String,
    status: String,
    intake_type: String,
    region: String,
    tags: String,
    q: String,
}

impl DashboardFilters {
    fn from_query(q: &HashMap<String, String>) -> Self {
        Self {
            from: q.get("from").cloned().unwrap_or_default(),
            to: q.get("to").cloned().unwrap_or_default(),
            status: q.get("status").cloned().unwrap_or_default(),
            intake_type: q.get("intake_type").cloned().unwrap_or_default(),
            region: q.get("region").cloned().unwrap_or_default(),
            tags: q.get("tags").cloned().unwrap_or_default(),
            q: q.get("q").cloned().unwrap_or_default(),
        }
    }

    /// Builds the shared `WHERE` fragment + bind list used by every
    /// count query. A single source of truth so summary + export
    /// ALWAYS honor the exact same filter semantics.
    fn where_clause(&self) -> (String, Vec<String>) {
        let mut parts: Vec<String> = vec!["1=1".into()];
        let mut binds: Vec<String> = Vec::new();
        if !self.from.is_empty() {
            parts.push("created_at >= ?".into());
            binds.push(self.from.clone());
        }
        if !self.to.is_empty() {
            parts.push("created_at <= ?".into());
            binds.push(self.to.clone());
        }
        if !self.intake_type.is_empty() {
            parts.push("intake_type = ?".into());
            binds.push(self.intake_type.clone());
        }
        if !self.status.is_empty() {
            parts.push("status = ?".into());
            binds.push(self.status.clone());
        }
        if !self.region.is_empty() {
            parts.push("region = ?".into());
            binds.push(self.region.clone());
        }
        if !self.tags.is_empty() {
            parts.push("tags LIKE ?".into());
            binds.push(format!("%{}%", self.tags));
        }
        if !self.q.is_empty() {
            // Simple substring "full-text" across the free-form fields.
            parts.push("(details LIKE ? OR region LIKE ? OR tags LIKE ?)".into());
            let pat = format!("%{}%", self.q);
            binds.push(pat.clone());
            binds.push(pat.clone());
            binds.push(pat);
        }
        (parts.join(" AND "), binds)
    }
}

pub async fn summary(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    let filters = DashboardFilters::from_query(&q);
    let metrics = compute_metrics(&state, t, &filters).await?;
    Ok(Json(serde_json::to_value(metrics).unwrap()))
}

#[derive(serde::Serialize)]
struct Metrics {
    // Filtered by intake query params (from/to/status/intake_type/region/tags/q)
    rescue_volume: i64,
    adoption_conversion: f64,
    donations_logged: i64,
    // Global metrics — NOT scoped by intake filters because tasks and
    // inventory are separate domains that don't link to intake_records.
    // These are explicitly labeled so consumers know the scope.
    task_completion_rate: f64,
    task_completion_scope: &'static str,
    inventory_on_hand: i64,
    inventory_scope: &'static str,
    filters: serde_json::Value,
}

async fn count_with(
    pool: &sqlx::SqlitePool,
    sql: String,
    binds: &[String],
    t: &str,
) -> Result<i64, AppError> {
    let mut q = sqlx::query_as::<_, (i64,)>(&sql);
    for b in binds {
        q = q.bind(b);
    }
    let (n,) = q.fetch_one(pool).await.map_err(db_err(t))?;
    Ok(n)
}

async fn compute_metrics(
    state: &AppState,
    t: &str,
    f: &DashboardFilters,
) -> Result<Metrics, AppError> {
    let (where_sql, binds) = f.where_clause();

    let intake_total = count_with(
        &state.db,
        format!("SELECT COUNT(*) FROM intake_records WHERE {}", where_sql),
        &binds,
        t,
    )
    .await?;

    let mut donations_binds = binds.clone();
    donations_binds.push("donation".into());
    let donations = count_with(
        &state.db,
        format!(
            "SELECT COUNT(*) FROM intake_records WHERE {} AND intake_type = ?",
            where_sql
        ),
        &donations_binds,
        t,
    )
    .await?;

    let mut animals_binds = binds.clone();
    animals_binds.push("animal".into());
    let animals = count_with(
        &state.db,
        format!(
            "SELECT COUNT(*) FROM intake_records WHERE {} AND intake_type = ?",
            where_sql
        ),
        &animals_binds,
        t,
    )
    .await?;

    // Adoption numerator: only animal records that reached "adopted" status.
    // This ensures the adoption KPI is animal-scoped consistently.
    let mut adopted_binds = binds.clone();
    adopted_binds.push("adopted".into());
    adopted_binds.push("animal".into());
    let adopted = count_with(
        &state.db,
        format!(
            "SELECT COUNT(*) FROM intake_records WHERE {} AND status = ? AND intake_type = ?",
            where_sql
        ),
        &adopted_binds,
        t,
    )
    .await?;

    let tasks_done: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tasks WHERE status = 'completed'")
        .fetch_one(&state.db)
        .await
        .map_err(db_err(t))?;
    let tasks_open: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM tasks WHERE status IN ('open','in_progress')")
        .fetch_one(&state.db)
        .await
        .map_err(db_err(t))?;

    // Inventory on hand: canonical source is the stock_movements ledger.
    // NOT COUNT(supply_entries) any more.
    let inventory_on_hand = crate::modules::stock::handlers::sum_on_hand(&state.db)
        .await
        .map_err(db_err(t))?;

    let adoption_conversion = if animals > 0 { adopted as f64 / animals as f64 } else { 0.0 };
    let task_completion_rate = if tasks_done.0 + tasks_open.0 > 0 {
        tasks_done.0 as f64 / (tasks_done.0 + tasks_open.0) as f64
    } else {
        0.0
    };

    Ok(Metrics {
        rescue_volume: intake_total,
        adoption_conversion,
        donations_logged: donations,
        task_completion_rate,
        task_completion_scope: "global (all tasks, not filtered by intake params)",
        inventory_on_hand,
        inventory_scope: "global (all stock movements, not filtered by intake params)",
        filters: serde_json::json!({
            "from": f.from,
            "to": f.to,
            "status": f.status,
            "intake_type": f.intake_type,
            "region": f.region,
            "tags": f.tags,
            "q": f.q,
        }),
    })
}

pub async fn export_csv(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<impl IntoResponse, AppError> {
    let t = &tid.0;
    require_admin_or_auditor(&user, t)?;

    // Export uses the EXACT same filter set as summary — same parser,
    // same WHERE builder — so operators can't accidentally get a
    // different result from the CSV than what they see on screen.
    let filters = DashboardFilters::from_query(&q);
    let m = compute_metrics(&state, t, &filters).await?;

    let csv = format!(
        "metric,value\nrescue_volume,{}\ndonations_logged,{}\ninventory_on_hand,{}\nadoption_conversion,{}\ntask_completion_rate,{}\nfilter_region,{}\nfilter_tags,{}\nfilter_q,{}\n",
        m.rescue_volume,
        m.donations_logged,
        m.inventory_on_hand,
        m.adoption_conversion,
        m.task_completion_rate,
        filters.region,
        filters.tags,
        filters.q,
    );

    let mut headers = HeaderMap::new();
    headers.insert(header::CONTENT_TYPE, HeaderValue::from_static("text/csv"));
    headers.insert(
        "Content-Disposition",
        HeaderValue::from_static("attachment; filename=\"report.csv\""),
    );
    Ok((StatusCode::OK, headers, csv))
}

pub async fn adoption_conversion(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    let animals: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM intake_records WHERE intake_type = 'animal'")
        .fetch_one(&state.db)
        .await
        .map_err(db_err(t))?;
    let adopted: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM intake_records WHERE status = 'adopted' AND intake_type = 'animal'")
        .fetch_one(&state.db)
        .await
        .map_err(db_err(t))?;
    let rate = if animals.0 > 0 {
        (adopted.0 as f64) / (animals.0 as f64)
    } else {
        0.0
    };
    Ok(Json(serde_json::json!({
        "total": animals.0,
        "adopted": adopted.0,
        "conversion_rate": rate,
    })))
}
