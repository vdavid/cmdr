# Stubs module

Non-macOS/non-Linux compilation stubs for platform-specific modules.
Linux now has real implementations for all modules: volumes (`volumes_linux/`), MTP (`mtp/`),
network (`network/`), accent color (`accent_color_linux.rs`), and permissions (`permissions_linux.rs`).
On other platforms (not macOS, not Linux), all stubs are used. Never compiled on macOS.

## File map

| File | Purpose |
|------|---------|
| `mod.rs` | Declares sub-modules; all gated with `#[cfg(not(target_os = "linux"))]` since Linux has real implementations for everything |
| `volumes.rs` | Returns root `/`, Home, and existing Desktop/Documents/Downloads; `get_volume_space` uses `libc::statvfs`; `start_volume_watcher` is a no-op. Only compiled on non-macOS, non-Linux platforms. |
| `permissions.rs` | `check_full_disk_access` always returns `true`; `open_privacy_settings` returns an error. Only compiled on non-macOS, non-Linux platforms. |
| `network.rs` | All ~20 network commands return empty results or errors; types mirror the macOS shapes for JSON compatibility. Only compiled on non-macOS, non-Linux platforms. |
| `accent_color.rs` | `get_accent_color` returns `"#d4a006"` (brand gold fallback). Only compiled on non-macOS, non-Linux platforms. |
| `mtp.rs` | All MTP commands return `MtpConnectionError::NotSupported`; defines its own local `FileEntry` subset and additional stub types: `ConnectedDeviceInfo`, `MtpOperationResult`, `MtpObjectInfo`, `MtpScanResult`. Only compiled on non-macOS, non-Linux platforms. |

## Key patterns and gotchas

- **JSON shape must match macOS.** The frontend does not branch on platform. If a macOS type gains or loses fields, the corresponding stub type needs manual alignment. This is most fragile in `stubs/mtp.rs`, which has a local `FileEntry` that mirrors `crate::file_system::FileEntry` — if the real struct changes, update the stub too.
- **`stubs/mtp.rs` avoids importing `crate::file_system`** to keep the stub dependency-free and fast to compile. The local `FileEntry` is a deliberate duplication.
- **`#[allow(dead_code)]`** on no-op functions (e.g., `start_volume_watcher`, `start_discovery`, `load_known_shares`) that exist only to keep the API surface symmetric with macOS for any internal callers.
- **`libc` is the only external dependency** (used in `volumes.rs` for `statvfs`). Everything else is intentionally minimal.
- **Do not add logic here.** Stubs must remain trivial. Real functionality belongs in platform-specific subsystem modules.

## Dependencies

- `libc` — `volumes.rs` only, for `statvfs`
- `dirs` — `volumes.rs`, for `home_dir()`
- Tauri runtime types (`tauri::command`, `tauri::AppHandle`, `tauri::Runtime`)
