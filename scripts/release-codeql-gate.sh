#!/bin/bash
# Release gate, CodeQL half: a release can't ship over a known high or critical CodeQL finding.
#
# Usage: release-codeql-gate.sh [--wait] <sha>
#
# Passes when both hold:
#   1. The newest `codeql.yml` run on the commit succeeded. A run fails on any high or critical
#      finding it leaves after the accepted list (`.github/codeql/`), so this covers the code being
#      released. `--wait` waits for a run still in progress; without it, an unfinished run fails.
#      The commit is the one whose full CI run counts (`scripts/release-finish/ci.go`): the tagged
#      release commit, whose CodeQL run `release.sh` dispatches beside its CI run.
#   2. No open CodeQL alert on `main` is high, critical, or error. A push skips Rust when no Rust
#      changed (`codeql.yml`), so the newest Rust analysis may come from an older commit; its
#      alerts stay open until fixed, accepted, or dismissed, and this catches them.
#
# Called by the release CI gate (`release-finish.sh -ci-gate`, locally with --wait and in
# `release-pipeline.yml`'s `ci-gate` job without), which owns the emergency bypass.
set -euo pipefail

WAIT=false
if [[ "${1:-}" == "--wait" ]]; then
  WAIT=true
  shift
fi
if [[ $# -ne 1 ]]; then
  echo "Usage: $0 [--wait] <sha>" >&2
  exit 2
fi
SHA="$1"

REPO_URL="https://github.com/$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
RUN_ID=$(gh api "repos/{owner}/{repo}/actions/workflows/codeql.yml/runs?head_sha=$SHA&per_page=1" \
  --jq '.workflow_runs[0].id // empty')
if [[ -z "$RUN_ID" ]]; then
  echo "Error: no CodeQL run (codeql.yml) on ${SHA:0:9}."
  echo "release.sh starts one on the release branch; if it never started: gh workflow run codeql.yml --ref <branch>"
  exit 1
fi
URL="$REPO_URL/actions/runs/$RUN_ID"
if [[ "$(gh run view "$RUN_ID" --json status --jq .status)" != "completed" ]]; then
  if ! $WAIT; then
    echo "Error: the CodeQL run on ${SHA:0:9} is still going ($URL). Wait for it, then try again."
    exit 1
  fi
  echo "Waiting for the CodeQL run on ${SHA:0:9} ($URL)..."
  gh run watch "$RUN_ID" --interval 30 >/dev/null || true
fi
CONCLUSION=$(gh run view "$RUN_ID" --json conclusion --jq .conclusion)
if [[ "$CONCLUSION" != "success" ]]; then
  echo "Error: the CodeQL run on ${SHA:0:9} ended '$CONCLUSION': $URL"
  echo "Its log lists each blocking finding. Fix it, or accept a false positive in .github/codeql/accepted-findings.json."
  echo "A flake? gh run rerun $RUN_ID --failed"
  exit 1
fi
GREEN="${SHA:0:9} ($URL)"

BLOCKING=$(gh api --paginate \
  "repos/{owner}/{repo}/code-scanning/alerts?tool_name=CodeQL&state=open&ref=refs/heads/main&per_page=100" \
  --jq '.[] | select(.rule.security_severity_level == "high" or .rule.security_severity_level == "critical" or .rule.severity == "error")
        | "\(.html_url) \(.rule.id) at \(.most_recent_instance.location.path):\(.most_recent_instance.location.start_line)"')
if [[ -n "$BLOCKING" ]]; then
  echo "Error: open high or critical CodeQL alerts on main:"
  echo "$BLOCKING"
  exit 1
fi
echo "✅ CodeQL is green on $GREEN, with no open high or critical alerts."
