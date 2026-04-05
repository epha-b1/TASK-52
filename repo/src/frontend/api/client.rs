use fieldtrace_shared::*;
use gloo_net::http::Request;

// ── Error type ──────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct ApiError {
    pub status: u16,
    pub code: String,
    pub message: String,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.message)
    }
}

async fn parse_error(resp: gloo_net::http::Response) -> ApiError {
    let status = resp.status();
    // Session expired mid-action: flash a user-visible message and preserve
    // the current route so the app shell can restore the same page after
    // re-login. Any active draft stays in localStorage untouched.
    if status == 401 {
        crate::draft::flash_session_expired();
        // Best-effort route capture — the browser's location hash/path is
        // what the app shell uses as its route key.
        #[cfg(target_arch = "wasm32")]
        if let Some(w) = web_sys::window() {
            if let Ok(p) = w.location().pathname() {
                crate::draft::preserve_route(&p);
            }
        }
    }
    match resp.json::<ErrorResponse>().await {
        Ok(e) => ApiError { status: e.status, code: e.code, message: e.message },
        Err(_) => ApiError { status, code: "UNKNOWN".into(), message: format!("HTTP {}", status) },
    }
}

// ── Health ───────────────────────────────────────────────────────────

pub async fn check_health() -> Result<String, String> {
    let resp = Request::get("/health")
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    let body: HealthResponse = resp.json().await.map_err(|e| format!("Parse error: {}", e))?;
    Ok(body.status)
}

// ── Auth ─────────────────────────────────────────────────────────────

pub async fn register(username: &str, password: &str) -> Result<AuthResponse, ApiError> {
    let body = serde_json::json!({"username": username, "password": password});
    let resp = Request::post("/auth/register")
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send()
        .await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;

    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn login(username: &str, password: &str) -> Result<AuthResponse, ApiError> {
    let body = serde_json::json!({"username": username, "password": password});
    let resp = Request::post("/auth/login")
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send()
        .await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;

    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn logout() -> Result<(), ApiError> {
    let resp = Request::post("/auth/logout")
        .send()
        .await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    Ok(())
}

pub async fn get_me() -> Result<UserResponse, ApiError> {
    let resp = Request::get("/auth/me")
        .send()
        .await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

// ── Address Book ─────────────────────────────────────────────────────

pub async fn list_addresses() -> Result<Vec<AddressResponse>, ApiError> {
    let resp = Request::get("/address-book").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn create_address(req: &AddressRequest) -> Result<AddressResponse, ApiError> {
    let resp = Request::post("/address-book")
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(req).unwrap()).map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await.map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn delete_address(id: &str) -> Result<(), ApiError> {
    let resp = Request::delete(&format!("/address-book/{}", id)).send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    Ok(())
}

// ── Intake ───────────────────────────────────────────────────────────

pub async fn list_intake() -> Result<Vec<IntakeResponse>, ApiError> {
    let resp = Request::get("/intake").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn create_intake(req: &IntakeRequest) -> Result<IntakeResponse, ApiError> {
    let resp = Request::post("/intake").header("Content-Type", "application/json")
        .body(serde_json::to_string(req).unwrap())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await.map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

// ── Transfers ────────────────────────────────────────────────────────

pub async fn list_transfers() -> Result<Vec<TransferResponse>, ApiError> {
    let resp = Request::get("/transfers").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

// ── Inspections ──────────────────────────────────────────────────────

pub async fn list_inspections() -> Result<Vec<InspectionResponse>, ApiError> {
    let resp = Request::get("/inspections").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

// ── Evidence with filters ────────────────────────────────────────────

pub async fn list_evidence(keyword: &str, tag: &str, from: &str, to: &str)
    -> Result<Vec<EvidenceResponse>, ApiError>
{
    let mut q: Vec<String> = Vec::new();
    if !keyword.is_empty() { q.push(format!("keyword={}", urlencode(keyword))); }
    if !tag.is_empty() { q.push(format!("tag={}", urlencode(tag))); }
    if !from.is_empty() { q.push(format!("from={}", urlencode(from))); }
    if !to.is_empty() { q.push(format!("to={}", urlencode(to))); }
    let path = if q.is_empty() { "/evidence".to_string() }
               else { format!("/evidence?{}", q.join("&")) };
    let resp = Request::get(&path).send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

fn urlencode(s: &str) -> String {
    // Minimal percent-encoding for safe ASCII query values.
    let mut out = String::with_capacity(s.len());
    for c in s.chars() {
        if c.is_ascii_alphanumeric() || "-_.~".contains(c) {
            out.push(c);
        } else {
            for b in c.to_string().bytes() {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

// ── Reports ──────────────────────────────────────────────────────────

pub async fn reports_summary() -> Result<serde_json::Value, ApiError> {
    let resp = Request::get("/reports/summary").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

// ── Account lifecycle ────────────────────────────────────────────────

pub async fn request_account_deletion() -> Result<serde_json::Value, ApiError> {
    let resp = Request::post("/account/delete").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn cancel_account_deletion() -> Result<serde_json::Value, ApiError> {
    let resp = Request::post("/account/cancel-deletion").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn change_password(current: &str, new_pw: &str) -> Result<(), ApiError> {
    let body = serde_json::json!({"current_password": current, "new_password": new_pw});
    let resp = Request::patch("/auth/change-password")
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send()
        .await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    Ok(())
}
