#!/usr/bin/env bash
#
# Rebuilds the website hero's three layers from the brand masters.
#
# The hero is one screenshot cut into a frame plus two pane rectangles, so the panes can
# animate independently over a chrome that stays put. This script does the cutting: six
# intermediate PNGs into `brand/hero-masters/` (regeneration inputs, never shipped) and
# twelve lossless WebPs into `apps/website/public/hero/` (2x and 1x, the only files the
# site loads).
#
# ❗ The pane rectangles come from `brand/screenshots/hero-cutouts.json`, measured off
# the live DOM in the same run that took the masters. ❌ Never hardcode them here: that
# is precisely how the shipped hero ended up a redesign behind its own screenshot.
#
# Run it after `pnpm marketing:shots`, from anywhere:
#
#   apps/website/scripts/regenerate-hero.sh
#
# Needs ImageMagick (`magick`) and Node.

set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)
screenshots="$root/brand/screenshots"
masters="$root/brand/hero-masters"
shipped="$root/apps/website/public/hero"
cutouts="$screenshots/hero-cutouts.json"

for tool in magick node; do
  command -v "$tool" >/dev/null || {
    echo "regenerate-hero: needs $tool on PATH" >&2
    exit 1
  }
done
[[ -f $cutouts ]] || {
  echo "regenerate-hero: no $cutouts. Run \`pnpm marketing:shots\` first: the rectangles are measured during the shot." >&2
  exit 1
}

mkdir -p "$masters" "$shipped"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT

# ❗ Keep the output byte-reproducible. ImageMagick stamps every PNG it writes with the
# current time, so without this a regeneration that changed no pixel still rewrites all
# six masters, and git stores six new ~300 KB blobs forever. Excluding the date chunks
# (rather than `-strip`, which would also drop the colour profile) makes an unchanged
# layer come out identical to the committed one, so `git status` stays quiet and the repo
# only grows when the screenshots actually changed.
readonly REPRODUCIBLE=(-define png:exclude-chunk=date,time)

# Pane rectangles, relative to the WINDOW's top-left (which is what the spec measures).
read -r left_x left_y pane_w pane_h right_x right_y < <(
  # `process.stdout.write`, not `console.log`: console adds ANSI color to numbers
  # whenever Node thinks the stream is a terminal, and bash arithmetic chokes on it.
  node -e '
    const c = require(process.argv[1]).panes
    const fields = [c.left.x, c.left.y, c.left.width, c.left.height, c.right.x, c.right.y]
    process.stdout.write(fields.join(" ") + "\n")
  ' "$cutouts"
)

for variant in dark light; do
  src="$screenshots/app-main-${variant}.webp"
  [[ -f $src ]] || {
    echo "regenerate-hero: no $src" >&2
    exit 1
  }

  # Where the window sits on the canvas, read from the master itself rather than from a
  # constant: every master carries the focused window's shadow as transparent margin, and
  # the opaque bounding box IS the window. `-threshold 99%` is the same cut the capture's
  # own frame check uses, so the two agree on the edge to the pixel.
  bbox=$(magick "$src" -alpha extract -threshold 99% -format '%@' info:) # 2284x1410+112+76
  offsets=${bbox#*+}
  win_x=${offsets%%+*}
  win_y=${offsets##*+}

  lx=$((win_x + left_x))
  ly=$((win_y + left_y))
  rx=$((win_x + right_x))
  ry=$((win_y + right_y))
  canvas=$(magick identify -format '%wx%h' "$src")

  # Each pane: its rectangle, alone on a transparent canvas of the master's size, at the
  # position it occupies in the master. Same canvas for all three layers, so the browser
  # stacks them with no offsets to keep in sync.
  magick -size "$canvas" xc:none \
    \( "$src" -crop "${pane_w}x${pane_h}+${lx}+${ly}" +repage \) -geometry "+${lx}+${ly}" -composite \
    "${REPRODUCIBLE[@]}" "$masters/cmdr-hero-left-pane-${variant}.png"
  magick -size "$canvas" xc:none \
    \( "$src" -crop "${pane_w}x${pane_h}+${rx}+${ry}" +repage \) -geometry "+${rx}+${ry}" -composite \
    "${REPRODUCIBLE[@]}" "$masters/cmdr-hero-right-pane-${variant}.png"

  # The frame: the master with both pane rectangles punched out. Multiplying the master's
  # own alpha by the mask keeps the shadow's soft edge, which a flat `-transparent` would
  # turn into a hard cut.
  magick -size "$canvas" xc:white \
    -fill black -draw "rectangle ${lx},${ly} $((lx + pane_w - 1)),$((ly + pane_h - 1))" \
    -fill black -draw "rectangle ${rx},${ry} $((rx + pane_w - 1)),$((ry + pane_h - 1))" \
    "$work/mask.png"
  magick "$src" -alpha extract "$work/src-alpha.png"
  magick "$work/src-alpha.png" "$work/mask.png" -compose Multiply -composite "$work/new-alpha.png"
  magick "$src" "$work/new-alpha.png" -alpha off -compose CopyOpacity -composite \
    "${REPRODUCIBLE[@]}" "$masters/cmdr-hero-frame-${variant}.png"
done

# Lossless WebP, 2x and 1x. ❌ Not lossy: flat UI chrome compresses BETTER losslessly
# here, and pixel-perfectly (`apps/website/public/hero/CLAUDE.md`).
for variant in dark light; do
  for layer in frame left-pane right-pane; do
    base="cmdr-hero-${layer}-${variant}"
    magick "$masters/$base.png" -define webp:lossless=true -define webp:method=6 "$shipped/$base.webp"
    magick "$masters/$base.png" -resize 50% -define webp:lossless=true -define webp:method=6 "$shipped/$base-1x.webp"
  done
done

echo "regenerate-hero: 6 masters in brand/hero-masters/, 12 WebPs in apps/website/public/hero/"
echo "regenerate-hero: panes ${pane_w}x${pane_h} at +${left_x}+${left_y} and +${right_x}+${right_y} in the window"
