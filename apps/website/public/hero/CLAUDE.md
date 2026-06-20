# Hero images

Three-layer composited hero illustration for the getcmdr.com landing page. Each screenshot (dark and light) is split
into a frame + two pane cutouts so they animate independently in `Hero.astro`. Reshoot + regeneration procedure (script,
verify, cutout geometry): [DETAILS.md](DETAILS.md).

This directory ships ONLY WebP. The intermediate master PNGs live in [`brand/hero-masters/`](../../../../brand/hero-masters)
(regeneration inputs, never shipped); the regeneration script writes the WebPs here from them.

## Files (all shipped, all WebP)

| File                               | Purpose                                                                   |
| ---------------------------------- | ------------------------------------------------------------------------- |
| `cmdr-hero-frame-{dark,light}.webp`     | 2x window chrome (title bar, toolbar, borders, status bar), transparent pane areas |
| `cmdr-hero-left-pane-{dark,light}.webp` | 2x left pane screenshot content on a transparent canvas                   |
| `cmdr-hero-right-pane-{dark,light}.webp`| 2x right pane screenshot content on a transparent canvas                  |
| `cmdr-hero-*-{dark,light}-1x.webp`      | 1x lossless WebP for 1x-DPR displays                                      |

The 2x WebPs are 2508 x 1634 px (2x retina, the master canvas size); the 1x WebPs are 1254 x 817 px.

## Guardrails

- **This directory ships verbatim into `dist/`, so keep ONLY WebP here.** The master PNGs live in `brand/hero-masters/`
  on purpose, so the bundle never ships the ~3 MB of intermediate PNGs. Don't move masters back here.
- **Only the WebPs are referenced by the site**; the masters in `brand/hero-masters/` exist solely for regeneration.
- **Don't switch the WebPs to lossy without measuring.** Lossless WebP beats lossy q90 here (flat UI chrome compresses
  better losslessly) and is pixel-perfect.
- **Don't convert the layers back to `<img>` tags.** `Hero.astro` loads them as CSS `background-image` with
  `image-set()` (1x/2x by device pixel ratio), switching dark/light via CSS selectors on `data-theme` /
  `prefers-color-scheme`. The browser's preload scanner downloads every `<img>` even inside a `display: none` subtree,
  so `<img>` would fetch both theme variants on every visit; background images only load for rendered elements.
