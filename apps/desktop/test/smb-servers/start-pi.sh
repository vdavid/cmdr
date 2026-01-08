#!/bin/bash
# Start SMB test containers on Raspberry Pi with macvlan networking
# Containers will be discoverable via Bonjour/mDNS
#
# Before first run:
#   1. Edit docker-compose.pi.yml to match your network (interface, subnet, etc.)
#   2. Reserve IPs 192.168.1.200-215 on your router's DHCP settings
#
# Usage:
#   ./start-pi.sh          # Start all containers
#   ./start-pi.sh minimal  # Start only guest + auth containers
#   ./start-pi.sh stop     # Stop all containers

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

COMPOSE_FILE="docker-compose.pi.yml"

case "${1:-all}" in
    minimal)
        echo "Starting minimal SMB test containers (guest + auth)..."
        docker compose -f "$COMPOSE_FILE" up -d smb-guest smb-auth
        ;;
    stop)
        echo "Stopping all SMB test containers..."
        docker compose -f "$COMPOSE_FILE" down
        exit 0
        ;;
    all|*)
        echo "Starting all SMB test containers..."
        docker compose -f "$COMPOSE_FILE" up -d
        ;;
esac

echo ""
echo "Containers started! They should be discoverable via Bonjour:"
echo ""
docker compose -f "$COMPOSE_FILE" ps --format "table {{.Name}}\t{{.Status}}\t{{.Networks}}"
echo ""
echo "mDNS hostnames:"
echo "  smb-guest-test.local    (192.168.1.200) - Guest access"
echo "  smb-auth-test.local     (192.168.1.201) - Credentials: testuser/testpass"
echo "  smb-both-test.local     (192.168.1.202) - Guest + auth"
echo "  smb-readonly-test.local (192.168.1.203) - Read-only share"
echo ""
echo "Test with: smbutil view -G -N //smb-guest-test.local"
