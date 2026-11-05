#!/bin/bash
# Copies a release's update archives, signatures, DMGs, pkg, and `checksums.txt` from GitHub into
# the EU R2 bucket behind `download.getcmdr.com`, then checks the public host serves the same
# bytes. The release pipeline's `publish` job runs it; by hand it backfills a release. What it
# does: `scripts/release-mirror/main.go`. The flow: `docs/guides/releasing.md`.
#
# Usage: ./scripts/release-mirror.sh [-manifest latest.json] <version>
#        ./scripts/release-mirror.sh -preflight
# Needs CLOUDFLARE_API_TOKEN and CLOUDFLARE_ACCOUNT_ID in the environment.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}/scripts/release-mirror"
exec go run . "$@"
