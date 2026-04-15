# MTP module

MTP (Media Transfer Protocol) support for Android devices and PTP cameras connected via USB.
Available on macOS and Linux (`#[cfg(any(target_os = "macos", target_os = "linux"))]`).
On Linux, users may need udev rules for USB device permissions (see `resources/99-cmdr-mtp.rules`).

## File map

| File | Purpose |
|------|---------|
| `mod.rs` | Re-exports public surface; module-level doc |
| `types.rs` | `MtpDeviceInfo`, `MtpStorageInfo` — camelCase JSON via `serde(rename_all)` |
| `discovery.rs` | `list_mtp_devices()` via `mtp_rs::MtpDevice::list_devices()`; device IDs formatted as `"mtp-{location_id}"` |
| `watcher.rs` | `start_mtp_watcher()` — nusb hotplug watcher; 500 ms delay on connect before re-checking; auto-connects detected devices via `MtpConnectionManager::connect()` and auto-disconnects removed ones |
| `macos_workaround.rs` | macOS-only (`#[cfg(target_os = "macos")]`). Auto-suppresses `ptpcamerad` via `launchctl disable` + `pkill`; restores on disconnect/exit; `ensure_ptpcamerad_enabled()` on startup for crash recovery. Falls back to manual `PTPCAMERAD_WORKAROUND_COMMAND` dialog if suppression fails |
| `connection/mod.rs` | `MtpConnectionManager` singleton (`LazyLock`); `DeviceEntry` map; `connect()` (idempotent, probes write capability, registers `MtpVolume`); `disconnect()` |
| `connection/cache.rs` | `PathHandleCache` (path → MTP object handle), `ListingCache` (5 s TTL), `EventDebouncer` (500 ms per device) |
| `connection/errors.rs` | `MtpConnectionError` enum with typed variants and `map_mtp_error()` from `mtp_rs::Error` |
| `connection/event_loop.rs` | Background tokio task per device: polls `device.next_event()`, computes diffs, emits `directory-diff` events using the unified diff system |
| `connection/directory_ops.rs` | `list_directory()` (with lock-contention logging), `resolve_path_to_handle()` (cache-only) |
| `connection/file_ops.rs` | `download_file()`, `upload_file()` — emit `mtp-transfer-progress` Tauri events |
| `connection/mutation_ops.rs` | `delete()` (recursive, children-first), `create_folder()`, `rename()`, `move_object()` — no copy+delete fallback |
| `connection/bulk_ops.rs` | `scan_for_copy()`, `upload_recursive()` — use `Box::pin` for async recursion |
| `virtual_device.rs` | Virtual MTP device for E2E testing; creates backing dirs + registers device via `mtp-rs`. Gated behind `virtual-mtp` feature. Run with: `cd apps/desktop && pnpm tauri dev -c src-tauri/tauri.dev.json --features virtual-mtp` |

## Architecture / data flow

```
USB plug-in
  → nusb hotplug event (watcher.rs)
  → 500 ms delay
  → check MTP_ENABLED gate — skip if disabled
  → list_mtp_devices() (discovery.rs)
  → auto_connect_device() (watcher.rs)
    → MtpConnectionManager::connect()
    → open_device() via MtpDeviceBuilder
    → probe_write_capability() per storage
    → register MtpVolume in global VolumeManager
    → start_event_loop() per device
    → emit mtp-device-connected (JSON payload includes `deviceName`: from `connected_info.device.product`, empty string if unknown)
    → broadcast::emit_volumes_changed()

USB unplug
  → nusb hotplug event (watcher.rs)
  → auto_disconnect_device() (watcher.rs)
    → MtpConnectionManager::disconnect()
    → emit mtp-device-disconnected
    → broadcast::emit_volumes_changed()

Event loop (event_loop.rs)
  → device.next_event()
  → ObjectAdded/Removed/Changed → compute_diff() → emit directory-diff
  → StoreAdded → handle_storage_added() → register MtpVolume → emit volumes-changed
  → StoreRemoved → handle_storage_removed() → unregister MtpVolume → emit volumes-changed
```

### MTP enabled/disabled toggle

`MTP_ENABLED` (`AtomicBool`, default `true`) in `watcher.rs` gates all auto-connect behavior. The watcher loop always runs (it's `OnceLock`-based, no shutdown channel), but `check_for_device_changes()` returns early when disabled.

- **`set_mtp_enabled_flag(bool)`** — Sets the flag without side effects. Called at startup from `lib.rs` before `start_mtp_watcher()` so the initial auto-connect respects the persisted setting.
- **`set_mtp_enabled(bool, app)`** — Async. Called at runtime via the `set_mtp_enabled` Tauri command. When disabling: disconnects all devices, clears `KNOWN_DEVICES`, restores ptpcamerad (macOS). When enabling: calls `check_for_device_changes()` to pick up already-plugged devices.
- **Setting key**: `fileOperations.mtpEnabled` in `settings.json`, read by `settings/loader.rs` at startup.
- **Interaction with ptpcamerad**: disabling MTP calls `restore_ptpcamerad_unconditionally()`. Re-enabling triggers auto-connect, which re-suppresses ptpcamerad if devices are found.

The frontend is a passive consumer: it subscribes to `volumes-changed` (for the volume picker)
and `mtp-device-connected`/`mtp-device-disconnected` (for device connection state tracking).
It never orchestrates MTP connections.

## Key patterns and gotchas

- **Device lock**: `Arc<tokio::sync::Mutex<MtpDevice>>` held for the entire USB I/O call (tokio's Mutex can be held across `.await` points, unlike `std::sync::Mutex`). Operations are serialized per device with a 30 s timeout (`MTP_TIMEOUT_SECS`). Holding the lock too long logs a warning.
- **Cache-only path resolution**: `resolve_path_to_handle()` fails if the path has not appeared in a prior `list_directory()` call. There is no on-demand path walk.
- **Write capability probe**: `probe_write_capability()` creates a hidden `.cmdr_write_probe` folder to detect cameras that advertise write support but reject writes at runtime (`StoreReadOnly`). Timeout or non-fatal errors are treated as writable (benefit of the doubt).
- **Automatic ptpcamerad suppression**: on macOS, the watcher auto-suppresses `ptpcamerad` via `launchctl disable` + `pkill -9` before connecting to MTP devices, and restores it when all devices disconnect or the app exits. On startup, `ensure_ptpcamerad_enabled()` runs to recover from a previous crash. If suppression fails, the existing `ExclusiveAccess` dialog serves as a manual fallback.
- **ExclusiveAccess errors (fallback)**: when `ptpcamerad` claims a device despite suppression, `connect()` emits `mtp-exclusive-access-error` with the blocking process name (from `ioreg`) so the frontend can show a dialog with the workaround command. On Linux, the blocking process is reported as `None`.
- **PermissionDenied errors (Linux)**: when `open_device()` fails with "permission denied" (missing udev rules), `connect()` emits `mtp-permission-error`. Frontend shows `MtpPermissionDialog` with a copyable udev install command. Rules file at `resources/99-cmdr-mtp.rules`.
- **Async recursion**: all recursive operations in `bulk_ops.rs` use `Box::pin(async move { ... })`.
- **Event loop shutdown**: uses a biased `tokio::select!` so the shutdown signal (broadcast channel) is always checked first.
- **Volume IDs**: MTP storage volumes use `"{device_id}:{storage_id}"` (e.g., `"mtp-336592896:65537"`).

## Dependencies

- `mtp_rs` — MTP session, object listing, file transfer
- `nusb` — USB hotplug events
- `futures_util` — `StreamExt` for hotplug stream
- `crate::file_system` — `VolumeManager`, `MtpVolume`, `FileEntry`, `compute_diff`
