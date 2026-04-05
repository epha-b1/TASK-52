#!/bin/bash
#
# Test orchestrator — compose-project-agnostic.
#
# Detection order:
#   1. If any docker container is already publishing port 8080 and its
#      /health endpoint answers {"status":"ok"}, we reuse it via
#      `docker exec <cid> ...`. This is the path the CI validator uses:
#      it runs `docker compose up --build` (compose project name derived
#      from the directory, e.g. "repo"), so `repo-api-1` is already on
#      port 8080 by the time this script runs.
#   2. Otherwise, try `docker compose -p w2t52 ps -q api`.
#   3. Otherwise, start our own stack with `docker compose -p w2t52 up -d --build`.
#
# Tests run with `docker exec -T <cid> bash /app/...` in all three cases,
# so the rest of the script never cares which compose project owns the
# container or what it is named.
#
# DB is reset between suites so each suite starts clean.

set -e

FALLBACK_PROJECT="w2t52"
HEALTH_URL="http://localhost:8080/health"
MAX_WAIT=120

# The container id we'll execute tests inside. Set by detect_or_start().
API_CID=""

# Probe the host's published 8080 and return 0 if /health responds.
host_health_ok() {
    curl -sf "$HEALTH_URL" 2>/dev/null | grep -q '"status"'
}

# Find a running container publishing host port 8080.
find_container_on_8080() {
    # docker ps with a publish filter returns every container advertising
    # the port. We then confirm by inspecting the actual host bindings.
    local ids
    ids=$(docker ps -q --filter 'publish=8080')
    for cid in $ids; do
        local binding
        binding=$(docker inspect --format \
            '{{range $p, $conf := .NetworkSettings.Ports}}{{if eq $p "8080/tcp"}}{{range $conf}}{{.HostPort}}{{end}}{{end}}{{end}}' \
            "$cid" 2>/dev/null)
        if [ "$binding" = "8080" ]; then
            echo "$cid"
            return 0
        fi
    done
    return 1
}

# Wait for /health to respond.
#
# CI/build environments can report false negatives when probing via
# `docker exec ... wget http://localhost:8080/health` immediately after a
# container restart, even while the service is already healthy from the host
# side. To avoid flaky suite failures, treat either probe as success:
#
# 1) host-side:   curl http://localhost:8080/health
# 2) container-side fallback: docker exec ... wget http://localhost:8080/health
#
# This keeps compatibility with both "reused running stack" and
# "self-started fallback stack" paths.
wait_healthy() {
    local elapsed=0
    while [ $elapsed -lt $MAX_WAIT ]; do
        if host_health_ok; then
            echo "      Ready (${elapsed}s)"
            return 0
        fi

        if [ -n "$API_CID" ] && docker exec -T "$API_CID" wget -qO- "$HEALTH_URL" 2>/dev/null | grep -q '"status"'; then
            echo "      Ready (${elapsed}s)"
            return 0
        fi
        sleep 2
        elapsed=$((elapsed + 2))
    done
    echo "ERROR: API not healthy after ${MAX_WAIT}s"
    docker logs --tail 50 "$API_CID" 2>&1 || true
    return 1
}

# Reset the DB inside the target container, then wait for health.
reset_db() {
    docker exec -T "$API_CID" rm -f \
        /app/storage/app.db /app/storage/app.db-wal /app/storage/app.db-shm \
        2>/dev/null || true
    docker restart "$API_CID" >/dev/null 2>&1
    sleep 3
    wait_healthy
}

TOTAL_FAIL=0

run_suite() {
    local name="$1" script="$2"
    echo "  [$name]..."
    local EXIT=0
    docker exec -T "$API_CID" bash "/app/$script" || EXIT=$?
    if [ $EXIT -eq 0 ]; then
        echo "  $name: PASSED"
    else
        echo "  $name: FAILED"
        TOTAL_FAIL=$((TOTAL_FAIL + 1))
    fi
}

# Resolve the target container — reuse a running one if present, otherwise
# bring up our own stack.
detect_or_start() {
    # 1. Is something already serving :8080 on the host?
    if host_health_ok; then
        local cid
        if cid=$(find_container_on_8080); then
            API_CID="$cid"
            local cname
            cname=$(docker inspect --format '{{.Name}}' "$cid" 2>/dev/null | sed 's|^/||')
            echo "      Reusing running container: $cname ($cid)"
            return 0
        fi
    fi

    # 2. Is our named project up?
    local cid
    cid=$(docker compose -p "$FALLBACK_PROJECT" ps -q api 2>/dev/null)
    if [ -n "$cid" ]; then
        local state
        state=$(docker inspect --format '{{.State.Status}}' "$cid" 2>/dev/null)
        if [ "$state" = "running" ]; then
            API_CID="$cid"
            echo "      Using $FALLBACK_PROJECT compose stack ($cid)"
            return 0
        fi
    fi

    # 3. Bring our own stack up.
    echo "      No running API — starting $FALLBACK_PROJECT stack..."
    docker compose -p "$FALLBACK_PROJECT" up -d --build 2>&1 | tail -3
    echo ""
    echo "[Step 2] Waiting for API..."
    cid=$(docker compose -p "$FALLBACK_PROJECT" ps -q api 2>/dev/null)
    if [ -z "$cid" ]; then
        echo "ERROR: failed to start $FALLBACK_PROJECT stack"
        return 1
    fi
    API_CID="$cid"
    wait_healthy
}

# ─── Step 1: pick a target container ─────────────────────────────────
echo "[Step 1] Checking stack status..."
detect_or_start

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

echo "[Step 11] Acceptance boundary + exhaustive matrix..."
reset_db
run_suite "AcceptanceBoundary" "API_tests/acceptance_boundary_test.sh"

# Summary
echo "========================================"
if [ $TOTAL_FAIL -eq 0 ]; then
    echo "  ALL SUITES PASSED"
else
    echo "  FAILED SUITES: $TOTAL_FAIL"
fi
echo "========================================"
exit $TOTAL_FAIL
