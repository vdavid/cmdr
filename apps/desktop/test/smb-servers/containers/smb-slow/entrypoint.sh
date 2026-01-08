#!/bin/sh
set -e

DELAY_MS=${DELAY_MS:-500}

echo "Applying ${DELAY_MS}ms network delay..."

# Add network latency using tc (traffic control)
# This requires NET_ADMIN capability
tc qdisc add dev eth0 root netem delay "${DELAY_MS}ms" 2>/dev/null || \
    echo "Warning: Could not add network delay (needs NET_ADMIN cap)"

echo "Starting Samba with artificial delay..."
exec smbd --foreground --no-process-group --debug-stdout
