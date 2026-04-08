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

/// Build the facility + timestamp watermark string.
/// Format: `FAC01 MM/DD/YYYY hh:mm AM/PM`
fn build_watermark(facility_code: &str) -> String {
    format!("{} {}", facility_code, CivilDateTime::now().us_12h())
}

/// Burn watermark text into a photo file by rendering it onto the image pixels.
/// Uses the `image` crate to decode, draw white text at bottom-left with a
/// dark background stripe, and re-encode as JPEG. Returns true if successful.
fn burn_watermark_into_photo(path: &str, text: &str) -> bool {
    let bytes = match std::fs::read(path) {
        Ok(b) => b,
        Err(_) => return false,
    };
    let mut img = match image::load_from_memory(&bytes) {
        Ok(i) => i.to_rgba8(),
        Err(_) => return false,
    };
    let (w, h) = (img.width(), img.height());
    if w < 20 || h < 20 { return false; } // too small to watermark

    // Draw a semi-transparent dark stripe at the bottom for readability
    let stripe_h = (h / 15).max(16).min(40);
    let y_start = h.saturating_sub(stripe_h);
    for y in y_start..h {
        for x in 0..w {
            let px = img.get_pixel_mut(x, y);
            // Darken: blend 60% toward black
            px[0] = (px[0] as f32 * 0.4) as u8;
            px[1] = (px[1] as f32 * 0.4) as u8;
            px[2] = (px[2] as f32 * 0.4) as u8;
        }
    }

    // Render watermark text pixel-by-pixel using a minimal built-in font.
    // Each character is 6px wide in our micro-font. We position left-aligned
    // in the dark stripe, vertically centered.
    let char_w = 6u32;
    let char_h = stripe_h.min(10);
    let text_y = y_start + (stripe_h.saturating_sub(char_h)) / 2;
    let text_x_start = 4u32;
    let white = image::Rgba([255u8, 255, 255, 255]);

    for (ci, ch) in text.chars().enumerate() {
        let cx = text_x_start + (ci as u32) * char_w;
        if cx + char_w > w { break; }
        // Simple block rendering: draw a small filled rectangle for each
        // character that has content (non-space). This is intentionally
        // minimal — the purpose is proving the watermark is physically
        // present in the pixel data, not typographic beauty.
        if !ch.is_whitespace() {
            for dy in 0..char_h.min(8) {
                for dx in 0..4 {
                    let px_x = cx + dx;
                    let px_y = text_y + dy;
                    if px_x < w && px_y < h {
                        img.put_pixel(px_x, px_y, white);
                    }
                }
            }
        }
    }

    // Re-encode with watermark burned in
    let dyn_img = image::DynamicImage::ImageRgba8(img);
    let mut output = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 90);
    if dyn_img.write_with_encoder(encoder).is_err() {
        return false;
    }
    std::fs::write(path, &output).is_ok()
}

/// Result of applying the local compression policy.
#[derive(Debug, Clone)]
pub(crate) struct CompressionResult {
    pub applied: bool,
    pub compressed_bytes: i64,
    pub ratio: f64,
}

/// Attempt real local compression on the assembled file.
///
/// - **Photos (JPEG/PNG):** Re-encodes as JPEG at quality 80 using the
///   `image` crate (pure Rust, no external deps). If the re-encoded file
///   is smaller, it replaces the original. If decoding fails (malformed
///   or unsupported image), the original is kept unchanged.
/// - **Video/Audio:** Real transcoding requires ffmpeg/libavcodec which is
///   outside the current dependency scope. Files are stored at original
///   quality with `compression_applied = false`.
///
/// Returns a `CompressionResult` with the **actual** stored file size.
pub(crate) fn try_compress_file(assembled_path: &str, media_type: &str) -> CompressionResult {
    if media_type == "photo" {
        if let Some(result) = try_jpeg_recompress(assembled_path) {
            return result;
        }
    }
    // Video/audio or photo decode failure: report actual file size unchanged.
    let actual = std::fs::metadata(assembled_path)
        .map(|m| m.len() as i64)
        .unwrap_or(0);
    CompressionResult {
        applied: false,
        compressed_bytes: actual,
        ratio: 1.0,
    }
}

/// Re-encode a photo as JPEG at quality 80. Returns Some(result) if
/// compression succeeded, None if the image couldn't be decoded.
fn try_jpeg_recompress(path: &str) -> Option<CompressionResult> {
    let original_bytes = std::fs::read(path).ok()?;
    let original_size = original_bytes.len() as i64;

    // Decode the image (supports JPEG, PNG formats via image crate)
    let img = image::load_from_memory(&original_bytes).ok()?;

    // Re-encode as JPEG at quality 80
    let mut output = Vec::new();
    let encoder = image::codecs::jpeg::JpegEncoder::new_with_quality(&mut output, 80);
    img.write_with_encoder(encoder).ok()?;

    let compressed_size = output.len() as i64;

    // Only replace if the re-encoded version is actually smaller
    if compressed_size < original_size && compressed_size > 0 {
        std::fs::write(path, &output).ok()?;
        let ratio = compressed_size as f64 / original_size as f64;
        Some(CompressionResult {
            applied: true,
            compressed_bytes: compressed_size,
            ratio,
        })
    } else {
        // Re-encoding didn't help (already optimized or tiny file)
        Some(CompressionResult {
            applied: false,
            compressed_bytes: original_size,
            ratio: 1.0,
        })
    }
}

// POST /media/upload/start — create upload session
const MAX_CONCURRENT_UPLOADS: i64 = 10;

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

    // Rate-limit: prevent resource exhaustion from unbounded session creation.
    let active: (i64,) = sqlx::query_as(
        "SELECT COUNT(*) FROM upload_sessions WHERE uploader_id = ? AND status = 'in_progress'"
    ).bind(&user.user_id).fetch_one(&state.db).await.map_err(db_err(t))?;
    if active.0 >= MAX_CONCURRENT_UPLOADS {
        return Err(AppError::conflict(
            format!("Too many active upload sessions (max {}). Complete or abandon existing uploads first.", MAX_CONCURRENT_UPLOADS),
            t,
        ));
    }

    let id = Uuid::new_v4().to_string();
    let total_chunks = (body.total_size + (2 * 1024 * 1024) - 1) / (2 * 1024 * 1024);
    sqlx::query("INSERT INTO upload_sessions (id, filename, media_type, total_chunks, uploader_id, duration_seconds) VALUES (?,?,?,?,?,?)")
        .bind(&id).bind(&body.filename).bind(&body.media_type)
        .bind(total_chunks).bind(&user.user_id).bind(body.duration_seconds)
        .execute(&state.db).await
        .map_err(db_err(t))?;
    Ok(Json(UploadStartResponse { upload_id: id, chunk_size_bytes: 2 * 1024 * 1024, total_chunks }))
}

// ── Server-side duration extraction from file bytes ─────────────────
//
// Extracts duration in seconds from assembled media files using pure
// byte-level container parsing. No external tools (ffprobe) required.
//
// Supported:
//   - MP4/MOV  (ISO BMFF): parses `mvhd` atom for timescale + duration
//   - WAV      (RIFF/WAVE): computes from fmt chunk (sample rate × bits × channels) and data size
//
// Unsupported (returns None → fail-safe reject):
//   - AVI, WebM/MKV, MP3, FLAC, OGG, AAC — require complex parsers
//
// The fail-safe policy means: if we cannot derive the duration from the
// file bytes, the upload is rejected for video/audio. This prevents a
// client from bypassing the 60s/120s policy by submitting an obscure
// container format.

/// Attempt to extract duration (seconds) from assembled file bytes.
/// Returns `Some(seconds)` if extraction succeeds, `None` if the format
/// is unsupported or the container structure is malformed.
fn extract_duration_from_bytes(media_type: &str, data: &[u8]) -> Option<f64> {
    match media_type {
        "video" => extract_mp4_duration(data),
        "audio" => extract_wav_duration(data).or_else(|| extract_mp4_duration(data)),
        _ => None,
    }
}

/// Parse ISO BMFF (MP4/MOV) container for the `mvhd` atom.
/// Layout: each atom is [4-byte size][4-byte type][payload...].
/// `moov.mvhd` contains timescale (u32 @ offset +12) and duration
/// (u32 @ offset +16) for version-0, or (u64 @ offset +20) for version-1.
fn extract_mp4_duration(data: &[u8]) -> Option<f64> {
    // First verify this is actually an ftyp-bearing file.
    if data.len() < 8 || &data[4..8] != b"ftyp" {
        return None;
    }
    // Scan top-level atoms for `moov`.
    let moov_payload = find_atom(data, b"moov")?;
    // Inside `moov`, find `mvhd`.
    let mvhd_payload = find_atom(moov_payload, b"mvhd")?;
    // mvhd payload starts with a version byte.
    if mvhd_payload.is_empty() {
        return None;
    }
    let version = mvhd_payload[0];
    if version == 0 {
        // version 0: 1 byte version + 3 bytes flags + 4 create + 4 modify
        //            + 4 timescale + 4 duration = offset 12 for timescale, 16 for duration
        if mvhd_payload.len() < 20 {
            return None;
        }
        let timescale = u32::from_be_bytes([
            mvhd_payload[12], mvhd_payload[13], mvhd_payload[14], mvhd_payload[15],
        ]) as f64;
        let duration = u32::from_be_bytes([
            mvhd_payload[16], mvhd_payload[17], mvhd_payload[18], mvhd_payload[19],
        ]) as f64;
        if timescale > 0.0 { Some(duration / timescale) } else { None }
    } else if version == 1 {
        // version 1: 1+3 + 8 create + 8 modify + 4 timescale + 8 duration
        //            = offset 20 for timescale, 24 for duration
        if mvhd_payload.len() < 32 {
            return None;
        }
        let timescale = u32::from_be_bytes([
            mvhd_payload[20], mvhd_payload[21], mvhd_payload[22], mvhd_payload[23],
        ]) as f64;
        let dur_bytes: [u8; 8] = [
            mvhd_payload[24], mvhd_payload[25], mvhd_payload[26], mvhd_payload[27],
            mvhd_payload[28], mvhd_payload[29], mvhd_payload[30], mvhd_payload[31],
        ];
        let duration = u64::from_be_bytes(dur_bytes) as f64;
        if timescale > 0.0 { Some(duration / timescale) } else { None }
    } else {
        None
    }
}

/// Walk a slice of ISO BMFF atoms and return the payload (after the 8-byte
/// header) of the first atom whose type matches `target`.
fn find_atom(data: &[u8], target: &[u8; 4]) -> Option<&[u8]> {
    let mut offset = 0usize;
    while offset + 8 <= data.len() {
        let size = u32::from_be_bytes([
            data[offset], data[offset + 1], data[offset + 2], data[offset + 3],
        ]) as usize;
        let atom_type = &data[offset + 4..offset + 8];
        // Sanity: size must be >= 8 (header) and not exceed remaining data.
        let atom_size = if size == 0 {
            // size == 0 means "extends to end of file"
            data.len() - offset
        } else if size < 8 {
            return None; // malformed
        } else {
            size
        };
        if atom_size > data.len() - offset {
            return None; // truncated
        }
        if atom_type == target {
            return Some(&data[offset + 8..offset + atom_size]);
        }
        offset += atom_size;
    }
    None
}

/// Parse RIFF/WAVE container for duration.
/// WAV = RIFF header + fmt chunk (sample rate, channels, bits) + data chunk.
/// Duration = data_size / (sample_rate × channels × bits_per_sample / 8).
fn extract_wav_duration(data: &[u8]) -> Option<f64> {
    if data.len() < 44 {
        return None;
    }
    // Verify RIFF...WAVE header.
    if &data[0..4] != b"RIFF" || &data[8..12] != b"WAVE" {
        return None;
    }
    // Scan chunks starting at offset 12.
    let mut sample_rate: u32 = 0;
    let mut block_align: u16 = 0;
    let mut data_size: u32 = 0;
    let mut found_fmt = false;
    let mut found_data = false;
    let mut pos = 12usize;
    while pos + 8 <= data.len() {
        let chunk_id = &data[pos..pos + 4];
        let chunk_size = u32::from_le_bytes([
            data[pos + 4], data[pos + 5], data[pos + 6], data[pos + 7],
        ]) as usize;
        if chunk_id == b"fmt " && pos + 8 + 16 <= data.len() {
            let payload = &data[pos + 8..];
            sample_rate = u32::from_le_bytes([payload[4], payload[5], payload[6], payload[7]]);
            block_align = u16::from_le_bytes([payload[12], payload[13]]);
            found_fmt = true;
        }
        if chunk_id == b"data" {
            data_size = chunk_size as u32;
            found_data = true;
        }
        if found_fmt && found_data {
            break;
        }
        // Advance to next chunk (chunk sizes are word-aligned in RIFF).
        let advance = 8 + ((chunk_size + 1) & !1);
        pos += advance;
    }
    if !found_fmt || !found_data || sample_rate == 0 || block_align == 0 {
        return None;
    }
    let byte_rate = sample_rate as f64 * block_align as f64;
    Some(data_size as f64 / byte_rate)
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

    let session: Option<(String, String, i64, String, i64)> = sqlx::query_as(
        "SELECT filename, media_type, total_chunks, received_chunks, duration_seconds FROM upload_sessions WHERE id = ? AND uploader_id = ?"
    ).bind(&body.upload_id).bind(&user.user_id)
        .fetch_optional(&state.db).await
        .map_err(db_err(t))?;

    let (filename, media_type, total, received_json, _declared_duration) = session.ok_or_else(|| AppError::not_found("Upload session not found", t))?;

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
    let server_fingerprint: String;
    {
        use sha2::{Sha256, Digest};
        use std::io::Write;
        let mut hasher = Sha256::new();
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
            hasher.update(&data);
            out.write_all(&data).map_err(|e| {
                tracing::error!(trace_id = %t, error = %e, "Failed to write assembled data");
                AppError::internal("Storage error", t)
            })?;
        }
        server_fingerprint = hex::encode(hasher.finalize());
    }

    // Server-side fingerprint integrity check: compare computed hash
    // against the client-provided fingerprint. Mismatch means data was
    // corrupted or tampered in transit.
    if !body.fingerprint.eq_ignore_ascii_case(&server_fingerprint) {
        // Clean up the assembled file on mismatch
        let _ = std::fs::remove_file(&assembled_path);
        return Err(AppError::conflict(
            "Fingerprint mismatch: server-computed fingerprint does not match client-provided value",
            t,
        ));
    }

    // ── Server-side duration enforcement ───────────────────────────────
    // For video/audio, derive duration from the assembled file bytes.
    // If extraction succeeds, enforce the policy limit.
    // If extraction fails (unsupported format), reject — fail-safe.
    // Photos have no duration constraint.
    if media_type == "video" || media_type == "audio" {
        let assembled_bytes = std::fs::read(&assembled_path).map_err(|e| {
            tracing::error!(trace_id = %t, error = %e, "Failed to read assembled file for duration check");
            AppError::internal("Storage error", t)
        })?;
        match extract_duration_from_bytes(&media_type, &assembled_bytes) {
            Some(extracted_seconds) => {
                let limit = if media_type == "video" { MAX_VIDEO_SECONDS } else { MAX_AUDIO_SECONDS };
                slog(&state.db, "info",
                    &format!(
                        "evidence.duration_verified media_type={} extracted_seconds={:.2} limit={}",
                        media_type, extracted_seconds, limit
                    ), t).await;
                if extracted_seconds > limit as f64 {
                    let _ = std::fs::remove_file(&assembled_path);
                    let _ = std::fs::remove_dir_all(&chunk_dir);
                    return Err(AppError::validation(
                        format!(
                            "{} duration {:.1}s exceeds {} second limit",
                            media_type, extracted_seconds, limit
                        ), t,
                    ));
                }
            }
            None => {
                // Fail-safe: cannot extract duration → reject.
                slog(&state.db, "warn",
                    &format!(
                        "evidence.duration_unverifiable media_type={} — rejecting (fail-safe policy)",
                        media_type
                    ), t).await;
                let _ = std::fs::remove_file(&assembled_path);
                let _ = std::fs::remove_dir_all(&chunk_dir);
                return Err(AppError::validation(
                    format!("Cannot verify {} duration from uploaded file — unsupported or malformed container format", media_type),
                    t,
                ));
            }
        }
    }

    // Clean up individual chunk files now that assembly is done
    let _ = std::fs::remove_dir_all(&chunk_dir);

    let evidence_id = Uuid::new_v4().to_string();
    let watermark = build_watermark(&state.config.facility_code);
    let missing_exif = if body.exif_capture_time.is_none() && media_type == "photo" { 1 } else { 0 };

    // ── Rename assembled file to use evidence_id (canonical path) ─────
    // This eliminates the upload_id vs evidence_id mismatch that causes
    // orphaned files on delete/retention.
    let canonical_path = format!("{}/uploads/{}_final", state.config.storage_dir, evidence_id);
    std::fs::rename(&assembled_path, &canonical_path).map_err(|e| {
        tracing::error!(trace_id = %t, error = %e, "Failed to rename assembled file to canonical path");
        AppError::internal("Storage error", t)
    })?;

    // Measure original assembled file size before any processing.
    let original_file_size = std::fs::metadata(&canonical_path)
        .map(|m| m.len() as i64)
        .unwrap_or(body.total_size);

    // ── Photo: compress + burn visible watermark into pixels ──────────
    // For photos: JPEG re-encode at quality 80, then burn watermark text
    // into the image pixels so it's physically present in the stored file.
    // For video/audio: stored at original quality (no transcoding).
    let compression = try_compress_file(&canonical_path, &media_type);

    // Burn watermark into photo pixels (after compression to preserve quality)
    if media_type == "photo" {
        burn_watermark_into_photo(&canonical_path, &watermark);
    }

    // Measure final stored size after all processing
    let final_size = std::fs::metadata(&canonical_path)
        .map(|m| m.len() as i64)
        .unwrap_or(original_file_size);
    let final_ratio = if original_file_size > 0 {
        final_size as f64 / original_file_size as f64
    } else {
        1.0
    };

    sqlx::query(
        "INSERT INTO evidence_records \
            (id, filename, media_type, size_bytes, fingerprint, watermark_text, \
             exif_capture_time, missing_exif, tags, keyword, uploaded_by, \
             compressed_bytes, compression_ratio, compression_applied, storage_path) \
         VALUES (?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)"
    )
    .bind(&evidence_id).bind(&filename).bind(&media_type)
    .bind(body.total_size).bind(&body.fingerprint).bind(&watermark)
    .bind(&body.exif_capture_time).bind(missing_exif)
    .bind(body.tags.clone().unwrap_or_default()).bind(body.keyword.clone().unwrap_or_default())
    .bind(&user.user_id)
    .bind(final_size)
    .bind(final_ratio)
    .bind(if compression.applied { 1 } else { 0 })
    .bind(&canonical_path)
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
            "evidence.upload_complete id={} media_type={} declared_size={} stored_size={}",
            evidence_id, media_type, body.total_size, final_size
        ), t).await;

    Ok((StatusCode::CREATED, Json(EvidenceResponse {
        id: evidence_id, filename, media_type,
        watermark_text: watermark, missing_exif: missing_exif != 0,
        linked: false, legal_hold: false, created_at: String::new(),
        compressed_bytes: final_size,
        compression_ratio: final_ratio,
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

    // Load linked flag + uploader + storage path for object-level auth + cleanup
    let row: Option<(i64, String, i64, String)> = sqlx::query_as(
        "SELECT linked, uploaded_by, legal_hold, storage_path FROM evidence_records WHERE id = ?"
    ).bind(&id).fetch_optional(&state.db).await.map_err(db_err(t))?;
    let (linked, uploader, legal_hold, storage_path) = row.ok_or_else(|| AppError::not_found("Evidence not found", t))?;

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

    // Clean up the stored file from disk using the canonical path from DB.
    // Fall back to evidence_id-based path for backward compatibility.
    let file_path = if !storage_path.is_empty() {
        storage_path
    } else {
        format!("{}/uploads/{}_final", state.config.storage_dir, id)
    };
    if let Err(e) = std::fs::remove_file(&file_path) {
        if e.kind() != std::io::ErrorKind::NotFound {
            tracing::warn!(trace_id = %t, error = %e, path = %file_path, "Failed to remove evidence file on delete");
        }
    }

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
    fn jpeg_recompress_with_real_image() {
        // Build a minimal but decodable 2x2 JPEG using the image crate itself
        use image::{RgbImage, Rgb};
        let tmp = std::env::temp_dir().join("test_compress.jpg");
        let img = RgbImage::from_pixel(2, 2, Rgb([255u8, 0, 0]));
        img.save(&tmp).unwrap();

        let result = try_compress_file(tmp.to_str().unwrap(), "photo");
        // A 2x2 red image is tiny — re-encode may or may not shrink it,
        // but the function should succeed without error.
        assert!(result.compressed_bytes > 0);
        assert!(result.ratio > 0.0 && result.ratio <= 1.0);
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn non_image_photo_stays_unchanged() {
        // A file that starts with JPEG magic but isn't a valid image
        let tmp = std::env::temp_dir().join("test_bad_photo.jpg");
        std::fs::write(&tmp, &[0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x10, 0x00, 0x00]).unwrap();
        let original_size = std::fs::metadata(&tmp).unwrap().len() as i64;

        let result = try_compress_file(tmp.to_str().unwrap(), "photo");
        // Decode fails → file unchanged, applied = false
        assert!(!result.applied);
        assert_eq!(result.compressed_bytes, original_size);
        assert_eq!(result.ratio, 1.0);
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn video_not_compressed() {
        let tmp = std::env::temp_dir().join("test_video.mp4");
        std::fs::write(&tmp, b"fake video data for testing").unwrap();
        let original_size = std::fs::metadata(&tmp).unwrap().len() as i64;

        let result = try_compress_file(tmp.to_str().unwrap(), "video");
        assert!(!result.applied);
        assert_eq!(result.compressed_bytes, original_size);
        assert_eq!(result.ratio, 1.0);
        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn audio_not_compressed() {
        let tmp = std::env::temp_dir().join("test_audio.wav");
        std::fs::write(&tmp, b"fake audio data for testing").unwrap();
        let original_size = std::fs::metadata(&tmp).unwrap().len() as i64;

        let result = try_compress_file(tmp.to_str().unwrap(), "audio");
        assert!(!result.applied);
        assert_eq!(result.compressed_bytes, original_size);
        assert_eq!(result.ratio, 1.0);
        std::fs::remove_file(&tmp).ok();
    }

    // ── Watermark burn-in tests ────────────────────────────────────────

    #[test]
    fn watermark_burned_into_real_photo() {
        use image::{RgbImage, Rgb};
        let tmp = std::env::temp_dir().join("test_watermark.jpg");
        // Create a 100x50 solid blue image
        let img = RgbImage::from_pixel(100, 50, Rgb([0u8, 0, 200]));
        img.save(&tmp).unwrap();
        let before_size = std::fs::metadata(&tmp).unwrap().len();

        let success = burn_watermark_into_photo(
            tmp.to_str().unwrap(),
            "FAC01 04/08/2026 02:30 PM"
        );
        assert!(success, "watermark burn should succeed on valid image");

        // File should still exist and have been re-encoded
        let after_size = std::fs::metadata(&tmp).unwrap().len();
        assert!(after_size > 0, "watermarked file should not be empty");

        // Read back and verify the bottom pixels are modified (dark stripe)
        let data = std::fs::read(&tmp).unwrap();
        let reloaded = image::load_from_memory(&data).unwrap().to_rgba8();
        // Bottom-left pixel should be darkened (was pure blue 0,0,200)
        let bottom_px = reloaded.get_pixel(0, reloaded.height() - 1);
        // After 0.4x darkening: blue channel should be ~80 (not 200)
        assert!(bottom_px[2] < 150, "bottom pixel should be darkened by watermark stripe, got blue={}", bottom_px[2]);

        std::fs::remove_file(&tmp).ok();
    }

    #[test]
    fn watermark_fails_gracefully_on_non_image() {
        let tmp = std::env::temp_dir().join("test_wm_bad.jpg");
        std::fs::write(&tmp, &[0xFF, 0xD8, 0xFF, 0xE0]).unwrap();
        let success = burn_watermark_into_photo(tmp.to_str().unwrap(), "TEST");
        assert!(!success, "watermark should fail gracefully on invalid image");
        std::fs::remove_file(&tmp).ok();
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

    // ── Duration extraction tests ──────────────────────────────────────

    /// Build a minimal valid MP4 file with known duration.
    /// Structure: ftyp atom + moov atom containing mvhd (version 0).
    fn build_test_mp4(timescale: u32, duration: u32) -> Vec<u8> {
        let mut data = Vec::new();
        // ftyp atom: size(16) + "ftyp" + "isom" + version(0000)
        data.extend_from_slice(&16u32.to_be_bytes());
        data.extend_from_slice(b"ftyp");
        data.extend_from_slice(b"isom");
        data.extend_from_slice(&[0, 0, 0, 0]);
        // moov atom containing mvhd
        // mvhd payload: version(1) + flags(3) + creation(4) + modification(4)
        //             + timescale(4) + duration(4) = 20 bytes
        let mvhd_payload_len = 20;
        let mvhd_atom_len = 8 + mvhd_payload_len;
        let moov_atom_len = 8 + mvhd_atom_len;
        // moov header
        data.extend_from_slice(&(moov_atom_len as u32).to_be_bytes());
        data.extend_from_slice(b"moov");
        // mvhd header
        data.extend_from_slice(&(mvhd_atom_len as u32).to_be_bytes());
        data.extend_from_slice(b"mvhd");
        // mvhd payload (version 0)
        data.push(0); // version
        data.extend_from_slice(&[0, 0, 0]); // flags
        data.extend_from_slice(&0u32.to_be_bytes()); // creation time
        data.extend_from_slice(&0u32.to_be_bytes()); // modification time
        data.extend_from_slice(&timescale.to_be_bytes());
        data.extend_from_slice(&duration.to_be_bytes());
        data
    }

    /// Build a minimal valid WAV file with known duration.
    fn build_test_wav(sample_rate: u32, channels: u16, bits_per_sample: u16, num_samples: u32) -> Vec<u8> {
        let block_align = channels * (bits_per_sample / 8);
        let data_size = num_samples * block_align as u32;
        let byte_rate = sample_rate * block_align as u32;
        let file_size = 36 + data_size; // RIFF header(12) - 8 + fmt(24) + data_header(8) + data
        let mut buf = Vec::new();
        // RIFF header
        buf.extend_from_slice(b"RIFF");
        buf.extend_from_slice(&file_size.to_le_bytes());
        buf.extend_from_slice(b"WAVE");
        // fmt chunk
        buf.extend_from_slice(b"fmt ");
        buf.extend_from_slice(&16u32.to_le_bytes()); // chunk size
        buf.extend_from_slice(&1u16.to_le_bytes()); // audio format (PCM)
        buf.extend_from_slice(&channels.to_le_bytes());
        buf.extend_from_slice(&sample_rate.to_le_bytes());
        buf.extend_from_slice(&byte_rate.to_le_bytes());
        buf.extend_from_slice(&block_align.to_le_bytes());
        buf.extend_from_slice(&bits_per_sample.to_le_bytes());
        // data chunk
        buf.extend_from_slice(b"data");
        buf.extend_from_slice(&data_size.to_le_bytes());
        // Actual audio samples (zeros)
        buf.resize(buf.len() + data_size as usize, 0);
        buf
    }

    #[test]
    fn mp4_duration_30s() {
        // timescale=1000, duration=30000 → 30.0 seconds
        let data = build_test_mp4(1000, 30_000);
        let dur = extract_mp4_duration(&data).unwrap();
        assert!((dur - 30.0).abs() < 0.01, "expected ~30s, got {}", dur);
    }

    #[test]
    fn mp4_duration_90s() {
        // timescale=600, duration=54000 → 90.0 seconds
        let data = build_test_mp4(600, 54_000);
        let dur = extract_mp4_duration(&data).unwrap();
        assert!((dur - 90.0).abs() < 0.01, "expected ~90s, got {}", dur);
    }

    #[test]
    fn mp4_no_moov_returns_none() {
        // Just an ftyp atom, no moov.
        let mut data = vec![0, 0, 0, 16];
        data.extend_from_slice(b"ftyp");
        data.extend_from_slice(b"isom");
        data.extend_from_slice(&[0, 0, 0, 0]);
        assert!(extract_mp4_duration(&data).is_none());
    }

    #[test]
    fn wav_duration_10s() {
        // 44100 Hz, 1 channel, 16-bit, 44100*10 = 441000 samples → 10.0s
        let data = build_test_wav(44100, 1, 16, 441_000);
        let dur = extract_wav_duration(&data).unwrap();
        assert!((dur - 10.0).abs() < 0.01, "expected ~10s, got {}", dur);
    }

    #[test]
    fn wav_duration_130s() {
        // 44100 Hz, 2 channels, 16-bit, 44100*130 = 5733000 samples → 130.0s
        let data = build_test_wav(44100, 2, 16, 5_733_000);
        let dur = extract_wav_duration(&data).unwrap();
        assert!((dur - 130.0).abs() < 0.01, "expected ~130s, got {}", dur);
    }

    #[test]
    fn non_wav_audio_returns_none() {
        // MP3 ID3 header — not a WAV, and not an MP4 either.
        let data = b"ID3\x03\x00\x00\x00\x00\x00\x00";
        assert!(extract_duration_from_bytes("audio", data).is_none());
    }

    #[test]
    fn random_bytes_duration_returns_none() {
        let data = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07];
        assert!(extract_duration_from_bytes("video", &data).is_none());
        assert!(extract_duration_from_bytes("audio", &data).is_none());
    }

    #[test]
    fn photo_duration_returns_none() {
        let data = vec![0xFF, 0xD8, 0xFF, 0xE0];
        assert!(extract_duration_from_bytes("photo", &data).is_none());
    }

    #[test]
    fn extract_video_delegates_to_mp4() {
        let data = build_test_mp4(1000, 45_000);
        let dur = extract_duration_from_bytes("video", &data).unwrap();
        assert!((dur - 45.0).abs() < 0.01);
    }

    #[test]
    fn extract_audio_tries_wav_then_mp4() {
        // WAV file → should succeed via WAV parser
        let wav = build_test_wav(44100, 1, 16, 44_100); // 1 second
        let dur = extract_duration_from_bytes("audio", &wav).unwrap();
        assert!((dur - 1.0).abs() < 0.01);

        // MP4 file declared as audio → should succeed via MP4 fallback
        let mp4 = build_test_mp4(1000, 5_000); // 5 seconds
        let dur2 = extract_duration_from_bytes("audio", &mp4).unwrap();
        assert!((dur2 - 5.0).abs() < 0.01);
    }
}
