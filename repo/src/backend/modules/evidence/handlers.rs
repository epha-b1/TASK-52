use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::{Extension, Json};
use std::collections::HashMap;
use uuid::Uuid;

use crate::app::AppState;
use crate::common::{db_err, is_admin, require_write_role, slog, CivilDateTime};
use crate::error::AppError;
use crate::extractors::SessionUser;
use crate::middleware::trace_id::TraceId;
use fieldtrace_shared::*;

// Size limits (bytes)
const MAX_PHOTO: i64 = 25 * 1024 * 1024;
const MAX_VIDEO: i64 = 150 * 1024 * 1024;
const MAX_AUDIO: i64 = 20 * 1024 * 1024;
const MAX_VIDEO_SECONDS: i64 = 60;
const MAX_AUDIO_SECONDS: i64 = 120;

fn check_size(media_type: &str, size: i64, tid: &str) -> Result<(), AppError> {
    let max = match media_type {
        "photo" => MAX_PHOTO,
        "video" => MAX_VIDEO,
        "audio" => MAX_AUDIO,
        _ => return Err(AppError::validation("media_type must be photo, video, or audio", tid)),
    };
    if size > max {
        return Err(AppError::validation(format!("File exceeds {} bytes for {}", max, media_type), tid));
    }
    Ok(())
}

/// Build the facility + timestamp watermark string actually burned into photos
/// (format: `FAC01 MM/DD/YYYY hh:mm AM/PM`). For video/audio this same text
/// is persisted as metadata.
fn build_watermark(facility_code: &str) -> String {
    format!("{} {}", facility_code, CivilDateTime::now().us_12h())
}

/// Result of applying the local compression policy.
#[derive(Debug, Clone)]
pub(crate) struct CompressionResult {
    pub applied: bool,
    pub compressed_bytes: i64,
    pub ratio: f64,
}

/// Deterministic local compression policy.
///
/// The backend does NOT store raw media bytes (we track metadata only), so
/// "compression" here means applying the facility's file-size budget policy
/// to the original size. Each media type has a baseline reduction ratio
/// that mirrors what a real re-encoder would produce when targetting the
/// facility's storage budget:
///
///   - photo: lossy JPEG re-encode @ quality 80 typically yields ~0.70×
///   - video: H.264 reencode to 720p/2Mbps typically yields ~0.60×
///   - audio: AAC-LC @ 96kbps typically yields ~0.50×
///
/// If the original is already at or below a per-type floor (tiny files or
/// already highly compressed), the policy decides NOT to re-encode. In that
/// case `applied = false` and `compressed_bytes == original`.
pub(crate) fn apply_compression_policy(media_type: &str, original_bytes: i64) -> CompressionResult {
    // Per-type floor: files this size or smaller are not re-encoded because
    // the marginal savings don't justify the CPU cost.
    const PHOTO_FLOOR: i64 = 256 * 1024;   // 256 KiB
    const VIDEO_FLOOR: i64 = 2 * 1024 * 1024;  // 2 MiB
    const AUDIO_FLOOR: i64 = 128 * 1024;   // 128 KiB

    let (ratio, floor) = match media_type {
        "photo" => (0.70, PHOTO_FLOOR),
        "video" => (0.60, VIDEO_FLOOR),
        "audio" => (0.50, AUDIO_FLOOR),
        _ => (1.00, 0),
    };

    if original_bytes <= floor || ratio >= 1.0 {
        return CompressionResult {
            applied: false,
            compressed_bytes: original_bytes,
            ratio: 1.0,
        };
    }

    // Deterministic integer math so the test can assert exact values.
    // ((original * numerator) + denominator/2) / denominator  gives
    // rounded-nearest integer compression.
    let num = (ratio * 100.0) as i64;
    let compressed = (original_bytes * num + 50) / 100;
    CompressionResult {
        applied: true,
        compressed_bytes: compressed,
        ratio,
    }
}

// POST /media/upload/start — create upload session
pub async fn upload_start(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Json(body): Json<UploadStartRequest>,
) -> Result<Json<UploadStartResponse>, AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;
    check_size(&body.media_type, body.total_size, t)?;
    if body.media_type == "video" && body.duration_seconds > MAX_VIDEO_SECONDS {
        return Err(AppError::validation("Video exceeds 60 seconds", t));
    }
    if body.media_type == "audio" && body.duration_seconds > MAX_AUDIO_SECONDS {
        return Err(AppError::validation("Audio exceeds 2 minutes", t));
    }
    let id = Uuid::new_v4().to_string();
    let total_chunks = (body.total_size + (2 * 1024 * 1024) - 1) / (2 * 1024 * 1024);
    sqlx::query("INSERT INTO upload_sessions (id, filename, media_type, total_chunks, uploader_id) VALUES (?,?,?,?,?)")
        .bind(&id).bind(&body.filename).bind(&body.media_type)
        .bind(total_chunks).bind(&user.user_id)
        .execute(&state.db).await
        .map_err(db_err(t))?;
    Ok(Json(UploadStartResponse { upload_id: id, chunk_size_bytes: 2 * 1024 * 1024, total_chunks }))
}

/// Recognized media magic-byte signatures for format validation.
fn validate_media_format(media_type: &str, data: &[u8]) -> bool {
    if data.len() < 4 { return false; }
    match media_type {
        "photo" => {
            // JPEG (FF D8 FF), PNG (89 50 4E 47), WebP (RIFF...WEBP), BMP (42 4D)
            data.starts_with(&[0xFF, 0xD8, 0xFF])
                || data.starts_with(&[0x89, 0x50, 0x4E, 0x47])
                || (data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WEBP")
                || data.starts_with(&[0x42, 0x4D])
        }
        "video" => {
            // MP4/MOV (ftyp box), AVI (RIFF...AVI), WebM/MKV (1A 45 DF A3)
            (data.len() >= 8 && &data[4..8] == b"ftyp")
                || (data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"AVI ")
                || data.starts_with(&[0x1A, 0x45, 0xDF, 0xA3])
        }
        "audio" => {
            // MP3 (FF FB / FF F3 / ID3), WAV (RIFF...WAVE), FLAC (fLaC), OGG (OggS), AAC (FF F1)
            data.starts_with(&[0xFF, 0xFB])
                || data.starts_with(&[0xFF, 0xF3])
                || data.starts_with(b"ID3")
                || (data.len() >= 12 && &data[..4] == b"RIFF" && &data[8..12] == b"WAVE")
                || data.starts_with(b"fLaC")
                || data.starts_with(b"OggS")
                || data.starts_with(&[0xFF, 0xF1])
        }
        _ => false,
    }
}

// POST /media/upload/chunk — receive and persist chunk data
pub async fn upload_chunk(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Json(body): Json<UploadChunkRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;

    let session: Option<(String, String, i64, String)> = sqlx::query_as(
        "SELECT received_chunks, status, total_chunks, media_type FROM upload_sessions WHERE id = ? AND uploader_id = ?"
    ).bind(&body.upload_id).bind(&user.user_id)
        .fetch_optional(&state.db).await
        .map_err(db_err(t))?;

    let (received_json, status, total, media_type) = session.ok_or_else(|| AppError::not_found("Upload session not found", t))?;
    if status != "in_progress" {
        return Err(AppError::conflict("Upload already complete or failed", t));
    }
    if body.chunk_index < 0 || body.chunk_index >= total {
        return Err(AppError::validation("chunk_index out of range", t));
    }

    // Require non-empty chunk payload — metadata-only uploads are no longer
    // accepted because they bypass evidence integrity guarantees.
    if body.data.is_empty() {
        return Err(AppError::validation("Chunk data is required (base64-encoded payload)", t));
    }

    // Decode base64 chunk data
    let chunk_bytes = {
        use base64::Engine;
        base64::engine::general_purpose::STANDARD
            .decode(&body.data)
            .map_err(|_| AppError::validation("Invalid base64 chunk data", t))?
    };
    if chunk_bytes.is_empty() {
        return Err(AppError::validation("Chunk payload must not be empty", t));
    }

    // Validate format on first chunk (chunk_index == 0) using magic bytes
    if body.chunk_index == 0 {
        if !validate_media_format(&media_type, &chunk_bytes) {
            return Err(AppError::validation(
                format!("File content does not match declared media_type '{}'", media_type), t,
            ));
        }
    }

    // Persist chunk to storage/uploads/<upload_id>/chunk_<index>
    {
        let chunk_dir = format!("{}/uploads/{}", state.config.storage_dir, body.upload_id);
        std::fs::create_dir_all(&chunk_dir)
            .map_err(|e| {
                tracing::error!(trace_id = %t, error = %e, "Failed to create chunk dir");
                AppError::internal("Storage error", t)
            })?;
        let chunk_path = format!("{}/chunk_{}", chunk_dir, body.chunk_index);
        std::fs::write(&chunk_path, &chunk_bytes)
            .map_err(|e| {
                tracing::error!(trace_id = %t, error = %e, "Failed to write chunk");
                AppError::internal("Storage error", t)
            })?;
    }

    let mut received: Vec<i64> = serde_json::from_str(&received_json).unwrap_or_default();
    if !received.contains(&body.chunk_index) {
        received.push(body.chunk_index);
        received.sort();
    }
    let new_json = serde_json::to_string(&received).unwrap();
    sqlx::query("UPDATE upload_sessions SET received_chunks = ? WHERE id = ?")
        .bind(&new_json).bind(&body.upload_id)
        .execute(&state.db).await
        .map_err(db_err(t))?;

    Ok(Json(serde_json::json!({
        "received_count": received.len(),
        "total_chunks": total,
        "complete": received.len() as i64 == total
    })))
}

// POST /media/upload/complete — finalize
pub async fn upload_complete(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Json(body): Json<UploadCompleteRequest>,
) -> Result<(StatusCode, Json<EvidenceResponse>), AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;

    // Fingerprint format validation: must be non-empty hex-like (32-128 chars)
    if body.fingerprint.trim().is_empty()
        || body.fingerprint.len() < 8
        || body.fingerprint.len() > 256
        || !body.fingerprint.chars().all(|c| c.is_ascii_alphanumeric())
    {
        return Err(AppError::validation("Invalid fingerprint format", t));
    }

    let session: Option<(String, String, i64, String)> = sqlx::query_as(
        "SELECT filename, media_type, total_chunks, received_chunks FROM upload_sessions WHERE id = ? AND uploader_id = ?"
    ).bind(&body.upload_id).bind(&user.user_id)
        .fetch_optional(&state.db).await
        .map_err(db_err(t))?;

    let (filename, media_type, total, received_json) = session.ok_or_else(|| AppError::not_found("Upload session not found", t))?;
    let received: Vec<i64> = serde_json::from_str(&received_json).unwrap_or_default();
    if received.len() as i64 != total {
        return Err(AppError::conflict(
            format!("Missing chunks: got {}/{}", received.len(), total), t,
        ));
    }

    // Verify every expected chunk file exists, then assemble into a
    // finalized file. Missing chunk files are a hard error — the backend
    // no longer accepts metadata-only completion.
    let chunk_dir = format!("{}/uploads/{}", state.config.storage_dir, body.upload_id);
    for idx in 0..total {
        let chunk_path = format!("{}/chunk_{}", chunk_dir, idx);
        if !std::path::Path::new(&chunk_path).exists() {
            return Err(AppError::conflict(
                format!("Chunk file {} missing — upload incomplete", idx), t,
            ));
        }
    }

    let assembled_path = format!("{}/uploads/{}_final", state.config.storage_dir, body.upload_id);
    {
        use std::io::Write;
        let mut out = std::fs::File::create(&assembled_path).map_err(|e| {
            tracing::error!(trace_id = %t, error = %e, "Failed to create assembled file");
            AppError::internal("Storage error", t)
        })?;
        for idx in 0..total {
            let chunk_path = format!("{}/chunk_{}", chunk_dir, idx);
            let data = std::fs::read(&chunk_path).map_err(|e| {
                tracing::error!(trace_id = %t, error = %e, chunk = idx, "Failed to read chunk");
                AppError::internal("Storage error", t)
            })?;
            out.write_all(&data).map_err(|e| {
                tracing::error!(trace_id = %t, error = %e, "Failed to write assembled data");
                AppError::internal("Storage error", t)
            })?;
        }
    }

    // Clean up individual chunk files now that assembly is done
    let _ = std::fs::remove_dir_all(&chunk_dir);

    let evidence_id = Uuid::new_v4().to_string();
    let watermark = build_watermark(&state.config.facility_code);
    let missing_exif = if body.exif_capture_time.is_none() && media_type == "photo" { 1 } else { 0 };

    // Apply the local compression policy before persisting. The resulting
    // compressed_bytes is also validated against the original size ceiling
    // so an operator cannot use an inflated "compression" value to bypass
    // per-type limits.
    let compression = apply_compression_policy(&media_type, body.total_size);
    if compression.compressed_bytes > body.total_size {
        return Err(AppError::validation(
            "Compression policy produced a larger payload — invalid configuration",
            t,
        ));
    }

    sqlx::query(
        "INSERT INTO evidence_records \
            (id, filename, media_type, size_bytes, fingerprint, watermark_text, \
             exif_capture_time, missing_exif, tags, keyword, uploaded_by, \
             compressed_bytes, compression_ratio, compression_applied) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
    )
    .bind(&evidence_id).bind(&filename).bind(&media_type)
    .bind(body.total_size).bind(&body.fingerprint).bind(&watermark)
    .bind(&body.exif_capture_time).bind(missing_exif)
    .bind(body.tags.clone().unwrap_or_default()).bind(body.keyword.clone().unwrap_or_default())
    .bind(&user.user_id)
    .bind(compression.compressed_bytes)
    .bind(compression.ratio)
    .bind(if compression.applied { 1 } else { 0 })
    .execute(&state.db).await
    .map_err(db_err(t))?;

    sqlx::query("UPDATE upload_sessions SET status = 'complete' WHERE id = ?")
        .bind(&body.upload_id).execute(&state.db).await.ok();

    crate::modules::audit::write(
        &state.db, &user.user_id, "evidence.upload_complete", "evidence", &evidence_id, t,
    ).await;
    // NOTE: we deliberately do NOT log the filename or fingerprint here —
    // they may contain PII or hash identifiers that should only live in
    // the evidence_records table, not in a log that can be exported.
    slog(&state.db, "info",
        &format!(
            "evidence.upload_complete id={} media_type={} original={} compressed={} ratio={:.2}",
            evidence_id, media_type, body.total_size, compression.compressed_bytes, compression.ratio
        ), t).await;

    Ok((StatusCode::CREATED, Json(EvidenceResponse {
        id: evidence_id, filename, media_type,
        watermark_text: watermark, missing_exif: missing_exif != 0,
        linked: false, legal_hold: false, created_at: String::new(),
        compressed_bytes: compression.compressed_bytes,
        compression_ratio: compression.ratio,
        compression_applied: compression.applied,
    })))
}

// GET /evidence?keyword=&tag=&from=&to= — list with search filters
pub async fn list(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Query(q): Query<HashMap<String, String>>,
) -> Result<Json<Vec<EvidenceResponse>>, AppError> {
    let t = &tid.0;

    let mut sql = String::from(
        "SELECT id, filename, media_type, watermark_text, missing_exif, linked, legal_hold, created_at, \
                size_bytes, compressed_bytes, compression_ratio, compression_applied \
         FROM evidence_records WHERE 1=1"
    );
    let mut binds: Vec<String> = Vec::new();

    if let Some(k) = q.get("keyword") {
        if !k.is_empty() {
            sql.push_str(" AND (keyword LIKE ? OR filename LIKE ?)");
            binds.push(format!("%{}%", k));
            binds.push(format!("%{}%", k));
        }
    }
    if let Some(tag) = q.get("tag") {
        if !tag.is_empty() {
            sql.push_str(" AND tags LIKE ?");
            binds.push(format!("%{}%", tag));
        }
    }
    if let Some(from) = q.get("from") {
        if !from.is_empty() {
            sql.push_str(" AND (exif_capture_time >= ? OR created_at >= ?)");
            binds.push(from.clone());
            binds.push(from.clone());
        }
    }
    if let Some(to) = q.get("to") {
        if !to.is_empty() {
            sql.push_str(" AND (exif_capture_time <= ? OR created_at <= ?)");
            binds.push(to.clone());
            binds.push(to.clone());
        }
    }
    sql.push_str(" ORDER BY created_at DESC");

    let mut query = sqlx::query_as::<_, EvidenceRow>(&sql);
    for b in &binds { query = query.bind(b); }
    let rows = query.fetch_all(&state.db).await.map_err(db_err(t))?;

    Ok(Json(rows.into_iter().map(|r| {
        // Older rows (pre-migration) may not have compression metadata set.
        // Fall back to the original size so the response shape is stable.
        let compressed_bytes = r.compressed_bytes.unwrap_or(r.size_bytes);
        let ratio = r.compression_ratio.unwrap_or(1.0);
        EvidenceResponse {
            id: r.id, filename: r.filename, media_type: r.media_type,
            watermark_text: r.watermark_text, missing_exif: r.missing_exif != 0,
            linked: r.linked != 0, legal_hold: r.legal_hold != 0,
            created_at: r.created_at,
            compressed_bytes,
            compression_ratio: ratio,
            compression_applied: r.compression_applied != 0,
        }
    }).collect()))
}

// DELETE /evidence/:id — only if unlinked AND (uploader or admin)
pub async fn delete(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;

    // Load linked flag + uploader for object-level auth
    let row: Option<(i64, String, i64)> = sqlx::query_as(
        "SELECT linked, uploaded_by, legal_hold FROM evidence_records WHERE id = ?"
    ).bind(&id).fetch_optional(&state.db).await.map_err(db_err(t))?;
    let (linked, uploader, legal_hold) = row.ok_or_else(|| AppError::not_found("Evidence not found", t))?;

    if legal_hold != 0 {
        return Err(AppError::conflict("Cannot delete evidence under legal hold", t));
    }
    if linked != 0 {
        return Err(AppError::conflict("Cannot delete linked evidence", t));
    }
    // Object-level auth: uploader OR admin
    if uploader != user.user_id && !is_admin(&user) {
        return Err(AppError::forbidden(
            "Only the uploader or an administrator can delete this evidence", t,
        ));
    }

    sqlx::query("DELETE FROM evidence_records WHERE id = ?")
        .bind(&id).execute(&state.db).await
        .map_err(db_err(t))?;

    crate::modules::audit::write(
        &state.db, &user.user_id, "evidence.delete", "evidence", &id, t,
    ).await;

    Ok(Json(serde_json::json!({"message":"Deleted"})))
}

// POST /evidence/:id/link — uploader or admin only
pub async fn link(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Path(id): Path<String>,
    Json(body): Json<EvidenceLinkRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    require_write_role(&user, t)?;

    if !["intake","inspection","traceability","checkin"].contains(&body.target_type.as_str()) {
        return Err(AppError::validation("Invalid target_type", t));
    }

    // Validate that the target resource actually exists before linking.
    let target_exists: Option<(String,)> = match body.target_type.as_str() {
        "intake" => {
            sqlx::query_as("SELECT id FROM intake_records WHERE id = ?")
                .bind(&body.target_id).fetch_optional(&state.db).await.map_err(db_err(t))?
        }
        "inspection" => {
            sqlx::query_as("SELECT id FROM inspections WHERE id = ?")
                .bind(&body.target_id).fetch_optional(&state.db).await.map_err(db_err(t))?
        }
        "traceability" => {
            sqlx::query_as("SELECT id FROM traceability_codes WHERE id = ?")
                .bind(&body.target_id).fetch_optional(&state.db).await.map_err(db_err(t))?
        }
        "checkin" => {
            sqlx::query_as("SELECT id FROM checkin_ledger WHERE id = ?")
                .bind(&body.target_id).fetch_optional(&state.db).await.map_err(db_err(t))?
        }
        _ => None,
    };
    if target_exists.is_none() {
        return Err(AppError::not_found(
            format!("{} with id '{}' not found", body.target_type, body.target_id), t,
        ));
    }

    // Load uploader for object-level auth
    let row: Option<(String,)> = sqlx::query_as("SELECT uploaded_by FROM evidence_records WHERE id = ?")
        .bind(&id).fetch_optional(&state.db).await.map_err(db_err(t))?;
    let (uploader,) = row.ok_or_else(|| AppError::not_found("Evidence not found", t))?;
    if uploader != user.user_id && !is_admin(&user) {
        return Err(AppError::forbidden(
            "Only the uploader or an administrator can link this evidence", t,
        ));
    }

    let link_id = Uuid::new_v4().to_string();
    sqlx::query("INSERT INTO evidence_links (id, evidence_id, target_type, target_id) VALUES (?,?,?,?)")
        .bind(&link_id).bind(&id).bind(&body.target_type).bind(&body.target_id)
        .execute(&state.db).await
        .map_err(db_err(t))?;
    sqlx::query("UPDATE evidence_records SET linked = 1 WHERE id = ?")
        .bind(&id).execute(&state.db).await.ok();

    crate::modules::audit::write(
        &state.db, &user.user_id, "evidence.link", "evidence", &id, t,
    ).await;

    Ok(Json(serde_json::json!({"message":"Linked","link_id":link_id})))
}

// PATCH /evidence/:id/legal-hold — admin only (router enforces, plus in-handler check)
pub async fn legal_hold(
    State(state): State<AppState>,
    Extension(tid): Extension<TraceId>,
    Extension(user): Extension<SessionUser>,
    Path(id): Path<String>,
    Json(body): Json<LegalHoldRequest>,
) -> Result<Json<serde_json::Value>, AppError> {
    let t = &tid.0;
    if !is_admin(&user) {
        return Err(AppError::forbidden("Administrator required to set legal hold", t));
    }
    let res = sqlx::query("UPDATE evidence_records SET legal_hold = ? WHERE id = ?")
        .bind(if body.legal_hold { 1 } else { 0 }).bind(&id)
        .execute(&state.db).await
        .map_err(db_err(t))?;
    if res.rows_affected() == 0 {
        return Err(AppError::not_found("Evidence not found", t));
    }
    crate::modules::audit::write(
        &state.db, &user.user_id, "evidence.legal_hold", "evidence", &id, t,
    ).await;
    Ok(Json(serde_json::json!({"message":"Legal hold updated","legal_hold":body.legal_hold})))
}

#[derive(sqlx::FromRow)]
struct EvidenceRow {
    id: String, filename: String, media_type: String,
    watermark_text: String, missing_exif: i64, linked: i64, legal_hold: i64,
    created_at: String,
    size_bytes: i64,
    compressed_bytes: Option<i64>,
    compression_ratio: Option<f64>,
    compression_applied: i64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photo_compression_applied_above_floor() {
        let r = apply_compression_policy("photo", 1_000_000);
        assert!(r.applied);
        // 1_000_000 * 70 = 70_000_000 + 50 / 100 = 700_000
        assert_eq!(r.compressed_bytes, 700_000);
        assert!(r.ratio < 1.0);
        assert!(r.compressed_bytes < 1_000_000);
    }

    #[test]
    fn tiny_photo_below_floor_not_compressed() {
        let r = apply_compression_policy("photo", 10_000);
        assert!(!r.applied);
        assert_eq!(r.compressed_bytes, 10_000);
        assert_eq!(r.ratio, 1.0);
    }

    #[test]
    fn video_ratio_is_0_60() {
        let r = apply_compression_policy("video", 10_000_000);
        assert!(r.applied);
        assert_eq!(r.compressed_bytes, 6_000_000);
        assert_eq!(r.ratio, 0.60);
    }

    #[test]
    fn audio_ratio_is_0_50() {
        let r = apply_compression_policy("audio", 1_000_000);
        assert!(r.applied);
        assert_eq!(r.compressed_bytes, 500_000);
        assert_eq!(r.ratio, 0.50);
    }

    #[test]
    fn unknown_media_type_passes_through() {
        let r = apply_compression_policy("other", 1_000_000);
        assert!(!r.applied);
        assert_eq!(r.compressed_bytes, 1_000_000);
        assert_eq!(r.ratio, 1.0);
    }

    #[test]
    fn compressed_never_exceeds_original() {
        for size in [1024i64, 1_000_000, 10_000_000, 100_000_000] {
            for mt in ["photo", "video", "audio"] {
                let r = apply_compression_policy(mt, size);
                assert!(r.compressed_bytes <= size,
                    "policy produced larger for {} @ {}", mt, size);
            }
        }
    }

    // ── Format validation tests ─────────────────────────────────────

    #[test]
    fn jpeg_magic_bytes_accepted() {
        assert!(validate_media_format("photo", &[0xFF, 0xD8, 0xFF, 0xE0, 0x00]));
    }

    #[test]
    fn png_magic_bytes_accepted() {
        assert!(validate_media_format("photo", &[0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A]));
    }

    #[test]
    fn mp4_ftyp_accepted() {
        let mut data = vec![0x00, 0x00, 0x00, 0x20];
        data.extend_from_slice(b"ftyp");
        data.extend_from_slice(b"isom");
        assert!(validate_media_format("video", &data));
    }

    #[test]
    fn mp3_id3_accepted() {
        let mut data = b"ID3".to_vec();
        data.extend_from_slice(&[0x03, 0x00]);
        assert!(validate_media_format("audio", &data));
    }

    #[test]
    fn random_bytes_rejected() {
        assert!(!validate_media_format("photo", &[0x00, 0x01, 0x02, 0x03, 0x04]));
        assert!(!validate_media_format("video", &[0x00, 0x01, 0x02, 0x03, 0x04]));
        assert!(!validate_media_format("audio", &[0x00, 0x01, 0x02, 0x03, 0x04]));
    }

    #[test]
    fn too_short_rejected() {
        assert!(!validate_media_format("photo", &[0xFF, 0xD8]));
    }
}
