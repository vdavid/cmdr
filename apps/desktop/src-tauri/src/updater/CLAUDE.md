# Updater module

Custom macOS updater that syncs files *into* the existing `.app` bundle, preserving its inode and
`com.apple.macl` xattr so macOS TCC (Full Disk Access) permissions survive across updates.

Compiled only on macOS (`#[cfg(target_os = "macos")]`). On other platforms, the Tauri updater plugin handles updates
and the frontend calls the plugin API directly.

## File map

| File | Purpose |
|------|---------|
| `mod.rs` | Three Tauri commands (`check_for_update`, `download_update`, `install_update`) and shared `UpdateState` |
| `manifest.rs` | Parses `latest.json`, compares versions, resolves platform key |
| `signature.rs` | Minisign signature verification (base64-wrapped, matching Tauri's format) |
| `installer.rs` | Extracts tarball, syncs into running bundle, handles privilege escalation |

## Key decisions

**Decision**: Sync files into the bundle instead of replacing the `.app` directory.
**Why**: Replacing the bundle changes its inode, which causes macOS TCC to lose FDA grants. Users would have to
re-grant Full Disk Access after every update.

**Decision**: Sync order is Resources, Info.plist, _CodeSignature, then MacOS binary last.
**Why**: Updating the binary last minimizes the window where the code signature is inconsistent with the binary on
disk. If the app crashes mid-update, the old binary is still intact.

**Decision**: Unconditional deletion of stale files after sync.
**Why**: Old files left behind could cause version mismatches or bloat. The deletion pass removes anything in the
destination that isn't in the source, then cleans up empty directories bottom-up.

**Decision**: Minisign verification before writing tarball to disk.
**Why**: Ensures integrity and authenticity of the update. The public key is compiled into the binary. Both key and
signatures use base64(minisign-text-format) encoding, matching Tauri's internal convention.

**Decision**: Privilege escalation via `osascript` with `rsync -a --delete`.
**Why**: When the app is installed in `/Applications` (owned by root), direct writes fail. `osascript`'s
`do shell script ... with administrator privileges` shows the native macOS auth dialog. `rsync` is used because it
expresses the full sync (copy + delete stale) in a single shell command.

## Key patterns and gotchas

- **macOS-only.** The module, command registrations, and `UpdateState` are all gated with `#[cfg(target_os = "macos")]`.
  On non-macOS, the frontend uses `@tauri-apps/plugin-updater` directly.
- **Staging dir is `/tmp/cmdr-update-staging`.** Cleaned before and after install. If the app crashes mid-install,
  leftover staging doesn't block the next attempt (it gets cleaned on retry).
- **Privilege escalation via `osascript`.** Only triggers when direct writes to `/Applications/Cmdr.app` are denied.
  Users who run from `~/Applications` or a dev build won't see the auth dialog.
- **CI guard.** `check_for_update` returns `None` when the `CI` env var is set, avoiding network calls in tests.
- **Manifest URL is hardcoded** (`https://getcmdr.com/latest.json`), not configurable from the frontend.

## Dependencies

- `reqwest` -- HTTP client for manifest and tarball download
- `minisign-verify` -- signature verification
- `flate2`, `tar` -- tarball extraction
- `filetime` -- touching the bundle after install to trigger LaunchServices refresh
- `base64` -- decoding the double-encoded minisign key and signatures
