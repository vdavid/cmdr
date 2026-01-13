#!/bin/bash
set -e

# Deploy website script
# Triggered by GitHub Actions via webhook after CI passes

echo "=== Starting website deployment ==="
echo "Time: $(date)"

cd /opt/cmdr

echo "Pulling latest code..."
git pull origin main

echo "Rebuilding website container..."
cd apps/website
docker compose down
docker compose build --no-cache
docker compose up -d

echo "=== Deployment complete ==="
echo "Time: $(date)"
