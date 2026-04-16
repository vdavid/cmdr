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
CARGO_DIR="$SCRIPT_DIR/../../src-tauri"

# Extract compose files from smb2 if not already done
if [ ! -f "$COMPOSE_DIR/docker-compose.yml" ]; then
    echo "Extracting smb2 consumer compose files..."
    cargo run --manifest-path "$CARGO_DIR/Cargo.toml" --example smb_compose --features smb-e2e -- "$COMPOSE_DIR"
fi

mode="${1:-core}"

case "$mode" in
    minimal)
        echo "Starting minimal SMB servers (guest, auth)..."
        docker compose -p "$PROJECT_NAME" -f "$COMPOSE_DIR/docker-compose.yml" up -d \
            smb-consumer-guest smb-consumer-auth
        ;;
    e2e)
        echo "Starting E2E SMB servers (guest, auth, 50shares, unicode)..."
        docker compose -p "$PROJECT_NAME" -f "$COMPOSE_DIR/docker-compose.yml" up -d \
            smb-consumer-guest smb-consumer-auth smb-consumer-50shares smb-consumer-unicode
        ;;
    core)
        echo "Starting core SMB servers (auth scenarios + edge cases)..."
        docker compose -p "$PROJECT_NAME" -f "$COMPOSE_DIR/docker-compose.yml" up -d \
            smb-consumer-guest smb-consumer-auth smb-consumer-both \
            smb-consumer-readonly smb-consumer-flaky smb-consumer-slow
        ;;
    all)
        echo "Starting all SMB servers (14 containers)..."
        docker compose -p "$PROJECT_NAME" -f "$COMPOSE_DIR/docker-compose.yml" up -d
        ;;
    *)
        echo "Unknown mode: $mode"
        echo "Usage: $0 [minimal|e2e|core|all]"
        exit 1
        ;;
esac

echo ""
echo "Waiting for containers to be healthy..."
sleep 3

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
