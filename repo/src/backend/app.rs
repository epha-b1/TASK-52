use axum::routing::{get, patch, post};
use axum::{Json, Router};
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;
use std::str::FromStr;
use std::sync::{Arc, RwLock};
use tower_http::services::ServeDir;

use crate::config::Config;
use crate::crypto::Crypto;
use crate::middleware::{auth_guard, idempotency, session, trace_id};
use crate::modules::address_book::handlers as addr;
use crate::modules::admin::handlers as admin;
use crate::modules::audit::handlers as audit;
use crate::modules::auth::handlers as auth;
use crate::modules::checkin::handlers as checkin;
use crate::modules::dashboard::handlers as dash;
use crate::modules::evidence::handlers as evid;
use crate::modules::inspections::handlers as insp;
use crate::modules::intake::handlers as intake;
use crate::modules::stock::handlers as stock;
use crate::modules::supply::handlers as supply;
use crate::modules::traceability::handlers as trace;
use crate::modules::transfers::handlers as transfers;
use crate::modules::users::handlers as users;

/// Shared application state passed to every handler via `State(AppState)`.
///
/// The crypto holder is wrapped in `Arc<RwLock<_>>` so key rotation can
/// atomically replace the cipher without blocking reads for long (writes
/// only happen during rotate-key).
#[derive(Clone)]
pub struct AppState {
    pub db: SqlitePool,
    pub config: Config,
    pub crypto: Arc<RwLock<Crypto>>,
}

impl AppState {
    /// Return a clone of the current cipher. `Crypto` is `Clone` because its
    /// internal `Aes256Gcm` is cheap to clone (it just holds a round-key
    /// schedule), so we never hold the read lock across await points.
    pub fn crypto(&self) -> Crypto {
        self.crypto.read().expect("crypto rwlock poisoned").clone()
    }

    /// Replace the cipher atomically (used by key rotation).
    pub fn set_crypto(&self, new_crypto: Crypto) {
        *self.crypto.write().expect("crypto rwlock poisoned") = new_crypto;
    }
}

pub async fn create_app(config: &Config) -> Router {
    let db = connect_db(&config.database_url).await;
    run_migrations(&db).await;

    let crypto = Crypto::new(&config.encryption_key);
    let state = AppState {
        db: db.clone(),
        config: config.clone(),
        crypto: Arc::new(RwLock::new(crypto)),
    };

    // Register background jobs at startup. Session cleanup, account deletion
    // purge, diagnostics cleanup, evidence retention.
    crate::jobs::register_all(db.clone(), config.clone());

    // Public routes
    let public = Router::new()
        .route("/health", get(health_handler))
        .route("/auth/register", post(auth::register))
        .route("/auth/login", post(auth::login))
        .route("/traceability/verify/:code", get(trace::verify_code));

    // Protected routes — valid session required
    let protected = Router::new()
        // Auth / Me
        .route("/auth/logout", post(auth::logout))
        .route("/auth/me", get(auth::me))
        .route("/auth/change-password", patch(auth::change_password))
        // Account deletion cooling-off
        .route("/account/delete", post(auth::request_account_deletion))
        .route("/account/cancel-deletion", post(auth::cancel_account_deletion))
        // Address Book
        .route("/address-book", get(addr::list).post(addr::create))
        .route("/address-book/:id", patch(addr::update).delete(addr::delete))
        // Intake
        .route("/intake", get(intake::list).post(intake::create))
        .route("/intake/:id", get(intake::get_one))
        .route("/intake/:id/status", patch(intake::update_status))
        // Inspections
        .route("/inspections", get(insp::list).post(insp::create))
        .route("/inspections/:id/resolve", patch(insp::resolve))
        // Evidence
        .route("/media/upload/start", post(evid::upload_start))
        .route("/media/upload/chunk", post(evid::upload_chunk))
        .route("/media/upload/complete", post(evid::upload_complete))
        .route("/evidence", get(evid::list))
        .route("/evidence/:id", axum::routing::delete(evid::delete))
        .route("/evidence/:id/link", post(evid::link))
        .route("/evidence/:id/legal-hold", patch(evid::legal_hold))
        // Supply
        .route("/supply-entries", get(supply::list).post(supply::create))
        .route("/supply-entries/:id/resolve", patch(supply::resolve))
        // Traceability
        .route("/traceability", get(trace::list).post(trace::create))
        .route("/traceability/:id/publish", post(trace::publish))
        .route("/traceability/:id/retract", post(trace::retract))
        .route("/traceability/:id/steps", get(trace::list_steps).post(trace::append_manual_step))
        // Transfers — first-class operational queue
        .route("/transfers", get(transfers::list).post(transfers::create))
        .route("/transfers/:id", get(transfers::get_one))
        .route("/transfers/:id/status", patch(transfers::update_status))
        // Stock movements ledger (canonical inventory source)
        .route("/stock/movements", get(stock::list).post(stock::create))
        .route("/stock/inventory", get(stock::inventory))
        // Check-in
        .route("/members", get(checkin::list_members).post(checkin::create_member))
        .route("/checkin", post(checkin::checkin))
        .route("/checkin/history", get(checkin::history))
        // Dashboard
        .route("/reports/summary", get(dash::summary))
        .route("/reports/export", get(dash::export_csv))
        .route("/reports/adoption-conversion", get(dash::adoption_conversion))
        // Audit
        .route("/audit-logs", get(audit::list))
        .route("/audit-logs/export", get(audit::export_csv))
        // Idempotency runs AFTER auth (auth-first scope) — it requires SessionUser.
        .layer(axum::middleware::from_fn_with_state(
            state.clone(),
            idempotency::idempotency_middleware,
        ))
        .layer(axum::middleware::from_fn(auth_guard::require_auth));

    // Admin-only routes
    let admin_routes = Router::new()
        .route("/users", get(users::list_users).post(users::create_user))
        .route("/users/:id", patch(users::update_user).delete(users::delete_user))
        .route("/admin/config", get(admin::get_config).patch(admin::update_config))
        .route("/admin/config/versions", get(admin::list_versions))
        .route("/admin/config/rollback/:id", post(admin::rollback))
        .route("/admin/diagnostics/export", post(admin::export_diagnostics))
        .route("/admin/diagnostics/download/:id", get(admin::download_diagnostics))
        .route("/admin/jobs", get(admin::jobs))
        .route("/admin/logs", get(admin::list_logs))
        .route("/admin/account-purge", post(admin::run_account_purge))
        .route("/admin/retention-purge", post(admin::run_evidence_retention))
        .route("/admin/security/rotate-key", post(admin::rotate_key))
        .layer(axum::middleware::from_fn(auth_guard::require_admin));

    Router::new()
        .merge(public)
        .merge(protected)
        .merge(admin_routes)
        .with_state(state.clone())
        .fallback_service(ServeDir::new(&config.static_dir))
        .layer(axum::middleware::from_fn_with_state(state.clone(), session::session_middleware))
        .layer(axum::middleware::from_fn(trace_id::trace_id_middleware))
}

async fn connect_db(url: &str) -> SqlitePool {
    let options = SqliteConnectOptions::from_str(url)
        .expect("Invalid DATABASE_URL")
        .create_if_missing(true)
        .journal_mode(SqliteJournalMode::Wal);

    SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await
        .expect("Failed to connect to SQLite")
}

async fn run_migrations(db: &SqlitePool) {
    sqlx::migrate!("../../migrations")
        .run(db)
        .await
        .expect("Failed to run migrations");
    tracing::info!("Migrations applied successfully");
}

async fn health_handler() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status": "ok"}))
}
