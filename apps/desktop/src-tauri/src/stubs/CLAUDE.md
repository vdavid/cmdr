# Stubs module

Linux / non-macOS compilation stubs for platform-specific modules.
Used exclusively by the Docker-based E2E test pipeline (tauri-driver on Linux).
Never compiled on macOS — selected at compile time via `cfg` gates in `commands/mod.rs` and `lib.rs`.

## File map

| File | Purpose |
|------|---------|
| `mod.rs` | Declares sub-modules |
| `volumes.rs` | Returns root `/`, Home, and existing Desktop/Documents/Downloads; `get_volume_space` uses `libc::statvfs`; `start_volume_watcher` is a no-op |
| `permissions.rs` | `check_full_disk_access` always returns `true`; `open_privacy_settings` returns an error |
| `network.rs` | All ~20 network commands return empty results or errors; types mirror the macOS shapes for JSON compatibility |
| `accent_color.rs` | `get_accent_color` returns `"#007aff"` (macOS default blue) |
| `mtp.rs` | All MTP commands return `MtpConnectionError::NotSupported`; defines its own local `FileEntry` subset and additional stub types: `ConnectedDeviceInfo`, `MtpOperationResult`, `MtpObjectInfo`, `MtpScanResult` |

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
