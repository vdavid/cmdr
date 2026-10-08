#!/bin/bash
# Run the app's transfer-engine flows (copy, move, rename, delete, cancel,
# pause, rollback, conflicts, archived objects, cost counts) against real
# accounts: the same scenarios the Docker cells run, through the same entry
# points the IPC commands call. Never part of `pnpm check` or CI; the cells
# skip without CMDR_S3_LIVE=1.
#
# Usage:
#   ./live-engine.sh                          # every provider, every flow
#   ./live-engine.sh r2,gcs                   # only these providers
#   ./live-engine.sh all copies_between       # every provider, cells matching a filter
#   CMDR_S3_LIVE_FLOWS="pause,1,005" ./live-engine.sh b2 keeps_the_users_data_safe
#                                             # only the flows whose names hold a piece
#
# The cells live in `apps/desktop/src-tauri/src/file_system/write_operations/
# backend_suites/s3_live_engine_test.rs` and are named `s3_live_engine_*`.
# Each prints `LIVE [<account>] <flow>: ok in …` or `FAILED …` per pair, and
# removes everything it wrote under `cmdr-live/<run>/` after each flow. The
# variables come from `live-env.sh` beside this script.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

only="${1:-all}"
filter="${2:-}"

# shellcheck source=live-env.sh
source "$SCRIPT_DIR/live-env.sh"
if [ "$only" != "all" ]; then
    export CMDR_S3_LIVE_ONLY="$only"
fi

cd "$REPO_ROOT"
# `cargo test`, not nextest: the workspace's 8 s cap would cut a live upload
# (the scenarios lift their own waits to ten minutes under CMDR_S3_LIVE=1).
# One cell at a time: the cells share buckets and the uplink.
cargo test -p cmdr --lib "s3_live_engine_${filter}" -- --test-threads=1 --nocapture
