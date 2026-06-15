# Hero images details

Reshoot and regeneration procedure for the composited hero illustration. `CLAUDE.md` holds the file map and guardrails.

## How to reshoot

The capture itself (sizing the window, setting up a nice app state, and the CleanShot steps) is shared with the README
and AlternativeTo, so it lives in one place: [`docs/guides/screenshots.md`](../../../../docs/guides/screenshots.md).
Shoot the masters per that guide, which saves them to `brand/screenshots/app-main-{dark,light}.png`. Then come back here
and run the compositing below to regenerate the hero layers.

### Regenerate the layers

Generates the 6 master PNGs and 12 shipped WebPs from the `brand/screenshots/` masters.

Prerequisite: ImageMagick (`magick` CLI). From the repo root:

```bash
cd apps/website/public/hero
root=../../../..  # repo root, where brand/ lives

# Create pane mask (white = keep, black = make transparent)
magick -size 2508x1634 xc:white \
  -fill black -draw "rectangle 114,281 1246,1387" \
  -fill black -draw "rectangle 1261,281 2393,1387" \
  /tmp/hero-mask.png

for variant in dark light; do
  src=$root/brand/screenshots/app-main-${variant}.png

  # Left pane cutout (paste onto transparent canvas)
  magick -size 2508x1634 xc:none \
    \( "$src" -crop 1133x1107+114+281 +repage \) \
    -geometry +114+281 -composite \
    cmdr-hero-left-pane-${variant}.png

  # Right pane cutout (paste onto transparent canvas)
  magick -size 2508x1634 xc:none \
    \( "$src" -crop 1133x1107+1261+281 +repage \) \
    -geometry +1261+281 -composite \
    cmdr-hero-right-pane-${variant}.png

  # Frame: multiply source alpha with mask to punch transparent holes in pane areas
  magick "$src" -alpha extract /tmp/src-alpha.png
  magick /tmp/src-alpha.png /tmp/hero-mask.png -compose Multiply -composite /tmp/new-alpha.png
  magick "$src" /tmp/new-alpha.png -alpha off -compose CopyOpacity -composite \
    cmdr-hero-frame-${variant}.png
done

# Shipped WebPs: lossless 2x + 1x from each master PNG
for variant in dark light; do
  for layer in frame left-pane right-pane; do
    base=cmdr-hero-${layer}-${variant}
    magick $base.png -define webp:lossless=true -define webp:method=6 $base.webp
    magick $base.png -resize 50% -define webp:lossless=true -define webp:method=6 $base-1x.webp
  done
done

rm -f /tmp/hero-mask.png /tmp/src-alpha.png /tmp/new-alpha.png
```

### Verify

Check that the six master PNGs are 2508 x 1634, the 1x WebPs are 1254 x 817, and the 2x WebP file sizes are roughly: ~85
KB frame, ~50 KB left pane, ~20 KB right pane. To verify the frame transparency, composite on a red background:

```bash
magick -size 2508x1634 xc:red \
  cmdr-hero-right-pane-dark.png -composite \
  cmdr-hero-left-pane-dark.png -composite \
  cmdr-hero-frame-dark.png -composite \
  /tmp/hero-composite-test.png
```

Red should only show through the shadow, not in the content area.

## Cutout geometry reference

All coordinates are in 2x retina pixels on the 2508 x 1634 canvas (which includes ~112 px macOS shadow on each side).

| Region                    | Origin (x, y)               | Size (w x h) | Notes                                              |
| ------------------------- | --------------------------- | ------------ | -------------------------------------------------- |
| Window (excl. shadow)     | (112, 76)                   | 2284 x 1410  | The actual window chrome boundary                  |
| Left pane cutout          | (114, 281)                  | 1133 x 1107  | Starts below column headers, ends above status bar |
| Right pane cutout         | (1261, 281)                 | 1133 x 1107  | Same size as left pane                             |
| Pane divider gap          | (1247, 281)                 | 14 px wide   | Between the two cutouts (includes resize bar)      |
| Left pane mask rectangle  | (114, 281) to (1246, 1387)  |              | Used in the `-draw` command                        |
| Right pane mask rectangle | (1261, 281) to (2393, 1387) |              | Used in the `-draw` command                        |
