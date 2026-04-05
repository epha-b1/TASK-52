#!/bin/bash
#
# Test orchestrator.
#
# If the w2t52 stack is already up and healthy, we skip the build/up step and
# just run the test suites against the running container. Otherwise we bring
# the stack up (building if needed) and then run tests.
#
# DB is reset between suites so each suite starts clean.

set -e

PROJECT="w2t52"
DC="docker compose -p $PROJECT"
HEALTH_URL="http://localhost:8080/health"
MAX_WAIT=120

wait_healthy() {
    local elapsed=0
    while [ $elapsed -lt $MAX_WAIT ]; do
        if $DC exec -T api wget -qO- "$HEALTH_URL" 2>/dev/null | grep -q '"status"'; then
            echo "      Ready (${elapsed}s)"
            return 0
        fi
        sleep 2; elapsed=$((elapsed + 2))
    done
    echo "ERROR: API not healthy after ${MAX_WAIT}s"
    $DC logs --tail 50 api
    return 1
}

is_stack_running() {
    # Returns 0 if the api container is running AND answering /health.
    # Portable: we ask docker compose for the running container id and then
    # probe the health endpoint from inside it.
    local cid
    cid=$($DC ps -q api 2>/dev/null)
    [ -z "$cid" ] && return 1
    # docker inspect returns "running" only for live containers.
    local state
    state=$(docker inspect --format '{{.State.Status}}' "$cid" 2>/dev/null)
    [ "$state" != "running" ] && return 1
    # Final check: health endpoint responds.
    $DC exec -T api wget -qO- "$HEALTH_URL" 2>/dev/null | grep -q '"status"' || return 1
    return 0
}

reset_db() {
    $DC exec -T api rm -f /app/storage/app.db /app/storage/app.db-wal /app/storage/app.db-shm 2>/dev/null
    $DC restart api >/dev/null 2>&1
    sleep 3
    wait_healthy
}

TOTAL_FAIL=0

run_suite() {
    local name="$1" script="$2"
    echo "  [$name]..."
    local EXIT=0
    $DC exec -T api bash "/app/$script" || EXIT=$?
    if [ $EXIT -eq 0 ]; then
        echo "  $name: PASSED"
    else
        echo "  $name: FAILED"
        TOTAL_FAIL=$((TOTAL_FAIL + 1))
    fi
}

# ─── Step 1: bring the stack up if it isn't already ──────────────────
echo "[Step 1] Checking stack status..."
if is_stack_running; then
    echo "      Stack already up and healthy — reusing existing containers"
else
    echo "      Stack is not running — starting (build if needed)..."
    $DC up -d --build 2>&1 | tail -3
    echo ""
    echo "[Step 2] Waiting for API..."
    wait_healthy
fi

echo "[Step 3] Slice 1 tests..."
reset_db
run_suite "S1-Unit" "unit_tests/bootstrap_test.sh"
run_suite "S1-API" "API_tests/health_api_test.sh"

echo "[Step 4] Slice 2 tests..."
reset_db
run_suite "S2-API-Auth" "API_tests/auth_api_test.sh"
reset_db
run_suite "S2-Unit-Auth" "unit_tests/auth_test.sh"

echo "[Step 5] Slice 3 tests..."
reset_db
run_suite "S3-API-AddrBook" "API_tests/address_book_api_test.sh"

echo "[Step 6] Slice 4 tests..."
reset_db
run_suite "S4-API-Intake" "API_tests/intake_api_test.sh"

echo "[Step 7] Slices 4-11 comprehensive tests..."
reset_db
run_suite "S4-11-Full" "API_tests/full_stack_test.sh"

echo "[Step 8] Remediation suite (audit fixes)..."
reset_db
run_suite "Remediation" "API_tests/remediation_api_test.sh"

echo "[Step 9] Blockers suite (final acceptance)..."
reset_db
run_suite "Blockers" "API_tests/blockers_api_test.sh"

echo "[Step 10] Frontend draft + session-restore integration..."
reset_db
run_suite "FrontendDraft" "API_tests/frontend_draft_test.sh"

# Summary
echo "========================================"
if [ $TOTAL_FAIL -eq 0 ]; then
    echo "  ALL SUITES PASSED"
else
    echo "  FAILED SUITES: $TOTAL_FAIL"
fi
echo "========================================"
exit $TOTAL_FAIL
