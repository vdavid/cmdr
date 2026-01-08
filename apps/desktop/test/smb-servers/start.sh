#!/bin/bash
# Start SMB test servers for local development
# Usage:
#   ./start.sh           # Start core containers only (~300MB RAM)
#   ./start.sh all       # Start all containers (~800MB RAM)
#   ./start.sh minimal   # Start just smb-guest and smb-auth (~100MB RAM)

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

mode="${1:-core}"

case "$mode" in
    minimal)
        echo "Starting minimal SMB servers (smb-guest, smb-auth)..."
        docker compose up -d smb-guest smb-auth
        ;;
    core)
        echo "Starting core SMB servers (auth scenarios + edge cases)..."
        docker compose up -d smb-guest smb-auth smb-both smb-flaky smb-slow smb-readonly
        ;;
    all)
        echo "Starting all SMB servers (16 containers, ~800MB RAM)..."
        docker compose up -d
        ;;
    *)
        echo "Unknown mode: $mode"
        echo "Usage: $0 [minimal|core|all]"
        exit 1
        ;;
esac

echo ""
echo "Waiting for containers to be healthy..."
sleep 3

# Show status
docker compose ps

echo ""
echo "SMB servers ready! Connection URLs:"
echo ""
echo "  smb://localhost:9445/public    # smb-guest (no auth)"
echo "  smb://localhost:9446/private   # smb-auth (user: testuser, pass: testpass)"
echo "  smb://localhost:9447/mixed     # smb-both (guest or auth)"
echo ""
echo "Use './stop.sh' to stop all containers."
