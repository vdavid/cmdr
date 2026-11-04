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
# The browser demo bundle for exactly this checkout's sources, which CI published before it fired
# this webhook (apps/website/DETAILS.md § Browser demo). A missing image fails the build, and the
# old site stays up.
DEMO_IMAGE="ghcr.io/vdavid/cmdr-demo:$(./scripts/website-demo-key.sh)"
echo "Browser demo: $DEMO_IMAGE"
cd apps/website
docker compose build --no-cache --build-arg DEMO_IMAGE="$DEMO_IMAGE"

echo "Swapping containers..."
docker compose down
docker compose up -d

echo "Verifying container is running..."
sleep 2
if docker compose ps --status running | grep -q getcmdr-static; then
    echo "Container is running."
else
    echo "=== ERROR: Container not running after deploy ==="
    docker compose logs --tail 20
    exit 1
fi

# Cloudflare keeps pages at the edge for up to an hour (`Cloudflare-CDN-Cache-Control` in
# nginx.conf), so the new site only shows everywhere once the old copies are gone. The token can
# only purge this zone's cache; systemd hands it over from `/etc/cmdr/deploy-webhook.env`
# (README.md). The zone id isn't a secret.
echo "Purging Cloudflare's cache..."
if [ -z "${CLOUDFLARE_PURGE_TOKEN:-}" ]; then
    echo "=== ERROR: CLOUDFLARE_PURGE_TOKEN isn't set, so visitors may see the old site for up to an hour ==="
    exit 1
fi
GETCMDR_ZONE_ID="3b1ddce127d21ce9802588dac5aee4e9"
if curl -fsS --max-time 30 --retry 3 --retry-all-errors -X POST \
    "https://api.cloudflare.com/client/v4/zones/$GETCMDR_ZONE_ID/purge_cache" \
    -H "Authorization: Bearer $CLOUDFLARE_PURGE_TOKEN" \
    -H "Content-Type: application/json" \
    --data '{"purge_everything":true}' | grep -q '"success":true'; then
    echo "=== Deployment succeeded ==="
else
    echo "=== ERROR: Cache purge failed, so visitors may see the old site for up to an hour ==="
    exit 1
fi

echo "Time: $(date --iso-8601=seconds)"
