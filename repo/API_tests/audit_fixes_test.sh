#!/bin/bash
# Audit fixes verification suite
# Tests: fingerprint integrity, duration enforcement, traceability steps
# visibility, privacy preferences, supply fields, cookie secure flag,
# adoption semantics, anti-passback override, key rotation, evidence metadata.
set -e

# Source shared helpers for session stability
SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
if [ -f "$SCRIPT_DIR/test_helpers.sh" ]; then
    source "$SCRIPT_DIR/test_helpers.sh"
fi

PASS=0; FAIL=0; BASE="http://localhost:8080"
DB="/app/storage/app.db"

# Use mktemp for cookie files if available, else fall back to /tmp
ADMIN_CK=$(mktemp /tmp/af_admin_XXXXXX 2>/dev/null || echo "/tmp/af_admin_$$")
STAFF_CK=$(mktemp /tmp/af_staff_XXXXXX 2>/dev/null || echo "/tmp/af_staff_$$")
AUDITOR_CK=$(mktemp /tmp/af_auditor_XXXXXX 2>/dev/null || echo "/tmp/af_auditor_$$")
trap 'rm -f "$ADMIN_CK" "$STAFF_CK" "$AUDITOR_CK"' EXIT

check() {
    local name="$1" expected="$2" actual="$3"
    if [ "$actual" = "$expected" ]; then
        echo "PASS: $name"; PASS=$((PASS+1))
    else
        echo "FAIL: $name (expected $expected, got $actual)"; FAIL=$((FAIL+1))
    fi
}

contains() {
    local name="$1" needle="$2" haystack="$3"
    if echo "$haystack" | grep -q "$needle"; then
        echo "PASS: $name"; PASS=$((PASS+1))
    else
        echo "FAIL: $name (expected to find '$needle')"; FAIL=$((FAIL+1))
    fi
}

not_contains() {
    local name="$1" needle="$2" haystack="$3"
    if echo "$haystack" | grep -q "$needle"; then
        echo "FAIL: $name (should NOT contain '$needle')"; FAIL=$((FAIL+1))
    else
        echo "PASS: $name"; PASS=$((PASS+1))
    fi
}

sql() {
    sqlite3 "$DB" "$1"
}

# Minimal JPEG header for chunk uploads
JPEG_B64=$(printf '\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00' | base64 -w0 2>/dev/null || printf '\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00' | base64 2>/dev/null)

echo "=== Audit Fixes Verification Suite ==="

# ─── Setup: ensure admin session (standalone-safe) ────────────────────
# Try login first (in case admin already exists from prior suite).
# If that fails, bootstrap register.
REG_CODE=$(curl -s -o /dev/null -w "%{http_code}" -c "$ADMIN_CK" -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" -d '{"username":"afadmin","password":"AuditFixAdm12"}')
if [ "$REG_CODE" != "200" ]; then
    curl -s -c "$ADMIN_CK" -X POST "$BASE/auth/register" -H "Content-Type: application/json" \
      -d '{"username":"afadmin","password":"AuditFixAdm12"}' > /dev/null
fi

curl -s -b "$ADMIN_CK" -X POST "$BASE/users" -H "Content-Type: application/json" \
  -d '{"username":"afstaff","password":"AuditFixStf12","role":"operations_staff"}' > /dev/null 2>&1
curl -s -b "$ADMIN_CK" -X POST "$BASE/users" -H "Content-Type: application/json" \
  -d '{"username":"afauditor","password":"AuditFixAud12","role":"auditor"}' > /dev/null 2>&1

curl -s -c "$STAFF_CK" -X POST "$BASE/auth/login" -H "Content-Type: application/json" \
  -d '{"username":"afstaff","password":"AuditFixStf12"}' > /dev/null
curl -s -c "$AUDITOR_CK" -X POST "$BASE/auth/login" -H "Content-Type: application/json" \
  -d '{"username":"afauditor","password":"AuditFixAud12"}' > /dev/null

# ═══════════════════════════════════════════════════════════════════════
# 1. FINGERPRINT INTEGRITY — server-side verification
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 1. Fingerprint integrity verification ━━━"

# Upload a photo and provide the CORRECT server-computed fingerprint
UBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"fp_test.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
UPLOAD_ID=$(echo "$UBODY" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)

curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UPLOAD_ID\",\"chunk_index\":0,\"data\":\"$JPEG_B64\"}" > /dev/null

# Compute the correct fingerprint from the JPEG data
JPEG_RAW=$(printf '\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00')
CORRECT_FP=$(echo -n "$JPEG_RAW" | sha256sum | cut -d' ' -f1)

# Happy path: correct fingerprint accepted
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" \
  -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UPLOAD_ID\",\"fingerprint\":\"$CORRECT_FP\",\"total_size\":1024}")
check "Correct fingerprint → 201" "201" "$R"

# Upload another file, provide WRONG fingerprint
UBODY2=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"fp_bad.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
UPLOAD_ID2=$(echo "$UBODY2" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)

curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UPLOAD_ID2\",\"chunk_index\":0,\"data\":\"$JPEG_B64\"}" > /dev/null

# Wrong fingerprint: must get 409 CONFLICT
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" \
  -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UPLOAD_ID2\",\"fingerprint\":\"0000000000000000000000000000000000000000000000000000000000000000\",\"total_size\":1024}")
check "Wrong fingerprint → 409 CONFLICT" "409" "$R"

RBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UPLOAD_ID2\",\"fingerprint\":\"deadbeefdeadbeefdeadbeefdeadbeef\",\"total_size\":1024}")
contains "Mismatch error contains 'Fingerprint mismatch'" "Fingerprint mismatch" "$RBODY"

# ═══════════════════════════════════════════════════════════════════════
# 2. SERVER-SIDE DURATION ENFORCEMENT (derived from file bytes)
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 2. Server-side duration enforcement ━━━"

# ── Helper: build a minimal valid MP4 binary with a known duration ────
# Structure: ftyp atom (16 bytes) + moov atom containing mvhd (version 0)
# mvhd fields: version(1) flags(3) create(4) modify(4) timescale(4) duration(4)
# Duration in seconds = duration_field / timescale
# Args: $1=timescale $2=duration_field
build_test_mp4() {
    local ts="$1" dur="$2"
    local tmp="/tmp/test_mp4_$$"
    # ftyp atom: size=16 type=ftyp brand=isom
    printf '\x00\x00\x00\x10ftypisom\x00\x00\x00\x00' > "$tmp"
    # moov atom: size=36 (8 header + 28 mvhd atom)
    printf '\x00\x00\x00\x24moov' >> "$tmp"
    # mvhd atom: size=28 (8 header + 20 payload)
    printf '\x00\x00\x00\x1cmvhd' >> "$tmp"
    # version=0, flags=000, creation=0, modification=0
    printf '\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00' >> "$tmp"
    # timescale (4 bytes big-endian)
    printf "$(printf '\\x%02x\\x%02x\\x%02x\\x%02x' \
      $(( (ts >> 24) & 0xFF )) $(( (ts >> 16) & 0xFF )) \
      $(( (ts >> 8) & 0xFF )) $(( ts & 0xFF )) )" >> "$tmp"
    # duration (4 bytes big-endian)
    printf "$(printf '\\x%02x\\x%02x\\x%02x\\x%02x' \
      $(( (dur >> 24) & 0xFF )) $(( (dur >> 16) & 0xFF )) \
      $(( (dur >> 8) & 0xFF )) $(( dur & 0xFF )) )" >> "$tmp"
    # base64 encode the whole thing
    base64 -w0 "$tmp" 2>/dev/null || base64 "$tmp"
    rm -f "$tmp"
}

# ── Helper: build a minimal valid WAV binary with a known duration ────
# Duration = data_chunk_size / (sample_rate × block_align)
# Args: $1=sample_rate $2=num_samples (mono 16-bit)
build_test_wav() {
    local sr="$1" ns="$2"
    local tmp="/tmp/test_wav_$$"
    local block_align=2   # 1 channel × 16-bit / 8
    local data_size=$((ns * block_align))
    local byte_rate=$((sr * block_align))
    local file_size=$((36 + data_size))
    # RIFF header
    printf 'RIFF' > "$tmp"
    printf "$(printf '\\x%02x\\x%02x\\x%02x\\x%02x' \
      $((file_size & 0xFF)) $(((file_size>>8) & 0xFF)) \
      $(((file_size>>16) & 0xFF)) $(((file_size>>24) & 0xFF)))" >> "$tmp"
    printf 'WAVE' >> "$tmp"
    # fmt chunk (16 bytes payload)
    printf 'fmt ' >> "$tmp"
    printf '\x10\x00\x00\x00' >> "$tmp"  # chunk size = 16
    printf '\x01\x00' >> "$tmp"          # PCM format
    printf '\x01\x00' >> "$tmp"          # 1 channel
    # sample rate (LE 32-bit)
    printf "$(printf '\\x%02x\\x%02x\\x%02x\\x%02x' \
      $((sr & 0xFF)) $(((sr>>8) & 0xFF)) \
      $(((sr>>16) & 0xFF)) $(((sr>>24) & 0xFF)))" >> "$tmp"
    # byte rate (LE 32-bit)
    printf "$(printf '\\x%02x\\x%02x\\x%02x\\x%02x' \
      $((byte_rate & 0xFF)) $(((byte_rate>>8) & 0xFF)) \
      $(((byte_rate>>16) & 0xFF)) $(((byte_rate>>24) & 0xFF)))" >> "$tmp"
    printf '\x02\x00' >> "$tmp"          # block align = 2
    printf '\x10\x00' >> "$tmp"          # bits per sample = 16
    # data chunk
    printf 'data' >> "$tmp"
    printf "$(printf '\\x%02x\\x%02x\\x%02x\\x%02x' \
      $((data_size & 0xFF)) $(((data_size>>8) & 0xFF)) \
      $(((data_size>>16) & 0xFF)) $(((data_size>>24) & 0xFF)))" >> "$tmp"
    # Write zero audio samples
    dd if=/dev/zero bs=1 count="$data_size" >> "$tmp" 2>/dev/null
    base64 -w0 "$tmp" 2>/dev/null || base64 "$tmp"
    rm -f "$tmp"
}

# Helper to do a full upload-start-chunk-complete cycle
# Args: $1=filename $2=media_type $3=chunk_b64 $4=total_size $5=duration_seconds
# Prints: HTTP status code of complete call
upload_and_complete() {
    local fname="$1" mtype="$2" chunk_data="$3" tsize="$4" dur="$5"
    local ub uid fp
    ub=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
      -d "{\"filename\":\"$fname\",\"media_type\":\"$mtype\",\"total_size\":$tsize,\"duration_seconds\":$dur}")
    uid=$(echo "$ub" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
    curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
      -d "{\"upload_id\":\"$uid\",\"chunk_index\":0,\"data\":\"$chunk_data\"}" > /dev/null
    # Compute SHA-256 fingerprint from the raw bytes
    fp=$(echo -n "$chunk_data" | base64 -d 2>/dev/null | sha256sum | cut -d' ' -f1)
    curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" \
      -H "Content-Type: application/json" \
      -d "{\"upload_id\":\"$uid\",\"fingerprint\":\"$fp\",\"total_size\":$tsize}"
}

# ── 2a. Video with server-verifiable duration <= 60s → ACCEPTED ───────
# Build MP4 with timescale=1000, duration_field=30000 → 30.0 seconds
MP4_30S=$(build_test_mp4 1000 30000)
R=$(upload_and_complete "ok30.mp4" "video" "$MP4_30S" 1048576 30)
check "Video 30s (server-verified) → 201" "201" "$R"

# ── 2b. Video with server-verifiable duration > 60s → REJECTED ────────
# Build MP4 with timescale=1000, duration_field=90000 → 90.0 seconds
MP4_90S=$(build_test_mp4 1000 90000)
R=$(upload_and_complete "bad90.mp4" "video" "$MP4_90S" 1048576 30)
check "Video 90s (server-verified) → 400" "400" "$R"

# ── 2c. Audio WAV with duration <= 120s → ACCEPTED ────────────────────
# Build WAV: 8000 Hz × 80000 samples = 10.0 seconds
WAV_10S=$(build_test_wav 8000 80000)
R=$(upload_and_complete "ok10.wav" "audio" "$WAV_10S" 1048576 10)
check "Audio WAV 10s (server-verified) → 201" "201" "$R"

# ── 2d. Audio WAV with duration > 120s → REJECTED ────────────────────
# Build WAV: 8000 Hz × 1040000 samples = 130.0 seconds
WAV_130S=$(build_test_wav 8000 1040000)
R=$(upload_and_complete "bad130.wav" "audio" "$WAV_130S" 3145728 130)
check "Audio WAV 130s (server-verified) → 400" "400" "$R"

# ── 2e. Unverifiable video format → REJECTED (fail-safe) ─────────────
# A file with ftyp but NO moov atom (so duration can't be extracted)
MP4_NOMOOV=$(printf '\x00\x00\x00\x10ftypisom\x00\x00\x00\x00' | base64 -w0 2>/dev/null || printf '\x00\x00\x00\x10ftypisom\x00\x00\x00\x00' | base64 2>/dev/null)
R=$(upload_and_complete "nomoov.mp4" "video" "$MP4_NOMOOV" 1048576 30)
check "Unverifiable video (no moov) → 400 fail-safe" "400" "$R"

# ── 2f. Unverifiable audio format → REJECTED (fail-safe) ─────────────
# ID3/MP3 header — no WAV, no MP4 container; duration unextractable
MP3_B64=$(printf 'ID3\x03\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00' | base64 -w0 2>/dev/null || printf 'ID3\x03\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00\x00' | base64 2>/dev/null)
R=$(upload_and_complete "unverif.mp3" "audio" "$MP3_B64" 1048576 10)
check "Unverifiable audio (MP3/ID3) → 400 fail-safe" "400" "$R"

# ── 2g. Client duration_seconds no longer controls acceptance ─────────
# Upload a 90-second MP4 but declare duration_seconds=30 at start.
# Server derives 90s from bytes → must reject despite client claim.
R=$(upload_and_complete "lie30.mp4" "video" "$MP4_90S" 1048576 30)
check "Client lies duration=30 but file=90s → 400" "400" "$R"

# ── 2h. Photo still has no duration constraint ────────────────────────
PBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"photo_ok.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
PID=$(echo "$PBODY" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$PID\",\"chunk_index\":0,\"data\":\"$JPEG_B64\"}" > /dev/null
PHOTO_FP=$(printf '\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00' | sha256sum | cut -d' ' -f1)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" \
  -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$PID\",\"fingerprint\":\"$PHOTO_FP\",\"total_size\":1024}")
check "Photo has no duration constraint → 201" "201" "$R"

# ═══════════════════════════════════════════════════════════════════════
# 3. TRACEABILITY STEPS — auditor visibility policy
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 3. Traceability steps visibility ━━━"

# Create a traceability code (draft status)
TBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/traceability" -H "Content-Type: application/json" \
  -d '{"intake_id":null}')
TRACE_ID=$(echo "$TBODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

# Admin can see steps of draft code
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" "$BASE/traceability/$TRACE_ID/steps")
check "Admin can see draft code steps → 200" "200" "$R"

# Staff can see steps of draft code
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" "$BASE/traceability/$TRACE_ID/steps")
check "Staff can see draft code steps → 200" "200" "$R"

# Auditor CANNOT see steps of draft code
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" "$BASE/traceability/$TRACE_ID/steps")
check "Auditor cannot see draft code steps → 403" "403" "$R"

# Publish the code
curl -s -b "$ADMIN_CK" -X POST "$BASE/traceability/$TRACE_ID/publish" -H "Content-Type: application/json" \
  -d '{"comment":"Publishing for audit test"}' > /dev/null

# Auditor CAN see steps of published code
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" "$BASE/traceability/$TRACE_ID/steps")
check "Auditor can see published code steps → 200" "200" "$R"

# Retract the code
curl -s -b "$ADMIN_CK" -X POST "$BASE/traceability/$TRACE_ID/retract" -H "Content-Type: application/json" \
  -d '{"comment":"Retracting for audit test"}' > /dev/null

# Auditor CANNOT see steps of retracted code
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" "$BASE/traceability/$TRACE_ID/steps")
check "Auditor cannot see retracted code steps → 403" "403" "$R"

# Admin CAN still see retracted code steps
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" "$BASE/traceability/$TRACE_ID/steps")
check "Admin can see retracted code steps → 200" "200" "$R"

# 404 for nonexistent code
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" "$BASE/traceability/nonexistent/steps")
check "Nonexistent code steps → 404" "404" "$R"

# ═══════════════════════════════════════════════════════════════════════
# 4. PRIVACY PREFERENCES — CRUD + user isolation
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 4. Privacy preferences ━━━"

# GET default preferences (lazy initialization)
PREFS=$(curl -s -b "$ADMIN_CK" "$BASE/profile/privacy-preferences")
R=$(echo "$PREFS" | grep -c '"show_email":true' || true)
check "Default show_email is true" "1" "$R"

R=$(echo "$PREFS" | grep -c '"show_phone":false' || true)
check "Default show_phone is false" "1" "$R"

R=$(echo "$PREFS" | grep -c '"allow_data_sharing":false' || true)
check "Default allow_data_sharing is false" "1" "$R"

# PATCH to update preferences
PREFS=$(curl -s -b "$ADMIN_CK" -X PATCH "$BASE/profile/privacy-preferences" \
  -H "Content-Type: application/json" \
  -d '{"show_phone":true,"allow_data_sharing":true}')
R=$(echo "$PREFS" | grep -c '"show_phone":true' || true)
check "Updated show_phone to true" "1" "$R"
R=$(echo "$PREFS" | grep -c '"allow_data_sharing":true' || true)
check "Updated allow_data_sharing to true" "1" "$R"
# show_email should remain unchanged
R=$(echo "$PREFS" | grep -c '"show_email":true' || true)
check "show_email unchanged after partial update" "1" "$R"

# GET to verify persistence
PREFS=$(curl -s -b "$ADMIN_CK" "$BASE/profile/privacy-preferences")
R=$(echo "$PREFS" | grep -c '"show_phone":true' || true)
check "show_phone persisted on re-read" "1" "$R"

# User isolation: staff user has separate preferences
PREFS_STAFF=$(curl -s -b "$STAFF_CK" "$BASE/profile/privacy-preferences")
R=$(echo "$PREFS_STAFF" | grep -c '"show_phone":false' || true)
check "Staff has own default show_phone=false (isolation)" "1" "$R"

# Staff updates own preferences
curl -s -b "$STAFF_CK" -X PATCH "$BASE/profile/privacy-preferences" \
  -H "Content-Type: application/json" \
  -d '{"allow_audit_log_export":false}' > /dev/null
PREFS_STAFF=$(curl -s -b "$STAFF_CK" "$BASE/profile/privacy-preferences")
R=$(echo "$PREFS_STAFF" | grep -c '"allow_audit_log_export":false' || true)
check "Staff updated own allow_audit_log_export" "1" "$R"

# Admin's preferences unaffected by staff's changes
PREFS=$(curl -s -b "$ADMIN_CK" "$BASE/profile/privacy-preferences")
R=$(echo "$PREFS" | grep -c '"allow_audit_log_export":true' || true)
check "Admin's allow_audit_log_export unchanged (isolation)" "1" "$R"

# Requires auth
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/profile/privacy-preferences")
check "Privacy prefs without auth → 401" "401" "$R"

# ═══════════════════════════════════════════════════════════════════════
# 5. SUPPLY — new fields (stock_status, media_references, review_summary)
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 5. Supply new fields ━━━"

# Create with all new fields
SBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/supply-entries" -H "Content-Type: application/json" \
  -d '{"name":"Dog Food Premium","sku":"DF-001","size":"large","color":"brown","price_cents":2499,"discount_cents":0,"notes":"bulk","stock_status":"in_stock","media_references":"ev-001,ev-002","review_summary":"Good quality"}')
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/supply-entries" -H "Content-Type: application/json" \
  -d '{"name":"Test Item","sku":null,"size":"small","color":"red","price_cents":100,"discount_cents":0,"notes":"","stock_status":"in_stock","media_references":"","review_summary":""}')
check "Supply create with stock_status → 201" "201" "$R"

# Verify new fields in response
contains "stock_status in create response" "in_stock" "$SBODY"
contains "media_references in create response" "ev-001,ev-002" "$SBODY"
contains "review_summary in create response" "Good quality" "$SBODY"

# Verify list returns new fields
SLIST=$(curl -s -b "$ADMIN_CK" "$BASE/supply-entries")
contains "stock_status in list response" "in_stock" "$SLIST"
contains "media_references in list response" "ev-001,ev-002" "$SLIST"
contains "review_summary in list response" "Good quality" "$SLIST"

# Invalid stock_status rejected
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/supply-entries" \
  -H "Content-Type: application/json" \
  -d '{"name":"Bad Status","sku":null,"size":"M","color":"blue","price_cents":100,"discount_cents":0,"notes":"","stock_status":"invalid_status","media_references":"","review_summary":""}')
check "Invalid stock_status → 400" "400" "$R"

# Default stock_status when not provided
R2=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/supply-entries" -H "Content-Type: application/json" \
  -d '{"name":"Default Status","sku":null,"size":"S","color":"green","price_cents":50,"discount_cents":0,"notes":""}')
contains "Default stock_status is unknown" "unknown" "$R2"

# ═══════════════════════════════════════════════════════════════════════
# 6. COOKIE HARDENING — Secure flag behavior
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 6. Cookie hardening ━━━"

# In the test environment, COOKIE_SECURE defaults to false.
# Verify session cookie has HttpOnly, SameSite=Strict, Path=/
HEADERS=$(curl -s -D - -o /dev/null -X POST "$BASE/auth/login" -H "Content-Type: application/json" \
  -d '{"username":"afadmin","password":"AuditFixAdm12"}')
contains "Cookie has HttpOnly" "HttpOnly" "$HEADERS"
contains "Cookie has SameSite=Strict" "SameSite=Strict" "$HEADERS"
contains "Cookie has Path=/" "Path=/" "$HEADERS"

# Without COOKIE_SECURE=true, Secure attribute should NOT be present
# (local dev mode). This is correct behavior — Secure is only added
# in production HTTPS mode.
not_contains "Cookie does not have Secure in HTTP mode" "; Secure" "$HEADERS"

# ═══════════════════════════════════════════════════════════════════════
# 7. ADOPTION SEMANTICS — type-aware status transitions + KPI
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 7. Adoption semantics ━━━"

# Create an animal intake, transition to in_care, then adopted → OK
ABODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"animal","details":"adoption test dog"}')
ANIMAL_ID=$(echo "$ABODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

curl -s -b "$ADMIN_CK" -X PATCH "$BASE/intake/$ANIMAL_ID/status" \
  -H "Content-Type: application/json" -d '{"status":"in_care"}' > /dev/null
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X PATCH "$BASE/intake/$ANIMAL_ID/status" \
  -H "Content-Type: application/json" -d '{"status":"adopted"}')
check "Animal in_care → adopted → 200" "200" "$R"

# Create a supply intake, transition to in_stock, then try adopted → 400
SBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"supply","details":"adoption test supply"}')
SUPPLY_ID=$(echo "$SBODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

curl -s -b "$ADMIN_CK" -X PATCH "$BASE/intake/$SUPPLY_ID/status" \
  -H "Content-Type: application/json" -d '{"status":"in_stock"}' > /dev/null
# Supply can't go to in_care first, but let's test a direct path:
# received → in_care is valid for any type, then in_care → adopted should fail for supply
SBODY2=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"supply","details":"adoption test supply 2"}')
SUPPLY_ID2=$(echo "$SBODY2" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X PATCH "$BASE/intake/$SUPPLY_ID2/status" \
  -H "Content-Type: application/json" -d '{"status":"in_care"}' > /dev/null
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X PATCH "$BASE/intake/$SUPPLY_ID2/status" \
  -H "Content-Type: application/json" -d '{"status":"adopted"}')
check "Supply in_care → adopted → 400 (animal-only)" "400" "$R"

# Donation cannot be adopted either
DBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"donation","details":"adoption test donation"}')
DON_ID=$(echo "$DBODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X PATCH "$BASE/intake/$DON_ID/status" \
  -H "Content-Type: application/json" -d '{"status":"in_care"}' > /dev/null
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X PATCH "$BASE/intake/$DON_ID/status" \
  -H "Content-Type: application/json" -d '{"status":"adopted"}')
check "Donation in_care → adopted → 400 (animal-only)" "400" "$R"

# Adoption KPI must exclude non-animal records from adopted count
KPIBODY=$(curl -s -b "$ADMIN_CK" "$BASE/reports/adoption-conversion")
# We have 1 animal adopted above. The total animals depends on test state,
# but adopted count must be animal-scoped. Check that the endpoint returns.
contains "Adoption KPI has total field" '"total"' "$KPIBODY"
contains "Adoption KPI has adopted field" '"adopted"' "$KPIBODY"

# ═══════════════════════════════════════════════════════════════════════
# 8. ANTI-PASSBACK OVERRIDE — non-empty reason required
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 8. Anti-passback override reason validation ━━━"

# Create a member for checkin tests
curl -s -b "$ADMIN_CK" -X POST "$BASE/members" -H "Content-Type: application/json" \
  -d '{"member_id":"APB001","name":"Override Test"}' > /dev/null

# Normal checkin works
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"APB001"}')
check "Normal checkin → 201" "201" "$R"

# Override with valid non-empty reason → 201 (admin)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"APB001","override_reason":"Emergency access"}')
check "Override with reason → 201" "201" "$R"

# Override with empty reason → 400
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"APB001","override_reason":""}')
check "Override with empty reason → 400" "400" "$R"

# Override with whitespace-only reason → 400
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"APB001","override_reason":"   "}')
check "Override with whitespace reason → 400" "400" "$R"

# ═══════════════════════════════════════════════════════════════════════
# 9. KEY ROTATION DURABILITY — requires ENCRYPTION_KEY_FILE
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 9. Key rotation durability ━━━"

# Key rotation should work when ENCRYPTION_KEY_FILE is configured
# (docker-compose.yml sets it to /app/storage/encryption.key)
# First create an address to encrypt
curl -s -b "$ADMIN_CK" -X POST "$BASE/address-book" -H "Content-Type: application/json" \
  -d '{"label":"RotTest","street":"1 Key St","city":"Crypto","state":"CA","zip_plus4":"90210-0000","phone":"555-KEY-0001"}' > /dev/null

# Check if ENCRYPTION_KEY_FILE is set by attempting rotation
NEW_KEY="aabbccddeeff00112233445566778899aabbccddeeff00112233445566778899"
ROT_RESP=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/admin/security/rotate-key" -H "Content-Type: application/json" \
  -d "{\"new_key_hex\":\"$NEW_KEY\"}")

if echo "$ROT_RESP" | grep -q "ENCRYPTION_KEY_FILE"; then
    # Key file not configured — rotation correctly rejected
    echo "PASS: Rotation rejected without ENCRYPTION_KEY_FILE (durability enforced)"; PASS=$((PASS+1))
elif echo "$ROT_RESP" | grep -q '"rotated_rows"'; then
    # Key file is configured — rotation succeeded
    echo "PASS: Key rotation succeeded with persisted key file"; PASS=$((PASS+1))

    # Verify address still readable after rotation
    ADDR_LIST=$(curl -s -b "$ADMIN_CK" "$BASE/address-book")
    contains "Address readable after rotation" "RotTest" "$ADDR_LIST"

    # Verify persisted_to_file is true
    contains "Key persisted to file" '"persisted_to_file":true' "$ROT_RESP"
else
    echo "FAIL: Unexpected rotation response: $ROT_RESP"; FAIL=$((FAIL+1))
fi

# ═══════════════════════════════════════════════════════════════════════
# 10. EVIDENCE METADATA — truthful (no simulated compression)
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 10. Evidence metadata truthfulness ━━━"

# Upload a photo — metadata must reflect actual stored file, not projection
CPBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"truth_test.jpg","media_type":"photo","total_size":1048576,"duration_seconds":0}')
CPID=$(echo "$CPBODY" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$CPID\",\"chunk_index\":0,\"data\":\"$JPEG_B64\"}" > /dev/null
CP_FP=$(printf '\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00' | sha256sum | cut -d' ' -f1)
CPRESP=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$CPID\",\"fingerprint\":\"$CP_FP\",\"total_size\":1048576}")

# compressed_bytes reflects actual file (no fake projection)
COMP_BYTES=$(echo "$CPRESP" | grep -o '"compressed_bytes":[0-9]*' | cut -d':' -f2)
if [ -n "$COMP_BYTES" ] && [ "$COMP_BYTES" -gt 0 ]; then
    echo "PASS: compressed_bytes ($COMP_BYTES) reflects actual file"; PASS=$((PASS+1))
else
    echo "FAIL: compressed_bytes invalid: $COMP_BYTES"; FAIL=$((FAIL+1))
fi

# No real transcoding → compression_applied must be false
contains "compression_applied is false (truthful)" '"compression_applied":false' "$CPRESP"

# No real transcoding → ratio must be 1.0
if echo "$CPRESP" | grep -qE '"compression_ratio":1'; then
    echo "PASS: compression_ratio is 1.0 (no transcoding)"; PASS=$((PASS+1))
else
    echo "FAIL: compression_ratio should be 1.0: $CPRESP"; FAIL=$((FAIL+1))
fi

# ═══════════════════════════════════════════════════════════════════════
# 11. EVIDENCE FILE LIFECYCLE — canonical path + physical cleanup
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 11. Evidence file lifecycle (upload_id vs evidence_id) ━━━"

# Upload a photo, get its evidence_id, then verify the file exists at
# the evidence_id-based canonical path (not the upload_id path).
LCBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"lifecycle.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
LCUID=$(echo "$LCBODY" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$LCUID\",\"chunk_index\":0,\"data\":\"$JPEG_B64\"}" > /dev/null
LC_FP=$(printf '\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00' | sha256sum | cut -d' ' -f1)
LCRESP=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$LCUID\",\"fingerprint\":\"$LC_FP\",\"total_size\":1024}")
LCEID=$(echo "$LCRESP" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

# Verify: file should exist at evidence_id path, NOT at upload_id path
if [ -f "/app/storage/uploads/${LCEID}_final" ]; then
    echo "PASS: File exists at canonical evidence_id path"; PASS=$((PASS+1))
else
    echo "FAIL: File missing at evidence_id path /app/storage/uploads/${LCEID}_final"; FAIL=$((FAIL+1))
fi
if [ ! -f "/app/storage/uploads/${LCUID}_final" ]; then
    echo "PASS: No orphan file at upload_id path"; PASS=$((PASS+1))
else
    echo "FAIL: Orphan file exists at upload_id path"; FAIL=$((FAIL+1))
fi

# Verify: DELETE physically removes the file
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X DELETE "$BASE/evidence/$LCEID")
check "DELETE evidence → 200" "200" "$R"
if [ ! -f "/app/storage/uploads/${LCEID}_final" ]; then
    echo "PASS: File physically removed after DELETE"; PASS=$((PASS+1))
else
    echo "FAIL: File still on disk after DELETE"; FAIL=$((FAIL+1))
fi

# ═══════════════════════════════════════════════════════════════════════
# 12. WATERMARK — watermark_text present in response
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 12. Watermark behavior ━━━"

# Upload a photo and verify watermark_text is in the response
WMBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"wm.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
WMUID=$(echo "$WMBODY" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$WMUID\",\"chunk_index\":0,\"data\":\"$JPEG_B64\"}" > /dev/null
WMRESP=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$WMUID\",\"fingerprint\":\"$LC_FP\",\"total_size\":1024}")

# Watermark text must contain facility code and timestamp pattern
if echo "$WMRESP" | grep -qE '"watermark_text":"FAC01 [0-9]{2}/[0-9]{2}/[0-9]{4} [0-9]{2}:[0-9]{2} (AM|PM)"'; then
    echo "PASS: Watermark text matches FAC01 MM/DD/YYYY hh:mm AM/PM"; PASS=$((PASS+1))
else
    echo "FAIL: Watermark text format wrong: $WMRESP"; FAIL=$((FAIL+1))
fi

# Verify storage_path is tracked in DB (evidence has canonical path)
WMEID=$(echo "$WMRESP" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
SP=$(sql "SELECT storage_path FROM evidence_records WHERE id='$WMEID';")
if echo "$SP" | grep -q "${WMEID}_final"; then
    echo "PASS: storage_path in DB contains evidence_id"; PASS=$((PASS+1))
else
    echo "FAIL: storage_path missing or wrong: $SP"; FAIL=$((FAIL+1))
fi

# ═══════════════════════════════════════════════════════════════════════
# 13. EVIDENCE LINKING — API contract verification
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 13. Evidence linking ━━━"

# Create an intake target
LINK_INTAKE=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"animal","details":"link test"}')
LINK_IID=$(echo "$LINK_INTAKE" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

# Upload evidence for linking test
LINKUB=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"link.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
LINKUID=$(echo "$LINKUB" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$LINKUID\",\"chunk_index\":0,\"data\":\"$JPEG_B64\"}" > /dev/null
LINK_FP=$(printf '\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00' | sha256sum | cut -d' ' -f1)
LINKRESP=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$LINKUID\",\"fingerprint\":\"$LINK_FP\",\"total_size\":1024}")
LINK_EID=$(echo "$LINKRESP" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

# Link to intake → 200
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/evidence/$LINK_EID/link" \
  -H "Content-Type: application/json" -d "{\"target_type\":\"intake\",\"target_id\":\"$LINK_IID\"}")
check "Link evidence to intake → 200" "200" "$R"

# Link to nonexistent target → 404
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/evidence/$LINK_EID/link" \
  -H "Content-Type: application/json" -d '{"target_type":"intake","target_id":"nonexistent"}')
check "Link to nonexistent target → 404" "404" "$R"

# Auditor cannot link
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/evidence/$LINK_EID/link" \
  -H "Content-Type: application/json" -d "{\"target_type\":\"intake\",\"target_id\":\"$LINK_IID\"}")
check "Auditor cannot link evidence → 403" "403" "$R"

# Legal hold (admin only)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X PATCH "$BASE/evidence/$LINK_EID/legal-hold" \
  -H "Content-Type: application/json" -d '{"legal_hold":true}')
check "Admin set legal hold → 200" "200" "$R"

# Staff cannot set legal hold
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" -X PATCH "$BASE/evidence/$LINK_EID/legal-hold" \
  -H "Content-Type: application/json" -d '{"legal_hold":false}')
check "Staff cannot set legal hold → 403" "403" "$R"

# ═══════════════════════════════════════════════════════════════════════
# 14. CHECK-IN OVERRIDE — admin-only with non-empty reason
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 14. Check-in override (admin API contract) ━━━"

# Create member
curl -s -b "$ADMIN_CK" -X POST "$BASE/members" -H "Content-Type: application/json" \
  -d '{"member_id":"OVR001","name":"Override Test 2"}' > /dev/null 2>&1

# Normal checkin
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"OVR001"}')
check "Normal checkin → 201" "201" "$R"

# Anti-passback blocks second checkin
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"OVR001"}')
check "Anti-passback blocks → 409" "409" "$R"

# Admin override with valid reason succeeds
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"OVR001","override_reason":"Emergency access needed"}')
check "Admin override with reason → 201" "201" "$R"

# Staff cannot override (even with reason)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"OVR001","override_reason":"Staff trying override"}')
check "Staff cannot override → 403" "403" "$R"

# ═══════════════════════════════════════════════════════════════════════
# 15. ADMIN OPS — API contract for admin-only endpoints
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 15. Admin operations API ━━━"

# Admin can access config
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" "$BASE/admin/config")
check "Admin GET /admin/config → 200" "200" "$R"

# Admin can access jobs
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" "$BASE/admin/jobs")
check "Admin GET /admin/jobs → 200" "200" "$R"

# Admin can access logs
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" "$BASE/admin/logs")
check "Admin GET /admin/logs → 200" "200" "$R"

# Admin can access config versions
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" "$BASE/admin/config/versions")
check "Admin GET /admin/config/versions → 200" "200" "$R"

# Staff cannot access admin endpoints
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" "$BASE/admin/config")
check "Staff GET /admin/config → 403" "403" "$R"
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" "$BASE/admin/jobs")
check "Staff GET /admin/jobs → 403" "403" "$R"

# Auditor cannot access admin endpoints
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" "$BASE/admin/logs")
check "Auditor GET /admin/logs → 403" "403" "$R"

# ═══════════════════════════════════════════════════════════════════════
# Summary
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "========================================"
echo "  Audit Fixes: $PASS passed, $FAIL failed"
echo "========================================"
[ $FAIL -eq 0 ]
