# MTP connection

The MTP session layer: opens devices, owns the per-device tokio task, exposes typed read/write ops.
Parent: `../CLAUDE.md`.

## File map

- **`mod.rs`** `MtpConnectionManager`, `DeviceEntry` map, connect/disconnect; **`errors.rs`** `map_mtp_error()`;
  **`scheduler.rs`** `DevicePriorityGate`; **`cache.rs`** `PathHandleCache` (path ↔ handle), `ListingCache` (5 s TTL),
  `EventDebouncer`.
- **`directory_ops.rs`** `list_directory()`, `list_directory_for_scan()`, `resolve_path_to_handle()`,
  `handle_device_disconnected()`; **`bulk_ops.rs`** `scan_for_copy()`; **`handle_resolver.rs`**
  `resolve_handle_to_path()`, `resolve_object_for_index()`; **`event_loop.rs`** per-device `next_event()` poll,
  refreshing the live pane and feeding the index.
- **`file_ops.rs`** transfers (`open_read_session` + `read_next_window`, `read_range_direct`, `upload_from_stream`);
  **`mutation_ops.rs`** recursive `delete()`, `create_folder()`, `rename()`, `move_object()`, no copy+delete fallback;
  **`session_reset.rs`** `handle_device_session_reset()`.

## Must-knows

- **❌ Never wrap an mtp-rs call in `tokio::time::timeout`, and never abort a task holding one.** The deadline DROPS
  the future mid-transaction, and a device left mid-data-phase wedges until replug; mtp-rs bounds every USB transfer
  itself and fails CLEANLY. A `CancelToken` bails at a safe boundary instead (`delete()`,
  `list_objects_with_cancel`, `list_directory_for_scan`). Enforced by `pnpm check mtp-dropping-timeout`.
  `DETAILS.md` § "No dropping timeouts".
- **Device lock**: `Arc<Mutex<MtpDevice>>` held across `.await` for one USB call; ops serialize per device.
  `DEVICE_LOCK_WAIT_SECS` (300 s) caps only the WAIT for it, never a device call — ops legitimately run for minutes.
  Event polling clones `MtpDevice` to sidestep it.
- **Foreground-priority scheduler (`scheduler.rs`)**: ❌ Every foreground op (nav, delete, rename, move, upload,
  visible-pane resolve) MUST hold `foreground_guard(device_id)`, or background users won't yield. ❌ A READ takes NO
  guard (a copy would yield to itself forever). Background users (index scan via `list_directory_for_scan`, never
  `list_directory*`; a running transfer) poll the gate between units. ❌ Gate the live index feed BEFORE device
  resolve. `DETAILS.md` § "Foreground-priority device scheduler".
- **`resolve_path_to_handle()` is cache-only**: fails unless a prior `list_directory()` saw the path — list ancestors first.
- **`PathHandleCache` is bidirectional; write through `insert` / `remove_path`**, never `path_to_handle`: a one-sided
  write desyncs the reverse map, and devices REUSE handles, so a stale entry resolves a NEW object to a dead path.
- **`ListingCache` TTL is per-entry, NOT invalidated by mutations**: a reader sees the pre-mutation listing for 5 s.
  Invalidate explicitly for read-after-write.
- **Disconnect from the event loop must clear the device registry**: on `Error::Disconnected`, `event_loop.rs` calls
  `handle_device_disconnected(...)`, else the next `connect()` fails as "already connected". It ALSO flips indexed
  storages Stale (`indexing::on_mtp_watch_continuity_lost`; a Fresh index would lie post-unplug).
- **❌ A `SessionReset` (mtp-rs `DeviceReset`) is NOT a disconnect** — only the PTP session died. `session_reset.rs`
  drops the entry, flips the index Stale, KEEPS the volume in the sidebar, then reopens with backoff. ❌ Never route it
  to `handle_device_disconnected`; ❌ never tighten the backoff (hammering re-wedges it); ❌ never add a USB transport
  reset — on Android that's a kill switch costing the user a replug, and the reopen self-heals without it
  (`pnpm check mtp-no-transport-reset`). Failing ops report the RETRYABLE `DeviceSessionReset`. `DETAILS.md` §§ "Session reset is not a disconnect", "No transport reset in
  recovery".
- **The event loop feeds the per-volume index, not just the live pane**: `ObjectAdded`/`ObjectInfoChanged` →
  `feed_index_added_or_changed` (upsert STORING the handle in `inode`); `ObjectRemoved` → `feed_index_removed`.
- **`MtpDisconnectReason`** drives logs/UI: `User` only for the settings toggle / explicit disconnect;
  hotplug loss and I/O drops are `Removed`, else unstable USB reads as repeated unplugs.
- **Failed PTP uploads must delete the partial object** (`UploadError.partial`; mtp-rs doesn't);
  `upload_from_stream` does, cancel too. `DETAILS.md` § "Upload partial cleanup".
- **A stale cached parent handle on upload self-heals into a one-shot retry** (`StaleParentHandle`;
  `stream_pipe_file` retries). ❌ DROP the device lock before `refresh_dir_handle` (it re-lists; the `Mutex` isn't
  reentrant → deadlock). `DETAILS.md` § "Stale parent handle on upload".
- **A ranged read takes `read_range_direct`, NOT `open_read_session`**: one `GetPartialObject64`, storage handle from
  `DeviceEntry`'s cache. ❌ Not for COPY — that needs `total_size` for progress and the yield checkpoint.

Depth: `DETAILS.md`.
