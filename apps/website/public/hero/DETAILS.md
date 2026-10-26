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

The six master PNGs are 2286 x 1412 and the 1x WebPs 1143 x 706. To check the frame's transparency, composite on red
(from the repo root):

```bash
magick -size 2286x1412 xc:red \
  brand/hero-masters/cmdr-hero-right-pane-dark.png -composite \
  brand/hero-masters/cmdr-hero-left-pane-dark.png -composite \
  brand/hero-masters/cmdr-hero-frame-dark.png -composite \
  /tmp/hero-composite-test.png
```

Red should show only outside the rounded corners and faintly through the 1 px outline, never inside the window.

## Cutout geometry

❗ Nothing here hardcodes a rectangle, and nothing should.

- **The pane rectangles come from `brand/screenshots/hero-cutouts.json`**, measured off the live DOM during the same run
  that took the masters (`apps/desktop/test/e2e-playwright/marketing-shots.spec.ts`). They are relative to the WINDOW's
  top-left, and each names the file-list area: below the column headers, above the status bar, inset 2 px so the window
  border and the pane divider stay in the FRAME layer instead of riding along with an animating pane.
- **The window's position on the canvas is read from the master itself**, as the bounding box of everything opaque
  (`magick … -alpha extract -threshold 99% -format '%@'`). Every master is the window inside a 1 px outline, so it lands
  at `+1+1` on a 2286 x 1412 canvas. Reading it beats restating it: the same threshold is what the capture's own frame
  check uses, so the two agree to the pixel.

## The shadow and the inset

The masters carry no shadow (`docs/guides/screenshots.md` says why), so `Hero.astro` draws one with
`filter: drop-shadow(...)` on each `.hero-variant`: it follows the window's rounded corners, sits in the window's own
plane under the 3D tilt, and covers the panes too, so they cast it while flying in. It lives on the variant and not on
`.hero-frame` because the layers' entry animation ends on `filter: none`, which would wipe it. The variant is inset by
the old shadow margins (76 and 148 of 1634 px, as `4.65%` and `9.06%`) so the window keeps the size and place it had
when the shadow was baked into the canvas; drop the inset and `background-size: contain` blows it up about 16%.

❗ **The shadow has 42 px of room above the window and 81 px below, and no more.** `.hero-base` carries the right-edge
fade as a `mask-image`, and a mask clips everything outside its element's box, so any shadow reaching past it ends in a
hard line (2026-10-26: a single `0 18px 36px` blur did, about 50 px above the window under the tilt). That's why the
shadow is a faint contact layer plus a soft one pushed down by its own offset: the soft one fades out above the window
before the box edge. Check a new shadow with a strong colour first (`drop-shadow(0 0 40px red)`) to see where the box
cuts. With the shadow off there's still a faint lighter wedge along that edge; that's the page's grid under the box, not
the shadow.

A hardcoded copy of either is how the shipped hero ended up a redesign behind its own screenshot.
