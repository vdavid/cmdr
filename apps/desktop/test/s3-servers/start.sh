#!/bin/bash
# Start the S3 fixture servers for local development and the integration lane.
#
# Usage:
#   ./start.sh             # core: VersityGW and Garage
#   ./start.sh minimal     # just VersityGW
#   ./start.sh all         # everything the compose file defines

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_NAME="s3-fixture"
COMPOSE_FILE="$SCRIPT_DIR/docker-compose.yml"
REPO_ROOT="$(cd "$SCRIPT_DIR/../../../.." && pwd)"

# The fixture credentials, public on purpose (see the README).
ACCESS_KEY="GK00000000000000000000c0de"
SECRET_KEY="c0dec0dec0dec0dec0dec0dec0dec0dec0dec0dec0dec0dec0dec0dec0dec0de"

# ❗ Keep this table in lock-step with `modeServices` in
# scripts/check/stacklease/registry.go (`TestS3ModeServicesAgree`).
mode="${1:-core}"
services=()

case "$mode" in
    minimal)
        echo "Starting the minimal S3 server (VersityGW)..."
        services=(s3-fixture-versitygw)
        ;;
    core)
        echo "Starting the core S3 servers (VersityGW and Garage)..."
        services=(s3-fixture-versitygw s3-fixture-garage)
        ;;
    all)
        echo "Starting every S3 server the compose file defines..."
        ;;
    *)
        echo "Unknown mode: $mode"
        echo "Usage: $0 [minimal|core|all]"
        exit 1
        ;;
esac

# Adopt-or-start through the machine-wide S3 lease, so a sibling worktree's live
# suite is never recreated or torn down under it. Same model as the other
# fixture stacks, its own lease namespace: scripts/check/stacklease.
#
# A bare `start.sh` registers as the "manual" sentinel holder that the dead-PID
# sweep never reaps; `stop.sh` is what clears it.
lease_ok=false
if command -v go &> /dev/null; then
    if (cd "$REPO_ROOT/scripts/check" && go run ./stack-lease acquire s3 manual "$mode"); then
        lease_ok=true
    else
        echo "WARN: S3 lease helper failed; falling back to a direct 'compose up' (no cross-worktree refcounting)." >&2
    fi
else
    echo "WARN: 'go' not found; falling back to a direct 'compose up' (no cross-worktree S3 lease refcounting)." >&2
fi

if [ "$lease_ok" = false ]; then
    docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" up -d --build "${services[@]}"
fi

if [ ${#services[@]} -eq 0 ]; then
    # A `read` loop, not `mapfile`: macOS's /bin/bash is 3.2, which has none.
    while IFS= read -r service; do
        services+=("$service")
    done < <(docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" ps --services)
fi

# Readiness is the container's HEALTHCHECK, which passes only after the
# entrypoint has created the buckets: a bound port alone would hand a cell a
# `NoSuchBucket`. Then one signed ListBuckets from here, which proves the
# published port, the credentials, and the signing all line up. ❌ Never a
# `sleep N` (see ../CLAUDE.md, "No magic timer waits").
echo ""
echo "Waiting for each server to bootstrap and answer a signed request..."
for service in "${services[@]}"; do
    deadline=$((SECONDS + 120))
    until [ "$(docker inspect -f '{{.State.Health.Status}}' "$(docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" ps -q "$service")" 2>/dev/null)" = "healthy" ]; do
        if [ $SECONDS -ge $deadline ]; then
            echo "ERROR: $service didn't report healthy within 120s" >&2
            docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" logs --tail=50 "$service" >&2
            exit 1
        fi
        sleep 0.2
    done

    # ❗ An `if`, not `[ … ] && port=3900`: under `set -e` that one-liner is a
    # trap for whoever edits around it.
    if [ "$service" = "s3-fixture-garage" ]; then
        container_port=3900
    else
        container_port=7070
    fi
    host_port=$(docker compose -p "$PROJECT_NAME" -f "$COMPOSE_FILE" port "$service" "$container_port" 2>/dev/null | awk -F: '{print $NF}')
    code=$(curl -s -o /dev/null -w '%{http_code}' --aws-sigv4 "aws:amz:us-east-1:s3" \
        --user "$ACCESS_KEY:$SECRET_KEY" "http://127.0.0.1:$host_port/")
    if [ "$code" != "200" ]; then
        echo "ERROR: $service is healthy but a signed ListBuckets on :$host_port answered $code" >&2
        exit 1
    fi
    echo "  ✓ $service ready at http://127.0.0.1:$host_port"
done

echo ""
echo "S3 servers ready. Region us-east-1, path-style, buckets cmdr-test and cmdr-test-2."
echo "  curl --aws-sigv4 aws:amz:us-east-1:s3 --user $ACCESS_KEY:$SECRET_KEY http://127.0.0.1:<port>/cmdr-test/"
echo ""
echo "Stop them with './apps/desktop/test/s3-servers/stop.sh'."
