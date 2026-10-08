# MTP connection

The MTP session layer: opens devices, owns the per-device tokio task, exposes typed read/write ops. Parent:
`crates/cmdr-mtp/CLAUDE.md`.

## File map

- Session: `mod.rs` (the manager, connect/disconnect, `map_device_error`), `errors.rs`, `scheduler.rs`
  (`DevicePriorityGate`), `cache.rs` (path ↔ handle + 5 s-TTL listing caches, `EventDebouncer`), `events.rs`
  (`MtpDeviceEvents`), `volume_registrar.rs`, `session_reset.rs`.
- Ops: `directory_ops.rs` (listings), `resolve.rs` (path → handle), `bulk_ops.rs` (copy pre-scan), `handle_resolver.rs`
  (handle → path), `event_loop.rs` (per-device poll, feeding the pane AND the index), `file_ops.rs` (windowed and ranged
  reads, uploads), `mutation_ops.rs` (delete, create, rename, move).

## Must-knows

- **❌ Never wrap an mtp-rs call in `tokio::time::timeout`, and never abort a task holding one.** The deadline drops the
  future mid-transaction and wedges the device until replug; a `CancelToken` bails at a safe boundary. Enforced by
  `pnpm check mtp-dropping-timeout`.
- **Ops serialize per device** on an `Arc<Mutex<MtpDevice>>` held across `.await`. `DEVICE_LOCK_WAIT_SECS` (300 s) caps
  the WAIT, never the call; event polling clones `MtpDevice` to sidestep it.
- **Every foreground op MUST hold `foreground_guard(device_id)`** (nav, mutate, upload, visible-pane resolve), or
  background users won't yield. ❌ A READ takes none (a copy would yield to itself forever); gate the live index feed
  BEFORE device resolve; background users list via `list_directory_for_scan`, ❌ never `list_directory*`.
- **❌ A `SessionReset` (mtp-rs `DeviceReset`) is NOT a disconnect**: `session_reset.rs` drops the entry, flips the
  index Stale, KEEPS the sidebar volume, and reopens with backoff. ❌ Never route it to `handle_device_disconnected`,
  tighten the backoff, or add a USB transport reset (`pnpm check mtp-no-transport-reset`). A REAL `Error::Disconnected`
  DOES take that path, else the next `connect()` fails as "already connected".
- **The caches lie in specific ways.** `resolve_path_to_handle()` heals a miss by re-listing parents, else answers a
  typed `ObjectNotFound`; ❌ never call it holding the device lock. `PathHandleCache` is bidirectional: write via
  `insert` / `remove_path`, ❌ never `path_to_handle` (devices REUSE handles). `ListingCache`'s 5 s TTL survives
  mutations; invalidate for read-after-write.
- **A batch copy scan shares its `ScanBoundary` with `bulk_ops.rs`**, consulting the stop per entry and BEFORE each
  child listing. Plain `scan_for_copy` uses a silent boundary. Selected-source progress: `../../DETAILS.md`.
- **A suppressed event must win `EventDebouncer::claim_trailing` before re-emitting**: one per burst, never one per
  event, else a bulk copy livelocks the pane.
- **A failed PTP upload must delete the partial object** (mtp-rs doesn't); a stale cached parent handle self-heals into
  a one-shot retry, and ❌ drop the device lock before `refresh_dir_handle`, which re-lists into a non-reentrant
  `Mutex`.
- **A ranged read takes `read_range_direct`, ❌ NOT `open_read_session`**, and ❌ not for COPY, which needs `total_size`
  for progress and the yield checkpoint.
- **The event loop reports through the host seams** at the `mtp://` URL; the host keys panes by
  `MtpVolume::listing_path`. ❌ Never diff here, ❌ never one call per entry, ❌ never `tokio::spawn` (use
  `host().runtime()`), and clear the `ListingCache` before a `FullRefresh`. Which change goes where: `DETAILS.md`.
- **❌ Nothing here names a `tauri` type.** Lifecycle leaves as a typed `MtpDeviceEvent` through the manager's
  `MtpDeviceEvents` sink; the app's `mtp/events.rs` maps it. A manager with no window gets `no_device_events()`, so ❌
  no `Option` to unwrap. Whether the device is POLLED is the separate `DeviceWatch` argument.
- **`MtpDisconnectReason::User` is only the settings toggle or an explicit disconnect**; hotplug loss and I/O drops are
  `Removed`, else unstable USB reads as repeated unplugs.
- **Test-gated behavior takes `any(test, feature = "testing")`, ❌ never `cfg(test)`**, which is off in a consumer's
  test build.

Locks, caches, recovery, and the event-to-index wiring: `DETAILS.md`. Read it before any non-trivial work here.
