#!/usr/bin/env bash
# PreToolUse hook for Bash.
# Block any command that truncates or partially reads AGENTS.md. It's the
# agent-orientation doc; truncating misses critical context (gotchas,
# decisions, constraints) often documented late in the file. Use the
# `Read` tool in full instead. CLAUDE.md files are not gated by this hook.
# See ~/.claude/rules/read-agents-md.md.
#
# Stdin: PreToolUse hook JSON. Stdout (only on block): permissionDecision JSON.
# Exit 0 = allow. Non-zero exits are also treated as allow by Claude Code, so
# any unexpected shape falls through to allow.

set -u

input=$(cat)
cmd=$(jq -r '.tool_input.command // empty' <<<"$input" 2>/dev/null || true)

[ -z "$cmd" ] && exit 0

# Must mention AGENTS.md or CLAUDE.md as a filename component.
if ! grep -qE 'AGENTS\.md' <<<"$cmd"; then
  exit 0
fi

# Match any of:
#   head [flags] PATH/AGENTS.md         tail [flags] PATH/AGENTS.md
#   sed -n '...' PATH/AGENTS.md         awk '...' PATH/AGENTS.md
#   cat PATH/AGENTS.md | head|tail|sed -n
deny=0

if grep -qE '(^|[[:space:]&;|()`$])(head|tail)([[:space:]]+-[^[:space:]]*)*[[:space:]]+[^|;&]*AGENTS\.md' <<<"$cmd"; then
  deny=1
elif grep -qE '(^|[[:space:]&;|()`$])sed[[:space:]]+-n[^|]*AGENTS\.md' <<<"$cmd"; then
  deny=1
elif grep -qE '(^|[[:space:]&;|()`$])awk[[:space:]]+[^|]*AGENTS\.md' <<<"$cmd"; then
  deny=1
elif grep -qE 'AGENTS\.md[^|]*\|[[:space:]]*(head|tail|sed[[:space:]]+-n)' <<<"$cmd"; then
  deny=1
fi

[ "$deny" -eq 0 ] && exit 0

cat <<'EOF'
{
  "hookSpecificOutput": {
    "hookEventName": "PreToolUse",
    "permissionDecision": "deny",
    "permissionDecisionReason": "Don't truncate or partially read AGENTS.md. Critical context (architecture, gotchas, decisions) often lives late in the file. Use the Read tool in full (no offset/limit). Rule: ~/.claude/rules/read-agents-md.md"
  }
}
EOF
exit 0
