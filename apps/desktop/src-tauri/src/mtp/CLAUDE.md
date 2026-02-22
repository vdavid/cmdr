# MTP module

MTP (Media Transfer Protocol) support for Android devices and PTP cameras connected via USB.
macOS-only (`#[cfg(target_os = "macos")]` at the command registration level).

## File map

| File | Purpose |
|------|---------|
| `mod.rs` | Re-exports public surface; module-level doc |
| `types.rs` | `MtpDeviceInfo`, `MtpStorageInfo` — camelCase JSON via `serde(rename_all)` |
| `discovery.rs` | `list_mtp_devices()` via `mtp_rs::MtpDevice::list_devices()`; device IDs formatted as `"mtp-{location_id}"` |
| `watcher.rs` | `start_mtp_watcher()` — nusb hotplug watcher; 500 ms delay on connect before re-checking; emits `mtp-device-detected` / `mtp-device-removed` Tauri events |
| `macos_workaround.rs` | Detects `ptpcamerad` via `ioreg`; exposes `PTPCAMERAD_WORKAROUND_COMMAND` (a bash one-liner) |
| `connection/mod.rs` | `MtpConnectionManager` singleton (`LazyLock`); `DeviceEntry` map; `connect()` (idempotent, probes write capability, registers `MtpVolume`); `disconnect()` |
| `connection/cache.rs` | `PathHandleCache` (path → MTP object handle), `ListingCache` (5 s TTL), `EventDebouncer` (500 ms per device) |
| `connection/errors.rs` | `MtpConnectionError` enum with typed variants and `map_mtp_error()` from `mtp_rs::Error` |
| `connection/event_loop.rs` | Background tokio task per device: polls `device.next_event()`, computes diffs, emits `directory-diff` events using the unified diff system |
| `connection/directory_ops.rs` | `list_directory()` (with lock-contention logging), `resolve_path_to_handle()` (cache-only) |
| `connection/file_ops.rs` | `download_file()`, `upload_file()` — emit `mtp-transfer-progress` Tauri events |
| `connection/mutation_ops.rs` | `delete()` (recursive, children-first), `create_folder()`, `rename()`, `move_object()` — no copy+delete fallback |
| `connection/bulk_ops.rs` | `scan_for_copy()`, `download_recursive()`, `upload_recursive()` — use `Box::pin` for async recursion |

## Architecture / data flow

```
USB plug-in
  → nusb hotplug event (watcher.rs)
  → 500 ms delay
  → list_mtp_devices() (discovery.rs)
  → emit mtp-device-detected

Frontend calls connect_mtp_device
  → MtpConnectionManager::connect()
  → open_device() via MtpDeviceBuilder
  → probe_write_capability() per storage
  → register MtpVolume in global VolumeManager
  → start_event_loop() per device
  → emit mtp-device-connected

Event loop (event_loop.rs)
  → device.next_event()
  → compute_diff()
  → emit directory-diff (same format as local file watching)
```

## Key patterns and gotchas

- **Device lock**: `Arc<tokio::sync::Mutex<MtpDevice>>` held for the entire USB I/O call (tokio's Mutex can be held across `.await` points, unlike `std::sync::Mutex`). Operations are serialized per device with a 30 s timeout (`MTP_TIMEOUT_SECS`). Holding the lock too long logs a warning.
- **Cache-only path resolution**: `resolve_path_to_handle()` fails if the path has not appeared in a prior `list_directory()` call. There is no on-demand path walk.
- **Write capability probe**: `probe_write_capability()` creates a hidden `.cmdr_write_probe` folder to detect cameras that advertise write support but reject writes at runtime (`StoreReadOnly`). Timeout or non-fatal errors are treated as writable (benefit of the doubt).
- **ExclusiveAccess errors**: when `ptpcamerad` claims a device, `connect()` emits `mtp-exclusive-access-error` with the blocking process name (from `ioreg`) so the frontend can show a dialog with the workaround command.
- **Async recursion**: all recursive operations in `bulk_ops.rs` use `Box::pin(async move { ... })`.
- **Event loop shutdown**: uses a biased `tokio::select!` so the shutdown signal (broadcast channel) is always checked first.
- **Volume IDs**: MTP storage volumes use `"{device_id}:{storage_id}"` (e.g., `"mtp-336592896:65537"`).

## Dependencies

- `mtp_rs` — MTP session, object listing, file transfer
- `nusb` — USB hotplug events
- `futures_util` — `StreamExt` for hotplug stream
- `crate::file_system` — `VolumeManager`, `MtpVolume`, `FileEntry`, `compute_diff`
