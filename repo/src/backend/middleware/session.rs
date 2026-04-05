use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;
use sqlx::SqlitePool;

use crate::app::AppState;
use crate::extractors::SessionUser;

/// Session middleware: enriches the request with the authenticated user if a
/// valid, non-expired session cookie is present. Never blocks — auth
/// enforcement lives in the `SessionUser` / `AdminUser` extractors.
pub async fn session_middleware(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Response {
    if let Some(session_id) = extract_session_cookie(&request) {
        if let Some(user) = validate_session(&state.db, &session_id).await {
            touch_session(&state.db, &session_id).await;
            request.extensions_mut().insert(user);
        }
    }
    next.run(request).await
}

fn extract_session_cookie(req: &Request) -> Option<String> {
    req.headers()
        .get("cookie")?
        .to_str()
        .ok()?
        .split(';')
        .find_map(|c| {
            let c = c.trim();
            c.strip_prefix("session_id=")
                .map(|v| v.to_string())
        })
}

async fn validate_session(db: &SqlitePool, session_id: &str) -> Option<SessionUser> {
    let row = sqlx::query_as::<_, SessionRow>(
        "SELECT s.id, s.user_id, u.username, u.role \
         FROM sessions s JOIN users u ON s.user_id = u.id \
         WHERE s.id = ? AND s.last_active > datetime('now', '-30 minutes') \
           AND u.anonymized = 0",
    )
    .bind(session_id)
    .fetch_optional(db)
    .await
    .ok()
    .flatten()?;

    Some(SessionUser {
        session_id: row.id,
        user_id: row.user_id,
        username: row.username,
        role: row.role,
    })
}

async fn touch_session(db: &SqlitePool, session_id: &str) {
    let _ = sqlx::query("UPDATE sessions SET last_active = datetime('now') WHERE id = ?")
        .bind(session_id)
        .execute(db)
        .await;
}

#[derive(sqlx::FromRow)]
struct SessionRow {
    id: String,
    user_id: String,
    username: String,
    role: String,
}
