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

# Check for uncommitted changes
if ! git diff --quiet || ! git diff --staged --quiet; then
  echo "Error: Working tree has uncommitted changes. Commit or stash them first."
  exit 1
fi

# Check CHANGELOG.md has unreleased content
UNRELEASED_CONTENT=$(sed -n '/## \[Unreleased\]/,/## \[/p' CHANGELOG.md | sed '1d;$d' | grep -v '^$' || true)
if [[ -z "$UNRELEASED_CONTENT" ]]; then
  echo "Error: CHANGELOG.md has no entries under [Unreleased]."
  echo "Add release notes before releasing!"
  exit 1
fi

echo "Releasing version $VERSION..."

# Update version in package.json
cd apps/desktop
npm pkg set version="$VERSION"
cd ../..

# Update version in tauri.conf.json
cd apps/desktop/src-tauri
jq ".version = \"$VERSION\"" tauri.conf.json > tauri.conf.json.tmp
mv tauri.conf.json.tmp tauri.conf.json
cd ../../..

# Update version in Cargo.toml and refresh Cargo.lock
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" apps/desktop/src-tauri/Cargo.toml
(cd apps/desktop/src-tauri && cargo update -p cmdr --quiet)

# Update CHANGELOG.md: rename [Unreleased] to [version] and add new [Unreleased]
TODAY=$(date +%Y-%m-%d)
sed -i '' "s/## \[Unreleased\]/## [Unreleased]\n\n## [$VERSION] - $TODAY/" CHANGELOG.md

# Commit and tag
git add -A
git commit -m "chore(release): v$VERSION"
git tag "v$VERSION"

echo ""
echo "Release v$VERSION prepared locally."
echo "To publish, run: git push origin main --tags"
