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
                  smb-consumer-readonly smb-consumer-flaky smb-consumer-slow)
        ;;
    all)
        echo "Starting all SMB servers (14 containers)..."
        # Empty services list means "all defined in compose" for both `up` and
        # the post-up probe loop. We resolve the actual set via `compose ps`.
        ;;
    *)
        echo "Unknown mode: $mode"
        echo "Usage: $0 [minimal|e2e|core|all]"
        exit 1
        ;;
esac

docker compose -p "$PROJECT_NAME" -f "$COMPOSE_DIR/docker-compose.yml" up -d "${services[@]}"

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
echo "Use './stop.sh' to stop all containers."
