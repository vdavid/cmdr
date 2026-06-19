# MTP connection

The MTP session layer: opens devices, owns the per-device tokio task, and exposes typed read/write operations to higher
layers. Parent: [`../CLAUDE.md`](../CLAUDE.md).

## File map

- **`mod.rs`**: `MtpConnectionManager` singleton (`LazyLock`), `DeviceEntry` map, `connect()` /
  `disconnect(MtpDisconnectReason)`, IPC DTOs (`MtpTransferProgress`, `MtpOperationResult`, `MtpObjectInfo`).
- **`cache.rs`**: `PathHandleCache` (path ↔ `ObjectHandle`, both directions), `ListingCache` (5 s TTL,
  `LISTING_CACHE_TTL_SECS`), `EventDebouncer` (per-device 500 ms, `EVENT_DEBOUNCE_MS`).
- **`errors.rs`**: `MtpConnectionError` enum + `map_mtp_error()` from `mtp_rs::Error`.
- **`event_loop.rs`**: per-device background task: polls `device.next_event()` (clones `MtpDevice` so the interrupt-endpoint
  poll doesn't hold the bulk mutex), resolves pathful events to a targeted dir refresh, emits `directory-diff`.
- **`handle_resolver.rs`**: `resolve_handle_to_path()` walks an `ObjectHandle`'s parent chain to a full path (the
  pathless-PTP-event fix; [DETAILS.md](DETAILS.md) § "Pathful change events").
- **`directory_ops.rs`**: `list_directory()`, `resolve_path_to_handle()` (cache-only, see must-knows).
- **`file_ops.rs`**: `download_file()`, `upload_file()`, `upload_from_stream()`, `open_download_stream()` (emit
  `mtp-transfer-progress`; `open_download_stream` returns a `FileDownload` consumed by `MtpReadStream` in `volume/mtp.rs`).
- **`mutation_ops.rs`**: `delete()` (recursive, children-first), `create_folder()`, `rename()`, `move_object()`: no
  copy+delete fallback.
- **`bulk_ops.rs`**: `scan_for_copy()`, async recursion via `Box::pin`.

## Must-knows

- **Device lock**: `Arc<tokio::sync::Mutex<MtpDevice>>` held across `.await` for one USB I/O call; operations serialize
  per device with a 30 s timeout (`MTP_TIMEOUT_SECS`). Event polling sidesteps the lock by cloning `MtpDevice` (cheap `Arc`).
- **`resolve_path_to_handle()` is cache-only**: returns `ObjectNotFound` if the path hasn't appeared in a prior
  `list_directory()`. There's no on-demand path walk, so the caller (usually `MtpVolume`) must list ancestors first.
  Whole-tree ops that bypass list-first fail here, not at the USB call. (Its inverse `resolve_handle_to_path()` *does*
  walk on demand — for pathless PTP events, not browsing.)
- **`PathHandleCache` is bidirectional; insert ONLY via `PathHandleCache::insert`**, never `path_to_handle.insert(...)`.
  A forward-only insert silently desyncs the reverse map (`handle_to_path`) the change-event resolver short-circuits on.
- **`ListingCache` TTL is per-entry and NOT invalidated by mutations**: a concurrent reader within the 5 s window still
  sees the stale listing after `create_folder` / `rename` / `delete`. Invalidate explicitly for read-after-write consistency.
- **Disconnect from the event loop must clear the device registry**: on `next_event()` → `Error::Disconnected`,
  `event_loop.rs` calls `handle_device_disconnected(...)`. Skipping it leaves a dead `devices` entry and the next
  `connect()` fails with "already connected".
- **`MtpDisconnectReason` is load-bearing for logs/UI**: pass `User` only for the settings-toggle / explicit-disconnect
  path; hotplug loss and I/O drops are `Removed`. Misclassifying makes unstable-USB sessions read like repeated unplugs.
- **Failed PTP uploads must delete the partial object.** A failed/cancelled data phase leaves a created-but-incomplete
  object (`UploadError { partial: Some(handle), .. }`, which mtp-rs deliberately doesn't auto-delete). Both `file_ops.rs`
  call sites best-effort `storage.delete(handle)` before surfacing the mapped error, including on cancel. A failed cleanup
  logs under `target: "mtp_upload"` and never masks the original error. Pinned by
  `upload_failure_deletes_partial_object_on_device` / `upload_cancel_deletes_partial_and_surfaces_cancelled`.
- **Stale cached parent handle on upload self-heals, then signals a one-shot retry.** A re-keyed handle (Android
  MediaProvider rescan between listing and upload) makes `SendObjectInfo` fail with `InvalidParentObject` /
  `InvalidObjectHandle`; `upload_from_stream` refreshes the handle and returns `StaleParentHandle`, and `stream_pipe_file`
  owns the retry (re-opening the consumed source). Two guardrails: DROP the device lock before `refresh_dir_handle` (it
  re-lists, and the per-device `tokio::sync::Mutex` isn't reentrant → deadlock), and never map this to a hard not-found
  (the field bug: an intact source shown as "Path not found"). Full mechanism in [DETAILS.md](DETAILS.md) § "Stale parent handle on upload".
- **Cancel propagation**: see parent `../CLAUDE.md` § "Cancel propagation". Cancel-aware entry points here are `delete()`
  and the `list_objects_with_cancel` path threaded to `mtp-rs`.

## Conventions

- **Event debounce**: `EventDebouncer` collapses MTP event bursts to one frontend emit per 500 ms; cleared on disconnect.
- **Async recursion** (`bulk_ops.rs`, recursive `delete()`): `Box::pin(async move { ... })` breaks the infinite future.
- **Event-loop shutdown**: biased `tokio::select!` so the shutdown signal always wins over the event poll.

Full details (upload two-phase mechanics): [DETAILS.md](DETAILS.md).
