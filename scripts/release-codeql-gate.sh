#!/bin/bash
# Release gate, CodeQL half: a release can't ship over a known high or critical CodeQL finding.
#
# Usage: release-codeql-gate.sh [--wait] <sha>...
#
# Passes when both hold:
#   1. The newest `codeql.yml` run on the first given commit with a finished run succeeded. A run
#      fails on any high or critical finding it leaves after the accepted list (`.github/codeql/`),
#      so this covers the code being released. `--wait` waits for a run still in progress; without
#      it, an unfinished run passes the turn to the next commit given. Several SHAs: the tagged
#      `chore(release)` commit, whose run starts with the tag push, then its parent.
#   2. No open CodeQL alert on `main` is high, critical, or error. A push skips Rust when no Rust
#      changed (`codeql.yml`), so the newest Rust analysis may come from an older commit; its
#      alerts stay open until fixed, accepted, or dismissed, and this catches them.
#
# Called by `release-ci-gate.sh` (locally, with --wait) and by the `ci-gate` job in
# `release-pipeline.yml` (server-side, no wait). Both callers own the emergency bypass.
set -euo pipefail

WAIT=false
if [[ "${1:-}" == "--wait" ]]; then
  WAIT=true
  shift
fi
if [[ $# -eq 0 ]]; then
  echo "Usage: $0 [--wait] <sha>..." >&2
  exit 2
fi

REPO_URL="https://github.com/$(gh repo view --json nameWithOwner --jq .nameWithOwner)"
GREEN=""
for sha in "$@"; do
  RUN_ID=$(gh api "repos/{owner}/{repo}/actions/workflows/codeql.yml/runs?head_sha=$sha&per_page=1" \
    --jq '.workflow_runs[0].id // empty')
  [[ -z "$RUN_ID" ]] && continue
  URL="$REPO_URL/actions/runs/$RUN_ID"
  if [[ "$(gh run view "$RUN_ID" --json status --jq .status)" != "completed" ]]; then
    # Server-side, the version-bump commit's own run starts with the tag push; its parent's
    # finished run is the one that counts, so move on to it.
    if ! $WAIT; then
      echo "The CodeQL run on ${sha:0:9} is still going ($URL); trying the next commit."
      continue
    fi
    echo "Waiting for the CodeQL run on ${sha:0:9} ($URL)..."
    gh run watch "$RUN_ID" --interval 30 >/dev/null || true
  fi
  CONCLUSION=$(gh run view "$RUN_ID" --json conclusion --jq .conclusion)
  if [[ "$CONCLUSION" != "success" ]]; then
    echo "Error: the CodeQL run on ${sha:0:9} ended '$CONCLUSION': $URL"
    echo "Its log lists each blocking finding. Fix it, or accept a false positive in .github/codeql/accepted-findings.json."
    exit 1
  fi
  GREEN="${sha:0:9} ($URL)"
  break
done
if [[ -z "$GREEN" ]]; then
  echo "Error: no finished CodeQL run (codeql.yml) on $(printf '%.9s ' "$@")."
  echo "Every push to main starts one. If it's still going, wait and re-run; if it never started: gh workflow run codeql.yml --ref main"
  exit 1
fi

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
