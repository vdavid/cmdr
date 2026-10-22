#!/bin/bash
# Release gate: waits for the release's full CI run and CodeQL on the tagged commit, and fails the
# moment any CI job goes red, naming it, how to see why, and what to do next (re-run a flake, or
# abort the release with `./scripts/release.sh -abort X.Y.Z`).
#
# "Full" means a `workflow_dispatch` run with `run_all`, never a push or PR run: those skip every
# job whose paths didn't change. `release.sh` dispatches it on the release branch right after it
# pushes the tag, so it runs beside the build. The rules (which run counts, the parent fallback
# of the security-only patch flow, the emergency bypass `RELEASE_SKIP_CI_GATE=1`) live in one
# place: `scripts/release-finish/ci.go`. `release-finish.sh` runs the same wait before it signs,
# and `release-pipeline.yml`'s `ci-gate` job checks it again before `publish`.
#
# Usage: ./scripts/release-ci-gate.sh <version>
set -euo pipefail

if [[ $# -ne 1 ]]; then
  echo "Usage: $0 <version>   (like 0.56.0; the local tag vX.Y.Z names the commit)" >&2
  exit 2
fi
exec "$(dirname "$0")/release-finish.sh" -ci-gate -wait "$1"
