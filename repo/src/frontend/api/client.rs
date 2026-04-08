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
    // ── Centralized 401 handling (app-wide) ────────────────────────────
    // On ANY 401 from ANY API call, immediately:
    //   1. Flash a "session expired" message
    //   2. Preserve the current browser route for post-login restore
    //   3. Force a page reload — the app mount checks /auth/me, sees 401,
    //      and lands on Login with the preserved route + flash message.
    // This ensures stale dashboard/form state is never left visible after
    // session expiry, regardless of which component triggered the call.
    if status == 401 {
        crate::draft::flash_session_expired();
        #[cfg(target_arch = "wasm32")]
        if let Some(w) = web_sys::window() {
            if let Ok(p) = w.location().pathname() {
                crate::draft::preserve_route(&p);
            }
            // Force reload to clear all stale component state.
            let _ = w.location().reload();
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

// ── Privacy Preferences ─────────────────────────────────────────────

pub async fn get_privacy_preferences() -> Result<PrivacyPreferencesResponse, ApiError> {
    let resp = Request::get("/profile/privacy-preferences").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn update_privacy_preferences(req: &PrivacyPreferencesUpdate) -> Result<PrivacyPreferencesResponse, ApiError> {
    let resp = Request::patch("/profile/privacy-preferences")
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(req).unwrap())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
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

// ── Evidence Upload ─────────────────────────────────────────────────

pub async fn upload_start(req: &UploadStartRequest) -> Result<UploadStartResponse, ApiError> {
    let resp = Request::post("/media/upload/start")
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(req).unwrap())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn upload_chunk(req: &UploadChunkRequest) -> Result<serde_json::Value, ApiError> {
    let resp = Request::post("/media/upload/chunk")
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(req).unwrap())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn upload_complete(req: &UploadCompleteRequest) -> Result<EvidenceResponse, ApiError> {
    let resp = Request::post("/media/upload/complete")
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(req).unwrap())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

// ── Supply Entries ──────────────────────────────────────────────────

pub async fn list_supply() -> Result<Vec<SupplyResponse>, ApiError> {
    let resp = Request::get("/supply-entries").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn create_supply(req: &SupplyRequest) -> Result<SupplyResponse, ApiError> {
    let resp = Request::post("/supply-entries")
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(req).unwrap())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn resolve_supply(id: &str, req: &SupplyResolveRequest) -> Result<serde_json::Value, ApiError> {
    let resp = Request::patch(&format!("/supply-entries/{}/resolve", id))
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(req).unwrap())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

// ── Traceability ────────────────────────────────────────────────────

pub async fn list_traceability() -> Result<Vec<TraceCodeResponse>, ApiError> {
    let resp = Request::get("/traceability").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn create_traceability(req: &TraceCodeRequest) -> Result<TraceCodeResponse, ApiError> {
    let resp = Request::post("/traceability")
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(req).unwrap())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn publish_traceability(id: &str, comment: &str) -> Result<serde_json::Value, ApiError> {
    let body = serde_json::json!({"comment": comment});
    let resp = Request::post(&format!("/traceability/{}/publish", id))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn retract_traceability(id: &str, comment: &str) -> Result<serde_json::Value, ApiError> {
    let body = serde_json::json!({"comment": comment});
    let resp = Request::post(&format!("/traceability/{}/retract", id))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn list_trace_steps(id: &str) -> Result<Vec<TraceStepResponse>, ApiError> {
    let resp = Request::get(&format!("/traceability/{}/steps", id)).send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

// ── Check-In ────────────────────────────────────────────────────────

pub async fn list_members() -> Result<Vec<MemberResponse>, ApiError> {
    let resp = Request::get("/members").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn create_member(req: &MemberRequest) -> Result<MemberResponse, ApiError> {
    let resp = Request::post("/members")
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(req).unwrap())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn checkin(req: &CheckinRequest) -> Result<CheckinResponse, ApiError> {
    let resp = Request::post("/checkin")
        .header("Content-Type", "application/json")
        .body(serde_json::to_string(req).unwrap())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn checkin_history() -> Result<serde_json::Value, ApiError> {
    let resp = Request::get("/checkin/history").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

// ── Dashboard with filters ──────────────────────────────────────────

pub async fn reports_summary_filtered(
    from: &str, to: &str, status: &str, intake_type: &str,
    region: &str, tags: &str, q: &str,
) -> Result<serde_json::Value, ApiError> {
    let mut params: Vec<String> = Vec::new();
    if !from.is_empty() { params.push(format!("from={}", urlencode(from))); }
    if !to.is_empty() { params.push(format!("to={}", urlencode(to))); }
    if !status.is_empty() { params.push(format!("status={}", urlencode(status))); }
    if !intake_type.is_empty() { params.push(format!("intake_type={}", urlencode(intake_type))); }
    if !region.is_empty() { params.push(format!("region={}", urlencode(region))); }
    if !tags.is_empty() { params.push(format!("tags={}", urlencode(tags))); }
    if !q.is_empty() { params.push(format!("q={}", urlencode(q))); }
    let path = if params.is_empty() { "/reports/summary".to_string() }
               else { format!("/reports/summary?{}", params.join("&")) };
    let resp = Request::get(&path).send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub fn build_export_url(
    from: &str, to: &str, status: &str, intake_type: &str,
    region: &str, tags: &str, q: &str,
) -> String {
    let mut params: Vec<String> = Vec::new();
    if !from.is_empty() { params.push(format!("from={}", urlencode(from))); }
    if !to.is_empty() { params.push(format!("to={}", urlencode(to))); }
    if !status.is_empty() { params.push(format!("status={}", urlencode(status))); }
    if !intake_type.is_empty() { params.push(format!("intake_type={}", urlencode(intake_type))); }
    if !region.is_empty() { params.push(format!("region={}", urlencode(region))); }
    if !tags.is_empty() { params.push(format!("tags={}", urlencode(tags))); }
    if !q.is_empty() { params.push(format!("q={}", urlencode(q))); }
    if params.is_empty() { "/reports/export".to_string() }
    else { format!("/reports/export?{}", params.join("&")) }
}

// ── Adoption conversion ─────────────────────────────────────────────

pub async fn adoption_conversion() -> Result<serde_json::Value, ApiError> {
    let resp = Request::get("/reports/adoption-conversion").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

// ── Evidence Linking + Legal Hold ──────────────────────────────────

pub async fn link_evidence(evidence_id: &str, target_type: &str, target_id: &str) -> Result<serde_json::Value, ApiError> {
    let body = serde_json::json!({"target_type": target_type, "target_id": target_id});
    let resp = Request::post(&format!("/evidence/{}/link", evidence_id))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn set_legal_hold(evidence_id: &str, hold: bool) -> Result<serde_json::Value, ApiError> {
    let body = serde_json::json!({"legal_hold": hold});
    let resp = Request::patch(&format!("/evidence/{}/legal-hold", evidence_id))
        .header("Content-Type", "application/json")
        .body(body.to_string())
        .map_err(|e| ApiError { status: 0, code: "REQUEST".into(), message: format!("{:?}", e) })?
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn delete_evidence(evidence_id: &str) -> Result<serde_json::Value, ApiError> {
    let resp = Request::delete(&format!("/evidence/{}", evidence_id)).send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

// ── Admin Operations ───────────────────────────────────────────────

pub async fn admin_get_config() -> Result<serde_json::Value, ApiError> {
    let resp = Request::get("/admin/config").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn admin_config_versions() -> Result<serde_json::Value, ApiError> {
    let resp = Request::get("/admin/config/versions").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn admin_rollback(version_id: &str) -> Result<serde_json::Value, ApiError> {
    let resp = Request::post(&format!("/admin/config/rollback/{}", version_id))
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn admin_export_diagnostics() -> Result<serde_json::Value, ApiError> {
    let resp = Request::post("/admin/diagnostics/export")
        .send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn admin_jobs() -> Result<serde_json::Value, ApiError> {
    let resp = Request::get("/admin/jobs").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}

pub async fn admin_logs() -> Result<serde_json::Value, ApiError> {
    let resp = Request::get("/admin/logs").send().await
        .map_err(|e| ApiError { status: 0, code: "NETWORK".into(), message: e.to_string() })?;
    if !resp.ok() { return Err(parse_error(resp).await); }
    resp.json().await.map_err(|e| ApiError { status: 0, code: "PARSE".into(), message: e.to_string() })
}
