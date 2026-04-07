#!/bin/bash
# Remediation test suite — validates the fixes from the acceptance audit.
set -e

PASS=0; FAIL=0
BASE="http://localhost:8080"
ADMIN_CK="/tmp/rem_admin"
STAFF_CK="/tmp/rem_staff"
AUDITOR_CK="/tmp/rem_auditor"

check() {
    local name="$1" expected="$2" actual="$3"
    if [ "$actual" = "$expected" ]; then
        echo "PASS: $name"; PASS=$((PASS+1))
    else
        echo "FAIL: $name (expected $expected, got $actual)"; FAIL=$((FAIL+1))
    fi
}

json_has() {
    echo "$1" | grep -q "$2"
}

# Minimal JPEG header for chunk uploads
JPEG_B64=$(printf '\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00' | base64 -w0 2>/dev/null || printf '\xff\xd8\xff\xe0\x00\x10JFIF\x00\x01\x01\x00\x00\x01\x00\x01\x00\x00' | base64 2>/dev/null)

echo "=== Remediation Suite: Security + Lifecycle + Idempotency ==="

# ─── Setup: admin + staff + auditor ───────────────────────────────────
curl -s -c "$ADMIN_CK" -X POST "$BASE/auth/register" -H "Content-Type: application/json" \
  -d '{"username":"remadmin","password":"RemediationAdm12"}' > /dev/null

curl -s -b "$ADMIN_CK" -X POST "$BASE/users" -H "Content-Type: application/json" \
  -d '{"username":"remstaff","password":"RemediationStf12","role":"operations_staff"}' > /dev/null
curl -s -b "$ADMIN_CK" -X POST "$BASE/users" -H "Content-Type: application/json" \
  -d '{"username":"remauditor","password":"RemediationAud12","role":"auditor"}' > /dev/null

curl -s -c "$STAFF_CK" -X POST "$BASE/auth/login" -H "Content-Type: application/json" \
  -d '{"username":"remstaff","password":"RemediationStf12"}' > /dev/null
curl -s -c "$AUDITOR_CK" -X POST "$BASE/auth/login" -H "Content-Type: application/json" \
  -d '{"username":"remauditor","password":"RemediationAud12"}' > /dev/null

# Sanity check: admin can list users
check "Admin can list users" "200" "$(curl -s -o /dev/null -w '%{http_code}' -b "$ADMIN_CK" "$BASE/users")"

# ─── Section 1: Auditor mutation matrix (must all be 403) ─────────────
echo ""
echo "── Auditor forbidden mutation matrix ──"

R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/intake" \
  -H "Content-Type: application/json" -d '{"intake_type":"animal","details":"{}"}')
check "Auditor POST /intake → 403" "403" "$R"

# Need an intake first (as admin) so auditor has a target
IBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"animal","details":"{}"}')
INTAKE_ID=$(echo "$IBODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X PATCH "$BASE/intake/$INTAKE_ID/status" \
  -H "Content-Type: application/json" -d '{"status":"in_care"}')
check "Auditor PATCH /intake/:id/status → 403" "403" "$R"

R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/inspections" \
  -H "Content-Type: application/json" -d "{\"intake_id\":\"$INTAKE_ID\"}")
check "Auditor POST /inspections → 403" "403" "$R"

R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/supply-entries" \
  -H "Content-Type: application/json" -d '{"name":"x","sku":null,"size":"12 oz","color":"blue","price_cents":100,"discount_cents":0,"notes":""}')
check "Auditor POST /supply-entries → 403" "403" "$R"

R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/checkin" \
  -H "Content-Type: application/json" -d '{"member_id":"nonexistent"}')
check "Auditor POST /checkin → 403" "403" "$R"

R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/media/upload/start" \
  -H "Content-Type: application/json" -d '{"filename":"x.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
check "Auditor POST /media/upload/start → 403" "403" "$R"

# Admin is allowed — sanity
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/inspections" \
  -H "Content-Type: application/json" -d "{\"intake_id\":\"$INTAKE_ID\"}")
check "Admin POST /inspections (sanity) → 201" "201" "$R"

# Auditor CAN publish/retract traceability (explicit allow list)
TBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/traceability" -H "Content-Type: application/json" \
  -d "{\"intake_id\":\"$INTAKE_ID\"}")
TRACE_ID=$(echo "$TBODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
CODE_STR=$(echo "$TBODY" | grep -o '"code":"[^"]*"' | cut -d'"' -f4)

R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/traceability/$TRACE_ID/publish" \
  -H "Content-Type: application/json" -d '{"comment":"Auditor publish"}')
check "Auditor POST /traceability/:id/publish (allowed) → 200" "200" "$R"

# ─── Section 2: Traceability code uses real date (not hardcoded) ──────
echo ""
echo "── Traceability real date ──"
TODAY=$(date -u +%Y%m%d)
if echo "$CODE_STR" | grep -q "$TODAY"; then
    echo "PASS: Traceability code contains today's date ($TODAY)"; PASS=$((PASS+1))
else
    echo "FAIL: Traceability code $CODE_STR does not contain today's date $TODAY"; FAIL=$((FAIL+1))
fi

# ─── Section 3: Watermark real format ─────────────────────────────────
echo ""
echo "── Evidence watermark real format ──"
UBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"wm_test.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
UPLOAD_ID=$(echo "$UBODY" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UPLOAD_ID\",\"chunk_index\":0,\"data\":\"$JPEG_B64\"}" > /dev/null
EBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UPLOAD_ID\",\"fingerprint\":\"abc12345def67890\",\"total_size\":1024,\"exif_capture_time\":null,\"tags\":\"dog\",\"keyword\":\"foo\"}")
EVIDENCE_ID=$(echo "$EBODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
if echo "$EBODY" | grep -qE '"watermark_text":"FAC01 [0-9]{2}/[0-9]{2}/[0-9]{4} [0-9]{2}:[0-9]{2} (AM|PM)"'; then
    echo "PASS: Watermark matches MM/DD/YYYY hh:mm AM/PM format"; PASS=$((PASS+1))
else
    echo "FAIL: Watermark wrong format: $EBODY"; FAIL=$((FAIL+1))
fi

# ─── Section 4: Evidence object-level auth ────────────────────────────
echo ""
echo "── Evidence object-level auth ──"

# Staff cannot delete admin's unlinked evidence (not uploader)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" -X DELETE "$BASE/evidence/$EVIDENCE_ID")
check "Staff DELETE other user's evidence → 403" "403" "$R"

# Admin can delete it
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X DELETE "$BASE/evidence/$EVIDENCE_ID")
check "Admin DELETE own evidence → 200" "200" "$R"

# Fingerprint format validation
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"fp.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
UBODY2=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"fp2.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
UPLOAD_ID2=$(echo "$UBODY2" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UPLOAD_ID2\",\"chunk_index\":0,\"data\":\"$JPEG_B64\"}" > /dev/null
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UPLOAD_ID2\",\"fingerprint\":\"\",\"total_size\":1024,\"exif_capture_time\":null,\"tags\":null,\"keyword\":null}")
check "Empty fingerprint → 400" "400" "$R"

# ─── Section 5: Evidence search filters ───────────────────────────────
echo ""
echo "── Evidence search filters ──"

# Upload two more with different keywords/tags
up1=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"f1.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
UID1=$(echo "$up1" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UID1\",\"chunk_index\":0,\"data\":\"$JPEG_B64\"}" > /dev/null
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UID1\",\"fingerprint\":\"deadbeef12345678\",\"total_size\":1024,\"exif_capture_time\":null,\"tags\":\"alpha\",\"keyword\":\"apple\"}" > /dev/null

up2=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"f2.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
UID2=$(echo "$up2" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UID2\",\"chunk_index\":0,\"data\":\"$JPEG_B64\"}" > /dev/null
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UID2\",\"fingerprint\":\"cafebabe87654321\",\"total_size\":1024,\"exif_capture_time\":null,\"tags\":\"beta\",\"keyword\":\"banana\"}" > /dev/null

KW_RESULT=$(curl -s -b "$ADMIN_CK" "$BASE/evidence?keyword=apple")
if echo "$KW_RESULT" | grep -q "f1.jpg" && ! echo "$KW_RESULT" | grep -q "f2.jpg"; then
    echo "PASS: keyword filter returns only matching evidence"; PASS=$((PASS+1))
else
    echo "FAIL: keyword filter wrong: $KW_RESULT"; FAIL=$((FAIL+1))
fi

TAG_RESULT=$(curl -s -b "$ADMIN_CK" "$BASE/evidence?tag=beta")
if echo "$TAG_RESULT" | grep -q "f2.jpg" && ! echo "$TAG_RESULT" | grep -q "f1.jpg"; then
    echo "PASS: tag filter returns only matching evidence"; PASS=$((PASS+1))
else
    echo "FAIL: tag filter wrong: $TAG_RESULT"; FAIL=$((FAIL+1))
fi

# ─── Section 6: Account deletion cooling-off ──────────────────────────
echo ""
echo "── Account deletion cooling-off ──"

# Request deletion as staff
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" -X POST "$BASE/account/delete")
check "POST /account/delete → 200" "200" "$R"

# Staff can still log in during cooling-off
CK_TEST="/tmp/rem_test"
R=$(curl -s -o /dev/null -w "%{http_code}" -c "$CK_TEST" -X POST "$BASE/auth/login" -H "Content-Type: application/json" \
  -d '{"username":"remstaff","password":"RemediationStf12"}')
check "Login during cooling-off → 200" "200" "$R"

# Cancel deletion
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" -X POST "$BASE/account/cancel-deletion")
check "POST /account/cancel-deletion → 200" "200" "$R"

# Cancel again → 409 (nothing to cancel)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" -X POST "$BASE/account/cancel-deletion")
check "Cancel again → 409" "409" "$R"
rm -f "$CK_TEST"

# ─── Section 7: Idempotency replay ────────────────────────────────────
echo ""
echo "── Idempotency middleware ──"

# Same key + same body + same actor → replay
KEY=$(uuidgen 2>/dev/null || date +%s%N)
IBODY1=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -H "Idempotency-Key: $KEY" -d '{"intake_type":"supply","details":"idem1"}')
IID1=$(echo "$IBODY1" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

# Check replay header on retry
RETRY_HDR=$(curl -s -D - -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -H "Idempotency-Key: $KEY" -d '{"intake_type":"supply","details":"idem1"}' -o /tmp/rem_idem_body.json)
if echo "$RETRY_HDR" | grep -qi "idempotent-replay: true"; then
    echo "PASS: Idempotency replay header set"; PASS=$((PASS+1))
else
    echo "FAIL: No Idempotent-Replay header on retry"; FAIL=$((FAIL+1))
fi

# Body should match original
IBODY2=$(cat /tmp/rem_idem_body.json)
IID2=$(echo "$IBODY2" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
if [ "$IID1" = "$IID2" ]; then
    echo "PASS: Idempotent replay returns same body"; PASS=$((PASS+1))
else
    echo "FAIL: Replay body differs ($IID1 vs $IID2)"; FAIL=$((FAIL+1))
fi

# Different actor with same key → NOT a replay
IBODY3=$(curl -s -b "$STAFF_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -H "Idempotency-Key: $KEY" -d '{"intake_type":"supply","details":"idem-staff"}')
IID3=$(echo "$IBODY3" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
if [ -n "$IID3" ] && [ "$IID3" != "$IID1" ]; then
    echo "PASS: Cross-actor idempotency isolated"; PASS=$((PASS+1))
else
    echo "FAIL: Cross-actor idempotency leaked"; FAIL=$((FAIL+1))
fi
rm -f /tmp/rem_idem_body.json

# ─── Section 8: Anti-passback retry_after_seconds ─────────────────────
echo ""
echo "── Anti-passback retry_after_seconds ──"

curl -s -b "$ADMIN_CK" -X POST "$BASE/members" -H "Content-Type: application/json" \
  -d '{"member_id":"REM_M1","name":"Test Member"}' > /dev/null
curl -s -b "$ADMIN_CK" -X POST "$BASE/checkin" -H "Content-Type: application/json" \
  -d '{"member_id":"REM_M1"}' > /dev/null

SECOND=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/checkin" -H "Content-Type: application/json" \
  -d '{"member_id":"REM_M1"}')
if echo "$SECOND" | grep -q '"retry_after_seconds"'; then
    echo "PASS: Anti-passback response includes retry_after_seconds"; PASS=$((PASS+1))
else
    echo "FAIL: retry_after_seconds missing: $SECOND"; FAIL=$((FAIL+1))
fi
if echo "$SECOND" | grep -q '"code":"ANTI_PASSBACK"'; then
    echo "PASS: Anti-passback code present"; PASS=$((PASS+1))
else
    echo "FAIL: ANTI_PASSBACK code missing: $SECOND"; FAIL=$((FAIL+1))
fi

# ─── Section 9: Error sanitization ────────────────────────────────────
echo ""
echo "── Error sanitization ──"

# Try to trigger an internal error path by sending malformed input through
# a validated route. Most paths now return VALIDATION_ERROR instead of
# INTERNAL. We confirm the response does NOT contain raw sqlx/database
# error fragments.
BAD=$(curl -s -b "$ADMIN_CK" -X PATCH "$BASE/intake/nonexistent-id/status" \
  -H "Content-Type: application/json" -d '{"status":"in_care"}')
# Should be 404, not a raw DB error
if echo "$BAD" | grep -qi "sqlx\|sqlite\|error code\|no such table"; then
    echo "FAIL: Response leaks internal DB details: $BAD"; FAIL=$((FAIL+1))
else
    echo "PASS: Response does not leak internal DB details"; PASS=$((PASS+1))
fi

# ─── Section 10: Diagnostics ZIP lifecycle ────────────────────────────
echo ""
echo "── Diagnostics ZIP lifecycle ──"

DZIP=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/admin/diagnostics/export")
DZID=$(echo "$DZIP" | grep -o '"download_id":"[^"]*"' | cut -d'"' -f4)
if [ -n "$DZID" ]; then
    echo "PASS: Diagnostics export returned download_id"; PASS=$((PASS+1))
else
    echo "FAIL: No download_id in diagnostics response: $DZIP"; FAIL=$((FAIL+1))
fi
if echo "$DZIP" | grep -q '"size_bytes"'; then
    echo "PASS: Diagnostics response includes size_bytes (real file)"; PASS=$((PASS+1))
else
    echo "FAIL: size_bytes missing from diagnostics response"; FAIL=$((FAIL+1))
fi
# Download and sniff magic bytes
curl -s -o /tmp/rem_diag.zip -b "$ADMIN_CK" "$BASE/admin/diagnostics/download/$DZID"
# ZIP starts with PK\x03\x04 (0x504b0304)
if head -c 4 /tmp/rem_diag.zip 2>/dev/null | od -An -tx1 | tr -d ' \n' | grep -q "504b0304"; then
    echo "PASS: Downloaded file is a valid ZIP (PK magic)"; PASS=$((PASS+1))
else
    echo "FAIL: Downloaded file is not a ZIP"; FAIL=$((FAIL+1))
fi
rm -f /tmp/rem_diag.zip

# Staff cannot export
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" -X POST "$BASE/admin/diagnostics/export")
check "Staff diagnostics export → 403" "403" "$R"

# ─── Section 11: Key rotation ─────────────────────────────────────────
echo ""
echo "── Key rotation ──"

# Add an address so we have something to re-encrypt
curl -s -b "$ADMIN_CK" -X POST "$BASE/address-book" -H "Content-Type: application/json" \
  -d '{"label":"Rotation","street":"1 Key St","city":"Town","state":"CA","zip_plus4":"90210-0000","phone":"555-111-2222"}' > /dev/null

# New key (different from default)
NEW_KEY="1111111111111111111111111111111111111111111111111111111111111111"
R=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/admin/security/rotate-key" -H "Content-Type: application/json" \
  -d "{\"new_key_hex\":\"$NEW_KEY\"}")
if echo "$R" | grep -q '"rotated_rows"'; then
    echo "PASS: Key rotation returned rotated_rows count"; PASS=$((PASS+1))
else
    echo "FAIL: Rotation response missing rotated_rows: $R"; FAIL=$((FAIL+1))
fi

# Verify address list still decrypts correctly with the new key
AFTER=$(curl -s -b "$ADMIN_CK" "$BASE/address-book")
if echo "$AFTER" | grep -q '"Rotation"'; then
    echo "PASS: Address book still readable after rotation (label present)"; PASS=$((PASS+1))
else
    echo "FAIL: Address book lost after rotation: $AFTER"; FAIL=$((FAIL+1))
fi

# Invalid new key → 400
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/admin/security/rotate-key" -H "Content-Type: application/json" \
  -d '{"new_key_hex":"notvalidhex"}')
check "Invalid hex key → 400" "400" "$R"

# ─── Summary ──────────────────────────────────────────────────────────
rm -f "$ADMIN_CK" "$STAFF_CK" "$AUDITOR_CK"
echo ""
echo "========================================"
echo "  Remediation Tests - Passed: $PASS  Failed: $FAIL"
echo "========================================"
[ $FAIL -gt 0 ] && exit 1
exit 0
