# Finder extension

A top-level "Open in Cmdr" in Finder's right-click menu, opt-in from Settings. It's a small sandboxed Swift Finder Sync
extension that calls the macOS service the app already declares (`src-tauri/Info.plist`, `../src-tauri/src/reveal/`).

## Module map

- `OpenInCmdr.swift`: the menu item. `Info.plist`, `Entitlements.plist`: the bundle's own, sandboxed.
- `build.sh`: `compile` in the pipeline's `compile` job, `assemble` (join, stamp, sign) in its `sign` job.

## Must-knows

- **It calls the service, ❌ never a URL.** A sandboxed caller can't send a scheme the target doesn't claim
  (`kLSAppDoesNotClaimTypeErr`), and registering `cmdr-cli` was rejected on purpose. `NSPerformService` works from the
  sandbox and needs no file access. `DETAILS.md`.
- **`serviceName` must match `NSMenuItem` in `src-tauri/Info.plist` exactly**: that's how macOS finds the service.
- **Startup disk only.** ❌ Don't add volumes to `directoryURLs`: each one then shows Cmdr's icon in Finder's sidebar.
- **Release bundles only.** A dev build carries no extension, so a worktree build can't register one. The Settings row
  is `src/lib/settings/sections/FinderExtensionCard.svelte`, its backend `src-tauri/src/reveal/finder_extension.rs`.
- **The updater copies `PlugIns` before `MacOS`** (`src-tauri/src/updater/installer.rs`), keeping the main binary last.

Why this shape, what it costs, and how it's released: `DETAILS.md`.
