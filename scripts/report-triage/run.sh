#!/usr/bin/env bash
# The morning report triager, run headless on the M1 box at 05:30 Stockholm time by launchd, an hour before the API
# server's morning brief. See CLAUDE.md here for what it does and how it's locked down.
#
# Secrets come from the M1 box's own `secret` store, resolved BEFORE `env -i`, so the agent's process holds exactly
# two scoped credentials and nothing else from the box's environment. No board token, on purpose: the agent writes its
# rating as the last line of its triage comment and the API server's cron applies it (scripts/report-triage/DETAILS.md).
#   TRIAGE_GH_TOKEN           fine-grained PAT: vdavid/cmdr-reports ONLY, Issues read+write, Metadata read
#   TRIAGE_R2_ACCESS_KEY_ID   R2 API token: Object Read on the cmdr-error-reports-eu bucket ONLY
#   TRIAGE_R2_SECRET_ACCESS_KEY
set -euo pipefail

here="$(cd "$(dirname "$0")" && pwd)"
work="$HOME/report-triage"
mkdir -p "$work"

# A fresh, shallow, public checkout of Cmdr for code research, kept apart from any clone David works in.
if [[ -d "$work/cmdr/.git" ]]; then
  git -C "$work/cmdr" fetch --quiet --depth 1 origin main
  git -C "$work/cmdr" reset --quiet --hard origin/main
else
  git clone --quiet --depth 1 https://github.com/vdavid/cmdr "$work/cmdr"
fi
# The agent's two tools, fresh every run so nothing it did yesterday carries over. settings.json denies the sandbox
# writes to both, so a run can't rewrite them (or unzip a bundle over them) mid-run.
for tool in board.sh fetch-bundle.sh; do
  rm -f "$work/$tool"
  cp "$here/$tool" "$work/$tool"
  chmod 0555 "$work/$tool"
done

gh_token="$(secret TRIAGE_GH_TOKEN)"
r2_key_id="$(secret TRIAGE_R2_ACCESS_KEY_ID)"
r2_secret="$(secret TRIAGE_R2_SECRET_ACCESS_KEY)"
# Claude Code's own login. Over SSH and under launchd the login keychain isn't reliably unlocked, so the headless
# run authenticates with the box's OAuth token. settings.json strips it from every sandboxed command's environment
# (`sandbox.credentials.envVars`), so only the claude process itself holds it.
claude_token="$(secret CLAUDE_CODE_OAUTH_TOKEN)"
prompt="$(cat "$here/prompt.md")"

{
  echo "=== report-triage run $(date '+%Y-%m-%d %H:%M:%S %Z') ==="
  echo "claude $(claude --version 2>/dev/null)"
} >&2

cd "$work"
# Bundles hold the reporter's note and email, and the privacy policy promises they're gone within 90 days, so none may
# outlive a run on this box: empty the folder before the run (in case one was killed) and after it (trap).
rm -rf -- "$work/bundles"
trap 'rm -rf -- "$work/bundles"' EXIT
# SHELL + ZDOTDIR: Claude Code snapshots the user's shell setup (outside the sandbox) and replays it in every Bash
# command, so without these the box's `~/.zshenv` re-exports its broad GH_TOKEN (over ours), CLOUDFLARE_API_TOKEN, and
# the rest straight back into the agent's shell, undoing `env -i`. `/var/empty` is a root-owned empty dir, so zsh
# finds no startup files to read (verified on the M1, Claude Code 2.1.299, 2026-10-14).
#
# A run gets 90 minutes, then SIGALRM stops it (perl's `alarm` survives the `exec`; macOS ships no `timeout`). Without
# it, a stalled run waits forever, and launchd never starts the next day's run while one is still going: the 2026-10-15
# run sat 4.5 hours at startup with no CPU (the cause was the launchd job's `ProcessType`, see the plist in the infra
# repo's `m1mbp/ansible/roles/report_triage`).
limit_seconds=5400
status=0
env -i \
  HOME="$HOME" \
  SHELL=/bin/zsh \
  ZDOTDIR=/var/empty \
  PATH="/opt/homebrew/bin:/usr/local/bin:/usr/bin:/bin:$HOME/.local/bin" \
  CLAUDE_CODE_OAUTH_TOKEN="$claude_token" \
  GH_TOKEN="$gh_token" \
  TRIAGE_R2_ACCOUNT_ID="6a4433bf11c3cf86feda057f76f47991" \
  TRIAGE_R2_ACCESS_KEY_ID="$r2_key_id" \
  TRIAGE_R2_SECRET_ACCESS_KEY="$r2_secret" \
  /usr/bin/perl -e 'alarm shift; exec @ARGV or die "exec: $!"' "$limit_seconds" \
  claude -p "$prompt" \
  --model claude-opus-5-5 \
  --settings "$here/settings.json" \
  --setting-sources project,local \
  --strict-mcp-config --mcp-config '{"mcpServers":{}}' \
  --tools Bash,Read,Grep,Glob \
  --permission-mode dontAsk \
  --permission-prompts none ||
  status=$?
if ((status == 142)); then
  echo "Stopped after $((limit_seconds / 60)) minutes without finishing (SIGALRM). Cards it didn't reach stay unprocessed for tomorrow." >&2
fi
exit "$status"
