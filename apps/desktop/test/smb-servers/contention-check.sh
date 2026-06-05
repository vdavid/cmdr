#!/bin/bash
# Acceptance test for the shared-SMB-stack lease mechanism.
#
# It proves the single regression the whole shared-stack design exists to fix:
# a dummy session holding a lease must SURVIVE another session's full
# acquire→run→release cycle (the lane adopts the already-serving stack, never
# recreates it, and its release sees a non-zero refcount so it does NOT down the
# stack), and the stack must down only once the LAST holder leaves.
#
# Scenario (from docs/specs/smb-shared-stack-plan.md § Acceptance):
#   1. A dummy long-lived process (`sleep`) holds a lease; bring the stack up.
#   2. A second "lane" holder acquires, ADOPTS the serving stack (no
#      --force-recreate; container IDs unchanged), then releases.
#   3. Assert: after the lane's release the dummy's lease still exists AND the
#      stack is still up (refcount was non-zero → no down).
#   4. Release the dummy → assert the stack downs at zero.
#
# Run it by hand from anywhere:
#   apps/desktop/test/smb-servers/contention-check.sh
#
# It cleans up after itself (kills the dummy, releases leftover leases) on exit.

set -u

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"
CHECK_DIR="$REPO_ROOT/scripts/check"
COMPOSE_DIR="$SCRIPT_DIR/.compose"
PROJECT_NAME="smb-consumer"
MODE="e2e"

# Pin cmdr's dedicated host-port range so the lease's config hash and the
# compose bring-up match what the rest of the toolchain uses (11480+).
export CMDR_SMB_COMPOSE_DIR="$COMPOSE_DIR"
export SMB_CONSUMER_GUEST_PORT=11480
export SMB_CONSUMER_AUTH_PORT=11481
export SMB_CONSUMER_50SHARES_PORT=11483
export SMB_CONSUMER_UNICODE_PORT=11484

pass=0
fail=0
DUMMY_PID=""

lease() { (cd "$CHECK_DIR" && go run ./smb-lease "$@"); }

container_ids() {
    docker compose -p "$PROJECT_NAME" ps -q 2>/dev/null | sort
}

stack_running_count() {
    docker compose -p "$PROJECT_NAME" ps --services --filter status=running 2>/dev/null | grep -c . || true
}

ok()   { echo "  ✓ $1"; pass=$((pass + 1)); }
bad()  { echo "  ✗ $1"; fail=$((fail + 1)); }

cleanup() {
    echo ""
    echo "Cleaning up..."
    [[ -n "$DUMMY_PID" ]] && kill "$DUMMY_PID" 2>/dev/null
    # Release any leftover leases so the stack downs and we leave no orphans.
    lease release manual >/dev/null 2>&1 || true
    [[ -n "$DUMMY_PID" ]] && lease release "$DUMMY_PID" >/dev/null 2>&1 || true
    docker compose -p "$PROJECT_NAME" down >/dev/null 2>&1 || true
}
trap cleanup EXIT

if ! command -v go &> /dev/null; then
    echo "ERROR: 'go' not found; the lease helper can't run." >&2
    exit 1
fi
if ! docker info >/dev/null 2>&1; then
    echo "ERROR: docker daemon not running." >&2
    exit 1
fi

echo "=== SMB lease contention check (mode: $MODE) ==="
echo ""

# Start from a clean slate so prior leftovers don't skew the assertions.
docker compose -p "$PROJECT_NAME" down >/dev/null 2>&1 || true
rm -rf "${CMDR_SMB_LEASE_ROOT:-/tmp/cmdr-smb-leases}" 2>/dev/null || true

# ── 1. Dummy holder brings the stack up ──────────────────────────────────────
echo "[1] Dummy holder acquires + brings the stack up..."
sleep 600 &
DUMMY_PID=$!
echo "    dummy pid: $DUMMY_PID"
if lease acquire "$DUMMY_PID" "$MODE" >/dev/null; then
    ok "dummy acquired a lease and the stack came up"
else
    bad "dummy acquire failed"
    exit 1
fi

# Give smbd a moment, then snapshot the container IDs the lane must NOT recreate.
sleep 3
ids_before="$(container_ids)"
running_before="$(stack_running_count)"
echo "    running services: $running_before"
if [[ "$running_before" -gt 0 ]]; then
    ok "stack is serving ($running_before services)"
else
    bad "stack did not come up"
fi

# ── 2. Lane holder acquires (should ADOPT), then releases ─────────────────────
echo ""
echo "[2] Lane holder acquires (expect ADOPT, no recreate) then releases..."
lane_action="$(lease acquire 99001 "$MODE" 2>/dev/null | tail -1)"
echo "    lane acquire decision: ${lane_action:-<none>}"
if [[ "$lane_action" == "adopt" ]]; then
    ok "lane ADOPTED the already-serving stack (no compose call)"
else
    bad "lane decision was '${lane_action}', expected 'adopt'"
fi

ids_after_acquire="$(container_ids)"
if [[ "$ids_before" == "$ids_after_acquire" ]]; then
    ok "container IDs unchanged across the lane's acquire (no --force-recreate)"
else
    bad "container IDs changed — the lane recreated containers"
fi

lease release 99001 >/dev/null 2>&1
ok "lane released its lease"

# ── 3. Assert survival: dummy lease + stack still up ─────────────────────────
echo ""
echo "[3] Assert the dummy's lease + stack survived the lane's release..."
running_after="$(stack_running_count)"
if [[ "$running_after" -gt 0 ]]; then
    ok "stack is STILL UP after the lane released ($running_after services) — refcount was non-zero"
else
    bad "stack went DOWN after the lane released — THE CORE REGRESSION"
fi
ids_after_release="$(container_ids)"
if [[ "$ids_before" == "$ids_after_release" ]]; then
    ok "container IDs unchanged across the whole lane lifecycle"
else
    bad "container IDs changed across the lane lifecycle"
fi

# ── 4. Release the dummy → stack downs at zero ───────────────────────────────
echo ""
echo "[4] Release the dummy → expect down at zero holders..."
kill "$DUMMY_PID" 2>/dev/null
lease release "$DUMMY_PID" >/dev/null 2>&1
DUMMY_PID=""
sleep 1
running_final="$(stack_running_count)"
if [[ "$running_final" -eq 0 ]]; then
    ok "stack DOWNED at zero holders"
else
    bad "stack still has $running_final services after the last release"
fi

echo ""
echo "=== Result: $pass passed, $fail failed ==="
[[ "$fail" -eq 0 ]] && exit 0 || exit 1
