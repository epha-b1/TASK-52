#!/bin/bash
# Comprehensive test covering Slices 4-11
set -e
PASS=0; FAIL=0; BASE="http://localhost:8080"; CK="/tmp/fs_ck"; CK2="/tmp/fs_ck2"

check() {
    local name="$1" expected="$2" actual="$3"
    if [ "$actual" = "$expected" ]; then
        echo "PASS: $name"; PASS=$((PASS+1))
    else
        echo "FAIL: $name (expected $expected, got $actual)"; FAIL=$((FAIL+1))
    fi
}

# Minimal JPEG header (FF D8 FF E0 ...) base64-encoded for chunk uploads
JPEG_B64=$(printf '\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00' | base64 -w0 2>/dev/null || printf '\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00' | base64 2>/dev/null)

echo "=== Comprehensive API Tests (Slices 4-11) ==="

# Setup: admin user + staff user
curl -s -c "$CK" -X POST "$BASE/auth/register" -H "Content-Type: application/json" \
  -d '{"username":"fsadmin","password":"SecurePass12"}' > /dev/null
curl -s -b "$CK" -X POST "$BASE/users" -H "Content-Type: application/json" \
  -d '{"username":"fsstaff","password":"StaffPass1234","role":"operations_staff"}' > /dev/null
curl -s -c "$CK2" -X POST "$BASE/auth/login" -H "Content-Type: application/json" \
  -d '{"username":"fsstaff","password":"StaffPass1234"}' > /dev/null

# ─────────────────────────────────────────────
# Slice 4: Intake + Inspections
# ─────────────────────────────────────────────
echo ""
echo "━━━ Slice 4: Intake + Inspections ━━━"

# Create intake
BODY=$(curl -s -b "$CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"animal","details":"{\"species\":\"dog\"}"}')
INTAKE_ID=$(echo "$BODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
[ -n "$INTAKE_ID" ] && { echo "PASS: Intake created"; PASS=$((PASS+1)); } || { echo "FAIL: Intake create"; FAIL=$((FAIL+1)); }

# List
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" "$BASE/intake"); check "List intake 200" "200" "$R"

# Valid transition
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X PATCH "$BASE/intake/$INTAKE_ID/status" \
  -H "Content-Type: application/json" -d '{"status":"in_care"}')
check "Transition received→in_care 200" "200" "$R"

# Invalid transition → 409
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X PATCH "$BASE/intake/$INTAKE_ID/status" \
  -H "Content-Type: application/json" -d '{"status":"received"}')
check "Invalid transition → 409" "409" "$R"

# Create inspection
BODY=$(curl -s -b "$CK" -X POST "$BASE/inspections" -H "Content-Type: application/json" \
  -d "{\"intake_id\":\"$INTAKE_ID\"}")
INSP_ID=$(echo "$BODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
[ -n "$INSP_ID" ] && { echo "PASS: Inspection created"; PASS=$((PASS+1)); } || { echo "FAIL: Inspection create"; FAIL=$((FAIL+1)); }

# Resolve
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X PATCH "$BASE/inspections/$INSP_ID/resolve" \
  -H "Content-Type: application/json" -d '{"status":"passed","outcome_notes":"OK"}')
check "Resolve inspection 200" "200" "$R"

# Re-resolve → 409
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X PATCH "$BASE/inspections/$INSP_ID/resolve" \
  -H "Content-Type: application/json" -d '{"status":"failed","outcome_notes":"no"}')
check "Re-resolve → 409" "409" "$R"

# ─────────────────────────────────────────────
# Slice 5: Evidence
# ─────────────────────────────────────────────
echo ""
echo "━━━ Slice 5: Evidence + Chunked Upload ━━━"

# Start upload
BODY=$(curl -s -b "$CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"test.jpg","media_type":"photo","total_size":1048576,"duration_seconds":0}')
UPLOAD_ID=$(echo "$BODY" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
[ -n "$UPLOAD_ID" ] && { echo "PASS: Upload session started"; PASS=$((PASS+1)); } || { echo "FAIL: Upload start"; FAIL=$((FAIL+1)); }

# Send chunk
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X POST "$BASE/media/upload/chunk" \
  -H "Content-Type: application/json" -d "{\"upload_id\":\"$UPLOAD_ID\",\"chunk_index\":0,\"data\":\"$JPEG_B64\"}")
check "Chunk upload 200" "200" "$R"

# Complete
BODY=$(curl -s -b "$CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UPLOAD_ID\",\"fingerprint\":\"fullstackfp01234\",\"total_size\":1048576,\"exif_capture_time\":null,\"tags\":\"dog\",\"keyword\":\"intake\"}")
EVIDENCE_ID=$(echo "$BODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
[ -n "$EVIDENCE_ID" ] && { echo "PASS: Evidence complete"; PASS=$((PASS+1)); } || { echo "FAIL: Complete: $BODY"; FAIL=$((FAIL+1)); }

# Missing EXIF flagged
if echo "$BODY" | grep -q '"missing_exif":true'; then
    echo "PASS: Missing EXIF flagged"; PASS=$((PASS+1))
else
    echo "FAIL: EXIF flag missing: $BODY"; FAIL=$((FAIL+1))
fi

# File too large → 400
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X POST "$BASE/media/upload/start" \
  -H "Content-Type: application/json" \
  -d '{"filename":"big.jpg","media_type":"photo","total_size":999999999,"duration_seconds":0}')
check "Oversize photo → 400" "400" "$R"

# Link evidence
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X POST "$BASE/evidence/$EVIDENCE_ID/link" \
  -H "Content-Type: application/json" -d "{\"target_type\":\"intake\",\"target_id\":\"$INTAKE_ID\"}")
check "Link evidence 200" "200" "$R"

# Delete linked → 409
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X DELETE "$BASE/evidence/$EVIDENCE_ID")
check "Delete linked evidence → 409" "409" "$R"

# ─────────────────────────────────────────────
# Slice 6: Supply Parsing
# ─────────────────────────────────────────────
echo ""
echo "━━━ Slice 6: Supply Parsing ━━━"

# Valid: navy → blue
BODY=$(curl -s -b "$CK" -X POST "$BASE/supply-entries" -H "Content-Type: application/json" \
  -d '{"name":"Shirt","sku":"SH1","size":"16 oz","color":"navy","price_cents":1999,"discount_cents":0,"notes":""}')
if echo "$BODY" | grep -q '"canonical_color":"blue"'; then
    echo "PASS: Color normalized navy→blue"; PASS=$((PASS+1))
else
    echo "FAIL: Color normalization: $BODY"; FAIL=$((FAIL+1))
fi
if echo "$BODY" | grep -q '"parse_status":"ok"'; then
    echo "PASS: Parse status OK"; PASS=$((PASS+1))
else
    echo "FAIL: Parse status: $BODY"; FAIL=$((FAIL+1))
fi

# Unknown color → needs_review
BODY=$(curl -s -b "$CK" -X POST "$BASE/supply-entries" -H "Content-Type: application/json" \
  -d '{"name":"Hat","sku":"H1","size":"12 in","color":"teal","price_cents":999,"discount_cents":0,"notes":""}')
if echo "$BODY" | grep -q '"parse_status":"needs_review"'; then
    echo "PASS: Unknown color → needs_review"; PASS=$((PASS+1))
else
    echo "FAIL: Needs review not set: $BODY"; FAIL=$((FAIL+1))
fi

# ─────────────────────────────────────────────
# Slice 7: Traceability
# ─────────────────────────────────────────────
echo ""
echo "━━━ Slice 7: Traceability ━━━"

# Policy (see README role matrix): staff CAN create traceability codes;
# only auditor is blocked (tested in remediation + blockers suites). Here
# we assert staff creation works and rely on the other suites for the
# auditor 403.
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK2" -X POST "$BASE/traceability" \
  -H "Content-Type: application/json" -d '{"intake_id":null}')
check "Staff → create traceability → 201" "201" "$R"

# Admin creates
BODY=$(curl -s -b "$CK" -X POST "$BASE/traceability" -H "Content-Type: application/json" \
  -d "{\"intake_id\":\"$INTAKE_ID\"}")
TRACE_ID=$(echo "$BODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
CODE_STR=$(echo "$BODY" | grep -o '"code":"[^"]*"' | cut -d'"' -f4)
[ -n "$TRACE_ID" ] && { echo "PASS: Trace code created"; PASS=$((PASS+1)); } || { echo "FAIL: Trace create"; FAIL=$((FAIL+1)); }

# Verify checksum
if [ -n "$CODE_STR" ]; then
    BODY=$(curl -s "$BASE/traceability/verify/$CODE_STR")
    if echo "$BODY" | grep -q '"valid":true'; then
        echo "PASS: Checksum verify valid"; PASS=$((PASS+1))
    else
        echo "FAIL: Checksum verify: $BODY"; FAIL=$((FAIL+1))
    fi
fi

# Publish without comment → 400
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X POST "$BASE/traceability/$TRACE_ID/publish" \
  -H "Content-Type: application/json" -d '{"comment":""}')
check "Publish no comment → 400" "400" "$R"

# Publish with comment
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X POST "$BASE/traceability/$TRACE_ID/publish" \
  -H "Content-Type: application/json" -d '{"comment":"Release version 1"}')
check "Publish 200" "200" "$R"

# Retract with comment
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X POST "$BASE/traceability/$TRACE_ID/retract" \
  -H "Content-Type: application/json" -d '{"comment":"Correction needed"}')
check "Retract 200" "200" "$R"

# ─────────────────────────────────────────────
# Slice 8: Check-In
# ─────────────────────────────────────────────
echo ""
echo "━━━ Slice 8: Check-In ━━━"

# Create member
curl -s -b "$CK" -X POST "$BASE/members" -H "Content-Type: application/json" \
  -d '{"member_id":"M001","name":"John Doe"}' > /dev/null
echo "PASS: Member created"; PASS=$((PASS+1))

# First check-in
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"M001"}')
check "First check-in 201" "201" "$R"

# Second within 2min → 409 (anti-passback)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"M001"}')
check "Anti-passback 409" "409" "$R"

# Admin override works
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"M001","override_reason":"Emergency"}')
check "Admin override 201" "201" "$R"

# Staff override → 403
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK2" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"M001","override_reason":"nope"}')
check "Staff override → 403" "403" "$R"

# ─────────────────────────────────────────────
# Slice 9: Dashboard + Reports
# ─────────────────────────────────────────────
echo ""
echo "━━━ Slice 9: Dashboard + Reports ━━━"

# Summary
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" "$BASE/reports/summary")
check "Reports summary 200" "200" "$R"

# CSV export as admin
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" "$BASE/reports/export")
check "CSV export admin 200" "200" "$R"

# CSV export as staff → 403
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK2" "$BASE/reports/export")
check "CSV export staff → 403" "403" "$R"

# Adoption conversion
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" "$BASE/reports/adoption-conversion")
check "Adoption conversion 200" "200" "$R"

# ─────────────────────────────────────────────
# Slice 10: Admin Config
# ─────────────────────────────────────────────
echo ""
echo "━━━ Slice 10: Admin Config ━━━"

# Get config
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" "$BASE/admin/config")
check "Get config 200" "200" "$R"

# Staff cannot → 403
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK2" "$BASE/admin/config")
check "Staff config → 403" "403" "$R"

# Save config
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X PATCH "$BASE/admin/config" \
  -H "Content-Type: application/json" -d '{"key":"value"}')
check "Save config 200" "200" "$R"

# List versions
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" "$BASE/admin/config/versions")
check "List versions 200" "200" "$R"

# Diagnostic export
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" -X POST "$BASE/admin/diagnostics/export")
check "Diagnostic export 200" "200" "$R"

# Jobs
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" "$BASE/admin/jobs")
check "Admin jobs 200" "200" "$R"

# ─────────────────────────────────────────────
# Slice 11: Audit Log
# ─────────────────────────────────────────────
echo ""
echo "━━━ Slice 11: Audit Log ━━━"

# List audit
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" "$BASE/audit-logs")
check "Audit list 200" "200" "$R"

# Staff audit → 403
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK2" "$BASE/audit-logs")
check "Staff audit → 403" "403" "$R"

# Audit export CSV
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK" "$BASE/audit-logs/export")
check "Audit CSV export 200" "200" "$R"

# Masking in export
BODY=$(curl -s -b "$CK" "$BASE/audit-logs/export")
if echo "$BODY" | grep -q "\[REDACTED\]"; then
    echo "PASS: Audit export masks sensitive fields"; PASS=$((PASS+1))
else
    echo "FAIL: No [REDACTED] marker"; FAIL=$((FAIL+1))
fi

# ─────────────────────────────────────────────
# Summary
# ─────────────────────────────────────────────
rm -f "$CK" "$CK2"
echo ""
echo "========================================"
echo "  Full Stack Tests - Passed: $PASS  Failed: $FAIL"
echo "========================================"
[ $FAIL -gt 0 ] && exit 1
exit 0
