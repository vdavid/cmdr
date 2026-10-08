#!/bin/bash
# Release gate: the commit being released must have a FULLY green CI run, and CodeQL must be green
# on it with no open high or critical alert (`release-codeql-gate.sh`, run last).
#
# "Fully" means a `workflow_dispatch` run with `run_all`, never a push run: CI on a push to main
# skips every job whose paths didn't change, so a docs-only HEAD reads green while the commit
# before it broke the Rust lane. Only a run_all run on the exact commit covers every job. The runs
# API carries no inputs, so a run counts as full only when its `Full run (run_all)` marker job
# (`ci.yml`, `if: inputs.run_all`) didn't skip; a dispatch with run_all off doesn't count.
#
# Idempotent: it reuses a full run that already exists on the commit (finished or still going),
# starts one if there's none, waits for it, and exits non-zero unless it succeeded. So `/release`
# calls it early to overlap the ~25-minute run with changelog drafting, and `release.sh` calls it
# again before tagging, where it returns at once.
#
# Emergency bypass (a hotfix while CI is red for a reason outside the repo): RELEASE_SKIP_CI_GATE=1.
# `release.yml` enforces the same rule server-side (its `ci-gate` job), with its own bypass: the
# repository variable RELEASE_SKIP_CI_GATE_TAG set to the exact tag.
set -euo pipefail

WORKFLOW=ci.yml

if [[ "${RELEASE_SKIP_CI_GATE:-}" == "1" ]]; then
  echo "⚠️  RELEASE_SKIP_CI_GATE=1: releasing WITHOUT a green full CI run."
  echo "   release.yml checks this again: set the repository variable RELEASE_SKIP_CI_GATE_TAG to the exact tag"
  echo "   (like v1.2.3) before pushing it, and clear it once the release run finishes."
  exit 0
fi

git fetch --quiet origin main
SHA=$(git rev-parse HEAD)
REMOTE_SHA=$(git rev-parse origin/main)
if [[ "$SHA" != "$REMOTE_SHA" ]]; then
  echo "Error: HEAD ($SHA) isn't origin/main ($REMOTE_SHA)."
  echo "CI only sees pushed commits, so push main first (git push origin main), then re-run."
  exit 1
fi

# The marker job's conclusion in a run: `success` in a full run, `skipped` with run_all off, and
# empty while the job hasn't finished (or the run hasn't listed its jobs yet). Matched by job name,
# which `ci.yml` owns.
marker_of() {
  gh run view "$1" --json jobs \
    --jq '[.jobs[] | select(.name == "Full run (run_all)")][0].conclusion // ""'
}

# The newest full run on this exact commit. `--commit` filters on the run's head SHA. The marker
# needs no other job, so it settles within seconds of a run starting; a just-queued run gets a
# minute for it.
find_run() {
  local id status marker
  while read -r id status; do
    for _ in $(seq 1 12); do
      marker=$(marker_of "$id")
      # A finished run without the marker predates it, so it can't prove anything: move on.
      [[ -n "$marker" || "$status" == "completed" ]] && break
      sleep 5
    done
    if [[ "$marker" == "success" ]]; then
      echo "$id"
      return
    fi
  done < <(gh run list --workflow "$WORKFLOW" --event workflow_dispatch --commit "$SHA" --limit 10 \
    --json databaseId,status --jq '.[] | "\(.databaseId) \(.status)"')
}

RUN_ID=$(find_run)
if [[ -z "$RUN_ID" ]]; then
  echo "No full CI run on ${SHA:0:9} yet; starting one (run_all)."
  gh workflow run "$WORKFLOW" --ref main -f run_all=true
  # The dispatch returns before the run is listed; give it up to a minute to appear.
  for _ in $(seq 1 12); do
    sleep 5
    RUN_ID=$(find_run)
    [[ -n "$RUN_ID" ]] && break
  done
  if [[ -z "$RUN_ID" ]]; then
    echo "Error: started a CI run, but it didn't show up for ${SHA:0:9} within a minute."
    echo "Check https://github.com/vdavid/cmdr/actions and re-run this gate."
    exit 1
  fi
fi

URL="https://github.com/vdavid/cmdr/actions/runs/$RUN_ID"
STATUS=$(gh run view "$RUN_ID" --json status --jq .status)
if [[ "$STATUS" != "completed" ]]; then
  echo "Waiting for the full CI run on ${SHA:0:9} ($URL)..."
  # `--exit-status` would fail the script on red; the conclusion check below words that instead.
  gh run watch "$RUN_ID" --interval 30 >/dev/null || true
fi

CONCLUSION=$(gh run view "$RUN_ID" --json conclusion --jq .conclusion)
if [[ "$CONCLUSION" != "success" ]]; then
  echo "Error: the full CI run on ${SHA:0:9} ended '$CONCLUSION': $URL"
  echo "Fix main, push, and re-run. A new commit gets its own run."
  exit 1
fi
echo "✅ Full CI run on ${SHA:0:9} is green: $URL"

# CodeQL runs as its own workflow, so it has its own gate: a green run on this commit and no open
# high or critical alert.
"$(dirname "$0")/release-codeql-gate.sh" --wait "$SHA"
