# Stubs module details

Per-stub behavior and rationale. `CLAUDE.md` holds the invariants; the catalog below is reference.

## Per-stub behavior (non-macOS, non-Linux unless noted)

- **`accent_color.rs`**: `get_accent_color` returns `"#d4a006"` (brand gold fallback).
- No network or MTP stub: SMB browsing, the server commands, and MTP compile on every OS, and the OS-mount part answers
  per OS behind `network/os_mount/platform/` (Windows refuses a mount with `MountError::Unexpected`).
- **`permissions.rs`**: `check_full_disk_access` / `check_full_disk_access_quiet` return `true`;
  `open_privacy_settings` and the appearance/System-Settings deep-link commands return errors.
- **`text_size.rs`** (non-macOS, so also Linux): `get_system_text_size_multiplier` returns `1.0` (no system scaling).
- **`display_accessibility.rs`** / **`glass_tint.rs`** (non-macOS): `get_display_accessibility` reports every option
  off (the frontend still honors `prefers-contrast: more` where the webview reflects it),
  `get_glass_tint_amount` returns `None` (the frontend then uses the slider's middle, 0.5).
- **`volumes.rs`**: returns root `/`, Home, and existing Desktop/Documents/Downloads; `get_volume_space` uses
  `libc::statvfs`; `start_volume_watcher` is a no-op.

## Decisions

### Hardcoded success values rather than errors

The frontend doesn't branch on platform; it calls the same commands everywhere. Returning errors would trigger
error-handling UI (toasts, retry prompts) on unsupported platforms. Returning empty/success makes the feature silently
not appear, which is the correct UX for "not available on this platform."

### `#[allow(dead_code)]` on no-op functions

Some no-op functions (for example `start_volume_watcher`) exist only to keep the API surface symmetric with macOS for
internal callers, so they carry `#[allow(dead_code)]`.
