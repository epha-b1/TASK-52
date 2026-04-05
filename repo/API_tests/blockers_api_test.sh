#!/bin/bash
#
# Blocker fixes test suite — validates:
#   1. Auditor forbidden on address-book writes (HIGH)
#   2. Account deletion request + cancel + 7-day purge with FK-linked data (HIGH)
#   3. Config version cap stays <= 10 across update+rollback (MEDIUM)
#   4. Diagnostics ZIP contains full config snapshots + logs + metrics (MEDIUM)
#   5. structured_logs rows inserted on core operations (MEDIUM)
#   6. No sensitive data leak in log exports (MEDIUM)

set -e
PASS=0; FAIL=0
BASE="http://localhost:8080"
ADMIN_CK="/tmp/blk_admin"
STAFF_CK="/tmp/blk_staff"
AUDITOR_CK="/tmp/blk_auditor"

check() {
    local name="$1" expected="$2" actual="$3"
    if [ "$actual" = "$expected" ]; then
        echo "PASS: $name"; PASS=$((PASS+1))
    else
        echo "FAIL: $name (expected $expected, got $actual)"; FAIL=$((FAIL+1))
    fi
}

echo "=== Blockers Suite: 6 fix categories ==="

# ─── Setup: admin + staff + auditor ───────────────────────────────────
curl -s -c "$ADMIN_CK" -X POST "$BASE/auth/register" -H "Content-Type: application/json" \
  -d '{"username":"blkadmin","password":"BlockerAdm1234"}' > /dev/null

curl -s -b "$ADMIN_CK" -X POST "$BASE/users" -H "Content-Type: application/json" \
  -d '{"username":"blkstaff","password":"BlockerStf1234","role":"operations_staff"}' > /dev/null
curl -s -b "$ADMIN_CK" -X POST "$BASE/users" -H "Content-Type: application/json" \
  -d '{"username":"blkaud","password":"BlockerAud1234","role":"auditor"}' > /dev/null

curl -s -c "$STAFF_CK" -X POST "$BASE/auth/login" -H "Content-Type: application/json" \
  -d '{"username":"blkstaff","password":"BlockerStf1234"}' > /dev/null
curl -s -c "$AUDITOR_CK" -X POST "$BASE/auth/login" -H "Content-Type: application/json" \
  -d '{"username":"blkaud","password":"BlockerAud1234"}' > /dev/null

# ═══════════════════════════════════════════════════════════════════════
# 1. HIGH — Auditor forbidden on address-book writes
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 1. Auditor forbidden on address-book writes (HIGH) ━━━"

# Auditor create → 403
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/address-book" \
  -H "Content-Type: application/json" \
  -d '{"label":"X","street":"1 A","city":"B","state":"C","zip_plus4":"12345-6789","phone":"555"}')
check "Auditor POST /address-book → 403" "403" "$R"

# Admin creates one for the update/delete tests
ABODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/address-book" -H "Content-Type: application/json" \
  -d '{"label":"Home","street":"1 Main","city":"City","state":"CA","zip_plus4":"90210-1234","phone":"555-123-0000"}')
ADDR_ID=$(echo "$ABODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X PATCH "$BASE/address-book/$ADDR_ID" \
  -H "Content-Type: application/json" \
  -d '{"label":"Y","street":"2 B","city":"D","state":"E","zip_plus4":"12345-0000","phone":"555"}')
check "Auditor PATCH /address-book/:id → 403" "403" "$R"

R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X DELETE "$BASE/address-book/$ADDR_ID")
check "Auditor DELETE /address-book/:id → 403" "403" "$R"

# Auditor can still READ
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" "$BASE/address-book")
check "Auditor GET /address-book (read allowed) → 200" "200" "$R"

# Admin cleanup
curl -s -b "$ADMIN_CK" -X DELETE "$BASE/address-book/$ADDR_ID" > /dev/null

# ═══════════════════════════════════════════════════════════════════════
# 2. HIGH — Account deletion with FK-linked resources
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 2. Account deletion + 7-day purge with FK-linked data (HIGH) ━━━"

# Staff creates an intake, inspection, evidence, and supply entry — these
# will all carry FK references to staff's user id. After purge the records
# must remain but personal references must be anonymized.
IBODY=$(curl -s -b "$STAFF_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"animal","details":"{\"note\":\"purge-test\"}"}')
INTAKE_ID=$(echo "$IBODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

INSPBODY=$(curl -s -b "$STAFF_CK" -X POST "$BASE/inspections" -H "Content-Type: application/json" \
  -d "{\"intake_id\":\"$INTAKE_ID\"}")
INSP_ID=$(echo "$INSPBODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

# Upload evidence
UB=$(curl -s -b "$STAFF_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"staff.jpg","media_type":"photo","total_size":1024,"duration_seconds":0}')
UPID=$(echo "$UB" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$STAFF_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UPID\",\"chunk_index\":0}" > /dev/null
EB=$(curl -s -b "$STAFF_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$UPID\",\"fingerprint\":\"aabbccdd11223344\",\"total_size\":1024,\"exif_capture_time\":null,\"tags\":\"x\",\"keyword\":\"y\"}")
EV_ID=$(echo "$EB" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

# Staff adds an address book entry (personal data — will be wiped)
curl -s -b "$STAFF_CK" -X POST "$BASE/address-book" -H "Content-Type: application/json" \
  -d '{"label":"Purge","street":"9 Purge St","city":"PurgeCity","state":"CA","zip_plus4":"90210-9999","phone":"555-999-0000"}' > /dev/null

# Staff requests deletion
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" -X POST "$BASE/account/delete")
check "Staff POST /account/delete → 200" "200" "$R"

# Cancel works
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" -X POST "$BASE/account/cancel-deletion")
check "Staff POST /account/cancel-deletion → 200" "200" "$R"

# Cancel again → 409 (nothing pending)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$STAFF_CK" -X POST "$BASE/account/cancel-deletion")
check "Cancel again (no pending) → 409" "409" "$R"

# Request again, then admin runs purge with grace_period_days=0 (immediate)
curl -s -b "$STAFF_CK" -X POST "$BASE/account/delete" > /dev/null
PURGE_RESP=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/admin/account-purge" -H "Content-Type: application/json" \
  -d '{"grace_period_days":0}')
if echo "$PURGE_RESP" | grep -q '"purged":1'; then
    echo "PASS: Admin account-purge returned purged=1"; PASS=$((PASS+1))
else
    echo "FAIL: Unexpected purge response: $PURGE_RESP"; FAIL=$((FAIL+1))
fi

# Staff can no longer log in (anonymized)
R=$(curl -s -o /dev/null -w "%{http_code}" -X POST "$BASE/auth/login" \
  -H "Content-Type: application/json" \
  -d '{"username":"blkstaff","password":"BlockerStf1234"}')
check "Login after purge → 401" "401" "$R"

# Admin /users should NOT list the anonymized user
UL=$(curl -s -b "$ADMIN_CK" "$BASE/users")
if echo "$UL" | grep -q '"blkstaff"'; then
    echo "FAIL: anonymized user still appears in /users: $UL"; FAIL=$((FAIL+1))
else
    echo "PASS: anonymized user removed from /users listing"; PASS=$((PASS+1))
fi

# But the records (intake/inspection/evidence) still exist — preserved
# for audit. Admin can still retrieve them.
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" "$BASE/intake/$INTAKE_ID")
check "Intake preserved after owner purge → 200" "200" "$R"

# Evidence list still shows the upload
EV_LIST=$(curl -s -b "$ADMIN_CK" "$BASE/evidence")
if echo "$EV_LIST" | grep -q "$EV_ID"; then
    echo "PASS: Evidence preserved after uploader purge"; PASS=$((PASS+1))
else
    echo "FAIL: Evidence lost after purge"; FAIL=$((FAIL+1))
fi

# Purge again with nothing pending → purged=0
PURGE2=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/admin/account-purge" -H "Content-Type: application/json" \
  -d '{"grace_period_days":0}')
if echo "$PURGE2" | grep -q '"purged":0'; then
    echo "PASS: Purge with no pending → 0"; PASS=$((PASS+1))
else
    echo "FAIL: Unexpected second purge: $PURGE2"; FAIL=$((FAIL+1))
fi

# Staff ck is now invalid
rm -f "$STAFF_CK"

# ═══════════════════════════════════════════════════════════════════════
# 3. MEDIUM — Config version cap after rollback
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 3. Config version cap after repeated update + rollback (MEDIUM) ━━━"

# Push 15 configs
for i in $(seq 1 15); do
    curl -s -b "$ADMIN_CK" -X PATCH "$BASE/admin/config" \
      -H "Content-Type: application/json" \
      -d "{\"iter\":$i}" > /dev/null
done

# Then 10 rollbacks to the earliest visible version
FIRST=$(curl -s -b "$ADMIN_CK" "$BASE/admin/config/versions" | \
    grep -o '"id":[0-9]*' | tail -1 | cut -d':' -f2)
for _ in $(seq 1 10); do
    curl -s -b "$ADMIN_CK" -X POST "$BASE/admin/config/rollback/$FIRST" > /dev/null
done

# Version list must still return <= 10
VCOUNT=$(curl -s -b "$ADMIN_CK" "$BASE/admin/config/versions" | grep -o '"id":' | wc -l)
if [ "$VCOUNT" -le 10 ]; then
    echo "PASS: Config version count after update+rollback = $VCOUNT (≤10)"; PASS=$((PASS+1))
else
    echo "FAIL: Version count $VCOUNT exceeds cap"; FAIL=$((FAIL+1))
fi

# ═══════════════════════════════════════════════════════════════════════
# 4. MEDIUM — Diagnostics package contains full config snapshots + logs
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 4. Diagnostics ZIP contains snapshots/logs/metrics (MEDIUM) ━━━"

# Trigger a diagnostic export
DIAG=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/admin/diagnostics/export")
DZID=$(echo "$DIAG" | grep -o '"download_id":"[^"]*"' | cut -d'"' -f4)

# Download
curl -s -o /tmp/diag_$$.zip -b "$ADMIN_CK" "$BASE/admin/diagnostics/download/$DZID"

# Inspect with unzip — must list logs.txt, metrics.json, config_history.json, audit_summary.csv
LIST=$(unzip -l /tmp/diag_$$.zip 2>&1 || echo "UNZIP_FAILED")
for f in logs.txt metrics.json config_history.json audit_summary.csv; do
    if echo "$LIST" | grep -q "$f"; then
        echo "PASS: diagnostics zip contains $f"; PASS=$((PASS+1))
    else
        echo "FAIL: diagnostics zip missing $f"; FAIL=$((FAIL+1))
    fi
done

# Extract and confirm config_history.json is JSON array with snapshot keys
unzip -p /tmp/diag_$$.zip config_history.json > /tmp/cfg_$$.json 2>/dev/null || true
if [ -s /tmp/cfg_$$.json ] && grep -q '"snapshot"' /tmp/cfg_$$.json; then
    echo "PASS: config_history.json contains snapshot payloads"; PASS=$((PASS+1))
else
    echo "FAIL: config_history.json missing snapshot payloads"; FAIL=$((FAIL+1))
fi

# logs.txt must be non-empty (we've generated many slog rows by now)
unzip -p /tmp/diag_$$.zip logs.txt > /tmp/logs_$$.txt 2>/dev/null || true
if [ -s /tmp/logs_$$.txt ]; then
    echo "PASS: logs.txt is non-empty"; PASS=$((PASS+1))
else
    echo "FAIL: logs.txt is empty"; FAIL=$((FAIL+1))
fi

# Sensitive leak check: logs.txt must not contain "password" or known secrets
if grep -qiE "password|\\\$argon2|BlockerAdm1234|BlockerStf1234|BlockerAud1234" /tmp/logs_$$.txt; then
    echo "FAIL: logs.txt contains sensitive data"; FAIL=$((FAIL+1))
else
    echo "PASS: logs.txt has no sensitive data"; PASS=$((PASS+1))
fi

rm -f /tmp/diag_$$.zip /tmp/cfg_$$.json /tmp/logs_$$.txt

# ═══════════════════════════════════════════════════════════════════════
# 5. MEDIUM — structured_logs rows on core operations
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 5. structured_logs inserted on core ops (MEDIUM) ━━━"

# Trigger a new intake as admin so we know a slog row is fresh
curl -s -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"supply","details":"slog-check"}' > /dev/null

# Trigger a traceability publish
T1BODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/traceability" -H "Content-Type: application/json" \
  -d '{"intake_id":null}')
T1ID=$(echo "$T1BODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/traceability/$T1ID/publish" -H "Content-Type: application/json" \
  -d '{"comment":"slog-check"}' > /dev/null

# Query /admin/logs
LOGS=$(curl -s -b "$ADMIN_CK" "$BASE/admin/logs")
if echo "$LOGS" | grep -q "intake.create"; then
    echo "PASS: structured_logs has intake.create rows"; PASS=$((PASS+1))
else
    echo "FAIL: no intake.create in structured_logs: $LOGS" | head -c 500; echo
    FAIL=$((FAIL+1))
fi
if echo "$LOGS" | grep -q "traceability.publish"; then
    echo "PASS: structured_logs has traceability.publish rows"; PASS=$((PASS+1))
else
    echo "FAIL: no traceability.publish in structured_logs"; FAIL=$((FAIL+1))
fi
if echo "$LOGS" | grep -q "auth.login failed"; then
    echo "PASS: structured_logs has failed-login entries from earlier setup"; PASS=$((PASS+1))
else
    # Force a failed login to generate one
    curl -s -o /dev/null -X POST "$BASE/auth/login" -H "Content-Type: application/json" \
      -d '{"username":"nope","password":"WrongPasssss"}'
    LOGS2=$(curl -s -b "$ADMIN_CK" "$BASE/admin/logs")
    if echo "$LOGS2" | grep -q "auth.login failed"; then
        echo "PASS: structured_logs records failed-login"; PASS=$((PASS+1))
    else
        echo "FAIL: failed-login not logged"; FAIL=$((FAIL+1))
    fi
fi

# Staff cannot access /admin/logs (admin-only guard)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" "$BASE/admin/logs")
check "Non-admin /admin/logs → 403" "403" "$R"

# ═══════════════════════════════════════════════════════════════════════
# 6. MEDIUM — No sensitive leak in responses / log exports
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 6. No sensitive leak (MEDIUM) ━━━"

# /admin/logs response must not leak passwords or argon hashes
LOGS=$(curl -s -b "$ADMIN_CK" "$BASE/admin/logs")
if echo "$LOGS" | grep -qiE "BlockerAdm1234|BlockerAud1234|\\\$argon2|password_hash"; then
    echo "FAIL: /admin/logs leaks sensitive data"; FAIL=$((FAIL+1))
else
    echo "PASS: /admin/logs response is clean"; PASS=$((PASS+1))
fi

# Audit CSV export also must not leak passwords
AUDIT=$(curl -s -b "$ADMIN_CK" "$BASE/audit-logs/export")
if echo "$AUDIT" | grep -qiE "BlockerAdm1234|BlockerAud1234|\\\$argon2"; then
    echo "FAIL: audit export leaks credentials"; FAIL=$((FAIL+1))
else
    echo "PASS: audit export is clean"; PASS=$((PASS+1))
fi

# Trigger a forced DB-error path and confirm it doesn't leak internals.
# (Hitting an invalid Path param forces a 404 with sanitized envelope.)
BAD=$(curl -s -b "$ADMIN_CK" "$BASE/intake/definitely-not-a-real-id")
if echo "$BAD" | grep -qiE "sqlx|sqlite|rusqlite|Error\\{"; then
    echo "FAIL: error response leaks DB internals: $BAD"; FAIL=$((FAIL+1))
else
    echo "PASS: error response does not leak DB internals"; PASS=$((PASS+1))
fi

# ═══════════════════════════════════════════════════════════════════════
# 7. 365-day evidence retention with legal-hold + linked exceptions
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 7. 365-day evidence retention ━━━"

upload_evidence_as_admin() {
    local name="$1"
    local ub
    ub=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
      -d "{\"filename\":\"$name.jpg\",\"media_type\":\"photo\",\"total_size\":1048576,\"duration_seconds\":0}")
    local uid
    uid=$(echo "$ub" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
    curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
      -d "{\"upload_id\":\"$uid\",\"chunk_index\":0}" > /dev/null
    local eb
    eb=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
      -d "{\"upload_id\":\"$uid\",\"fingerprint\":\"retent01abcdef02\",\"total_size\":1048576,\"exif_capture_time\":null,\"tags\":\"ret\",\"keyword\":\"$name\"}")
    echo "$eb" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4
}

# Create an intake so we have a link target.
IBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"animal","details":"retention-target"}')
RET_INTAKE_ID=$(echo "$IBODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

# Three evidence rows: plain, linked, legal-hold.
EV_PLAIN=$(upload_evidence_as_admin "plain_retent")
EV_LINKED=$(upload_evidence_as_admin "linked_retent")
EV_HOLD=$(upload_evidence_as_admin "hold_retent")

# Link the middle one to the intake so it becomes un-purgeable.
curl -s -b "$ADMIN_CK" -X POST "$BASE/evidence/$EV_LINKED/link" -H "Content-Type: application/json" \
  -d "{\"target_type\":\"intake\",\"target_id\":\"$RET_INTAKE_ID\"}" > /dev/null

# Put the third on legal hold.
curl -s -b "$ADMIN_CK" -X PATCH "$BASE/evidence/$EV_HOLD/legal-hold" -H "Content-Type: application/json" \
  -d '{"legal_hold":true}' > /dev/null

# Run the retention sweep with max_age_days=0 so every just-inserted row
# that is not linked and not legal-hold is eligible.
RSWEEP=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/admin/retention-purge" -H "Content-Type: application/json" \
  -d '{"max_age_days":0}')
if echo "$RSWEEP" | grep -q '"deleted"'; then
    echo "PASS: retention-purge returns deleted count: $RSWEEP"; PASS=$((PASS+1))
else
    echo "FAIL: retention-purge missing deleted field: $RSWEEP"; FAIL=$((FAIL+1))
fi

# Verify plain row is gone.
LIST=$(curl -s -b "$ADMIN_CK" "$BASE/evidence")
if echo "$LIST" | grep -q "$EV_PLAIN"; then
    echo "FAIL: plain evidence still present after retention sweep"; FAIL=$((FAIL+1))
else
    echo "PASS: plain evidence deleted by retention sweep"; PASS=$((PASS+1))
fi

# Verify linked row is preserved.
if echo "$LIST" | grep -q "$EV_LINKED"; then
    echo "PASS: linked evidence preserved by retention sweep"; PASS=$((PASS+1))
else
    echo "FAIL: linked evidence wrongly deleted"; FAIL=$((FAIL+1))
fi

# Verify legal-hold row is preserved.
if echo "$LIST" | grep -q "$EV_HOLD"; then
    echo "PASS: legal-hold evidence preserved by retention sweep"; PASS=$((PASS+1))
else
    echo "FAIL: legal-hold evidence wrongly deleted"; FAIL=$((FAIL+1))
fi

# Non-admin cannot trigger retention.
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/admin/retention-purge" \
  -H "Content-Type: application/json" -d '{"max_age_days":0}')
check "Non-admin /admin/retention-purge → 403" "403" "$R"

# Negative days rejected.
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/admin/retention-purge" \
  -H "Content-Type: application/json" -d '{"max_age_days":-1}')
check "Negative max_age_days → 400" "400" "$R"

# ═══════════════════════════════════════════════════════════════════════
# 8. Local media compression on upload_complete
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 8. Local media compression ━━━"

# Upload a photo with a size above the floor (256 KiB) so compression applies.
PHB=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"comp_photo.jpg","media_type":"photo","total_size":1048576,"duration_seconds":0}')
PHUID=$(echo "$PHB" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$PHUID\",\"chunk_index\":0}" > /dev/null
PHRESP=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$PHUID\",\"fingerprint\":\"compphoto1234567\",\"total_size\":1048576,\"exif_capture_time\":null,\"tags\":\"c\",\"keyword\":\"c\"}")

# Compression metadata must be present
for field in compressed_bytes compression_ratio compression_applied; do
    if echo "$PHRESP" | grep -q "\"$field\""; then
        echo "PASS: photo upload response has $field"; PASS=$((PASS+1))
    else
        echo "FAIL: photo upload response missing $field: $PHRESP"; FAIL=$((FAIL+1))
    fi
done

# Compression must actually have been applied at 1 MiB (above the 256 KiB floor)
if echo "$PHRESP" | grep -q '"compression_applied":true'; then
    echo "PASS: compression applied on 1 MiB photo"; PASS=$((PASS+1))
else
    echo "FAIL: compression not applied on 1 MiB photo"; FAIL=$((FAIL+1))
fi

# Ratio must be 0.7 for photo
if echo "$PHRESP" | grep -qE '"compression_ratio":0\.7'; then
    echo "PASS: photo compression ratio 0.7"; PASS=$((PASS+1))
else
    echo "FAIL: photo compression ratio wrong: $PHRESP"; FAIL=$((FAIL+1))
fi

# Upload a tiny photo below the 256 KiB floor — compression should NOT apply
SMB=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"tiny.jpg","media_type":"photo","total_size":10000,"duration_seconds":0}')
SMUID=$(echo "$SMB" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$SMUID\",\"chunk_index\":0}" > /dev/null
SMRESP=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$SMUID\",\"fingerprint\":\"tinyphoto1234567\",\"total_size\":10000,\"exif_capture_time\":null,\"tags\":\"c\",\"keyword\":\"c\"}")
if echo "$SMRESP" | grep -q '"compression_applied":false'; then
    echo "PASS: tiny photo below floor NOT compressed"; PASS=$((PASS+1))
else
    echo "FAIL: tiny photo wrongly compressed: $SMRESP"; FAIL=$((FAIL+1))
fi

# Upload an audio file with different ratio (0.5)
AB=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/start" -H "Content-Type: application/json" \
  -d '{"filename":"a.m4a","media_type":"audio","total_size":1048576,"duration_seconds":10}')
AUID=$(echo "$AB" | grep -o '"upload_id":"[^"]*"' | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/chunk" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$AUID\",\"chunk_index\":0}" > /dev/null
ARESP=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/media/upload/complete" -H "Content-Type: application/json" \
  -d "{\"upload_id\":\"$AUID\",\"fingerprint\":\"audioclip1234567\",\"total_size\":1048576,\"exif_capture_time\":null,\"tags\":\"c\",\"keyword\":\"c\"}")
if echo "$ARESP" | grep -qE '"compression_ratio":0\.5'; then
    echo "PASS: audio compression ratio 0.5"; PASS=$((PASS+1))
else
    echo "FAIL: audio compression ratio wrong: $ARESP"; FAIL=$((FAIL+1))
fi

# Compressed size must not exceed original
COMP=$(echo "$PHRESP" | grep -o '"compressed_bytes":[0-9]*' | cut -d':' -f2)
if [ -n "$COMP" ] && [ "$COMP" -le "1048576" ] && [ "$COMP" -lt "1048576" ]; then
    echo "PASS: compressed_bytes ($COMP) < original (1048576)"; PASS=$((PASS+1))
else
    echo "FAIL: compressed_bytes suspicious: $COMP"; FAIL=$((FAIL+1))
fi

# ═══════════════════════════════════════════════════════════════════════
# 9. Auditor cannot create traceability codes (HIGH role-policy fix)
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 9. Auditor traceability create = 403 ━━━"

# Auditor POST /traceability → 403 (role matrix: create is admin/staff only)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/traceability" \
  -H "Content-Type: application/json" -d '{"intake_id":null}')
check "Auditor POST /traceability → 403" "403" "$R"

# Admin POST /traceability → 201 (still allowed)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/traceability" \
  -H "Content-Type: application/json" -d '{"intake_id":null}')
check "Admin POST /traceability → 201" "201" "$R"

# Auditor still CAN publish/retract (the explicit auditor-allowed mutations).
# Create a fresh code as admin, then auditor publishes it.
TC=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/traceability" -H "Content-Type: application/json" \
  -d '{"intake_id":null}')
TCID=$(echo "$TC" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/traceability/$TCID/publish" \
  -H "Content-Type: application/json" -d '{"comment":"auditor publishing OK"}')
check "Auditor POST /traceability/:id/publish → 200 (explicitly allowed)" "200" "$R"

# ═══════════════════════════════════════════════════════════════════════
# 10. Transfers first-class lifecycle
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 10. Transfers lifecycle + state machine ━━━"

# Seed an intake so transfers can reference it
TIBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"supply","details":"transfer-target","region":"north","tags":"rescue,priority"}')
TIID=$(echo "$TIBODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)

# Create transfer (queued) as admin
TRBODY=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/transfers" -H "Content-Type: application/json" \
  -d "{\"intake_id\":\"$TIID\",\"destination\":\"FAC02\",\"reason\":\"overflow\",\"notes\":\"\"}")
TRID=$(echo "$TRBODY" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
if [ -n "$TRID" ] && echo "$TRBODY" | grep -q '"status":"queued"'; then
    echo "PASS: transfer created in queued state"; PASS=$((PASS+1))
else
    echo "FAIL: transfer create response wrong: $TRBODY"; FAIL=$((FAIL+1))
fi

# Auditor cannot create a transfer
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/transfers" \
  -H "Content-Type: application/json" -d '{"destination":"FAC03"}')
check "Auditor POST /transfers → 403" "403" "$R"

# Missing destination → 400
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/transfers" \
  -H "Content-Type: application/json" -d '{"destination":""}')
check "Transfer without destination → 400" "400" "$R"

# Happy-path state machine: queued → approved → in_transit → received
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X PATCH "$BASE/transfers/$TRID/status" \
  -H "Content-Type: application/json" -d '{"status":"approved"}')
check "queued → approved → 200" "200" "$R"
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X PATCH "$BASE/transfers/$TRID/status" \
  -H "Content-Type: application/json" -d '{"status":"in_transit"}')
check "approved → in_transit → 200" "200" "$R"
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X PATCH "$BASE/transfers/$TRID/status" \
  -H "Content-Type: application/json" -d '{"status":"received"}')
check "in_transit → received → 200" "200" "$R"

# Invalid transitions → 409
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X PATCH "$BASE/transfers/$TRID/status" \
  -H "Content-Type: application/json" -d '{"status":"queued"}')
check "received → queued → 409 (terminal)" "409" "$R"

# Create a second transfer to test cancel
TR2=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/transfers" -H "Content-Type: application/json" \
  -d '{"destination":"FAC04","reason":"supply","notes":""}')
TR2ID=$(echo "$TR2" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X PATCH "$BASE/transfers/$TR2ID/status" \
  -H "Content-Type: application/json" -d '{"status":"canceled"}')
check "queued → canceled → 200" "200" "$R"

# Auditor cannot update status either
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X PATCH "$BASE/transfers/$TR2ID/status" \
  -H "Content-Type: application/json" -d '{"status":"approved"}')
check "Auditor PATCH /transfers/:id/status → 403" "403" "$R"

# Listing works
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" "$BASE/transfers")
check "GET /transfers → 200" "200" "$R"

# ═══════════════════════════════════════════════════════════════════════
# 11. Stock movements ledger → inventory_on_hand
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 11. Stock movements ledger drives inventory ━━━"

# Fresh baseline: query inventory BEFORE any movements
BEFORE=$(curl -s -b "$ADMIN_CK" "$BASE/stock/inventory")
BEFORE_TOT=$(echo "$BEFORE" | grep -o '"total_on_hand":-*[0-9]*' | cut -d':' -f2)
if [ -z "$BEFORE_TOT" ]; then BEFORE_TOT=0; fi

# Receipt: +50
curl -s -b "$ADMIN_CK" -X POST "$BASE/stock/movements" -H "Content-Type: application/json" \
  -d '{"supply_id":null,"quantity_delta":50,"reason":"receipt","notes":"truck delivery"}' > /dev/null

# Allocation: -20
curl -s -b "$ADMIN_CK" -X POST "$BASE/stock/movements" -H "Content-Type: application/json" \
  -d '{"supply_id":null,"quantity_delta":-20,"reason":"allocation","notes":"to clinic"}' > /dev/null

# Adjustment: +5 (reconciliation)
curl -s -b "$ADMIN_CK" -X POST "$BASE/stock/movements" -H "Content-Type: application/json" \
  -d '{"supply_id":null,"quantity_delta":5,"reason":"adjustment","notes":"recount"}' > /dev/null

# After: should be BEFORE_TOT + 50 - 20 + 5 = BEFORE_TOT + 35
AFTER=$(curl -s -b "$ADMIN_CK" "$BASE/stock/inventory")
AFTER_TOT=$(echo "$AFTER" | grep -o '"total_on_hand":-*[0-9]*' | cut -d':' -f2)
EXPECTED=$((BEFORE_TOT + 35))
if [ "$AFTER_TOT" = "$EXPECTED" ]; then
    echo "PASS: inventory_on_hand = $AFTER_TOT (expected $EXPECTED)"; PASS=$((PASS+1))
else
    echo "FAIL: inventory_on_hand = $AFTER_TOT, expected $EXPECTED"; FAIL=$((FAIL+1))
fi

# Sign sanity: receipt with negative delta → 400
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/stock/movements" \
  -H "Content-Type: application/json" \
  -d '{"quantity_delta":-10,"reason":"receipt"}')
check "receipt with negative delta → 400" "400" "$R"

# Zero delta → 400
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/stock/movements" \
  -H "Content-Type: application/json" \
  -d '{"quantity_delta":0,"reason":"receipt"}')
check "zero delta → 400" "400" "$R"

# Invalid reason → 400
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" -X POST "$BASE/stock/movements" \
  -H "Content-Type: application/json" \
  -d '{"quantity_delta":10,"reason":"teleport"}')
check "invalid reason → 400" "400" "$R"

# Auditor cannot record movements
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/stock/movements" \
  -H "Content-Type: application/json" \
  -d '{"quantity_delta":1,"reason":"receipt"}')
check "Auditor POST /stock/movements → 403" "403" "$R"

# Dashboard summary now reflects the real inventory (not COUNT supply_entries)
SUMMARY=$(curl -s -b "$ADMIN_CK" "$BASE/reports/summary")
if echo "$SUMMARY" | grep -q "\"inventory_on_hand\":$EXPECTED"; then
    echo "PASS: /reports/summary.inventory_on_hand = $EXPECTED (from ledger)"
    PASS=$((PASS+1))
else
    echo "FAIL: summary did not reflect ledger total: $SUMMARY"; FAIL=$((FAIL+1))
fi

# ═══════════════════════════════════════════════════════════════════════
# 12. Dashboard filters (region / tags / q)
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 12. Dashboard filters: region / tags / q ━━━"

# Seed two more intakes with distinct region/tags
curl -s -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"animal","details":"filter-demo-a","region":"south","tags":"urgent"}' > /dev/null
curl -s -b "$ADMIN_CK" -X POST "$BASE/intake" -H "Content-Type: application/json" \
  -d '{"intake_type":"donation","details":"special note alpha","region":"east","tags":"routine"}' > /dev/null

# region=north should include at least the TIID row (seeded in section 10)
R=$(curl -s -b "$ADMIN_CK" "$BASE/reports/summary?region=north")
if echo "$R" | grep -q '"filters"'; then
    echo "PASS: region filter accepted and echoed"; PASS=$((PASS+1))
else
    echo "FAIL: region filter response wrong: $R"; FAIL=$((FAIL+1))
fi
if echo "$R" | grep -q '"rescue_volume":[1-9]'; then
    echo "PASS: region=north returns at least one matching intake"; PASS=$((PASS+1))
else
    echo "FAIL: region=north returned 0: $R"; FAIL=$((FAIL+1))
fi

# region=nonexistent should return 0 matches
R=$(curl -s -b "$ADMIN_CK" "$BASE/reports/summary?region=nonexistent-region")
if echo "$R" | grep -q '"rescue_volume":0'; then
    echo "PASS: unknown region → rescue_volume 0"; PASS=$((PASS+1))
else
    echo "FAIL: unknown region did not return 0: $R"; FAIL=$((FAIL+1))
fi

# tags filter (substring)
R=$(curl -s -b "$ADMIN_CK" "$BASE/reports/summary?tags=urgent")
if echo "$R" | grep -q '"rescue_volume":[1-9]'; then
    echo "PASS: tags=urgent matches"; PASS=$((PASS+1))
else
    echo "FAIL: tags=urgent did not match: $R"; FAIL=$((FAIL+1))
fi

# full-text q on details
R=$(curl -s -b "$ADMIN_CK" "$BASE/reports/summary?q=alpha")
if echo "$R" | grep -q '"rescue_volume":[1-9]'; then
    echo "PASS: q=alpha full-text hit"; PASS=$((PASS+1))
else
    echo "FAIL: q=alpha did not match: $R"; FAIL=$((FAIL+1))
fi

# Combined filters
R=$(curl -s -b "$ADMIN_CK" "$BASE/reports/summary?region=south&tags=urgent")
if echo "$R" | grep -q '"rescue_volume":[1-9]'; then
    echo "PASS: region + tags combination returns hit"; PASS=$((PASS+1))
else
    echo "FAIL: combined filter empty: $R"; FAIL=$((FAIL+1))
fi

# CSV export mirrors filters
EXP=$(curl -s -b "$ADMIN_CK" "$BASE/reports/export?region=north&tags=priority&q=overflow")
if echo "$EXP" | grep -q "filter_region,north" && echo "$EXP" | grep -q "filter_tags,priority"; then
    echo "PASS: CSV export echoes region + tags filters"; PASS=$((PASS+1))
else
    echo "FAIL: CSV export missing filter echo"; FAIL=$((FAIL+1))
fi

# ═══════════════════════════════════════════════════════════════════════
# 13. Traceability steps append-only timeline
# ═══════════════════════════════════════════════════════════════════════
echo ""
echo "━━━ 13. Traceability steps timeline ━━━"

# Create a new code; publish; retract; then fetch steps
NC=$(curl -s -b "$ADMIN_CK" -X POST "$BASE/traceability" -H "Content-Type: application/json" \
  -d '{"intake_id":null}')
NCID=$(echo "$NC" | grep -o '"id":"[^"]*"' | head -1 | cut -d'"' -f4)
curl -s -b "$ADMIN_CK" -X POST "$BASE/traceability/$NCID/publish" -H "Content-Type: application/json" \
  -d '{"comment":"steps test publish"}' > /dev/null
curl -s -b "$ADMIN_CK" -X POST "$BASE/traceability/$NCID/retract" -H "Content-Type: application/json" \
  -d '{"comment":"steps test retract"}' > /dev/null

# Manual note step (admin/staff only)
curl -s -b "$ADMIN_CK" -X POST "$BASE/traceability/$NCID/steps" -H "Content-Type: application/json" \
  -d '{"label":"Operator note","details":"Double-checked by supervisor"}' > /dev/null

# List steps
STEPS=$(curl -s -b "$ADMIN_CK" "$BASE/traceability/$NCID/steps")
if echo "$STEPS" | grep -q '"step_type":"create"'; then
    echo "PASS: timeline contains create step"; PASS=$((PASS+1))
else
    echo "FAIL: create step missing: $STEPS"; FAIL=$((FAIL+1))
fi
if echo "$STEPS" | grep -q '"step_type":"publish"'; then
    echo "PASS: timeline contains publish step"; PASS=$((PASS+1))
else
    echo "FAIL: publish step missing"; FAIL=$((FAIL+1))
fi
if echo "$STEPS" | grep -q '"step_type":"retract"'; then
    echo "PASS: timeline contains retract step"; PASS=$((PASS+1))
else
    echo "FAIL: retract step missing"; FAIL=$((FAIL+1))
fi
if echo "$STEPS" | grep -q '"step_type":"note"'; then
    echo "PASS: timeline contains manual note step"; PASS=$((PASS+1))
else
    echo "FAIL: manual note step missing"; FAIL=$((FAIL+1))
fi

# Auditor cannot append a manual note
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$AUDITOR_CK" -X POST "$BASE/traceability/$NCID/steps" \
  -H "Content-Type: application/json" -d '{"label":"nope","details":""}')
check "Auditor POST /traceability/:id/steps → 403" "403" "$R"

# Unknown code → 404
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$ADMIN_CK" "$BASE/traceability/nonexistent/steps")
check "Steps for unknown code → 404" "404" "$R"

# ─── Summary ──────────────────────────────────────────────────────────
rm -f "$ADMIN_CK" "$AUDITOR_CK"
echo ""
echo "========================================"
echo "  Blockers Tests - Passed: $PASS  Failed: $FAIL"
echo "========================================"
[ $FAIL -gt 0 ] && exit 1
exit 0
