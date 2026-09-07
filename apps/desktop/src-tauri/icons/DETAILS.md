# App icons details

The bundle layout: which file macOS and Windows actually read, and why two icon systems coexist. The must-knows that
silently break things are in `CLAUDE.md`; the procedure for changing the artwork is `docs/guides/updating-icon.md`.

## Source files

- **Original artwork**: `brand/logos/cmdr.svg`. `pnpm tauri icon` takes an SVG directly.
- **Icon Composer project**: `_ignored/designs/Sequoia.icon` (macOS Tahoe Liquid Glass format), built from a 1024 PNG
  that `pnpm icons` renders to `_ignored/designs/app-logo-1024px-x-1024px.png`. `_ignored/` is gitignored, so this
  project is not in the repo and has gone missing before.

## What gets bundled

Three columns: bundled file, location + consumer, and how it's generated.

| File | Location and consumer | Generated from |
|------|------------------------|----------------|
| `icons/icon.icns` + PNGs | `Contents/Resources/icon.icns`, macOS pre-Tahoe via `CFBundleIconFile` | `pnpm tauri icon` |
| `resources/Assets.car` | `Contents/Resources/Assets.car`, macOS Tahoe 26+ via `CFBundleIconName` | `actool` from the `.icon` file |
| `icons/icon.ico` + Square PNGs | Windows / Store | `pnpm tauri icon` |

`actool` also emits a `Sequoia.icns` fallback next to `Assets.car`, which nothing currently reads.

## Regenerating

`pnpm icons`, then the manual Icon Composer pass for `Assets.car`. Full procedure, the `actool` invocation, and the
gotchas: `docs/guides/updating-icon.md`.

## Gotchas

- **Tauri native `.icon` support is pending** ([tauri#14207](https://github.com/tauri-apps/tauri/issues/14207)). Once
  it ships, the manual `actool` step and `bundle.macOS.files` config can be replaced with a path in the `bundle.icon`
  array, and the one unscriptable step in `docs/guides/updating-icon.md` goes away.
