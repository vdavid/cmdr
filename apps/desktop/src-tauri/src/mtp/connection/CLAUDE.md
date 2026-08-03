# MTP connection

The MTP session layer: opens devices, owns the per-device tokio task, exposes typed read/write ops.
Parent: `../CLAUDE.md`.

## File map

- **`mod.rs`** manager singleton, device map, connect/disconnect; **`errors.rs`** error mapping; **`scheduler.rs`**
  `DevicePriorityGate`; **`cache.rs`** path ↔ handle and listing (5 s TTL) caches, `EventDebouncer`;
  **`volume_registrar.rs`** the attach/detach hook.
- **`directory_ops.rs`** foreground and scan listings, path → handle; **`bulk_ops.rs`** copy pre-scan;
  **`handle_resolver.rs`** handle → path; **`event_loop.rs`** per-device event poll, feeding the live pane and the index.
- **`file_ops.rs`** transfers (windowed and ranged reads, uploads); **`mutation_ops.rs`** recursive delete, create,
  rename, move, no copy+delete fallback; **`session_reset.rs`** PTP session-reset recovery.

## Must-knows

- **❌ Never wrap an mtp-rs call in `tokio::time::timeout`, and never abort a task holding one.** The deadline DROPS
  the future mid-transaction and wedges the device until replug; mtp-rs bounds every transfer itself and fails
  CLEANLY. Use a `CancelToken`, which bails at a safe boundary. Enforced by
  `pnpm check mtp-dropping-timeout`.
  `DETAILS.md` § "No dropping timeouts".
- **Device lock**: `Arc<Mutex<MtpDevice>>` held across `.await` for one USB call; ops serialize per device.
  `DEVICE_LOCK_WAIT_SECS` (300 s) caps the WAIT only, never the call (ops run for minutes). Event polling clones
  `MtpDevice` to sidestep it.
- **Foreground-priority scheduler (`scheduler.rs`)**: ❌ Every foreground op (nav, mutate, upload, visible-pane
  resolve) MUST hold `foreground_guard(device_id)`, or background users won't yield. ❌ A READ takes NO
  guard (a copy would yield to itself forever). ❌ Gate the live index feed BEFORE device resolve. Background users
  (the scan via `list_directory_for_scan`, never `list_directory*`; a transfer) poll the gate between units.
  `DETAILS.md` § "Foreground-priority device scheduler".
- **A suppressed event must win `EventDebouncer::claim_trailing` before scheduling a trailing re-emit**: one per
  burst, never one per event, else a bulk copy livelocks the pane. `DETAILS.md` § "Trailing emits must be claimed".
- **`resolve_path_to_handle()` is cache-only**: fails unless a prior `list_directory()` saw the path — list ancestors first.
- **`PathHandleCache` is bidirectional; write through `insert` / `remove_path`**, never `path_to_handle`: a one-sided
  write desyncs the reverse map, and devices REUSE handles, so a stale entry resolves a NEW object to a dead path.
- **`ListingCache` TTL is per-entry, NOT invalidated by mutations**: a reader sees the pre-mutation listing for 5 s.
  Invalidate explicitly for read-after-write.
- **Disconnect from the event loop must clear the device registry**: on `Error::Disconnected`, `event_loop.rs` calls
  `handle_device_disconnected(...)`, else the next `connect()` fails as "already connected". It ALSO flips indexed
  storages Stale (a Fresh index would lie post-unplug).
- **❌ A `SessionReset` (mtp-rs `DeviceReset`) is NOT a disconnect**: only the PTP session died, so `session_reset.rs`
  drops the entry, flips the index Stale, KEEPS the sidebar volume, and reopens with backoff. ❌ Never route it to
  `handle_device_disconnected`; ❌ never tighten the backoff (hammering re-wedges it); ❌ never add a USB transport reset
  (an Android kill switch costing a replug; the reopen self-heals, `pnpm check mtp-no-transport-reset`). Failing ops
  report the RETRYABLE `DeviceSessionReset`. `DETAILS.md` § "Session reset is not a disconnect", then § "No transport
  reset in recovery".
- **The event loop feeds the per-volume index, not just the live pane**: adds/changes upsert STORING the handle in
  `inode`; removals resolve through it.
- **`MtpDisconnectReason`** drives logs/UI: `User` only for the settings toggle or an explicit disconnect; hotplug
  loss and I/O drops are `Removed`, else unstable USB reads as repeated unplugs.
- **Failed PTP uploads must delete the partial object** (`UploadError.partial`; mtp-rs doesn't);
  `upload_from_stream` does, cancel too. `DETAILS.md` § "Upload partial cleanup".
- **A stale cached parent handle on upload self-heals into a one-shot retry** (`StaleParentHandle`). ❌ DROP the
  device lock before `refresh_dir_handle` (it re-lists; the `Mutex` isn't reentrant → deadlock). `DETAILS.md`
  § "Stale parent handle on upload".
- **A ranged read takes `read_range_direct`, NOT `open_read_session`**: one `GetPartialObject64`, storage handle from
  `DeviceEntry`'s cache. ❌ Not for COPY — that needs `total_size` for progress and the yield checkpoint.

Depth: `DETAILS.md`.
