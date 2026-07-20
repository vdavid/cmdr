# Stubs module details

Per-stub behavior and rationale. `CLAUDE.md` holds the invariants; the catalog below is reference.

## Per-stub behavior (non-macOS, non-Linux unless noted)

- **`accent_color.rs`**: `get_accent_color` returns `"#d4a006"` (brand gold fallback).
- **`mtp.rs`**: all MTP commands return `MtpConnectionError::NotSupported`. Defines a local `FileEntry` subset plus stub
  types `ConnectedDeviceInfo`, `MtpObjectInfo`, `MtpScanResult`.
- **`network.rs`**: all network commands return empty results or errors; types mirror the macOS shapes for JSON
  compatibility.
- **`permissions.rs`**: `check_full_disk_access` / `check_full_disk_access_quiet` return `true`;
  `open_privacy_settings` and the appearance/System-Settings deep-link commands return errors.
- **`text_size.rs`** (non-macOS, so also Linux): `get_system_text_size_multiplier` returns `1.0` (no system scaling).
- **`volumes.rs`**: returns root `/`, Home, and existing Desktop/Documents/Downloads; `get_volume_space` uses
  `libc::statvfs`; `start_volume_watcher` is a no-op.

## Decisions

### Duplicate `FileEntry` in `mtp.rs` rather than importing `crate::file_system::FileEntry`

Stubs compile on platforms where the real `file_system` module may carry platform-specific dependencies or conditional
compilation that differs. A dependency-free stub avoids pulling in code that may not compile on the target and keeps
stub compilation fast by minimizing the dependency graph. Cost: when the real `FileEntry` changes, the stub needs manual
alignment (see the `CLAUDE.md` JSON-shape invariant).

### Hardcoded success values rather than errors

The frontend doesn't branch on platform; it calls the same commands everywhere. Returning errors would trigger
error-handling UI (toasts, retry prompts) on unsupported platforms. Returning empty/success makes the feature silently
not appear, which is the correct UX for "not available on this platform."

### `#[allow(dead_code)]` on no-op functions

Some no-op functions (for example `start_volume_watcher`) exist only to keep the API surface symmetric with macOS for
internal callers, so they carry `#[allow(dead_code)]`.
