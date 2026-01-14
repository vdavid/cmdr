#!/bin/bash
# PostToolUse hook - auto-formats files after Claude edits them

input=$(cat)
file_path=$(echo "$input" | jq -r '.tool_input.file_path // empty')

if [[ -z "$file_path" ]]; then exit 0; fi

case "$file_path" in
  *.ts|*.svelte|*.js|*.css)
    cd "$(dirname "$0")/../.." && pnpm exec prettier --write "$file_path" 2>/dev/null
    ;;
  *.rs)
    rustfmt "$file_path" 2>/dev/null
    ;;
  *.go)
    gofmt -w "$file_path" 2>/dev/null
    ;;
esac
exit 0
