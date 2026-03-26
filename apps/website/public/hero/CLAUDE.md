# Hero images

Three-layer composited hero illustration for the getcmdr.com landing page. Each screenshot (dark and light) is split
into a frame + two pane cutouts so they can animate independently in `Hero.astro`.

## Files

| File                              | Purpose                                                                             |
| --------------------------------- | ----------------------------------------------------------------------------------- |
| `cmdr-hero-frame-{dark,light}.png`     | Window chrome (title bar, toolbar, borders, status bar) with transparent pane areas |
| `cmdr-hero-left-pane-{dark,light}.png` | Left pane screenshot content on transparent canvas                                  |
| `cmdr-hero-right-pane-{dark,light}.png`| Right pane screenshot content on transparent canvas                                 |

All six files share the same canvas size: **2508 x 1634 px** (2x retina).

TODO: Light variants and `Hero.astro` update to use dark/light files (currently uses unsuffixed dark-only files).

## How to reshoot

### Prerequisites

- CleanShot X for macOS screenshot with shadow
- ImageMagick (`magick` CLI)
- The app running via `pnpm dev`

### 1. Resize the window

Using the Tauri MCP:

```
manage_window action: "resize", width: 1142, height: 705, logical: true
```

This produces a 2284 x 1410 retina window, which matches the hero frame proportions.

### 2. Disable dev mode indicators

Via Tauri MCP `webview_execute_js` on the main window:

```js
document.querySelector('.title-bar').classList.remove('dev-mode')
document.querySelector('.title-text').textContent = 'Cmdr'
```

### 3. Set up the app state

Using the Cmdr MCP tools:

- Set color to Cmdr gold: `set_setting id: "appearance.appColor", value: "cmdr-gold"`
- Close all but one tab on both sides (`tab close` extra tabs, or `tab close_others`)
- Left side: navigate to `src-tauri/src`, full mode, tab lock off, cursor on "mcp"
- Right side: navigate to `src/lib`, brief mode, tab lock on (pinned), cursor on "indexing"
- Hidden files visible (toggle if needed)
- Focus on the left pane (`switch_pane` if needed)

After the screenshots, revert the color: `set_setting id: "appearance.appColor", value: "system"`

### 4. Take screenshots (by a human)

Subprocess (read this first but dont execute): How to take a screenshot:
- Make sure [CleanShot](https://cleanshot.com/) is running.
- Open CleanShot's top menu, and click Capture Window. Important not to use the ⌘⇧4 shortcut because Cmdr bottom bar
  looks different with Shift held.
- Once taken, click Edit → switch background to None. Then Save.

So, the process:
- Make sure you're in dark mode
- Dark mode: take screenshot with CleanShot, save as `~/Downloads/cmdr-scrshot-dark.png`
- Press ⌘D, switch to light mode
- Light mode: take screenshot, save as `~/Downloads/cmdr-scrshot-light.png`

### 5. Generate the 6 hero PNGs

From the repo root:

```bash
cd apps/website/public/hero

# Create pane mask (white = keep, black = make transparent)
magick -size 2508x1634 xc:white \
  -fill black -draw "rectangle 114,281 1246,1387" \
  -fill black -draw "rectangle 1261,281 2393,1387" \
  /tmp/hero-mask.png

for variant in dark light; do
  src=~/Downloads/cmdr-scrshot-${variant}.png

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

rm -f /tmp/hero-mask.png /tmp/src-alpha.png /tmp/new-alpha.png
```

### 6. Verify

Check that all six PNGs are 2508 x 1634 and file sizes are roughly: ~650 KB frame, ~340 KB left pane, ~140 KB right
pane. To verify the frame transparency, composite on a red background:

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

| Region                     | Origin (x, y) | Size (w x h) | Notes                                            |
| -------------------------- | ------------- | ------------ | ------------------------------------------------ |
| Window (excl. shadow)      | (112, 76)     | 2284 x 1410  | The actual window chrome boundary                |
| Left pane cutout           | (114, 281)    | 1133 x 1107  | Starts below column headers, ends above status bar |
| Right pane cutout          | (1261, 281)   | 1133 x 1107  | Same size as left pane                           |
| Pane divider gap           | (1247, 281)   | 14 px wide   | Between the two cutouts (includes resize bar)    |
| Left pane mask rectangle   | (114, 281) to (1246, 1387)  | | Used in the `-draw` command        |
| Right pane mask rectangle  | (1261, 281) to (2393, 1387) | | Used in the `-draw` command        |
