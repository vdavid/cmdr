# MTP module — details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the must-knows; this is the depth.

## Virtual MTP device (dev + E2E activation)

The `virtual-mtp` feature compiles in `virtual_device.rs`; whether the device actually registers at startup is decided at
runtime by `activate_from_env_if_requested()` (called from `lib.rs`). It registers when **either** `CMDR_E2E_MODE=1` (an
E2E run) **or** `CMDR_VIRTUAL_MTP` is set (the dev opt-in), and never when `CMDR_E2E_SKIP_VIRTUAL_MTP_SETUP` is set (the
override non-MTP E2E shards use to avoid racing the shared backing dir). So a `virtual-mtp`-compiled binary launched with
none of those env vars stays inert and matches a plain build; the dev opt-in is purely additive to the E2E path.
`CMDR_VIRTUAL_MTP=<dir>` backs it with a custom dir. The fixture tree mirrors `test/e2e-shared/mtp-fixtures.ts`. The
gating logic (`decide_startup_root`) is pure and unit-tested in `virtual_device.rs::tests`.

## Architecture / data flow

```
USB plug-in
  → nusb hotplug event (watcher.rs)
  → 500 ms delay
  → check MTP_ENABLED gate, skip if disabled
  → list_mtp_devices() (discovery.rs)
  → auto_connect_device() (watcher.rs)
    → MtpConnectionManager::connect()
    → open_device() via MtpDeviceBuilder
    → probe_write_capability() per storage
    → register MtpVolume in global VolumeManager
    → start_event_loop() per device
    → emit mtp-device-connected (JSON includes `deviceName` from `connected_info.device.product`, "" if unknown)
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

`MtpDisconnectReason` distinguishes explicit toggle-off from hotplug-loss in logs and UI. Re-enabling MTP triggers
auto-connect, which re-suppresses ptpcamerad if devices are found.

## Cancel propagation wiring

Long MTP operations bail at the next per-USB-roundtrip boundary when the caller's write-op intent flips to
`Stopped` / `RollingBack`.

- `WriteOperationState.backend_cancel` (`Arc<AtomicBool>`) is created per write op alongside `intent`.
  `cancel_write_operation` and `cancel_all_write_operations` flip both together so any cancel path stops the wire
  activity.
- `MtpVolume::list_directory_with_cancel` and `MtpVolume::delete_with_cancel` wrap the flag as a fresh
  `mtp_rs::CancelToken` via `CancelToken::from_arc(Arc::clone(...))`, sharing the inner atomic (no second polling task).
- `MtpConnectionManager::list_directory_with_cancel`, `list_directory_with_progress_and_cancel`, and
  `delete_object_with_cancel` thread the token to `storage.list_objects_with_cancel` / `storage.delete_with_cancel` in
  `mtp-rs`. The token is also checked between iterations of the recursive child-delete loop.

The actual stop point is per-handle in `ObjectListing::next` (one `GetObjectInfo` USB roundtrip each, ~17 ms on real
Android), well under the "Cancelling…" indicator's settling window.

### Why not PTP `CancelTransaction (0x4001)` for list/delete?

PTP defines `CancelTransaction` (interrupt-OUT control request, SIC class-cancel, `bRequest=0x64`). mtp-rs implements it
via `Transport::cancel_transfer` for streaming downloads (`FileDownload::cancel`), where there's a multi-MB bulk-IN
transfer to drain. For `list_objects` and `delete_object`, each PTP transaction completes in milliseconds.
Mid-transaction cancel would be high-complexity (drain bulk endpoints, recover session state) for sub-roundtrip benefit.
Checking the token between roundtrips instead: bails within ≈one roundtrip's latency (the actual wedge point), leaves
bulk endpoints clean (no drain race), and leaves the session intact for the next op. Streaming downloads keep the SIC
class-cancel path (see "Transfer cancellation" in `mtp-rs/AGENTS.md`).

### Hardware caveats

Some Android devices (Pixel 6/7-era firmware observed) may still leave the session degraded after a flurry of ops even
when cancel is clean on our side. This is hardware-side and unfixable in software; the settled-state gate (see
`file_system/write_operations/DETAILS.md` § "Settle contract") ensures the user doesn't issue the next op until our side
is fully quiet, which avoids provoking the bug in practice.

## Dependencies

- `mtp_rs`: MTP session, object listing, file transfer.
- `nusb`: USB hotplug events.
- `futures_util`: `StreamExt` for the hotplug stream.
- `crate::file_system`: `VolumeManager`, `MtpVolume`, `FileEntry`, `compute_diff`.
