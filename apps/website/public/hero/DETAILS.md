# Hero images details

Reshoot and regeneration procedure for the composited hero illustration. `CLAUDE.md` holds the file map and guardrails.

## How to reshoot

The capture is shared with the README and the app directories, so it lives in one place:
[`docs/guides/screenshots.md`](../../../../docs/guides/screenshots.md). `pnpm marketing:shots` writes
`brand/screenshots/app-main-{dark,light}.webp` plus `brand/screenshots/hero-cutouts.json`. Then regenerate the layers.

### Regenerate the layers

```bash
apps/website/scripts/regenerate-hero.sh
```

Writes the six intermediate master PNGs (into `brand/hero-masters/`, regeneration inputs, never shipped, and untracked)
and the twelve shipped WebPs (into this directory). Only the WebPs reach `dist/`, and only they are committed; the PNGs
stay under `brand/` so the bundle ships only the WebPs. Needs ImageMagick (`magick`) and Node. A fresh clone has no
masters at all: run the script and they appear.

The script does three things per theme: crops each pane rectangle onto its own transparent canvas of the master's size,
punches those same rectangles out of the master's alpha to make the frame, and writes lossless WebPs at 2x and 1x. All
three layers share one canvas, so the browser stacks them with no offsets to keep in sync.

❗ **The output is byte-reproducible, and must stay that way.** ImageMagick stamps each PNG with the time it was
written, so the script excludes the date chunks (`png:exclude-chunk=date,time`; not `-strip`, which would also drop the
colour profile). A layer whose pixels didn't change comes out identical to the committed one, so a regeneration after a
reshoot that only touched one pane leaves `git status` quiet for the rest, and the repo grows only by what actually
changed. Rerunning the script twice in a row must produce zero diff.

### Verify

The six master PNGs are 2508 x 1634, the 1x WebPs 1254 x 817, and the 2x WebPs roughly ~95 KB frame, ~41 KB left pane,
~39 KB right pane. To check the frame's transparency, composite on red (from the repo root):

```bash
magick -size 2508x1634 xc:red \
  brand/hero-masters/cmdr-hero-right-pane-dark.png -composite \
  brand/hero-masters/cmdr-hero-left-pane-dark.png -composite \
  brand/hero-masters/cmdr-hero-frame-dark.png -composite \
  /tmp/hero-composite-test.png
```

Red should show only through the shadow, never inside the window.

## Cutout geometry

❗ Nothing here hardcodes a rectangle, and nothing should.

- **The pane rectangles come from `brand/screenshots/hero-cutouts.json`**, measured off the live DOM during the same run
  that took the masters (`apps/desktop/test/e2e-playwright/marketing-shots.spec.ts`). They are relative to the WINDOW's
  top-left, and each names the file-list area: below the column headers, above the status bar, inset 2 px so the window
  border and the pane divider stay in the FRAME layer instead of riding along with an animating pane.
- **The window's position on the canvas is read from the master itself**, as the bounding box of everything opaque
  (`magick … -alpha extract -threshold 99% -format '%@'`). Every master carries the focused window's shadow as
  transparent margin, currently 112 px left and right, 76 top, 148 bottom, so the window lands at `+112+76` on a 2508 x
  1634 canvas. Reading it beats restating it: the same threshold is what the capture's own frame check uses, so the two
  agree to the pixel.

A hardcoded copy of either is how the shipped hero ended up a redesign behind its own screenshot.
