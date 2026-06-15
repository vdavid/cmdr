# Hero images

Three-layer composited hero illustration for the getcmdr.com landing page. Each screenshot (dark and light) is split
into a frame + two pane cutouts so they animate independently in `Hero.astro`. Reshoot + regeneration procedure (script,
verify, cutout geometry): [DETAILS.md](DETAILS.md).

## Files

| File                                    | Purpose                                                                                        |
| --------------------------------------- | ---------------------------------------------------------------------------------------------- |
| `cmdr-hero-frame-{dark,light}.png`      | Master: 2x window chrome (title bar, toolbar, borders, status bar) with transparent pane areas |
| `cmdr-hero-left-pane-{dark,light}.png`  | Master: 2x left pane screenshot content on transparent canvas                                  |
| `cmdr-hero-right-pane-{dark,light}.png` | Master: 2x right pane screenshot content on transparent canvas                                 |
| `cmdr-hero-*-{dark,light}.webp`         | Shipped: 2x lossless WebP, generated from the master PNGs                                      |
| `cmdr-hero-*-{dark,light}-1x.webp`      | Shipped: 1x lossless WebP for 1x-DPR displays                                                  |

The six master PNGs share one canvas size: 2508 x 1634 px (2x retina). The 1x WebPs are 1254 x 817 px.

## Guardrails

- **Only the WebPs are referenced by the site**; the PNGs stay as regeneration masters.
- **Don't switch the WebPs to lossy without measuring.** Lossless WebP beats lossy q90 here (flat UI chrome compresses
  better losslessly) and is pixel-perfect.
- **Don't convert the layers back to `<img>` tags.** `Hero.astro` loads them as CSS `background-image` with
  `image-set()` (1x/2x by device pixel ratio), switching dark/light via CSS selectors on `data-theme` /
  `prefers-color-scheme`. The browser's preload scanner downloads every `<img>` even inside a `display: none` subtree,
  so `<img>` would fetch both theme variants on every visit; background images only load for rendered elements.
