# App icons

Bundled app icons for all platforms. Source artwork and the regeneration commands live in [DETAILS.md](DETAILS.md);
read it before regenerating.

## Must-knows

- **Two icon systems coexist in the `.app` for backward compatibility.** `icons/icon.icns` (+ PNGs) serves macOS
  pre-Tahoe via `CFBundleIconFile`; `resources/Assets.car` serves macOS Tahoe 26+ via `CFBundleIconName`. `Info.plist`
  sets `CFBundleIconName` = `"Sequoia"` (Tahoe reads `Assets.car`); older macOS ignores it and falls back to
  `CFBundleIconFile` (Tauri sets that automatically). Don't drop either system.
- **`--app-icon` must match `CFBundleIconName`.** The `actool` invocation passes `--app-icon Sequoia`; that name must
  equal `CFBundleIconName` in `Info.plist`. Change one, change both, or Tahoe shows no icon.
- **Tahoe squircle jail.** macOS Tahoe analyzes icon pixels; an icon that doesn't fill the expected squircle gets
  shrunk with a dark gray background added. The `Assets.car` path avoids this, so don't replace it with a plain PNG for
  Tahoe.
- **`pnpm tauri icon` overwrites the whole `icons/` directory.** It regenerates PNGs, `.icns`, and `.ico` from the
  source artwork, so hand-edits here are lost on the next run.
