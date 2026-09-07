#!/usr/bin/env bash
# Refresh the website's Playwright visual baselines.
#
# Usage:
#   update-visual-baselines.sh          Refresh the committed baselines that are actually stale.
#   update-visual-baselines.sh --force  Re-shoot every committed baseline, comparison skipped.
#   update-visual-baselines.sh --full   Capture the gitignored full-page set (upgrade diffing).
#
# Why Docker: baselines are Linux-only (`*-chromium-linux.png`) because macOS renders different font
# antialiasing, and `visual.spec.ts` skips itself on other platforms. This container is not just *a*
# Linux renderer, it is THE one: ci.yml's Website job runs inside the same image, so the run that
# verifies a baseline is the run that could have shot it. Both sides derive the tag from the
# resolved @playwright/test (here from the installed package, there from pnpm-lock.yaml) instead of
# naming it, so a Renovate bump moves them together. Verifying on the bare runner instead reddens
# the tightest region shots with nothing behind them; see apps/website/DETAILS.md § Visual baselines.
#
# Why compare-then-update-failures (not a blind `--update-snapshots`): a blind update rewrites every
# snapshot whose render isn't byte-identical, including ones that still pass CI's `maxDiffPixelRatio`
# threshold. That churns unrelated baselines on every run. Instead we run a normal comparison first
# and only re-shoot what genuinely failed (`--last-failed`).
#
# `--force` is the escape hatch for the blind spot that creates: a deliberate visual change too small
# to breach `maxDiffPixelRatio` leaves the baseline passing while showing the OLD render. A 24px logo
# in the 1280x533 footer shot is 0.08% of the pixels against a 1% threshold, so swapping the logo
# changed nothing here (2026-09-07). Use it when you KNOW you changed something these shots contain,
# and read the resulting diff: `--force` will happily bake in a regression.
#
# The committed set is small and region-scoped on purpose (see e2e/visual.spec.ts): it shoots the
# markdown fixture and a few components, never full marketing pages, so publishing a post or editing
# copy doesn't invalidate it. In practice this script now has little to do; run it when you change a
# markdown plugin, the blog prose styles, the site header/footer, or the pricing tier grid.
#
# `--full` is the escape hatch for the coverage the region set gives up. It shoots every real page
# full-page, in both themes, into `e2e/visual-full-snapshots/` (gitignored). Capture it BEFORE an
# Astro major upgrade, do the upgrade, re-run to compare, then delete the dir.
#
# Requires Docker. Idempotent: a no-op when every baseline already passes.
set -euo pipefail

FULL_MODE=0
FORCE_MODE=0
case "${1:-}" in
  --full) FULL_MODE=1 ;;
  --force) FORCE_MODE=1 ;;
  "") ;;
  *)
    echo "ERROR: unknown argument '$1' (expected --full, --force, or nothing)." >&2
    exit 2
    ;;
esac

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
WEBSITE_DIR="$REPO_ROOT/apps/website"
SNAPSHOTS_REL="apps/website/e2e/visual.spec.ts-snapshots"

cd "$REPO_ROOT"

if ! docker info >/dev/null 2>&1; then
  echo "ERROR: Docker is required to render the Linux visual baselines (macOS can't produce them)." >&2
  echo "       Start Docker and re-run." >&2
  exit 1
fi

echo "==> Installing website deps"
pnpm install --frozen-lockfile --filter @cmdr/website

# Pin the container to the exact installed Playwright version so its bundled chromium matches
# CI's. Deriving it (rather than hardcoding) keeps the image in lockstep with Renovate bumps.
PW_VERSION="$(node -p "require('$WEBSITE_DIR/node_modules/@playwright/test/package.json').version")"
IMAGE="mcr.microsoft.com/playwright:v${PW_VERSION}-noble"

if [[ "$FULL_MODE" == "1" ]]; then
  echo "==> full-page set (container: $IMAGE) -> apps/website/e2e/visual-full-snapshots/ (gitignored)"
elif [[ "$FORCE_MODE" == "1" ]]; then
  echo "==> committed baselines (container: $IMAGE): re-shoot ALL, comparison skipped"
else
  echo "==> committed baselines (container: $IMAGE): compare, refresh only failures"
fi

# Runs as root so corepack can write its pnpm shim, but the container's install and build go
# into anonymous volumes (the `-v /repo/...` with no host side) so the main clone's macOS
# node_modules and dist are never overwritten. Only the snapshots dir is a real bind-mount
# write; we chown it back to the host user before exiting.
docker run --rm \
  -e HOST_UID="$(id -u)" -e HOST_GID="$(id -g)" \
  -e CI=1 \
  -e VISUAL_FULL="$([[ "$FULL_MODE" == "1" ]] && echo 1 || echo '')" \
  -e VISUAL_FORCE="$([[ "$FORCE_MODE" == "1" ]] && echo 1 || echo '')" \
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
    if [ -n "$VISUAL_FORCE" ]; then
      pnpm exec playwright test visual.spec.ts --update-snapshots
    elif ! pnpm exec playwright test visual.spec.ts; then
      echo "   re-shooting failed baselines"
      pnpm exec playwright test visual.spec.ts --last-failed --update-snapshots
    fi
    # Both dirs: the committed baselines, and the --full output (visual-full-snapshots). Missing
    # either leaves root-owned PNGs on the host bind mount.
    chown -R "$HOST_UID:$HOST_GID" e2e/visual.spec.ts-snapshots e2e/visual-full-snapshots 2>/dev/null || true
  '

if [[ "$FULL_MODE" == "1" ]]; then
  echo "==> Done. Full-page shots in apps/website/e2e/visual-full-snapshots/ (gitignored; delete when you're finished)."
else
  echo "==> Done. Changed baselines:"
  git -C "$REPO_ROOT" status --short -- "$SNAPSHOTS_REL" || true
fi
