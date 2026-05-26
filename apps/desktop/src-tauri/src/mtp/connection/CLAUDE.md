# MTP connection

The MTP session layer: opens devices, owns the per-device tokio task, and exposes typed read/write operations to higher layers. Parent: [`../CLAUDE.md`](../CLAUDE.md).

## File map

| File | Purpose |
|------|---------|
| `mod.rs` | `MtpConnectionManager` singleton (`LazyLock`), `DeviceEntry` map, `connect()` / `disconnect(MtpDisconnectReason)`, IPC DTOs (`MtpTransferProgress`, `MtpOperationResult`, `MtpObjectInfo`) |
| `cache.rs` | `PathHandleCache` (path → `ObjectHandle`), `ListingCache` (5 s TTL via `LISTING_CACHE_TTL_SECS`), `EventDebouncer` (per-device 500 ms via `EVENT_DEBOUNCE_MS`) |
| `errors.rs` | `MtpConnectionError` enum (`DeviceNotFound`, `ExclusiveAccess`, `Disconnected`, `DeviceBusy`, `StoreReadOnly`, `PermissionDenied`, `Cancelled`, …) plus `map_mtp_error()` from `mtp_rs::Error` |
| `event_loop.rs` | Per-device background task: polls `device.next_event()` (clones `MtpDevice` so the interrupt endpoint poll doesn't hold the bulk mutex), computes diffs, emits `directory-diff` |
| `directory_ops.rs` | `list_directory()` (lock-contention warnings), `resolve_path_to_handle()` (cache-only — see Gotchas) |
| `file_ops.rs` | `download_file()`, `upload_file()`, `open_download_stream()`: emit `mtp-transfer-progress`; `open_download_stream` returns a `FileDownload` consumed by `MtpReadStream` in `volume/mtp.rs` |
| `mutation_ops.rs` | `delete()` (recursive, children-first), `create_folder()`, `rename()`, `move_object()`: no copy+delete fallback |
| `bulk_ops.rs` | `scan_for_copy()`: async recursion via `Box::pin` |

## Conventions

- **Device lock**: `Arc<tokio::sync::Mutex<MtpDevice>>` held across `.await` for the duration of one USB I/O call. Operations serialize per device with a 30 s timeout (`MTP_TIMEOUT_SECS`). Holding the lock too long logs a warning.
- **Event polling sidesteps the lock**: `event_loop.rs` clones `MtpDevice` (cheap, `Arc` internally) so `next_event()` reads the USB interrupt endpoint while bulk I/O continues on the locked clone.
- **Cache layers**: `ListingCache` is the read-through cache for `list_directory` (5 s TTL); `PathHandleCache` is populated as a side effect of listing. Both live on the singleton, keyed by full virtual path.
- **Event debounce**: `EventDebouncer` per-device collapses MTP event bursts (bulk copy / delete) to one frontend emit per 500 ms. Cleared on disconnect.
- **Async recursion** in `bulk_ops.rs` and recursive `delete()`: `Box::pin(async move { ... })` to break the infinitely-sized future.
- **Event loop shutdown**: biased `tokio::select!` so the broadcast shutdown signal always wins over the event poll.
- **Cancel propagation**: see parent `../CLAUDE.md` § "Cancel propagation (M2 of cancel-settled)". Cancel-aware entry points here are `delete()` and the `list_objects_with_cancel` path threaded down to `mtp-rs`.

## Gotchas

- **`resolve_path_to_handle()` is cache-only**: returns `ObjectNotFound` if the path has not appeared in a prior `list_directory()` call. There is no on-demand path walk — the caller (usually `MtpVolume`) lists ancestors first. Whole-tree operations that bypass list-first (synthesized paths, restored state) will fail here, not at the USB call.
- **`ListingCache` TTL is per-entry, not invalidated by mutations**: `create_folder` / `rename` / `delete` update the on-device state but a concurrent reader within the 5 s window still sees the stale listing. Mutations should invalidate explicitly if read-after-write consistency matters in that call site.
- **Disconnect-from-event-loop must clear the device registry**: when `next_event()` returns `Error::Disconnected`, `event_loop.rs` calls back into `connection_manager().handle_device_disconnected(...)`. Skipping this leaves a dead entry in `devices` and the next `connect()` fails with "already connected."
- **`MtpDisconnectReason` is load-bearing for logs/UI**: pass `User` only for the settings-toggle / explicit-disconnect path. Hotplug loss and I/O drops are `Removed`. Misclassifying makes unstable-USB sessions read like the user keeps pulling the cable.
