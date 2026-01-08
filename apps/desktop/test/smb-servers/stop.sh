#!/bin/bash
# Stop all SMB test servers
set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "Stopping all SMB test servers..."
docker compose down

echo "Done."
