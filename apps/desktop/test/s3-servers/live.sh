#!/bin/bash
# Run cmdr-s3's live-provider cells against real accounts (R2, Hetzner, GCS,
# DigitalOcean Spaces, AWS, Backblaze B2, Wasabi): what each provider
# enforces, how multipart behaves, and throughput. Never part of `pnpm check`
# or CI; the cells skip without CMDR_S3_LIVE=1.
#
# Usage:
#   ./live.sh                      # every provider, every live cell, then the sweep
#   ./live.sh r2,gcs               # only these providers
#   ./live.sh all live_batch       # every provider, only cells matching a filter
#   CMDR_S3_LIVE_HETZNER_BUCKET=<name> ./live.sh hetzner
#
# The variables come from `live-env.sh` beside this script, which other
# runners source too. Hetzner and Spaces run only with a bucket passed in like
# above, since they bill while a bucket exists (README § "Live providers").
#
# Each cell deletes what it wrote under `cmdr-live/<run>/`; the last step
# sweeps anything a crashed run left. The buckets themselves stay.

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

only="${1:-all}"
filter="${2:-live_}"

# shellcheck source=live-env.sh
source "$SCRIPT_DIR/live-env.sh"
if [ "$only" != "all" ]; then
    export CMDR_S3_LIVE_ONLY="$only"
fi

cd "$REPO_ROOT"
# One cell at a time: the cells share buckets, and the throughput numbers mean
# nothing with other cells on the same link.
cargo test -p cmdr-s3 --lib "$filter" -- --test-threads=1 --nocapture
cargo test -p cmdr-s3 --lib live_cleanup_removes_every_leftover -- --nocapture
