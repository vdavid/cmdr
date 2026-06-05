#!/bin/bash
# Start SMB test servers for local development and E2E testing.
#
# Containers come from smb2's consumer test harness. On first run, this script
# extracts the Docker Compose files from smb2 via a cargo example, then starts
# the requested containers.
#
# Usage:
#   ./start.sh           # Start core containers (guest, auth, both, readonly, flaky, slow)
#   ./start.sh all       # Start all 14 containers
#   ./start.sh minimal   # Start just guest + auth
#   ./start.sh e2e       # Start containers needed by E2E tests (guest, auth, 50shares, unicode)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_DIR="$SCRIPT_DIR/.compose"
PROJECT_NAME="smb-consumer"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

# Compose files are vendored from smb2's `tests/docker/consumer/`. See
# `.compose/VENDORED.md` for how to update when the smb2 dep bumps.
if [ ! -f "$COMPOSE_DIR/docker-compose.yml" ]; then
    echo "ERROR: $COMPOSE_DIR/docker-compose.yml is missing." >&2
    echo "See .compose/VENDORED.md for how to re-vendor the smb2 consumer containers." >&2
    exit 1
fi

mode="${1:-core}"
services=()

case "$mode" in
    minimal)
        echo "Starting minimal SMB servers (guest, auth)..."
        services=(smb-consumer-guest smb-consumer-auth)
        ;;
    e2e)
        echo "Starting E2E SMB servers (guest, auth, 50shares, unicode)..."
        services=(smb-consumer-guest smb-consumer-auth smb-consumer-50shares smb-consumer-unicode)
        ;;
    core)
        echo "Starting core SMB servers (auth scenarios + edge cases)..."
        services=(smb-consumer-guest smb-consumer-auth smb-consumer-both \
                  smb-consumer-readonly smb-consumer-flaky smb-consumer-slow \
                  smb-consumer-maxreadsize smb-consumer-50shares)
        ;;
    all)
        echo "Starting all SMB servers (15 containers)..."
        # Empty services list means "all defined in compose" for both `up` and
        # the post-up probe loop. We resolve the actual set via `compose ps`.
        ;;
    *)
        echo "Unknown mode: $mode"
        echo "Usage: $0 [minimal|e2e|core|all]"
        exit 1
        ;;
esac

# Adopt-or-start via the machine-wide SMB lease. The Go helper holds a flock,
# refcounts every concurrent session across worktrees, and either ADOPTS an
# already-serving stack (no compose call) or RECONCILES it (`up -d` under the
# lock) — so a sibling worktree's live suite is never recreated or torn down out
# from under it. This bare `start.sh` registers as the "manual" sentinel holder
# that the dead-PID sweep never reaps; clear it with `stop.sh`. See
# scripts/check/smblease for the model.
#
# On success the helper has already brought the stack up (or confirmed it's
# serving), so we skip our own `up` and go straight to the probe. If `go` is
# missing or the helper errors (a local-ergonomics edge — CI always has Go), we
# warn loudly and FALL BACK to the legacy direct `up`, never blocking the user.
#
# The override (`docker-compose.override.yml`, cmdr-owned, re-vendor-safe) layers
# `restart: unless-stopped` + `mem_limit` + `cpus` onto every non-flaky consumer.
# Only `up` applies those keys, so the override `-f` belongs at the up call sites
# (the helper and the fallback) — the bare `compose ps`/`port`/`logs` calls
# reconstruct config from container labels and work unchanged.
lease_ok=false
if command -v go &> /dev/null; then
    if (cd "$REPO_ROOT/scripts/check" && go run ./smb-lease acquire manual "$mode"); then
        lease_ok=true
    else
        echo "WARN: SMB lease helper failed; falling back to direct 'compose up' (no cross-worktree refcounting)." >&2
    fi
else
    echo "WARN: 'go' not found; falling back to direct 'compose up' (no cross-worktree SMB lease refcounting)." >&2
fi

if [ "$lease_ok" = false ]; then
    docker compose -p "$PROJECT_NAME" -f "$COMPOSE_DIR/docker-compose.yml" -f "$COMPOSE_DIR/docker-compose.override.yml" up -d "${services[@]}"
fi

# Resolve the list of running services if `all` was requested.
if [ ${#services[@]} -eq 0 ]; then
    mapfile -t services < <(docker compose -p "$PROJECT_NAME" -f "$COMPOSE_DIR/docker-compose.yml" ps --services)
fi

# Active TCP probe on each service's published port 445 until smbd accepts a
# connection. NEVER replace this with `sleep N`; see
# ../CLAUDE.md "Testing principles → No magic timer waits". `docker compose up -d`
# returns when containers transition to "running", which is well before smbd
# inside them has bound the port. Some images (auth, 50shares, unicode) legitimately
# need >3 s under load to finish user creation / share materialisation, and the
# previous `sleep 3` made E2E runs flake with "Cannot reach smb-consumer-X".
echo ""
echo "Waiting for smbd to accept TCP on each container..."
deadline=$((SECONDS + 60))
for service in "${services[@]}"; do
    host_port=$(docker compose -p "$PROJECT_NAME" -f "$COMPOSE_DIR/docker-compose.yml" port "$service" 445 2>/dev/null | awk -F: '{print $NF}')
    if [ -z "$host_port" ]; then
        echo "  ! could not resolve host port for $service (skipping probe)" >&2
        continue
    fi
    while ! (exec 3<>"/dev/tcp/127.0.0.1/$host_port") 2>/dev/null; do
        if [ $SECONDS -ge $deadline ]; then
            echo "ERROR: $service (port $host_port) did not accept TCP within 60s" >&2
            docker compose -p "$PROJECT_NAME" -f "$COMPOSE_DIR/docker-compose.yml" logs --tail=50 "$service" >&2
            exit 1
        fi
        sleep 0.1
    done
    exec 3<&-
    exec 3>&-
    echo "  ✓ $service ready on :$host_port"
done

# Show status
docker compose -p "$PROJECT_NAME" -f "$COMPOSE_DIR/docker-compose.yml" ps

echo ""
echo "SMB servers ready! Connection URLs:"
echo ""
echo "  smb://localhost:10480/public    # smb-consumer-guest (no auth)"
echo "  smb://localhost:10481/private   # smb-consumer-auth (user: testuser, pass: testpass)"
echo "  smb://localhost:10482/mixed     # smb-consumer-both (guest or auth)"
echo ""
echo "Use './apps/desktop/test/smb-servers/stop.sh' to stop all containers."
