# App icons

## Source files

- **Original artwork**: `_ignored/designs/app-logo-1024px-x-1024px.png` (1024x1024 PNG, transparent background)
- **Icon Composer project**: `_ignored/designs/Sequoia.icon` (macOS Tahoe Liquid Glass format)

## What gets bundled

Two icon systems coexist in the final `.app` for backward compatibility:

| File | Location in bundle | Used by | Generated from |
|------|--------------------|---------|----------------|
| `icons/icon.icns` + PNGs | `Contents/Resources/icon.icns` | macOS pre-Tahoe (via `CFBundleIconFile`) | `pnpm tauri icon` |
| `resources/Assets.car` | `Contents/Resources/Assets.car` | macOS Tahoe 26+ (via `CFBundleIconName`) | `actool` from `.icon` file |
| `icons/icon.ico` + Square PNGs | Windows/Store | Windows | `pnpm tauri icon` |

`Info.plist` has `CFBundleIconName` = `"Sequoia"` which tells Tahoe to look in `Assets.car`.
Older macOS ignores this and falls back to `CFBundleIconFile` (set automatically by Tauri).

## Regenerating icons

### All platforms (PNGs, .icns, .ico)

```bash
cd apps/desktop
pnpm tauri icon ../../_ignored/designs/app-logo-1024px-x-1024px.png
```

This overwrites everything in `src-tauri/icons/`.

### macOS Tahoe Liquid Glass icon (Assets.car)

1. Open Icon Composer (bundled with Xcode at `/Applications/Xcode.app/Contents/Applications/Icon Composer.app`)
2. Import the 1024x1024 PNG, adjust layers/translucency as desired
3. Export as `.icon` to `_ignored/designs/Sequoia.icon`
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

### Gotchas

- **`actool` needs Xcode first-launch**: If `actool` fails with a plugin error, run `xcodebuild -runFirstLaunch`.
- **`--app-icon` name matters**: The name passed to `--app-icon` (here `Sequoia`) must match `CFBundleIconName`
  in `Info.plist`. If you change one, change both.
- **Tahoe squircle jail**: macOS Tahoe analyzes icon pixels. If the icon doesn't fill the expected squircle area,
  the system shrinks it and adds a dark gray background. The `Assets.car` approach avoids this entirely.
- **Tauri native support pending**: Tauri has a commit ready to support `.icon` files natively
  ([tauri#14207](https://github.com/tauri-apps/tauri/issues/14207)). Once shipped, the manual `actool` step and
  `bundle.macOS.files` config can be replaced with a path in the `bundle.icon` array.
