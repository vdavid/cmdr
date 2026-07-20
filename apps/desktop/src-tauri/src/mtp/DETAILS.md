# MTP module — details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the must-knows; this is the depth.

## Device and volume identity (`identity.rs`)

The device id and volume id are built and parsed in ONE place so the scheme can't drift. `device_id_for(serial, location_id)` derives the device id: `mtp-{serial}` when the device reports a non-empty serial, else `mtp-{location_id}`. The serial-based id is stable across a replug to ANY USB port, which is what lets the persisted per-volume index (`indexing`, keyed `index-{volume_id}.db`) re-match a reconnected phone instead of forcing a rescan; the topology `location_id` only survives a same-port replug, so it's the fallback when no serial is reported (limitation surfaced in the drive-indexing tooltip). The volume id is `{device_id}:{storage_id}`.

**Why parsing must split from the right.** Some devices report serials containing `:`, so the device-id half of a volume id can contain `:`. The storage id is always the trailing numeric component, so `split_volume_id` uses `rsplit_once(':')` and parses the tail as a `u32`; `device_id_of_volume` / `storage_id_of_volume` are the convenience accessors. A naive `split(':').nth(1)` would take the wrong segment and either mis-route or fail the parse. Everything that needs to decompose a volume id goes through these helpers (Rust: `event_loop`, `eject`, indexing path-mapping; TS: `FilePane` and `mtp-path-utils` use `lastIndexOf(':')` to mirror it). `is_mtp_volume_id` / `is_mtp_device_id` classify by the `mtp-` prefix + numeric tail.

**The device id is opaque past construction.** Because a serial id can't be numerically decoded back to a `location_id`, `connect()` resolves a device id to the USB location to open by MATCHING it against the live `list_mtp_devices()` enumeration (`resolve_device_location_id`), not by parsing it. So adding a serial never breaks device opening, and no code interprets the serial's contents.

## Virtual MTP device (dev + E2E activation)

The `virtual-mtp` feature compiles in `virtual_device.rs`; whether the device actually registers at startup is decided at
runtime by `activate_from_env_if_requested()` (called from `lib.rs`). It registers when **either** `CMDR_E2E_MODE=1` (an
E2E run) **or** `CMDR_VIRTUAL_MTP` is set (the dev opt-in), and never when `CMDR_E2E_SKIP_VIRTUAL_MTP_SETUP` is set (the
override non-MTP E2E shards use to avoid racing the shared backing dir). So a `virtual-mtp`-compiled binary launched with
none of those env vars stays inert and matches a plain build; the dev opt-in is purely additive to the E2E path.
`CMDR_VIRTUAL_MTP=<dir>` backs it with a custom dir. The fixture tree mirrors `test/e2e-shared/mtp-fixtures.ts`. The
gating logic (`decide_startup_root`) is pure and unit-tested in `virtual_device.rs::tests`.

**Build `VirtualDeviceConfig` with `..Default::default()`** and state only the fields this fixture actually cares
about. mtp-rs 0.26 added `Default` precisely so a new field doesn't break us: every prior field addition was a compile
error here (0.24's `supports_partial_object_64` broke CI). Don't re-expand the literal to name every field. The
defaults model a modern Android device (`supports_rename` and `supports_partial_object_64` both true), which matches
the Pixel 9 this fixture stands in for; set `supports_partial_object_64: false` explicitly if you ever want to exercise
mtp-rs's 32-bit `GetPartialObject` fallback (the PTP-camera path).

### Rust tests that drive the device

`setup_virtual_mtp_device()` is the one entry point: it hands back a `VirtualDeviceFixture` owning a **fresh temp
backing root** and registers with the **watcher off**. Three properties matter, and the tests run in `pnpm check`
(`desktop-rust-tests` passes `--features virtual-mtp`), so breaking one shows up as suite flake:

- **Per-test root.** `setup_virtual_mtp_device_at` WIPES its root, so any two tests sharing one delete each other's
  fixtures mid-run. ❌ Never point a test at `MTP_FIXTURE_ROOT`; that's the E2E/dev startup root.
- **Watcher off.** Each device's backing-dir watch is a real FSEvents/inotify watch. Several concurrent test processes
  each holding one starve delivery and push these tests past nextest's 8 s cap. Tests sync the object tree explicitly
  with `rescan_virtual_device()` instead, so nothing needs the watcher. Only the E2E path arms it.
- **Lock + unregister.** Every virtual device registers under the same serial (`cmdr-e2e-virtual`), so they share ONE
  Cmdr device id: `resolve_device_location_id` matches the FIRST registration with that id, and `connect()` is
  idempotent per device id. Under `cargo nextest` (process per test) that's harmless, but under plain `cargo test` two
  tests would silently share one connection pointed at the wrong backing dir. `virtual_device_test_lock()` covers it;
  `unregister_virtual_mtp_device(location_id)` on teardown stops a finished test's registration from answering for the
  next one. Hold the guard across register → connect → use → disconnect → unregister;
  `connection/path_cache_sync_test.rs` is the reference shape.

There is deliberately NO nextest `virtual-mtp` test-group any more: with no shared resource left, serializing would
only hide the next real race.

### Virtual device watcher in E2E

The virtual device (via mtp-rs) runs a filesystem watcher over its backing dirs that turns out-of-band disk writes into
`ObjectAdded` / `ObjectRemoved` events. This models nothing in production MTP: real MTP has no watcher, and Cmdr treats
MTP listings as unwatched (`listing_is_watched(path) == false` — freshness comes from explicit `notify_mutation` +
refresh, never a watcher). The virtual watcher exists only so one E2E test can exercise Cmdr's device-event → directory-
diff pipeline.

**Contract: in E2E the watcher stays PAUSED for the whole test body.** Each MTP spec's `beforeEach` calls
`pause_virtual_mtp_watcher`, recreates the backing-dir fixtures, then syncs the object tree with `rescan_virtual_mtp`
(which reads the backing dir directly — disk is the source of truth). It does NOT resume. The one test that verifies the
live-watch pipeline (`mtp.spec.ts` "detects externally added file") resumes the watcher itself right before its single
write, by which point the `beforeEach` FSEvents have long drained during the pause.

**Gotcha / why (the flake this defends against):** `notify`/FSEvents deliver events asynchronously and don't preserve
cross-directory ordering, so if the watcher is resumed right after a fixture wipe+recreate, late REMOVE events for
REUSED paths (`report.txt`, `DCIM/photo-001.jpg`, seeded `cancel-*.jpg`, …) arrive after the resume and delete the
handles the rescan just re-added. The pane then lists a near-empty directory and `has_item` polls time out (rotating
victims across the MTP shard). An earlier sentinel-drain tried to resume safely by waiting for a marker file's event,
but a single marker can't order events across the whole tree. Keeping the watcher paused removes the resume window
entirely; the rescan is order-independent because it reads disk, not events. Don't reintroduce a resume in the
fixture-sync path.

## Hotplug watching

`watcher.rs` drives off `mtp_rs::mtp::watch_devices()`, a `Stream<Item = HotplugEvent>` of `Arrived(MtpDeviceInfo)` /
`Left(MtpDeviceInfo)`. mtp-rs owns the parts Cmdr used to hand-roll over raw `nusb`: it filters to MTP-capable devices
(a mouse or a hub never wakes us), applies its own settle delay before enumerating (`DEFAULT_SETTLE_DELAY`, 500 ms), and
reports devices already plugged in as `Arrived` on the first poll.

Each event is only a trigger; `check_for_device_changes()` stays the reconciler, for three reasons:

- **Virtual devices.** mtp-rs's watch is USB-only, so the E2E / `virtual-mtp` device produces no event. Only
  `list_mtp_devices()` sees both it and real hardware.
- **The `MTP_ENABLED` gate.** Events arrive while auto-connect is off; the `KNOWN_DEVICES` diff is what picks the device
  up when it's switched back on.
- **Cmdr's ids.** Auto-connect keys on `identity::device_id_for(serial, location_id)`, derived in `discovery.rs`.

**No double-count at startup:** `start_mtp_watcher` enumerates and seeds `KNOWN_DEVICES` synchronously, *before* it
spawns the watcher task, so the stream's initial already-connected `Arrived` burst diffs to nothing. When MTP is
disabled at startup the seed is deliberately left empty (we're not connecting those devices), so a later
`set_mtp_enabled(true)` still sees them as new; that mirrors the disable path, which clears the set.

## Architecture / data flow

```
USB plug-in
  → mtp_rs HotplugEvent::Arrived (watcher.rs; mtp-rs filters to MTP devices and owns the settle delay)
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
  → mtp_rs HotplugEvent::Left (watcher.rs)
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

- `mtp_rs`: MTP session, object listing, file transfer, and hotplug events (`mtp::watch_devices()`).
- `futures_util`: `StreamExt` for the hotplug stream.
- `crate::file_system`: `VolumeManager`, `MtpVolume`, `FileEntry`, `compute_diff`.
