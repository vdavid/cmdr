# MTP connection

The MTP session layer: opens devices, owns the per-device tokio task, and exposes typed read/write ops. Parent:
[`../CLAUDE.md`](../CLAUDE.md).

## File map

- **`mod.rs`**: `MtpConnectionManager` singleton (`LazyLock`), `DeviceEntry` map, connect/disconnect, IPC DTOs.
- **`cache.rs`**: `PathHandleCache` (bidirectional path ↔ `ObjectHandle`), `ListingCache` (5 s TTL), `EventDebouncer`
  (per-device 500 ms).
- **`errors.rs`**: `MtpConnectionError` + `map_mtp_error()`. **`directory_ops.rs`**: `list_directory()` (foreground),
  `list_directory_for_scan()` (per-unit scan), `resolve_path_to_handle()` (cache-only). **`bulk_ops.rs`**:
  `scan_for_copy()`. **`scheduler.rs`**: `DevicePriorityGate` (foreground vs background access).
- **`event_loop.rs`**: per-device task polling `device.next_event()`; refreshes the live pane and feeds the per-volume
  index (see Must-knows).
- **`handle_resolver.rs`**: `resolve_handle_to_path()` (parent-chain walk, the pathless-PTP-event fix) and
  `resolve_object_for_index()` (adds metadata for an index upsert).
- **`file_ops.rs`**: download/upload/stream ops (emit `mtp-transfer-progress`). **`mutation_ops.rs`**: `delete()`
  (recursive), `create_folder()`, `rename()`, `move_object()` — no copy+delete fallback.

## Must-knows

- **Device lock**: `Arc<tokio::sync::Mutex<MtpDevice>>` held across `.await` for one USB I/O call; ops serialize per
  device with a 30 s timeout (`MTP_TIMEOUT_SECS`). Event polling sidesteps it by cloning `MtpDevice` (cheap `Arc`).
- **Foreground-priority scheduler (`scheduler.rs`)**: ❌ Every foreground device op (nav, copy, delete, rename, move,
  upload, visible-pane resolve) MUST take `foreground_guard(device_id)` for its lifetime, or background users won't yield
  (livelock returns). Two background users consult the gate (`background_yield_point` / `foreground_pending`) between
  work units: the index scan (`list_directory_for_scan`, never `list_directory*`) and a running MTP transfer (its
  per-chunk checkpoint releases + reopens the PTP session — see `write_operations/transfer/`). ❌ Gate the live index
  feed BEFORE any device resolve (`feed_index_added_or_changed` → `indexing::buffer_mtp_handle_if_scanning` first).
  Deadlock-freedom + sizing: [DETAILS.md](DETAILS.md) § "Foreground-priority device scheduler".
- **`resolve_path_to_handle()` is cache-only**: returns `ObjectNotFound` if the path hasn't appeared in a prior
  `list_directory()` (no on-demand walk), so list ancestors first; whole-tree ops that skip list-first fail here, not at
  the USB call. (Its inverse `resolve_handle_to_path()` *does* walk on demand — for pathless PTP events.)
- **`PathHandleCache` is bidirectional; insert ONLY via `PathHandleCache::insert`**, never `path_to_handle.insert(...)`:
  a forward-only insert silently desyncs the reverse map (`handle_to_path`) the resolver short-circuits on.
- **`ListingCache` TTL is per-entry and NOT invalidated by mutations**: a reader within the 5 s window still sees the
  stale listing after `create_folder` / `rename` / `delete`. Invalidate explicitly for read-after-write.
- **Disconnect from the event loop must clear the device registry**: on `next_event()` → `Error::Disconnected`,
  `event_loop.rs` calls `handle_device_disconnected(...)`. Skipping it leaves a dead `devices` entry and the next
  `connect()` fails as "already connected". It ALSO calls `indexing::on_mtp_device_disconnected` to flip every indexed
  storage Stale (freshness D4) — don't drop it, or a Fresh index lies post-unplug.
- **The event loop feeds the per-volume index, not just the live pane.** `ObjectAdded`/`ObjectInfoChanged` →
  `feed_index_added_or_changed` (upsert STORING the handle in `inode`); `ObjectRemoved` → `feed_index_removed` (resolve by
  the STORED handle via `find_entry_by_inode`). Buffering: `indexing/mtp_watch.rs`; details in DETAILS.
- **`MtpDisconnectReason` is load-bearing for logs/UI**: pass `User` only for the settings-toggle / explicit-disconnect
  path; hotplug loss and I/O drops are `Removed`. Misclassifying makes unstable USB look like repeated unplugs.
- **Failed PTP uploads must delete the partial object** (`UploadError { partial: Some(handle), .. }`; mtp-rs doesn't
  auto-delete). Both `file_ops.rs` call sites best-effort `storage.delete(handle)` (cancel too) before surfacing the
  error. See [DETAILS.md](DETAILS.md) § "Upload partial cleanup".
- **Stale cached parent handle on upload self-heals, then signals a one-shot retry.** A re-keyed handle makes
  `SendObjectInfo` fail `InvalidParentObject` / `InvalidObjectHandle`; `upload_from_stream` refreshes and returns
  `StaleParentHandle`, `stream_pipe_file` retries. ❌ DROP the device lock before `refresh_dir_handle` (it re-lists; the
  per-device `Mutex` isn't reentrant → deadlock), and never map this to a hard not-found.
  [DETAILS.md](DETAILS.md) § "Stale parent handle on upload".
- **Cancel propagation** (parent `../CLAUDE.md`): cancel-aware entry points are `delete()`, `list_objects_with_cancel`,
  and `list_directory_for_scan` (all threaded to `mtp-rs`).

Conventions, upload mechanics, event resolution, index feeding, and the scheduler: [DETAILS.md](DETAILS.md). Read it
before any non-trivial work here.
