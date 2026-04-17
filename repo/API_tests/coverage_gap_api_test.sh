#!/bin/bash
#
# Coverage-gap API tests — true HTTP coverage for 6 endpoints that were
# previously not exercised through the live HTTP route layer:
#
#   1. GET   /inspections
#   2. PATCH /supply-entries/:id/resolve
#   3. GET   /traceability
#   4. GET   /transfers/:id
#   5. GET   /stock/movements
#   6. PATCH /users/:id
#
# Every test below hits the real route through the running container
# (method + path explicit), seeds real data when required, and asserts on
# response-body contract — not just status codes — so the evidence closes
# the "status-only" observability gap flagged in the audit.

set -e
PASS=0; FAIL=0
BASE="http://localhost:8080"
CK_ADMIN=$(mktemp /tmp/cgap_admin_XXXXXX)
CK_STAFF=$(mktemp /tmp/cgap_staff_XXXXXX)
CK_AUDIT=$(mktemp /tmp/cgap_audit_XXXXXX)
trap 'rm -f "$CK_ADMIN" "$CK_STAFF" "$CK_AUDIT"' EXIT

echo "=== API Tests: Coverage Gap (6 endpoints) ==="

check() {
    local label="$1" expect="$2" got="$3"
    if [ "$got" = "$expect" ]; then
        echo "PASS: $label"; PASS=$((PASS+1))
    else
        echo "FAIL: $label (expected $expect, got $got)"; FAIL=$((FAIL+1))
    fi
}

assert_contains() {
    local label="$1" needle="$2" haystack="$3"
    if echo "$haystack" | grep -q "$needle"; then
        echo "PASS: $label"; PASS=$((PASS+1))
    else
        echo "FAIL: $label (missing '$needle' in: $haystack)"; FAIL=$((FAIL+1))
    fi
}

assert_not_contains() {
    local label="$1" needle="$2" haystack="$3"
    if echo "$haystack" | grep -q "$needle"; then
        echo "FAIL: $label (unexpectedly found '$needle')"; FAIL=$((FAIL+1))
    else
        echo "PASS: $label"; PASS=$((PASS+1))
    fi
}

# ── Bootstrap: admin + staff + auditor ────────────────────────────────
curl -s -c "$CK_ADMIN" -X POST "$BASE/auth/register" -H "Content-Type: application/json" \
  -d '{"username":"cgapadmin","password":"CoveragePass12"}' > /dev/null

curl -s -b "$CK_ADMIN" -X POST "$BASE/users" -H "Content-Type: application/json" \
  -d '{"username":"cgapstaff","password":"StaffPass12345","role":"operations_staff"}' > /dev/null
curl -s -c "$CK_STAFF" -X POST "$BASE/auth/login" -H "Content-Type: application/json" \
  -d '{"username":"cgapstaff","password":"StaffPass12345"}' > /dev/null

curl -s -b "$CK_ADMIN" -X POST "$BASE/users" -H "Content-Type: application/json" \
  -d '{"username":"cgapaudit","password":"AuditPass12345","role":"auditor"}' > /dev/null
curl -s -c "$CK_AUDIT" -X POST "$BASE/auth/login" -H "Content-Type: application/json" \
  -d '{"username":"cgapaudit","password":"AuditPass12345"}' > /dev/null

# Capture the auditor user_id for PATCH /users/:id tests later.
AUDITOR_ID=$(curl -s -b "$CK_ADMIN" "$BASE/users" | \
    jq -r '.[] | select(.username=="cgapaudit") | .id')


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 1. GET /inspections — list all inspections
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
echo ""
echo "━━━ 1. GET /inspections ━━━"

# Seed: an intake + an inspection against it so the list is non-empty.
INTAKE_ID=$(curl -s -b "$CK_ADMIN" -X POST "$BASE/intake" \
    -H "Content-Type: application/json" \
    -d '{"intake_type":"animal","details":"{\"tag\":\"insp-seed\"}"}' | \
    jq -r '.id')

INSP_ID=$(curl -s -b "$CK_ADMIN" -X POST "$BASE/inspections" \
    -H "Content-Type: application/json" \
    -d "{\"intake_id\":\"$INTAKE_ID\"}" | \
    jq -r '.id')

# GET /inspections (auth required)
R=$(curl -s -w "\n%{http_code}" -b "$CK_ADMIN" "$BASE/inspections")
CODE=$(echo "$R" | tail -1); BODY=$(echo "$R" | sed '$d')
check "GET /inspections → 200" "200" "$CODE"
assert_contains "GET /inspections body is a JSON array" "^\[" "$BODY"
assert_contains "GET /inspections returns seeded inspection id" "$INSP_ID" "$BODY"
assert_contains "GET /inspections element has intake_id field" "intake_id" "$BODY"
assert_contains "GET /inspections element has inspector_id field" "inspector_id" "$BODY"
assert_contains "GET /inspections element has status field" "\"status\"" "$BODY"
assert_contains "GET /inspections element has created_at field" "created_at" "$BODY"
assert_contains "GET /inspections element references seeded intake" "$INTAKE_ID" "$BODY"

# 401 without cookie
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/inspections")
check "GET /inspections (no auth) → 401" "401" "$R"


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 2. PATCH /supply-entries/:id/resolve
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
echo ""
echo "━━━ 2. PATCH /supply-entries/:id/resolve ━━━"

# Seed a supply entry with parser conflicts (unknown color + malformed
# size → parse_status = needs_review) so resolve has something to fix.
SUP_ID=$(curl -s -b "$CK_ADMIN" -X POST "$BASE/supply-entries" \
    -H "Content-Type: application/json" \
    -d '{"name":"CoverageCan","sku":"COV-01","size":"??","color":"chartreuse","notes":"seed","stock_status":"in_stock","media_references":"","review_summary":""}' | \
    jq -r '.id')

# Pre-check: the seeded row is in needs_review state (drives the resolve flow).
PRE=$(curl -s -b "$CK_ADMIN" "$BASE/supply-entries")
assert_contains "Seeded supply entry has parse_status=needs_review" "needs_review" "$PRE"

# Admin resolves the row with canonical values.
R=$(curl -s -w "\n%{http_code}" -b "$CK_ADMIN" -X PATCH "$BASE/supply-entries/$SUP_ID/resolve" \
    -H "Content-Type: application/json" \
    -d '{"canonical_color":"green","canonical_size":"M"}')
CODE=$(echo "$R" | tail -1); BODY=$(echo "$R" | sed '$d')
check "PATCH /supply-entries/:id/resolve → 200" "200" "$CODE"
assert_contains "resolve response contains message=Resolved" '"message":"Resolved"' "$BODY"

# Post-check: row now has canonical_color=green and parse_status=ok
POST=$(curl -s -b "$CK_ADMIN" "$BASE/supply-entries")
assert_contains "Row post-resolve carries canonical_color=green" "\"canonical_color\":\"green\"" "$POST"
assert_contains "Row post-resolve carries canonical_size=M" "\"canonical_size\":\"M\"" "$POST"
assert_contains "Row post-resolve has parse_status=ok" "\"parse_status\":\"ok\"" "$POST"

# Auditor (read-only role) must be blocked from resolve.
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK_AUDIT" -X PATCH \
    "$BASE/supply-entries/$SUP_ID/resolve" \
    -H "Content-Type: application/json" \
    -d '{"canonical_color":"blue"}')
check "Auditor PATCH /supply-entries/:id/resolve → 403" "403" "$R"

# Unauthenticated → 401
R=$(curl -s -o /dev/null -w "%{http_code}" -X PATCH \
    "$BASE/supply-entries/$SUP_ID/resolve" \
    -H "Content-Type: application/json" -d '{}')
check "PATCH /supply-entries/:id/resolve (no auth) → 401" "401" "$R"


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 3. GET /traceability — list trace codes with auditor visibility filter
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
echo ""
echo "━━━ 3. GET /traceability ━━━"

# Seed: create one trace code, then publish it. Also create a second draft.
TRACE_A_ID=$(curl -s -b "$CK_ADMIN" -X POST "$BASE/traceability" \
    -H "Content-Type: application/json" \
    -d "{\"intake_id\":\"$INTAKE_ID\"}" | \
    jq -r '.id')
curl -s -b "$CK_ADMIN" -X POST "$BASE/traceability/$TRACE_A_ID/publish" \
    -H "Content-Type: application/json" -d '{"comment":"public release"}' > /dev/null

TRACE_B_ID=$(curl -s -b "$CK_ADMIN" -X POST "$BASE/traceability" \
    -H "Content-Type: application/json" \
    -d "{\"intake_id\":\"$INTAKE_ID\"}" | \
    jq -r '.id')

# Admin sees both rows
R=$(curl -s -w "\n%{http_code}" -b "$CK_ADMIN" "$BASE/traceability")
CODE=$(echo "$R" | tail -1); BODY=$(echo "$R" | sed '$d')
check "GET /traceability (admin) → 200" "200" "$CODE"
assert_contains "Admin list contains published trace" "$TRACE_A_ID" "$BODY"
assert_contains "Admin list contains draft trace"     "$TRACE_B_ID" "$BODY"
assert_contains "GET /traceability element has code field" "\"code\":\"FAC" "$BODY"
assert_contains "GET /traceability element has status field" "\"status\"" "$BODY"
assert_contains "GET /traceability element has version field" "\"version\"" "$BODY"

# Auditor sees only published rows (visibility filter)
R=$(curl -s -w "\n%{http_code}" -b "$CK_AUDIT" "$BASE/traceability")
CODE=$(echo "$R" | tail -1); BODY=$(echo "$R" | sed '$d')
check "GET /traceability (auditor) → 200" "200" "$CODE"
assert_contains "Auditor sees published trace"     "$TRACE_A_ID" "$BODY"
assert_not_contains "Auditor does NOT see draft trace" "$TRACE_B_ID" "$BODY"

# 401 unauth
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/traceability")
check "GET /traceability (no auth) → 401" "401" "$R"


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 4. GET /transfers/:id — single transfer lookup
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
echo ""
echo "━━━ 4. GET /transfers/:id ━━━"

# Seed a transfer.
TRANSFER_ID=$(curl -s -b "$CK_ADMIN" -X POST "$BASE/transfers" \
    -H "Content-Type: application/json" \
    -d "{\"intake_id\":\"$INTAKE_ID\",\"destination\":\"East Clinic\",\"reason\":\"transport\",\"notes\":\"same-day\"}" | \
    jq -r '.id')

# Happy path — returns full TransferResponse shape
R=$(curl -s -w "\n%{http_code}" -b "$CK_ADMIN" "$BASE/transfers/$TRANSFER_ID")
CODE=$(echo "$R" | tail -1); BODY=$(echo "$R" | sed '$d')
check "GET /transfers/:id → 200" "200" "$CODE"
assert_contains "Transfer body echoes seeded id"          "\"id\":\"$TRANSFER_ID\""  "$BODY"
assert_contains "Transfer body carries destination=East Clinic" "East Clinic" "$BODY"
assert_contains "Transfer body carries reason=transport"  "\"reason\":\"transport\"" "$BODY"
assert_contains "Transfer body carries notes=same-day"    "same-day"    "$BODY"
assert_contains "Transfer body has status=queued (initial)" "\"status\":\"queued\"" "$BODY"
assert_contains "Transfer body has created_by field"      "created_by"   "$BODY"
assert_contains "Transfer body has created_at field"      "created_at"   "$BODY"
assert_contains "Transfer body echoes intake_id"          "$INTAKE_ID"   "$BODY"

# Unknown id → 404 with error envelope
R=$(curl -s -w "\n%{http_code}" -b "$CK_ADMIN" "$BASE/transfers/00000000-0000-0000-0000-000000000000")
CODE=$(echo "$R" | tail -1); BODY=$(echo "$R" | sed '$d')
check "GET /transfers/<missing> → 404" "404" "$CODE"
assert_contains "Missing transfer has error envelope"    "Transfer not found" "$BODY"

# Auditor can read transfers (read-only role allowed on GET)
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK_AUDIT" "$BASE/transfers/$TRANSFER_ID")
check "GET /transfers/:id (auditor read) → 200" "200" "$R"

# 401 unauth
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/transfers/$TRANSFER_ID")
check "GET /transfers/:id (no auth) → 401" "401" "$R"


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 5. GET /stock/movements — ledger with optional filters
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
echo ""
echo "━━━ 5. GET /stock/movements ━━━"

# Seed: one receipt, one allocation, against the same supply.
MOV_RECEIPT_ID=$(curl -s -b "$CK_ADMIN" -X POST "$BASE/stock/movements" \
    -H "Content-Type: application/json" \
    -d "{\"supply_id\":\"$SUP_ID\",\"quantity_delta\":20,\"reason\":\"receipt\",\"notes\":\"pallet in\"}" | \
    jq -r '.id')
MOV_ALLOC_ID=$(curl -s -b "$CK_ADMIN" -X POST "$BASE/stock/movements" \
    -H "Content-Type: application/json" \
    -d "{\"supply_id\":\"$SUP_ID\",\"quantity_delta\":-5,\"reason\":\"allocation\",\"notes\":\"pulled\"}" | \
    jq -r '.id')

# Unfiltered list returns both rows with full ledger shape
R=$(curl -s -w "\n%{http_code}" -b "$CK_ADMIN" "$BASE/stock/movements")
CODE=$(echo "$R" | tail -1); BODY=$(echo "$R" | sed '$d')
check "GET /stock/movements → 200" "200" "$CODE"
assert_contains "Stock list contains receipt id"    "$MOV_RECEIPT_ID" "$BODY"
assert_contains "Stock list contains allocation id" "$MOV_ALLOC_ID"   "$BODY"
assert_contains "Stock element has supply_id field"       "supply_id"       "$BODY"
assert_contains "Stock element has quantity_delta field"  "quantity_delta"  "$BODY"
assert_contains "Stock element has reason field"          "\"reason\""      "$BODY"
assert_contains "Stock element has actor_id field"        "actor_id"        "$BODY"
assert_contains "Stock element exposes positive delta 20" "\"quantity_delta\":20" "$BODY"
assert_contains "Stock element exposes negative delta -5" "\"quantity_delta\":-5" "$BODY"

# Filter: ?reason=receipt must return the receipt but not the allocation
R=$(curl -s -w "\n%{http_code}" -b "$CK_ADMIN" "$BASE/stock/movements?reason=receipt")
CODE=$(echo "$R" | tail -1); BODY=$(echo "$R" | sed '$d')
check "GET /stock/movements?reason=receipt → 200" "200" "$CODE"
assert_contains     "Filtered list keeps receipt"         "$MOV_RECEIPT_ID" "$BODY"
assert_not_contains "Filtered list drops allocation"      "$MOV_ALLOC_ID"   "$BODY"

# Filter: ?supply_id=<id>&reason=allocation
R=$(curl -s -w "\n%{http_code}" -b "$CK_ADMIN" "$BASE/stock/movements?supply_id=$SUP_ID&reason=allocation")
CODE=$(echo "$R" | tail -1); BODY=$(echo "$R" | sed '$d')
check "GET /stock/movements?supply_id=&reason=allocation → 200" "200" "$CODE"
assert_contains     "Dual-filter list keeps allocation"  "$MOV_ALLOC_ID"   "$BODY"
assert_not_contains "Dual-filter list drops receipt"     "$MOV_RECEIPT_ID" "$BODY"

# 401 unauth
R=$(curl -s -o /dev/null -w "%{http_code}" "$BASE/stock/movements")
check "GET /stock/movements (no auth) → 401" "401" "$R"


# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
# 6. PATCH /users/:id — admin role update
# ━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
echo ""
echo "━━━ 6. PATCH /users/:id ━━━"

# Pre-check: auditor is currently "auditor"
PRE=$(curl -s -b "$CK_ADMIN" "$BASE/users")
assert_contains "Pre-PATCH: cgapaudit is auditor" \
    "\"username\":\"cgapaudit\",\"role\":\"auditor\"" "$PRE"

# Valid promotion auditor → operations_staff
R=$(curl -s -w "\n%{http_code}" -b "$CK_ADMIN" -X PATCH "$BASE/users/$AUDITOR_ID" \
    -H "Content-Type: application/json" \
    -d '{"role":"operations_staff"}')
CODE=$(echo "$R" | tail -1); BODY=$(echo "$R" | sed '$d')
check "PATCH /users/:id (role=operations_staff) → 200" "200" "$CODE"
assert_contains "PATCH /users/:id returns message=User updated" \
    '"message":"User updated"' "$BODY"

# Post-check: role is now operations_staff
POST=$(curl -s -b "$CK_ADMIN" "$BASE/users")
assert_contains "Post-PATCH: cgapaudit role is operations_staff" \
    "\"username\":\"cgapaudit\",\"role\":\"operations_staff\"" "$POST"

# Invalid role → 400
R=$(curl -s -w "\n%{http_code}" -b "$CK_ADMIN" -X PATCH "$BASE/users/$AUDITOR_ID" \
    -H "Content-Type: application/json" \
    -d '{"role":"superhero"}')
CODE=$(echo "$R" | tail -1); BODY=$(echo "$R" | sed '$d')
check "PATCH /users/:id (invalid role) → 400" "400" "$CODE"
assert_contains "Invalid-role response has error code field" "\"code\"" "$BODY"

# Staff blocked (admin-only endpoint) — 403
R=$(curl -s -o /dev/null -w "%{http_code}" -b "$CK_STAFF" -X PATCH "$BASE/users/$AUDITOR_ID" \
    -H "Content-Type: application/json" \
    -d '{"role":"auditor"}')
check "Staff PATCH /users/:id → 403" "403" "$R"

# Unauthenticated → 401
R=$(curl -s -o /dev/null -w "%{http_code}" -X PATCH "$BASE/users/$AUDITOR_ID" \
    -H "Content-Type: application/json" -d '{"role":"auditor"}')
check "PATCH /users/:id (no auth) → 401" "401" "$R"

# Restore auditor role so no downstream test depends on ordering.
curl -s -b "$CK_ADMIN" -X PATCH "$BASE/users/$AUDITOR_ID" \
    -H "Content-Type: application/json" -d '{"role":"auditor"}' > /dev/null


# ── Summary ──────────────────────────────────────────────────────────
echo ""
echo "========================================"
echo "  Coverage Gap Tests - Passed: $PASS  Failed: $FAIL"
echo "========================================"
[ $FAIL -gt 0 ] && exit 1; exit 0
