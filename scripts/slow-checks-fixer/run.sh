#!/usr/bin/env bash
# The slow-checks fixer, run hourly on the M1 box as the restricted `agents` user by a launchd daemon. It looks for an
# open `slow-checks` issue (opened by the nightly workflow's `report` job), and when that issue names a red run nobody
# has handled yet, it readies a worktree and starts a headless Claude Code run to diagnose and fix it. See CLAUDE.md
# here for what the agent may and may not do, and DETAILS.md for the credentials and the paths.
#
# Hourly and cheap: with no open issue, or one whose newest red run was already handled, a run is two `gh` calls.
# launchd never starts a second copy while one is still going, and `handled-runs` stops a red run being picked up twice.
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
repo="getcmdr/cmdr"
work="$HOME/slow-checks-fixer"      # logs/ (launchd's), cmdr/ (our own clone), runs/<run id>/ (one run's files)
clone="$work/cmdr"
runs="$work/runs"
warm_target="/Users/veszelovszki/projects-git/vdavid/cmdr/target" # David's build cache: read-only to us, cloned per run
state="$HOME/.local/state/slow-checks-fixer"
handled="$state/handled-runs"
mkdir -p "$state" "$runs"
touch "$handled"

log() { echo "$(date '+%Y-%m-%d %H:%M:%S')  $*" >&2; }

# GitHub credentials: the Cmdr agents App's installation token (`gh-app-token`, one hour), never a token of David's.
# It lives in a private per-run dir that `gh` (via GH_CONFIG_DIR) and git (via a credential helper set through
# GIT_CONFIG_*) read on every call, so it never lands in argv, a URL, a log, or an on-disk git config. A run can
# outlast one token (three-hour alarm), so a background loop re-mints it every 30 minutes.
unset GH_TOKEN GITHUB_TOKEN GH_ENTERPRISE_TOKEN
creds="$(mktemp -d "${TMPDIR:-/tmp}/slow-checks-fixer.XXXXXX")"
chmod 700 "$creds"
mint_token() {
  local token
  token="$(gh-app-token)" && [[ -n "$token" ]] || return 1
  printf '%s\n' "$token" >"$creds/token.tmp"
  printf 'github.com:\n    oauth_token: %s\n' "$token" >"$creds/hosts.yml.tmp"
  chmod 600 "$creds/token.tmp" "$creds/hosts.yml.tmp"
  mv "$creds/token.tmp" "$creds/token"
  mv "$creds/hosts.yml.tmp" "$creds/hosts.yml"
}
cat >"$creds/git-credential" <<EOF
#!/bin/sh
# git credential helper: hands git the App token for github.com over HTTPS.
[ "\$1" = get ] || exit 0
echo username=x-access-token
echo "password=\$(cat '$creds/token')"
EOF
chmod 700 "$creds/git-credential"
export GH_CONFIG_DIR="$creds"
# The empty `credential.helper` drops every helper from the system and user git config (osxkeychain), so ours is the
# only one git runs, and only for github.com (verified with GIT_TRACE=1 on git 2.56.0, macOS, 2026-11-10).
export GIT_CONFIG_COUNT=2
export GIT_CONFIG_KEY_0=credential.helper GIT_CONFIG_VALUE_0=""
export GIT_CONFIG_KEY_1=credential.https://github.com.helper GIT_CONFIG_VALUE_1="$creds/git-credential"
export GH_PROMPT_DISABLED=1 GIT_TERMINAL_PROMPT=0

refresher=""
run_dir=""
cleanup() {
  if [[ -n "$refresher" ]]; then kill "$refresher" 2>/dev/null || true; fi
  if [[ -n "$run_dir" ]]; then remove_run "$run_dir"; fi
  rm -rf -- "$creds"
}
trap cleanup EXIT

# Removes one run's worktree, its local branch, and its files (the target clone is inside the worktree). The pushed
# branch stays on GitHub: a fix that waits for David lives on in its PR.
remove_run() {
  local dir="$1" branch
  branch="$(git -C "$dir/cmdr" branch --show-current 2>/dev/null || true)"
  git -C "$clone" worktree remove --force "$dir/cmdr" 2>/dev/null || true
  rm -rf -- "$dir"
  git -C "$clone" worktree prune || true
  if [[ -n "$branch" ]]; then git -C "$clone" branch -D --quiet "$branch" 2>/dev/null || true; fi
}

mint_token || { log "gh-app-token printed no token; not starting"; exit 1; }

# Only an issue the workflow itself opened. Anyone with triage rights can add a label, so the label alone proves nothing.
issue=$(gh issue list -R "$repo" --label slow-checks --state open --limit 1 --json number,author \
  --jq '.[] | select(.author.login == "app/github-actions") | .number')
[[ -n "$issue" ]] || exit 0

# The newest red run the workflow reported: its body's FIRST link names the first red run (the fixer's notes, appended
# below, may link more), and each later red night adds a "went red" comment. Comments from anyone but the workflow are
# ignored, for the same reason as the author check above. Only this script reads comments; the agent never does.
run_id=$(gh issue view "$issue" -R "$repo" --json body,comments --jq '
  [.body, (.comments[] | select(.author.login == "github-actions" and (.body | test("went red"))) | .body)]
  | map([scan("actions/runs/([0-9]+)") | .[0]] | first) | map(select(. != null)) | last // empty')
[[ -n "$run_id" ]] || { log "#$issue names no run; nothing to do"; exit 0; }
grep -qx "$run_id" "$handled" && exit 0

# Claim it before starting, so a crash or a stopped run doesn't make every following hour retry it. A run that needs
# another look gets one from the next red night, which names a new run.
echo "$run_id" >>"$handled"
log "=== #$issue, red run $run_id: starting the fixer (claude $(claude --version 2>/dev/null)) ==="

( while sleep 1800; do mint_token || log "re-minting the App token didn't work; gh and git use the old one until it expires"; done ) &
refresher=$!

# Our own clone, HTTPS, nobody else's. Nobody works in it, so it simply follows origin/main.
if [[ -d "$clone/.git" ]]; then
  git -C "$clone" fetch --quiet --prune origin
  git -C "$clone" checkout --quiet -B main origin/main
else
  git clone --quiet https://github.com/getcmdr/cmdr.git "$clone"
fi

# Whatever a killed run left behind (launchd never runs two at once, so everything in runs/ is a leftover).
for leftover in "$runs"/*/; do
  if [[ -d "$leftover" ]]; then
    log "removing a leftover run: $leftover"
    remove_run "${leftover%/}"
  fi
done

run_dir="$runs/$run_id"
wt="$run_dir/cmdr"
mkdir -p "$run_dir"
git -C "$clone" worktree add --quiet -b "fixer/run-$run_id" "$wt" origin/main

# The failing jobs' logs, fetched here so the agent reads CI output only, never the issue's comments.
failed_log="$run_dir/failed-jobs.log"
gh run view "$run_id" -R "$repo" --log-failed >"$failed_log" 2>&1 || log "gh run view --log-failed exited $?; the agent sees what it printed"

# The warm build: an APFS clone of David's target/ (copy-on-write, so it's quick and costs no disk until written).
started=$SECONDS
if [[ -d "$warm_target" ]] && cp -c -R "$warm_target" "$wt/target" 2>"$run_dir/cp-errors.log"; then
  log "cloned the warm target/ in $((SECONDS - started)) s"
else
  log "no warm target/ (cp exited non-zero or $warm_target is missing; see the first lines below); building cold"
  head -5 "$run_dir/cp-errors.log" >&2 || true
fi
cd "$wt"
mise trust --quiet "$wt" >/dev/null 2>&1 || true
started=$SECONDS
pnpm install --frozen-lockfile --silent || log "pnpm install exited $?"
log "pnpm install took $((SECONDS - started)) s"
# One warm-up build, timed, so the log shows whether the warm target/ works across users (cold, it's a full build).
# Its verdict doesn't matter here (the red run may be a clippy finding); the agent runs the checks it needs. An hour
# at most, so a stuck build can't hold launchd's only slot.
started=$SECONDS
build_status=0
/usr/bin/perl -e 'alarm shift; exec @ARGV or die "exec: $!"' 3600 \
  pnpm check clippy >"$run_dir/warmup-build.log" 2>&1 || build_status=$?
log "warm-up build (pnpm check clippy) took $((SECONDS - started)) s, exit $build_status"

prompt="$(sed -e "s/{{ISSUE}}/$issue/g" -e "s/{{RUN_ID}}/$run_id/g" -e "s|{{FAILED_LOG}}|$failed_log|g" \
  -e "s|{{BRANCH}}|fixer/run-$run_id|g" "$here/prompt.md")"
claude_token="$(secret CLAUDE_CODE_OAUTH_TOKEN)"

# Three hours, then SIGALRM stops it (perl's `alarm` survives the `exec`; macOS ships no `timeout`). A fix plus a
# `pnpm land` fits well inside that; a run that's still going is stuck, and launchd would never start the next one.
started=$SECONDS
status=0
CLAUDE_CODE_OAUTH_TOKEN="$claude_token" \
  /usr/bin/perl -e 'alarm shift; exec @ARGV or die "exec: $!"' 10800 \
  claude -p "$prompt" \
  --model claude-opus-5-5 \
  --permission-mode bypassPermissions ||
  status=$?
log "=== #$issue, red run $run_id: the fixer exited $status after $(((SECONDS - started) / 60)) min ==="
exit "$status"
