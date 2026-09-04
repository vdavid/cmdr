# MTP connection

The MTP session layer: opens devices, owns the per-device tokio task, exposes typed read/write ops. Parent:
`../CLAUDE.md`.

## File map

- Session: `mod.rs` (the manager, connect/disconnect, `map_device_error`), `errors.rs`, `scheduler.rs`
  (`DevicePriorityGate`),
  `cache.rs` (path ↔ handle + 5 s-TTL listing caches, `EventDebouncer`), `events.rs` (`MtpDeviceEvents`),
  `volume_registrar.rs`, `session_reset.rs`.
- Ops: `directory_ops.rs` (listings, path → handle), `bulk_ops.rs` (copy pre-scan), `handle_resolver.rs` (handle →
  path), `event_loop.rs` (per-device poll, feeding the pane AND the per-volume index), `file_ops.rs` (windowed and
  ranged reads, uploads), `mutation_ops.rs` (delete, create, rename, move).

## Must-knows

- **❌ Never wrap an mtp-rs call in `tokio::time::timeout`, and never abort a task holding one.** The deadline drops the
  future mid-transaction and wedges the device until replug; a `CancelToken` bails at a safe boundary instead. Enforced
  by `pnpm check mtp-dropping-timeout`.
- **Ops serialize per device** on an `Arc<Mutex<MtpDevice>>` held across `.await`. `DEVICE_LOCK_WAIT_SECS` (300 s) caps
  the WAIT only, never the call; event polling clones `MtpDevice` to sidestep it.
- **Every foreground op MUST hold `foreground_guard(device_id)`** (nav, mutate, upload, visible-pane resolve), or
  background users won't yield. ❌ A READ takes none (a copy would yield to itself forever); ❌ gate the live index feed
  BEFORE device resolve; background users list via `list_directory_for_scan`, ❌ never `list_directory*`.
- **❌ A `SessionReset` (mtp-rs `DeviceReset`) is NOT a disconnect**: `session_reset.rs` drops the entry, flips the index
  Stale, KEEPS the sidebar volume, and reopens with backoff. ❌ Never route it to `handle_device_disconnected`, ❌ never
  tighten the backoff, ❌ never add a USB transport reset (`pnpm check mtp-no-transport-reset`). A REAL
  `Error::Disconnected` DOES call `handle_device_disconnected`, else the next `connect()` fails as "already connected".
- **The caches lie in specific ways.** `resolve_path_to_handle()` is cache-only, so list ancestors first.
  `PathHandleCache` is bidirectional: write via `insert` / `remove_path`, ❌ never `path_to_handle`, since devices REUSE
  handles and a desynced reverse map resolves a new object to a dead path. `ListingCache`'s 5 s TTL survives mutations;
  invalidate explicitly for read-after-write.
- **A copy scan takes `scan_for_copy_with_stop`** (`bulk_ops.rs`), consulting the `ScanStop` per entry and BEFORE each
  child listing: one listing is the round trip (~17 s for 1k entries), so that is as fine as this layer can be. Plain
  `scan_for_copy` passes `ScanStop::none()` and cannot be stopped.
- **A suppressed event must win `EventDebouncer::claim_trailing` before a trailing re-emit**: one per burst, never one
  per event, else a bulk copy livelocks the pane.
- **A failed PTP upload must delete the partial object** (mtp-rs doesn't), and a stale cached parent handle self-heals
  into a one-shot retry; ❌ drop the device lock before `refresh_dir_handle`, which re-lists into a non-reentrant
  `Mutex`.
- **A ranged read takes `read_range_direct`, ❌ NOT `open_read_session`**, and ❌ not for COPY, which needs `total_size`
  for progress and the yield checkpoint.
- **❌ Nothing here names a `tauri` type.** A device's lifecycle goes out as a typed `MtpDeviceEvent` through the
  `MtpDeviceEvents` sink the caller passes in; `crate::mtp::events` holds the payload structs, their derives, and the
  one match that maps them. A caller with no window passes `no_device_events()`, so ❌ there is no `Option` to unwrap
  and no "was anyone listening?" to branch on. Whether the device is POLLED is the separate `DeviceWatch` argument: a
  virtual fixture queues a `StorageInfoChanged` per file that lands in its backing dir, so a test that watches drops
  the cached storage handle under its own writes.
- **`MtpDisconnectReason::User` is only the settings toggle or an explicit disconnect**; hotplug loss and I/O drops are
  `Removed`, else unstable USB reads as repeated unplugs.

Locks, caches, recovery, and the event-to-index wiring: `DETAILS.md`. Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
