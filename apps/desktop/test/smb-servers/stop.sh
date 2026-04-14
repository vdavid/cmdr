#!/bin/bash
# Stop all SMB test servers
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
COMPOSE_DIR="$SCRIPT_DIR/.compose"
PROJECT_NAME="smb-consumer"

if [ ! -f "$COMPOSE_DIR/docker-compose.yml" ]; then
    echo "No compose files found. Nothing to stop."
    exit 0
fi

echo "Stopping all SMB test servers..."
docker compose -p "$PROJECT_NAME" -f "$COMPOSE_DIR/docker-compose.yml" down

echo "Done."
