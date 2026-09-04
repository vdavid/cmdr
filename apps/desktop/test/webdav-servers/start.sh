#!/bin/bash
# Start the WebDAV fixture servers for local development and the integration lane.
#
# First-party, like the SFTP stack next door: the compose file sits right here
# rather than under a `.compose/` marker dir, because there's no vendored tree
# to re-sync when a dependency bumps.
#
# Usage:
#   ./start.sh             # core: the three servers the integration lane talks to
#   ./start.sh minimal     # just the Basic-auth server
#   ./start.sh nextcloud   # just the sabre/dav server (slow: it installs itself)
#   ./start.sh all         # everything the compose file defines

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_NAME="webdav-fixture"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

# ❗ Keep this table in lock-step with `modeServices` in
# scripts/check/stacklease/registry.go. The lease helper is what actually brings
# the stack up on a check run; this list is what the fallback and the readiness
# probe below use, and a drift between them shows up as a cell with no server.
mode="${1:-core}"
services=()

case "$mode" in
    minimal)
        echo "Starting the minimal WebDAV server (Basic auth)..."
        services=(webdav-fixture-apache)
        ;;
    core)
        echo "Starting the core WebDAV servers (Basic, Digest, and the one that ignores Range)..."
        services=(webdav-fixture-apache webdav-fixture-digest webdav-fixture-norange)
        ;;
    nextcloud)
        # Deliberately alone and deliberately not in `core`: a ~1 GB image that
        # installs Nextcloud before it binds a port. `pnpm check
        # desktop-rust-webdav-nextcloud` brings this up on its own.
        echo "Starting the Nextcloud (sabre/dav) server; first boot installs it, which takes a while..."
        services=(webdav-fixture-nextcloud)
        ;;
    all)
        echo "Starting every WebDAV server the compose file defines..."
        # Empty means "all" to both `up` and the probe loop below, which resolves
        # the running set from `compose ps`.
        ;;
    *)
        echo "Unknown mode: $mode"
        echo "Usage: $0 [minimal|core|nextcloud|all]"
        exit 1
        ;;
esac

# Adopt-or-start through the machine-wide WebDAV lease, so a sibling worktree's
# live suite is never recreated or torn down under it. Same model as the SMB and
# SFTP stacks, its own lease namespace: scripts/check/stacklease.
#
# A bare `start.sh` registers as the "manual" sentinel holder that the dead-PID
# sweep never reaps; `stop.sh` is what clears it.
lease_ok=false
if command -v go &> /dev/null; then
    if (cd "$REPO_ROOT/scripts/check" && go run ./stack-lease acquire webdav manual "$mode"); then
        lease_ok=true
    else
        echo "WARN: WebDAV lease helper failed; falling back to a direct 'compose up' (no cross-worktree refcounting)." >&2
    fi
else
    echo "WARN: 'go' not found; falling back to a direct 'compose up' (no cross-worktree WebDAV lease refcounting)." >&2
fi

if [ "$lease_ok" = false ]; then
    docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" up -d --build "${services[@]}"
fi

if [ ${#services[@]} -eq 0 ]; then
    mapfile -t services < <(docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" ps --services)
fi

# Active TCP probe per service. ❌ Never a `sleep N` (see ../CLAUDE.md, "Testing
# principles → No magic timer waits"): `compose up -d` returns when a container
# reaches "running", which is well before httpd has bound its port, and seeding
# `large.bin` takes a moment.
echo ""
echo "Waiting for each container to accept TCP..."
for service in "${services[@]}"; do
    host_port=$(docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" port "$service" 80 2>/dev/null | awk -F: '{print $NF}')
    if [ -z "$host_port" ]; then
        echo "  ! could not resolve the host port for $service (skipping the probe)" >&2
        continue
    fi
    # ❗ Per service, because Nextcloud earns a budget the httpd pair doesn't:
    # it runs its whole first-boot install BEFORE it binds a port, so an unbound
    # port there means "still installing" rather than "broken". Keeping httpd at
    # 120 s means a genuinely broken one still fails fast.
    # ❗ An `if`, not `[ … ] && budget=300`: under `set -e` a one-liner whose
    # test is false is a trap waiting for whoever edits around it.
    if [ "$service" = "webdav-fixture-nextcloud" ]; then
        budget=300
    else
        budget=120
    fi
    deadline=$((SECONDS + budget))
    while ! (exec 3<>"/dev/tcp/127.0.0.1/$host_port") 2>/dev/null; do
        if [ $SECONDS -ge $deadline ]; then
            echo "ERROR: $service (port $host_port) did not accept TCP within ${budget}s" >&2
            docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" logs --tail=50 "$service" >&2
            exit 1
        fi
        sleep 0.1
    done
    exec 3<&-
    exec 3>&-
    echo "  ✓ $service ready on :$host_port"
done

docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" ps

echo ""
echo "WebDAV servers ready. The stock one:"
stock_port=$(docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" port webdav-fixture-apache 80 2>/dev/null | awk -F: '{print $NF}')
[ -n "$stock_port" ] && echo "  curl -u ada:openthedoor -X PROPFIND -H 'Depth: 1' http://127.0.0.1:$stock_port/dav/"
echo ""
echo "Stop them with './apps/desktop/test/webdav-servers/stop.sh'."
