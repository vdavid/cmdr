#!/bin/bash
set -euo pipefail

VERSION="${1:-}"

if [[ -z "$VERSION" ]]; then
  echo "Usage: ./scripts/release.sh <version>"
  echo "Example: ./scripts/release.sh 0.2.1"
  exit 1
fi

# Validate version format
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
  echo "Error: Version must be in format X.Y.Z (e.g., 0.2.1)"
  exit 1
fi

# Check for uncommitted changes. The release-prep edits are allowed through and get included in
# the release commit: the changelog, the roadmap (content in roadmap.ts, layout in roadmap.astro),
# and the feature-status source of truth. See the `release` skill for the steps that produce them.
EXCLUDE=(
  ':!CHANGELOG.md'
  ':!feature-status.json'
  ':!apps/website/src/lib/roadmap.ts'
  ':!apps/website/src/pages/roadmap.astro'
)
if ! git diff --quiet -- "${EXCLUDE[@]}" || ! git diff --staged --quiet -- "${EXCLUDE[@]}"; then
  echo "Error: Working tree has uncommitted changes (other than the changelog, roadmap, and feature status). Commit or stash them first."
  exit 1
fi

# Detach stale Cmdr* DMG mounts. The self-hosted runner builds on this same Mac;
# a leftover /Volumes/Cmdr (typically from inspecting an old DMG in Finder) makes
# `bundle_dmg.sh` fail fast on the runner because the volume name is already
# taken. The release workflow has the same guard before the tauri-action step,
# but failing here means we don't tag a release that can't ship.
while IFS= read -r vol; do
  if [[ -n "$vol" ]]; then
    echo "Detaching stale mount: $vol"
    hdiutil detach "$vol" -force >/dev/null 2>&1 || true
  fi
done < <(mount | awk -F' on ' '/\/Volumes\/Cmdr/ { sub(/ \(.*$/, "", $2); print $2 }')

# Stage allowed files before rebase so they don't block it
git add CHANGELOG.md feature-status.json apps/website/src/lib/roadmap.ts apps/website/src/pages/roadmap.astro 2>/dev/null || true

# Pull latest main to avoid push rejection after tagging
# --autostash: temporarily stashes staged changelog/roadmap changes so rebase can proceed
git pull --rebase --autostash origin main

# Check CHANGELOG.md has an [Unreleased] section with content
if ! grep -q '## \[Unreleased\]' CHANGELOG.md; then
  echo "Error: CHANGELOG.md has no [Unreleased] section."
  echo "Add a '## [Unreleased]' heading with release notes before the first versioned section."
  exit 1
fi
UNRELEASED_CONTENT=$(sed -n '/## \[Unreleased\]/,/## \[/p' CHANGELOG.md | sed '1d;$d' | grep -v '^$' || true)
if [[ -z "$UNRELEASED_CONTENT" ]]; then
  echo "Error: The [Unreleased] section in CHANGELOG.md is empty."
  echo "Add release notes under it before releasing!"
  exit 1
fi

echo "Releasing version $VERSION..."

# Update version in package.json
cd apps/desktop
npm pkg set version="$VERSION"
cd ../..

# Record the settings defaults THIS release ships with, so the analytics dashboard can tell
# "the user is on the default" from "the setting didn't exist yet" for installs on it. The
# manifest only gains an entry when a default actually moved, so most releases are a no-op
# here. Skipping it would leave this release's installs resolved against a predecessor's
# defaults; `analytics-settings-defaults` fails the next check run if that ever happens.
(cd apps/desktop && node scripts/gen-analytics-defaults.ts --promote "$VERSION")

# Update version in tauri.conf.json
cd apps/desktop/src-tauri
jq ".version = \"$VERSION\"" tauri.conf.json > tauri.conf.json.tmp
mv tauri.conf.json.tmp tauri.conf.json
cd ../../..

# Update version in Cargo.toml and sync Cargo.lock
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" apps/desktop/src-tauri/Cargo.toml
(cd apps/desktop/src-tauri && cargo update --workspace --quiet)

# Update CHANGELOG.md: replace [Unreleased] with the versioned heading
TODAY=$(date +%Y-%m-%d)
sed -i '' "s/## \[Unreleased\]/## [$VERSION] - $TODAY/" CHANGELOG.md

# Roll the BSL Change Date forward so THIS release converts to AGPL three years after it
# ships. The Change Date is a static field, not a rolling window: left alone, every version
# ever shipped converts on one shared date, and the protection window shrinks with every
# release (a build shipped in December 2028 against a 2029-01-10 date would go AGPL a month
# later). BSL takes whichever of the Change Date and the version's fourth anniversary comes
# FIRST, so the anniversary can only ever pull conversion earlier; three years leaves a year
# of headroom under that four-year cap.
CHANGE_DATE=$(date -v+3y +%Y-%m-%d)
sed -i '' "s/^Change Date:.*/Change Date:          $CHANGE_DATE/" LICENSE

# Refresh the website's visual baselines against the finalized release copy. Roadmap and
# feature-status edits grow pages that have snapshots (most often /features), so a release
# would otherwise ship a stale Linux baseline and turn CI red right after tagging. This
# regenerates both platforms and the `git add -u` below stages whatever actually moved.
# Requires Docker (the Linux baselines can't be rendered on macOS); a missing/stopped Docker
# aborts here, before tagging.
apps/website/scripts/update-visual-baselines.sh

# Run oxfmt across the repo so CHANGELOG / package.json / tauri.conf.json drift from
# manual edits + the sed/`npm pkg set` mutations above doesn't fail CI on the release commit.
# This also reformats any unrelated files that drifted (for example, a `.claude/commands/*.md`
# touched in the same uncommitted batch the user is releasing).
# `-m`: the release runs in the main clone by design, so opt past the worktree-only
# guard (the `--ci` gate below opts past it via --ci instead).
pnpm check oxfmt -m

# Stage the files the script itself just bumped.
git add \
  CHANGELOG.md \
  LICENSE \
  feature-status.json \
  apps/website/src/lib/roadmap.ts \
  apps/website/src/pages/roadmap.astro \
  apps/desktop/package.json \
  apps/desktop/src-tauri/tauri.conf.json \
  apps/desktop/src-tauri/Cargo.toml \
  Cargo.lock

# Pick up anything the steps above modified on top of those. `git add -u` only touches
# tracked files that are already modified, and the pre-flight at the top of this script
# guaranteed the working tree was clean before we started, so the only modifications that
# exist now are the version bumps above, the refreshed visual baselines, plus oxfmt's
# auto-fixes. This keeps the release commit in sync with what oxfmt --ci will see in CI on
# the freshly-pushed tag.
git add -u

# Belt-and-braces: confirm the staged tree passes oxfmt in CI mode (no auto-fix). If this
# fails, the release commit would land with formatting drift that CI rejects, so abort
# instead of pushing.
pnpm check oxfmt --ci

# Release gate: a stale translation must NOT ship. The i18n-stale check is warn-only in
# normal `pnpm check` (a maintenance signal, not a daily-dev build breaker), but at release
# we escalate it to a build-failing ERROR via CMDR_I18N_STALE_STRICT. With `set -e`, a stale
# finding aborts the release before we tag, so the fix lands first. (English-only today, so
# this is a clean no-op until a real locale exists.) See docs/guides/releasing.md.
CMDR_I18N_STALE_STRICT=1 pnpm check i18n-stale -m

git commit -m "chore(release): v$VERSION"
git tag "v$VERSION"

echo ""
echo "Release v$VERSION prepared locally."
echo "To publish, run: git push origin main --tags"
