#!/bin/bash
# Stop all SMB test servers
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_DIR="$SCRIPT_DIR/.compose"
PROJECT_NAME="smb-consumer"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

if [ ! -f "$COMPOSE_DIR/docker-compose.yml" ]; then
    echo "No compose files found. Nothing to stop."
    exit 0
fi

# Release the "manual" lease (the one bare start.sh takes). The Go helper downs
# the shared stack ONLY if no other session still holds a lease — so running
# stop.sh while a sibling worktree's suite is live leaves that stack UP. See
# scripts/check/smblease. If `go` is missing or the helper errors, we warn and
# fall back to a direct `down` (the legacy behavior — only safe when nothing
# else is using the stack).
echo "Releasing the manual SMB lease (stack downs only at zero holders)..."
released=false
if command -v go &> /dev/null; then
    if (cd "$REPO_ROOT/scripts/check" && go run ./smb-lease release manual); then
        released=true
    else
        echo "WARN: SMB lease helper failed; falling back to direct 'compose down'." >&2
    fi
else
    echo "WARN: 'go' not found; falling back to direct 'compose down' (no cross-worktree SMB lease refcounting)." >&2
fi

if [ "$released" = false ]; then
    docker compose -p "$PROJECT_NAME" -f "$COMPOSE_DIR/docker-compose.yml" down
fi

echo "Done."
