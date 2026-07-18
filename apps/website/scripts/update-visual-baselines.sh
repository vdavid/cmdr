#!/usr/bin/env bash
# Refresh the website's Playwright visual baselines that are actually stale, on BOTH platforms.
#
# Why both, and why Docker: the snapshots are per-OS (`*-chromium-darwin.png` for local macOS
# runs, `*-chromium-linux.png` for CI on ubuntu-latest). macOS can't render the Linux
# baselines (font antialiasing differs), so the Linux set is shot inside the pinned Playwright
# container, which matches CI's chromium version and Noble fonts. Refreshing only one platform
# is the recurring footgun that leaves the other stale until CI catches it.
#
# Why compare-then-update-failures (not a blind `--update-snapshots`): a blind update rewrites
# every snapshot whose render isn't byte-identical, including pages that still pass CI's
# `maxDiffPixelRatio` threshold. That churns unrelated baselines on every run. Instead we run
# a normal comparison first and only re-shoot what genuinely failed (`--last-failed`).
#
# Run this whenever a change alters a page that has a visual snapshot (most often /features,
# which renders `feature-status.json`). `scripts/release.sh` calls it automatically after the
# release copy is finalized, so a release never ships stale baselines.
#
# Requires Docker (for the Linux set). Idempotent: a no-op when every baseline already passes.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WEBSITE_DIR="$REPO_ROOT/apps/website"
SNAPSHOTS_REL="apps/website/e2e/visual.spec.ts-snapshots"

cd "$REPO_ROOT"

if ! docker info >/dev/null 2>&1; then
  echo "ERROR: Docker is required to refresh the Linux visual baselines (macOS can't render them)." >&2
  echo "       Start Docker and re-run." >&2
  exit 1
fi

echo "==> Installing website deps"
pnpm install --frozen-lockfile --filter @cmdr/website

# Pin the container to the exact installed Playwright version so its bundled chromium matches
# CI's. Deriving it (rather than hardcoding) keeps the image in lockstep with Renovate bumps.
PW_VERSION="$(node -p "require('$WEBSITE_DIR/node_modules/@playwright/test/package.json').version")"
IMAGE="mcr.microsoft.com/playwright:v${PW_VERSION}-noble"

echo "==> darwin baselines (native): compare, refresh only failures"
pnpm --filter @cmdr/website exec playwright install chromium
pnpm --filter @cmdr/website build
(
  cd "$WEBSITE_DIR"
  if ! CI=1 pnpm exec playwright test visual.spec.ts; then
    echo "   re-shooting failed darwin baselines"
    CI=1 pnpm exec playwright test visual.spec.ts --last-failed --update-snapshots
  fi
)

echo "==> linux baselines (container: $IMAGE): compare, refresh only failures"
# Runs as root so corepack can write its pnpm shim, but the container's install and build go
# into anonymous volumes (the `-v /repo/...` with no host side) so the main clone's macOS
# node_modules and dist are never overwritten. Only the snapshots dir is a real bind-mount
# write; we chown it back to the host user before exiting.
docker run --rm \
  -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
  -e CI=1 \
  -v "$REPO_ROOT":/repo -w /repo \
  -v /repo/node_modules \
  -v /repo/apps/website/node_modules \
  -v /repo/apps/website/dist \
  "$IMAGE" \
  bash -lc '
    set -e
    corepack enable
    pnpm install --frozen-lockfile --filter @cmdr/website
    pnpm --filter @cmdr/website build
    cd apps/website
    if ! pnpm exec playwright test visual.spec.ts; then
      echo "   re-shooting failed linux baselines"
      pnpm exec playwright test visual.spec.ts --last-failed --update-snapshots
    fi
    chown -R "$HOST_UID:$HOST_GID" e2e/visual.spec.ts-snapshots
  '

echo "==> Done. Changed baselines:"
git -C "$REPO_ROOT" status --short -- "$SNAPSHOTS_REL" || true
