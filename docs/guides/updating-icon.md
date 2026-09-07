# Updating the icon

Everything that shows the Cmdr mark, from the `.app` bundle to the newsletter, comes from one file:
`brand/logos/cmdr.svg`. Edit that, run `pnpm icons`, do the one manual step below, refresh two baselines, commit.

## The one command

```bash
pnpm icons
```

`scripts/regenerate-icons.sh` needs `magick` and `rsvg-convert` (`brew install imagemagick librsvg`). It rewrites:

- `apps/desktop/src-tauri/icons/**`: every PNG, `icon.icns`, `icon.ico`, the `Square*` set, `android/`, `ios/`. Via
  `pnpm tauri icon`, which takes the SVG directly.
- `brand/logos/cmdr-{512,128,32}.png`: the press kit, copied from the desktop icons.
- `apps/website/public/`: `logo.svg` (minified), `logo-512.png`, `favicon.png`, `favicon.ico`, `apple-touch-icon.png`.
- `apps/analytics-dashboard/static/logo.svg` and `apps/desktop/static/logo.svg`: minified copies, for the dashboard
  favicon and the About window.
- `apps/desktop/static/favicon.png`: the webview favicon, invisible in a Tauri window but real.
- `_ignored/designs/app-logo-1024px-x-1024px.png`: a 1024 master, only an input to the manual step below.

## The manual step: the macOS Tahoe icon

`apps/desktop/src-tauri/resources/Assets.car` is the Liquid Glass icon macOS Tahoe 26+ uses, and no script can build it.
Skipping it leaves Tahoe showing the **old** artwork while every other surface shows the new one, which is worse than
either alone. It needs a person, and the answers to `CLAUDE.md`'s "Tahoe squircle jail" note live there.

1. Open Icon Composer (`/Applications/Xcode.app/Contents/Applications/Icon Composer.app`).
2. **File → New**, then drag `brand/logos/cmdr.svg` onto the layer well. Prefer the SVG; fall back to
   `_ignored/designs/app-logo-1024px-x-1024px.png` if the vector is rejected. **File → Open** won't work: it only
   accepts `.icon` projects, so artwork of any format shows up grayed out there.
3. Adjust layers and translucency, then export as `.icon` to `_ignored/designs/Sequoia.icon`.
4. Compile:

```bash
actool _ignored/designs/Sequoia.icon \
  --compile apps/desktop/src-tauri/resources \
  --output-format human-readable-text --notices --warnings --errors \
  --output-partial-info-plist /dev/null \
  --app-icon Sequoia --include-all-app-icons \
  --enable-on-demand-resources NO \
  --target-device mac \
  --minimum-deployment-target 26.0 \
  --platform macosx
```

**`_ignored/` is gitignored, so `Sequoia.icon` is not in the repo and has gone missing before.** Keep a copy somewhere
durable; when it's gone, step 2 means rebuilding the layers from scratch. `actool` failing with a plugin error means
Xcode hasn't run its first launch: `xcodebuild -runFirstLaunch`.

## Then refresh two baselines

Both pin the old bytes and will fail until you do. Neither needs consent, per `.claude/rules/file-length-allowlist.md`.

```bash
rm scripts/check/checks/website-bundle-size-baseline.json && pnpm check website-bundle-size
apps/website/scripts/update-visual-baselines.sh   # the header and footer carry the logo
```

## Gotchas

- **ImageMagick renders the SVG solid black without librsvg.** It silently falls back to its own renderer, which drops
  every `url(#…)` paint and every `<use>`. There's no warning, just a black logo. `magick -list delegate | grep svg` has
  to show `rsvg-convert`. The script only ever hands `magick` a PNG for this reason.
- **`logo-512.png` stays a PNG on purpose.** The listmonk newsletter templates hotlink
  `https://getcmdr.com/logo-512.png` and email clients don't render SVG, and it's the `organization.logo` in the site's
  JSON-LD. Same for `favicon.ico` and `apple-touch-icon.png`: `.ico` is a raster container, and iOS ignores an SVG touch
  icon.
- **Don't hand-edit anything `pnpm icons` writes.** `pnpm tauri icon` overwrites the whole `icons/` directory, and the
  minified copies are generated. Edit `brand/logos/cmdr.svg`, rerun.
- **SVGO settings live in `brand/logos/svgo.config.mjs`.** `removeViewBox` and `removeTitle` are off: the viewBox is
  what lets CSS size the logo, and the title is its accessible name when inlined. The rest of `preset-default` is
  pixel-identical to the source (verified at 1024 against both librsvg and resvg, 2026-09-07).
- **Check it at 16px.** The haze collapses to a colored strip there, which is fine, but a change that only reads at 512
  is a change that doesn't reach the Dock or a browser tab.

## Where the artwork itself is described

`docs/guides/branding.md` holds the visual identity (colors, type, what the mark means). `brand/CLAUDE.md` lists the
exported deliverables. The bundle layout and why two macOS icon systems coexist:
`apps/desktop/src-tauri/icons/DETAILS.md`.
