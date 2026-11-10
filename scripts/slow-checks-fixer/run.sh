#!/usr/bin/env bash
# The slow-checks fixer, run hourly on the M1 box by launchd. It looks for an open `slow-checks` issue (opened by the
# nightly workflow's `report` job), and when that issue names a red run nobody has handled yet, it starts a headless
# Claude Code run to diagnose and fix it. See CLAUDE.md here for what the agent may and may not do.
#
# Hourly and cheap: with no open issue, or one whose newest red run was already handled, a run is two `gh` calls.
# launchd never starts a second copy while one is still going, and `handled-runs` stops a red run being picked up twice.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="getcmdr/cmdr"
clone="$HOME/projects-git/vdavid/cmdr"
state="$HOME/.local/state/slow-checks-fixer"
handled="$state/handled-runs"
mkdir -p "$state"
touch "$handled"

log() { echo "$(date '+%Y-%m-%d %H:%M:%S')  $*" >&2; }

# Only an issue the workflow itself opened. Anyone with triage rights can add a label, so the label alone proves nothing.
issue=$(gh issue list -R "$repo" --label slow-checks --state open --limit 1 --json number,author \
  --jq '.[] | select(.author.login == "app/github-actions") | .number')
[[ -n "$issue" ]] || exit 0

# The newest red run the workflow reported: its body's FIRST link names the first red run (the fixer's notes, appended
# below, may link more), and each later red night adds a "went red" comment. Comments from anyone but the workflow are
# ignored, for the same reason as the author check above.
run_id=$(gh issue view "$issue" -R "$repo" --json body,comments --jq '
  [.body, (.comments[] | select(.author.login == "github-actions" and (.body | test("went red"))) | .body)]
  | map([scan("actions/runs/([0-9]+)") | .[0]] | first) | map(select(. != null)) | last // empty')
[[ -n "$run_id" ]] || { log "#$issue names no run; nothing to do"; exit 0; }
grep -qx "$run_id" "$handled" && exit 0

# Claim it before starting, so a crash or a stopped run doesn't make every following hour retry it. A run that needs
# another look gets one from the next red night, which names a new run.
echo "$run_id" >>"$handled"
log "=== #$issue, red run $run_id: starting the fixer (claude $(claude --version 2>/dev/null)) ==="

# The agent works in a fresh worktree, and every worktree branches from the main clone's checked-out branch, so the
# main clone has to be on an up-to-date `main` first. A clone someone left mid-work is not ours to move.
branch=$(git -C "$clone" branch --show-current)
if [[ "$branch" != "main" || -n "$(git -C "$clone" status --porcelain --untracked-files=no)" ]]; then
  log "the M1's cmdr clone is on '$branch' or has local changes; not starting (handled-runs keeps $run_id claimed)"
  exit 1
fi
git -C "$clone" fetch --quiet origin main
git -C "$clone" merge --quiet --ff-only origin/main

prompt="$(sed -e "s/{{ISSUE}}/$issue/g" -e "s/{{RUN_ID}}/$run_id/g" "$here/prompt.md")"
claude_token="$(secret CLAUDE_CODE_OAUTH_TOKEN)"

# Three hours, then SIGALRM stops it (perl's `alarm` survives the `exec`; macOS ships no `timeout`). A fix plus a
# `pnpm land` fits well inside that; a run that's still going is stuck, and launchd would never start the next one.
cd "$clone"
status=0
CLAUDE_CODE_OAUTH_TOKEN="$claude_token" \
  /usr/bin/perl -e 'alarm shift; exec @ARGV or die "exec: $!"' 10800 \
  claude -p "$prompt" \
  --model claude-opus-5-5 \
  --permission-mode bypassPermissions ||
  status=$?
log "=== #$issue, red run $run_id: the fixer exited $status ==="
exit "$status"
