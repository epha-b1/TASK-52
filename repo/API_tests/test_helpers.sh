#!/bin/bash
# ─── Shared test helpers for session stability ───────────────────────
#
# Source this file at the top of any test suite to get:
#   - Unique usernames per run (no collisions across concurrent/repeated runs)
#   - Unique cookie files via mktemp (no stale cookie reuse)
#   - ensure_admin_session: bootstrap + login in one call
#   - create_user_and_login: create a user and get a session cookie
#   - cleanup via trap (removes temp files on exit)
#
# Usage:
#   source "$(dirname "$0")/test_helpers.sh"
#   BASE="http://localhost:8080"
#   ADMIN_CK=$(ensure_admin_session "$BASE" "myadmin" "MySecurePass12")
#   STAFF_CK=$(create_user_and_login "$BASE" "$ADMIN_CK" "mystaff" "StaffPass1234" "operations_staff")

# Generate a unique suffix for this run
_RUN_ID="${$}_$(date +%s)"
_TEMP_FILES=()

# Register temp files for cleanup
_register_temp() {
    _TEMP_FILES+=("$1")
}

# Cleanup handler
_cleanup_temps() {
    for f in "${_TEMP_FILES[@]}"; do
        rm -f "$f" 2>/dev/null
    done
}
trap _cleanup_temps EXIT

# Create a unique cookie file and register it for cleanup
make_cookie_file() {
    local f
    f=$(mktemp /tmp/ft_test_XXXXXX)
    _register_temp "$f"
    echo "$f"
}

# Make a username unique for this run
unique_name() {
    local base="$1"
    echo "${base}_${_RUN_ID}"
}

# ensure_admin_session BASE USERNAME PASSWORD
# Tries to login first. If 401/409, attempts bootstrap register.
# Prints the cookie file path to stdout.
ensure_admin_session() {
    local base="$1" user="$2" pass="$3"
    local ck
    ck=$(make_cookie_file)

    # Try login first
    local code
    code=$(curl -s -o /dev/null -w "%{http_code}" -c "$ck" -X POST "$base/auth/login" \
        -H "Content-Type: application/json" \
        -d "{\"username\":\"$user\",\"password\":\"$pass\"}")

    if [ "$code" = "200" ]; then
        echo "$ck"
        return 0
    fi

    # Login failed — try bootstrap register
    code=$(curl -s -o /dev/null -w "%{http_code}" -c "$ck" -X POST "$base/auth/register" \
        -H "Content-Type: application/json" \
        -d "{\"username\":\"$user\",\"password\":\"$pass\"}")

    if [ "$code" = "201" ]; then
        echo "$ck"
        return 0
    fi

    # Register also failed (maybe system already initialized with different admin)
    # This shouldn't happen with unique names + clean DB, but handle gracefully
    echo "$ck"
    return 1
}

# create_user_and_login BASE ADMIN_CK USERNAME PASSWORD ROLE
# Creates a user as admin and logs them in. Prints cookie file path.
create_user_and_login() {
    local base="$1" admin_ck="$2" user="$3" pass="$4" role="$5"
    local ck
    ck=$(make_cookie_file)

    # Create user
    curl -s -b "$admin_ck" -X POST "$base/users" \
        -H "Content-Type: application/json" \
        -d "{\"username\":\"$user\",\"password\":\"$pass\",\"role\":\"$role\"}" > /dev/null

    # Login
    curl -s -c "$ck" -X POST "$base/auth/login" \
        -H "Content-Type: application/json" \
        -d "{\"username\":\"$user\",\"password\":\"$pass\"}" > /dev/null

    echo "$ck"
}
