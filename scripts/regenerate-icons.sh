#!/usr/bin/env bash
# Regenerates every icon and logo raster in the repo from `brand/logos/cmdr.svg`.
#
# Run it with `pnpm icons` from the repo root. The one thing it can't do is the
# macOS Tahoe Liquid Glass icon (`Assets.car`), which needs Icon Composer by hand;
# it leaves you a 1024 master to feed that. Full procedure: docs/guides/updating-icon.md
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

src="brand/logos/cmdr.svg"
master="_ignored/designs/app-logo-1024px-x-1024px.png"

for tool in magick rsvg-convert; do
	if ! command -v "$tool" >/dev/null 2>&1; then
		echo "Missing '$tool'. Install both with: brew install imagemagick librsvg" >&2
		exit 1
	fi
done

# ImageMagick renders SVG through its own renderer when librsvg is absent, and that
# one silently drops every `url(#…)` paint and `<use>`: the logo comes out solid
# black. Everything here goes through `rsvg-convert` instead, and `magick` only ever
# touches PNGs.

echo "==> Minifying the SVG for the copies we ship over the wire"
pnpm exec svgo --config brand/logos/svgo.config.mjs -i "$src" -o apps/website/public/logo.svg

echo "==> Desktop app icons (PNGs, .icns, .ico, Square*, android, ios)"
(cd apps/desktop && pnpm tauri icon "../../$src")

echo "==> Press kit"
cp apps/desktop/src-tauri/icons/icon.png brand/logos/cmdr-512.png
cp apps/desktop/src-tauri/icons/128x128.png brand/logos/cmdr-128.png
cp apps/desktop/src-tauri/icons/32x32.png brand/logos/cmdr-32.png

echo "==> Website"
# `logo-512.png` is hotlinked by the listmonk newsletter templates and named as
# `organization.logo` in our JSON-LD, so it stays a PNG even though the site itself
# renders `logo.svg`.
cp apps/desktop/src-tauri/icons/icon.png apps/website/public/logo-512.png
cp apps/desktop/src-tauri/icons/64x64.png apps/website/public/favicon.png
cp apps/desktop/src-tauri/icons/icon.ico apps/website/public/favicon.ico
rsvg-convert -w 180 -h 180 "$src" -o apps/website/public/apple-touch-icon.png

echo "==> Desktop webview favicon"
cp apps/desktop/src-tauri/icons/128x128.png apps/desktop/static/favicon.png

echo "==> 1024 master for Icon Composer"
mkdir -p "$(dirname "$master")"
rsvg-convert -w 1024 -h 1024 "$src" -o "$master"

cat <<EOF

Done. Two things this script can't reach:

  1. $master is fresh, but the macOS Tahoe icon
     (apps/desktop/src-tauri/resources/Assets.car) still holds the old artwork.
     Rebuild it by hand: docs/guides/updating-icon.md
  2. Baselines that pin the old bytes:
       rm scripts/check/checks/website-bundle-size-baseline.json && pnpm check website-bundle-size
       apps/website/scripts/update-visual-baselines.sh
EOF
