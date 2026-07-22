# App icons details

Source artwork, the bundle layout, and the regeneration steps. The must-knows that silently break things are in
`CLAUDE.md`.

## Source files

- **Original artwork**: `_ignored/designs/app-logo-1024px-x-1024px.png` (1024x1024 PNG, transparent background).
- **Icon Composer project**: `_ignored/designs/Sequoia.icon` (macOS Tahoe Liquid Glass format).

## What gets bundled

Three columns: bundled file, location + consumer, and how it's generated.

| File | Location and consumer | Generated from |
|------|------------------------|----------------|
| `icons/icon.icns` + PNGs | `Contents/Resources/icon.icns`, macOS pre-Tahoe via `CFBundleIconFile` | `pnpm tauri icon` |
| `resources/Assets.car` | `Contents/Resources/Assets.car`, macOS Tahoe 26+ via `CFBundleIconName` | `actool` from the `.icon` file |
| `icons/icon.ico` + Square PNGs | Windows / Store | `pnpm tauri icon` |

## Regenerating

### All platforms (PNGs, .icns, .ico)

```bash
cd apps/desktop
pnpm tauri icon ../../_ignored/designs/app-logo-1024px-x-1024px.png
```

This overwrites everything in `src-tauri/icons/`.

### macOS Tahoe Liquid Glass icon (Assets.car)

1. Open Icon Composer (bundled with Xcode at `/Applications/Xcode.app/Contents/Applications/Icon Composer.app`).
2. Import the 1024x1024 PNG, adjust layers / translucency as desired.
3. Export as `.icon` to `_ignored/designs/Sequoia.icon`.
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

This produces `resources/Assets.car` (and a `Sequoia.icns` fallback, not currently used).

## Gotchas

- **`actool` needs Xcode first-launch.** If `actool` fails with a plugin error, run `xcodebuild -runFirstLaunch`.
- **Tauri native `.icon` support is pending** ([tauri#14207](https://github.com/tauri-apps/tauri/issues/14207)). Once
  it ships, the manual `actool` step and `bundle.macOS.files` config can be replaced with a path in the `bundle.icon`
  array.
