# Stubs module details

Per-stub behavior and rationale. `CLAUDE.md` holds the invariants; the catalog below is reference.

## Per-stub behavior (non-macOS, non-Linux unless noted)

- **`accent_color.rs`**: `get_accent_color` returns `"#d4a006"` (brand gold fallback).
- **`mtp.rs`**: `connect_mtp_device` returns `MtpConnectionError::NotSupported`, `set_mtp_enabled` is a no-op, and
  `get_ptpcamerad_workaround_command` returns an empty string. Stub types `MtpDeviceInfo`, `MtpStorageInfo`,
  `ConnectedDeviceInfo`.
- **`network.rs`**: all network commands return empty results or errors; types mirror the macOS shapes for JSON
  compatibility. A mount answers `MountError::Unexpected`, and the three "Connect directly" commands answer
  `UpgradeResult::NotSmbMount`: nothing on these platforms is an SMB mount, so that's the one real variant that holds.
  ⚠️ Nothing compiles this file on macOS or Linux (the only platforms built and the only ones in CI), so drift from
  `commands/network.rs` is caught by reading alone. Argument lists match the real commands (compared 2026-09-30). Four
  stubs still answer `Result<_, String>` where the real command answers a typed error, which breaks the JSON-shape
  invariant: `reconnect_volume` and `reconnect_volume_with_credentials` (`ReconnectError`), `disconnect_smb_volume`
  (`EjectError`), and `connect_to_server` (`AddServerError`). Each needs a variant chosen for "not available here".
- **`permissions.rs`**: `check_full_disk_access` / `check_full_disk_access_quiet` return `true`;
  `open_privacy_settings` and the appearance/System-Settings deep-link commands return errors.
- **`text_size.rs`** (non-macOS, so also Linux): `get_system_text_size_multiplier` returns `1.0` (no system scaling).
- **`reduce_transparency.rs`** / **`glass_tint.rs`** (non-macOS): `get_should_reduce_transparency` returns `false`,
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
