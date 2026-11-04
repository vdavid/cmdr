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
# The browser demo bundle: CI pushed it and signed its digest and content key into the webhook
# payload, which hooks.json hands over as DEMO_IMAGE_DIGEST and DEMO_KEY (apps/website/DETAILS.md §
# Browser demo). The key must equal this checkout's: if main moved on to other demo sources since
# that run, this deploy stops here, the old site stays up, and the newer commit's run deploys.
if [[ ! "${DEMO_IMAGE_DIGEST:-}" =~ ^[0-9a-f]{64}$ || -z "${DEMO_KEY:-}" ]]; then
    echo "=== ERROR: the webhook payload carries no browser demo digest and key ==="
    exit 1
fi
CHECKOUT_DEMO_KEY="$(./scripts/website-demo-key.sh)"
if [[ "$DEMO_KEY" != "$CHECKOUT_DEMO_KEY" ]]; then
    echo "=== ERROR: the demo image was built from sources $DEMO_KEY, this checkout has $CHECKOUT_DEMO_KEY; leaving the old site up for the newer commit's deploy ==="
    exit 1
fi
echo "Browser demo: ghcr.io/vdavid/cmdr-demo@sha256:$DEMO_IMAGE_DIGEST (sources $DEMO_KEY)"
cd apps/website
docker compose build --no-cache --build-arg DEMO_IMAGE_DIGEST="$DEMO_IMAGE_DIGEST"

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
