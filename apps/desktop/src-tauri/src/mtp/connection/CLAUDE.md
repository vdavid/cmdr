# MTP connection

The MTP session layer: opens devices, owns the per-device tokio task, exposes typed read/write ops.
Parent: [`../CLAUDE.md`](../CLAUDE.md).

## File map

- **`mod.rs`**: `MtpConnectionManager` singleton, `DeviceEntry` map, connect/disconnect. **`errors.rs`**:
  `MtpConnectionError` + `map_mtp_error()`. **`scheduler.rs`**: `DevicePriorityGate`.
- **`cache.rs`**: `PathHandleCache` (bidirectional path ↔ handle), `ListingCache` (5 s TTL), `EventDebouncer`.
- **`directory_ops.rs`**: `list_directory()`, `list_directory_for_scan()`, `resolve_path_to_handle()`,
  `handle_device_disconnected()`. **`bulk_ops.rs`**: `scan_for_copy()`.
- **`event_loop.rs`**: per-device `device.next_event()` poll; refreshes the live pane, feeds the index.
  **`handle_resolver.rs`**: `resolve_handle_to_path()`, `resolve_object_for_index()`.
- **`file_ops.rs`**: transfers (`open_read_session` + `read_next_window`, `read_range_direct`, `upload_from_stream`).
  **`mutation_ops.rs`**: recursive `delete()`, `create_folder()`, `rename()`, `move_object()` — no copy+delete fallback.
  **`session_reset.rs`**: `handle_device_session_reset()` + reopen backoff.

## Must-knows

- **Device lock**: `Arc<Mutex<MtpDevice>>` held across `.await` for one USB call; ops serialize per device, 30 s
  timeout (`MTP_TIMEOUT_SECS`). Event polling clones `MtpDevice` to sidestep it.
- **Foreground-priority scheduler (`scheduler.rs`)**: ❌ Every foreground op (nav, delete, rename, move, upload,
  visible-pane resolve) MUST hold `foreground_guard(device_id)`, or background users won't yield. ❌ A READ takes NO
  guard — a copy would yield to itself forever. Two background users poll the gate between units: the index scan
  (`list_directory_for_scan`, never `list_directory*`) and a running transfer. ❌ Gate the live index feed BEFORE device
  resolve (`feed_index_added_or_changed` → `buffer_mtp_handle_if_scanning`). [DETAILS.md](DETAILS.md) §
  "Foreground-priority device scheduler".
- **`resolve_path_to_handle()` is cache-only**: `ObjectNotFound` unless a prior `list_directory()` saw the path, so
  list ancestors first; ops skipping that fail here, not at the USB call.
- **`PathHandleCache` is bidirectional; write through `insert` / `remove_path`**, never `path_to_handle` directly: a
  one-sided write desyncs the reverse map the resolver short-circuits on, and devices REUSE handles, so a stale reverse
  entry resolves a NEW object to a dead path.
- **`ListingCache` TTL is per-entry and NOT invalidated by mutations**: inside the 5 s window a reader still sees the
  pre-mutation listing. Invalidate explicitly for read-after-write.
- **Disconnect from the event loop must clear the device registry**: on `next_event()` → `Error::Disconnected`,
  `event_loop.rs` calls `handle_device_disconnected(...)`; skip it and the next `connect()` fails as "already
  connected". It ALSO flips indexed storages Stale (`indexing::on_mtp_watch_continuity_lost`, D4) — drop that and a
  Fresh index lies post-unplug.
- **❌ A `SessionReset` (mtp-rs `DeviceReset`) is NOT a disconnect** — only the PTP session died. `session_reset.rs`
  drops the entry (its handle caches are dead), flips the index Stale, KEEPS the volume in the sidebar (no `Removed`
  event), then reopens with spaced backoff. ❌ Never route it to `handle_device_disconnected`; ❌ never shorten or
  tighten the backoff (early attempts fail by design; hammering re-wedges it). Failing ops report the RETRYABLE
  `VolumeError::DeviceSessionReset`, never `io_serious`. [DETAILS.md](DETAILS.md) § "Session reset is not a
  disconnect".
- **The event loop feeds the per-volume index, not just the live pane.** `ObjectAdded`/`ObjectInfoChanged` →
  `feed_index_added_or_changed` (upsert STORING the handle in `inode`); `ObjectRemoved` → `feed_index_removed` (by that
  handle). Buffering: `indexing/mtp_watch.rs`.
- **`MtpDisconnectReason` is load-bearing for logs/UI**: `User` only for the settings toggle / explicit disconnect;
  hotplug loss and I/O drops are `Removed`. Misclassifying makes unstable USB read as repeated unplugs.
- **Failed PTP uploads must delete the partial object** (`UploadError.partial`; mtp-rs doesn't auto-delete).
  `upload_from_stream` best-effort deletes it, cancel too. [DETAILS.md](DETAILS.md) § "Upload partial cleanup".
- **Stale cached parent handle on upload self-heals, then signals a one-shot retry.** A re-keyed handle fails
  `SendObjectInfo`; `upload_from_stream` refreshes and returns `StaleParentHandle`, `stream_pipe_file` retries. ❌ DROP
  the device lock before `refresh_dir_handle` (it re-lists; the `Mutex` isn't reentrant → deadlock), never a hard
  not-found. [DETAILS.md](DETAILS.md) § "Stale parent handle on upload".
- **A ranged read takes `read_range_direct`, NOT `open_read_session`**: one `GetPartialObject64`, storage handle from
  `DeviceEntry`'s cache (dropped via `invalidate_storage_cache` on storage changes). ❌ Not for COPY — that needs
  `total_size` for progress and the yield checkpoint.
- **Cancel-aware entry points**: `delete()`, `list_objects_with_cancel`, `list_directory_for_scan` (threaded to
  `mtp-rs`; see `../CLAUDE.md`).

Depth (conventions, upload/event/index mechanics, the scheduler): [DETAILS.md](DETAILS.md).
