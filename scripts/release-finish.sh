#!/bin/bash
# Finishes a release whose update archives are signed on this laptop (`RELEASE_UPDATE_SIGNING=local`):
# verifies the draft's build provenance, signs, uploads the signatures, and has CI publish.
# Resumable: re-run it after any interruption. What it does, step by step:
# `scripts/release-finish/main.go`. The release flow: `docs/guides/releasing.md`.
#
# Usage: ./scripts/release-finish.sh [-dry-run] [-out DIR] <version>
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "${ROOT}/scripts/release-finish"
exec go run . -root "${ROOT}" "$@"
