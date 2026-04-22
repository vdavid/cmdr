#!/usr/bin/env bash
# Soak test: SMB → Local copy pipeline, repeated for an extended period.
#
# Catches accumulating bugs (credit/handle leaks, memory growth, per-iteration
# slowdown) that short integration tests can't see. Assumes Docker SMB
# containers are running — start them with
# `apps/desktop/test/smb-servers/start.sh core` first.
#
# Usage:
#   ./scripts/soak-smb.sh              # 30-min default (CMDR_SOAK_DURATION_SECS=1800)
#   ./scripts/soak-smb.sh 3600         # 60-min run
#   CMDR_SOAK_ITERATIONS=500 ./scripts/soak-smb.sh
#
# The test source is `smb_soak_copy_loop` in
# `apps/desktop/src-tauri/src/file_system/volume/smb.rs`.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# If a positional duration is passed, use it; otherwise honor env vars or
# fall back to 30 min.
if [ $# -ge 1 ]; then
    export CMDR_SOAK_DURATION_SECS="$1"
elif [ -z "${CMDR_SOAK_DURATION_SECS:-}" ] && [ -z "${CMDR_SOAK_ITERATIONS:-}" ]; then
    export CMDR_SOAK_DURATION_SECS=1800
fi

echo "SMB soak — duration=${CMDR_SOAK_DURATION_SECS:-unset} iterations=${CMDR_SOAK_ITERATIONS:-unset}"
echo "Requires Docker SMB containers (apps/desktop/test/smb-servers/start.sh core)."

cd "$REPO_ROOT/apps/desktop/src-tauri"

RUST_LOG="${RUST_LOG:-info}" \
    cargo test --release --lib smb_soak_copy_loop -- --ignored --nocapture
