#!/bin/bash
set -e

# Deploy website script
# Triggered by GitHub Actions via webhook after CI passes
#
# Safety: builds the new image BEFORE stopping the old container.
# If the build fails, the existing site stays up.

LOG_DIR="/var/log/cmdr"
LOG_FILE="$LOG_DIR/deploy-website.log"
mkdir -p "$LOG_DIR"

# Redirect all output to both the log file and stdout
exec > >(tee -a "$LOG_FILE") 2>&1

echo ""
echo "=== Starting website deployment ==="
echo "Time: $(date --iso-8601=seconds)"

cd /opt/cmdr

echo "Fetching and resetting to origin/main..."
git fetch origin main
git reset --hard origin/main

echo "Building new image (old site stays up during build)..."
cd apps/website
docker compose build --no-cache

echo "Swapping containers..."
docker compose down
docker compose up -d

echo "Verifying container is running..."
sleep 2
if docker compose ps --status running | grep -q getcmdr-static; then
    echo "=== Deployment succeeded ==="
else
    echo "=== ERROR: Container not running after deploy ==="
    docker compose logs --tail 20
    exit 1
fi

echo "Time: $(date --iso-8601=seconds)"
