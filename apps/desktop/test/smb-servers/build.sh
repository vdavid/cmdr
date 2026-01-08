#!/bin/bash
# Build all SMB test server images
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Building all SMB test server images..."
docker compose build

echo "Done. Use './start.sh' to start the containers."
