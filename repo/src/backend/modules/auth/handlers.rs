use axum::extract::State;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::IntoResponse;
use axum::{Extension, Json};

use argon2::{Argon2, PasswordHash, PasswordHasher, PasswordVerifier};
use password_hash::{rand_core::OsRng, SaltString};
use sqlx::SqlitePool;
use uuid::Uuid;

use crate::app::AppState;
use crate::common::{db_err, slog};
use crate::error::AppError;
use crate::extractors::SessionUser;
use crate::middleware::trace_id::TraceId;
use fieldtrace_shared::*;

// ── POST /auth/register ─────────────────────────────────────────────

pub async fn register(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Json(body): Json<RegisterRequest>,
) -> Result<impl IntoResponse, AppError> {
    let t = &tid.0;

    let count: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM users WHERE anonymized = 0")
        .fetch_one(&state.db)
        .await
        .map_err(db_err(t))?;

    if count.0 > 0 {
        return Err(AppError::conflict(
            "System already initialized. Use admin-managed user creation.",
            t,
        ));
    }

    validate_password(&body.password, t)?;

    let user_id = Uuid::new_v4().to_string();
    let hash = hash_password(&body.password, t)?;

    sqlx::query(
        "INSERT INTO users (id, username, password_hash, role) VALUES (?, ?, ?, 'administrator')",
    )
    .bind(&user_id)
    .bind(&body.username)
    .bind(&hash)
    .execute(&state.db)
    .await
    .map_err(|e| {
        let msg = e.to_string();
        if msg.contains("UNIQUE") {
            AppError::conflict("Username already taken", t)
        } else {
            tracing::error!(trace_id = %t, error = %msg, "register insert failed");
            AppError::internal("Internal server error", t)
        }
    })?;

    let session_id = create_session(&state.db, &user_id)
        .await
        .map_err(db_err(t))?;

    let user = UserResponse {
        id: user_id,
        username: body.username,
        role: "administrator".into(),
        created_at: String::new(),
    };

    Ok((
        StatusCode::CREATED,
        session_headers(&session_id),
        Json(AuthResponse {
            user,
            message: "Administrator account created".into(),
        }),
    ))
}

// ── POST /auth/login ────────────────────────────────────────────────

pub async fn login(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Json(body): Json<LoginRequest>,
) -> Result<impl IntoResponse, AppError> {
    let t = &tid.0;

    check_lockout(&state.db, &body.username, t).await?;

    let row = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, password_hash, role, created_at FROM users \
         WHERE username = ? AND anonymized = 0",
    )
    .bind(&body.username)
    .fetch_optional(&state.db)
    .await
    .map_err(db_err(t))?;

    let user_row = match row {
        Some(u) => u,
        None => {
            record_failure(&state.db, &body.username).await;
            slog(&state.db, "warn",
                &format!("auth.login failed (user not found) username={}", body.username), t).await;
            return Err(AppError::unauthorized("Invalid credentials", t));
        }
    };

    if !verify_password(&body.password, &user_row.password_hash) {
        record_failure(&state.db, &body.username).await;
        slog(&state.db, "warn",
            &format!("auth.login failed (bad password) username={}", body.username), t).await;
        return Err(AppError::unauthorized("Invalid credentials", t));
    }

    // Clear failures on success
    let _ = sqlx::query("DELETE FROM auth_failures WHERE username = ?")
        .bind(&body.username)
        .execute(&state.db)
        .await;

    let session_id = create_session(&state.db, &user_row.id)
        .await
        .map_err(db_err(t))?;

    let user = UserResponse {
        id: user_row.id,
        username: user_row.username,
        role: user_row.role,
        created_at: user_row.created_at,
    };

    Ok((
        StatusCode::OK,
        session_headers(&session_id),
        Json(AuthResponse {
            user,
            message: "Login successful".into(),
        }),
    ))
}

// ── POST /auth/logout ───────────────────────────────────────────────

pub async fn logout(
    State(state): State<AppState>,
    Extension(session): Extension<SessionUser>,
) -> impl IntoResponse {
    let _ = sqlx::query("DELETE FROM sessions WHERE id = ?")
        .bind(&session.session_id)
        .execute(&state.db)
        .await;

    let mut headers = HeaderMap::new();
    headers.insert(
        "Set-Cookie",
        HeaderValue::from_static("session_id=; HttpOnly; Path=/; SameSite=Strict; Max-Age=0"),
    );
    (StatusCode::OK, headers, Json(serde_json::json!({"message": "Logged out"})))
}

// ── GET /auth/me ────────────────────────────────────────────────────

pub async fn me(Extension(session): Extension<SessionUser>) -> Json<UserResponse> {
    Json(UserResponse {
        id: session.user_id,
        username: session.username,
        role: session.role,
        created_at: String::new(),
    })
}

// ── PATCH /auth/change-password ─────────────────────────────────────

pub async fn change_password(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(session): Extension<SessionUser>,
    Json(body): Json<ChangePasswordRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;

    let row: (String,) = sqlx::query_as("SELECT password_hash FROM users WHERE id = ?")
        .bind(&session.user_id)
        .fetch_one(&state.db)
        .await
        .map_err(db_err(t))?;

    if !verify_password(&body.current_password, &row.0) {
        return Err(AppError::unauthorized("Current password is incorrect", t));
    }

    validate_password(&body.new_password, t)?;
    let new_hash = hash_password(&body.new_password, t)?;

    sqlx::query("UPDATE users SET password_hash = ?, updated_at = datetime('now') WHERE id = ?")
        .bind(&new_hash)
        .bind(&session.user_id)
        .execute(&state.db)
        .await
        .map_err(db_err(t))?;

    // Invalidate other sessions
    let _ = sqlx::query("DELETE FROM sessions WHERE user_id = ? AND id != ?")
        .bind(&session.user_id)
        .bind(&session.session_id)
        .execute(&state.db)
        .await;

    Ok(Json(serde_json::json!({"message": "Password changed"})))
}

// ── POST /account/delete ─────────────────────────────────────────────
//
// Marks the user for deletion. The user can still log in during the 7-day
// cooling-off window; the background job purges the account after that.

pub async fn request_account_deletion(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(session): Extension<SessionUser>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;

    // Idempotent: only set if not already requested.
    let res = sqlx::query(
        "UPDATE users SET deletion_requested_at = datetime('now') \
         WHERE id = ? AND deletion_requested_at IS NULL",
    )
    .bind(&session.user_id)
    .execute(&state.db)
    .await
    .map_err(db_err(t))?;

    // Also write an audit trail entry.
    crate::modules::audit::write(
        &state.db, &session.user_id, "account.delete_requested",
        "user", &session.user_id, t,
    ).await;

    Ok(Json(serde_json::json!({
        "message": "Account deletion scheduled. You have 7 days to cancel before permanent removal.",
        "cooling_off_days": 7,
        "already_requested": res.rows_affected() == 0,
    })))
}

// ── POST /account/cancel-deletion ───────────────────────────────────
//
// Clears the deletion request if within the cooling-off window.

pub async fn cancel_account_deletion(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(session): Extension<SessionUser>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;

    let res = sqlx::query(
        "UPDATE users SET deletion_requested_at = NULL \
         WHERE id = ? AND deletion_requested_at IS NOT NULL",
    )
    .bind(&session.user_id)
    .execute(&state.db)
    .await
    .map_err(db_err(t))?;

    if res.rows_affected() == 0 {
        return Err(AppError::conflict("No pending deletion to cancel", t));
    }

    crate::modules::audit::write(
        &state.db, &session.user_id, "account.delete_cancelled",
        "user", &session.user_id, t,
    ).await;

    Ok(Json(serde_json::json!({"message": "Account deletion cancelled"})))
}

// ── Helpers ─────────────────────────────────────────────────────────

fn validate_password(password: &str, tid: &str) -> Result<(), AppError> {
    if password.len() < 12 {
        return Err(AppError::validation(
            "Password must be at least 12 characters",
            tid,
        ));
    }
    Ok(())
}

fn hash_password(password: &str, tid: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default();
    argon2
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| {
            tracing::error!(trace_id = %tid, error = %e, "argon2 hash failure");
            AppError::internal("Internal server error", tid)
        })
}

fn verify_password(password: &str, hash: &str) -> bool {
    PasswordHash::new(hash)
        .ok()
        .map(|parsed| Argon2::default().verify_password(password.as_bytes(), &parsed).is_ok())
        .unwrap_or(false)
}

async fn create_session(db: &SqlitePool, user_id: &str) -> Result<String, sqlx::Error> {
    let session_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO sessions (id, user_id) VALUES (?, ?)")
        .bind(&session_id)
        .bind(user_id)
        .execute(db)
        .await?;
    Ok(session_id)
}

async fn check_lockout(db: &SqlitePool, username: &str, tid: &str) -> Result<(), AppError> {
    let count: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM auth_failures \
         WHERE username = ? AND attempted_at > datetime('now', '-15 minutes')",
    )
    .bind(username)
    .fetch_one(db)
    .await
    .map_err(db_err(tid))?;

    if count.0 >= 10 {
        return Err(AppError::locked(
            "Account temporarily locked due to too many failed attempts. Try again in 15 minutes.",
            tid,
        ));
    }
    Ok(())
}

async fn record_failure(db: &SqlitePool, username: &str) {
    let _ = sqlx::query("INSERT INTO auth_failures (username) VALUES (?)")
        .bind(username)
        .execute(db)
        .await;
}

fn session_headers(session_id: &str) -> HeaderMap {
    let mut headers = HeaderMap::new();
    headers.insert(
        "Set-Cookie",
        HeaderValue::from_str(&format!(
            "session_id={}; HttpOnly; Path=/; SameSite=Strict; Max-Age=1800",
            session_id
        ))
        .unwrap(),
    );
    headers
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: String,
    username: String,
    password_hash: String,
    role: String,
    created_at: String,
}
