use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Extension, Json};

use argon2::{Argon2, PasswordHasher};
use password_hash::{rand_core::OsRng, SaltString};
use uuid::Uuid;

use crate::app::AppState;
use crate::common::db_err;
use crate::error::AppError;
use crate::middleware::trace_id::TraceId;
use fieldtrace_shared::*;

// ── GET /users ──────────────────────────────────────────────────────

pub async fn list_users(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
) -> Result<Json<Vec<UserResponse>>, AppError> {
    let t = &tid.0;
    let rows = sqlx::query_as::<_, UserRow>(
        "SELECT id, username, role, created_at FROM users \
         WHERE anonymized = 0 ORDER BY created_at",
    )
    .fetch_all(&state.db)
    .await
    .map_err(db_err(t))?;

    Ok(Json(
        rows.into_iter()
            .map(|r| UserResponse {
                id: r.id,
                username: r.username,
                role: r.role,
                created_at: r.created_at,
            })
            .collect(),
    ))
}

// ── POST /users ─────────────────────────────────────────────────────

pub async fn create_user(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Json(body): Json<CreateUserRequest>,
) -> Result<(StatusCode, Json<UserResponse>), AppError> {
    let t = &tid.0;
    validate_role(&body.role, t)?;

    if body.password.len() < 12 {
        return Err(AppError::validation(
            "Password must be at least 12 characters",
            t,
        ));
    }

    let user_id = Uuid::new_v4().to_string();
    let hash = hash_pw(&body.password, t)?;

    sqlx::query("INSERT INTO users (id, username, password_hash, role) VALUES (?, ?, ?, ?)")
        .bind(&user_id)
        .bind(&body.username)
        .bind(&hash)
        .bind(&body.role)
        .execute(&state.db)
        .await
        .map_err(|e| {
            let msg = e.to_string();
            if msg.contains("UNIQUE") {
                AppError::conflict("Username already taken", t)
            } else {
                tracing::error!(trace_id = %t, error = %msg, "user insert failed");
                AppError::internal("Internal server error", t)
            }
        })?;

    Ok((
        StatusCode::CREATED,
        Json(UserResponse {
            id: user_id,
            username: body.username,
            role: body.role,
            created_at: String::new(),
        }),
    ))
}

// ── PATCH /users/:id ────────────────────────────────────────────────

pub async fn update_user(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Path(user_id): Path<String>,
    Json(body): Json<UpdateUserRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    if let Some(ref role) = body.role {
        validate_role(role, t)?;
        sqlx::query("UPDATE users SET role = ?, updated_at = datetime('now') WHERE id = ?")
            .bind(role)
            .bind(&user_id)
            .execute(&state.db)
            .await
            .map_err(db_err(t))?;
    }
    Ok(Json(serde_json::json!({"message": "User updated"})))
}

// ── DELETE /users/:id ───────────────────────────────────────────────

pub async fn delete_user(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Path(user_id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    let result = sqlx::query("DELETE FROM users WHERE id = ?")
        .bind(&user_id)
        .execute(&state.db)
        .await
        .map_err(db_err(t))?;

    if result.rows_affected() == 0 {
        return Err(AppError::not_found("User not found", t));
    }

    let _ = sqlx::query("DELETE FROM sessions WHERE user_id = ?")
        .bind(&user_id)
        .execute(&state.db)
        .await;

    Ok(Json(serde_json::json!({"message": "User deleted"})))
}

fn validate_role(role: &str, tid: &str) -> Result<(), AppError> {
    match role {
        "administrator" | "operations_staff" | "auditor" => Ok(()),
        _ => Err(AppError::validation(
            "Role must be administrator, operations_staff, or auditor",
            tid,
        )),
    }
}

fn hash_pw(password: &str, tid: &str) -> Result<String, AppError> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|h| h.to_string())
        .map_err(|e| {
            tracing::error!(trace_id = %tid, error = %e, "argon2 hash failure");
            AppError::internal("Internal server error", tid)
        })
}

#[derive(sqlx::FromRow)]
struct UserRow {
    id: String,
    username: String,
    role: String,
    created_at: String,
}
