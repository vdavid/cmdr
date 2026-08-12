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
MTP listings as uncovered (`listing_watch_coverage(path) == WatchCoverage::None` — freshness comes from explicit `notify_mutation` +
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
- **Cmdr's ids.** Auto-connect keys on `cmdr_fs::volume::mtp_ids::device_id_for(serial, location_id)`, derived in
  `discovery.rs`.

**The two setters.** `set_mtp_enabled_flag(bool)` writes the flag and nothing else; `start_mtp_watcher()` is called
after it, so startup respects the persisted setting instead of connecting and then tearing down.
`set_mtp_enabled(bool)` is the Tauri-command path: disabling disconnects every device, clears `KNOWN_DEVICES`, and
restores ptpcamerad on macOS; enabling re-runs `check_for_device_changes()`. The watcher loop itself always runs
(`OnceLock`, no shutdown channel) and `check_for_device_changes()` returns early when the flag is off. The persisted
key is `fileOperations.mtpEnabled` in `settings.json`, read by `settings/loader.rs` at startup.

**No double-count at startup:** `start_mtp_watcher` enumerates and seeds `KNOWN_DEVICES` synchronously, *before* it
spawns the watcher task, so the stream's initial already-connected `Arrived` burst diffs to nothing. When MTP is
disabled at startup the seed is deliberately left empty (we're not connecting those devices), so a later
`set_mtp_enabled(true)` still sees them as new; that mirrors the disable path, which clears the set.

## Delete has two scopes

`MtpConnectionManager`'s delete takes an explicit `MtpDeleteScope` (`connection/mutation_ops.rs`), because PTP
`DeleteObject` on a folder is whatever the code around it decides — POSIX gets `ENOTEMPTY` from `remove_dir` and SMB
gets `STATUS_DIRECTORY_NOT_EMPTY` from the server, but MTP has to choose.

- **`SingleNode`**: one file, or one EMPTY folder. A folder that still has children returns
  `MtpConnectionError::DirectoryNotEmpty` and deletes nothing — not the object, and not its path-cache bookkeeping,
  which still describes a live object. `MtpVolume::delete` / `delete_with_cancel` pass this, because `Volume::delete`
  means one node on every backend (`crates/cmdr-fs/src/volume/mod.rs`).
- **`Tree`**: the whole subtree, children first, with the cancel token checked between children.
  `commands::mtp::delete_mtp_object` is the ONLY caller in the repo, and says so in its own doc comment.

**The enum is fieldless with no `Default` and no `From<bool>`**, so a new caller has to decide rather than inherit.
Both entry points (`delete_object` and `delete_object_with_cancel`) take it, or the split would have a hole in it.

**Why the refusal is free.** The directory branch already lists its children before it can do anything else: MTP can't
delete a folder that holds anything, so `Tree` needs the listing to recurse and `SingleNode` needs it to decide.
Refusing costs one fewer USB roundtrip than deleting, never one more. ❌ Don't reach for `is_directory` here — on MTP
that's `get_metadata`, which lists the node's whole PARENT to stat one child, per node.

**Decision / why this shape rather than an opt-in `delete_tree` trait capability.** Naming a capability wouldn't have
caught the bug that motivated this: MTP never claimed recursion, it just did it. And a backend-native tree delete
serves zero callers — every genuine tree delete in the app walks caller-side (`delete/walker.rs` phase 2,
`transfer/volume/cleanup.rs::delete_preserving_inner`) precisely because it needs per-child error attribution and a
`preserve` set, neither of which a device-side recursive delete can offer. What catches the class is the shared
conformance assertion every backend's suite runs (`cmdr_fs::volume::conformance`).

**What changed for users on MTP.** Deleting a folder whose stat failed now reports a per-item failure with the folder
still there, instead of appearing to delete "one file" and quietly taking the whole tree (`delete/walker.rs` guesses
"file" when the no-preview `is_directory` probe errors, and a guessed file goes straight to a bare `delete`). That's
the honest outcome: Cmdr never removes more than it told the user it was removing, and a retry after a transient MTP
stat failure is cheap. It fires only on a probe error, so no normal delete changes.

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
    → attach_storage_volume() per storage (the registrar hook; see below)
    → start_event_loop() per device (strictly AFTER every attach)
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
  → StoreAdded → handle_storage_added() → attach_storage_volume() → emit volumes-changed
  → StoreRemoved → handle_storage_removed() → detach_storage_volume() → emit volumes-changed
```

`MtpDisconnectReason` distinguishes explicit toggle-off from hotplug-loss in logs and UI. Re-enabling MTP triggers
auto-connect, which re-suppresses ptpcamerad if devices are found.

**The write-capability probe.** `probe_write_capability()` (`connection/mod.rs`) creates a hidden `.cmdr_write_probe`
folder on each storage at connect time and deletes it again. Some cameras advertise write support in their device info
and then reject every write with `StoreReadOnly`, so the declared capability can't be trusted; an actual create is the
only reliable answer. Timeouts and non-fatal errors count as WRITABLE: a probe is a cheap hint, and refusing writes on
a device that's merely slow would be the worse failure.

**ptpcamerad suppression mechanics (macOS).** `macos_workaround.rs` suppresses with `launchctl disable` plus
`pkill -9`, and restores with the matching enable. `restore_ptpcamerad_unconditionally()` is the disable-MTP path (it
doesn't wait for a device to leave), and `ensure_ptpcamerad_enabled()` runs at startup so a crash mid-suppression
can't leave the user's camera unusable in Photos.

## Backends never register themselves

**Decision.** A backend's session layer reports that a storage attached or detached; it does not decide that a `Volume`
now exists. `connection/volume_registrar.rs` holds a `OnceLock<MtpVolumeRegistrar>` (two `fn` pointers, `attach` and
`detach`); `volume_wiring.rs` supplies them, and `lib.rs` installs it at startup right after `volume_broadcast::init`,
before anything can connect a device. `volume_wiring.rs` is deliberately the twin of `network/smb_upgrade.rs`, which
builds and registers the `SmbVolume` while the SMB session layer never does: a wiring module beside the feature, aware
of both the backend and the registry, with neither aware of it.

**Why.** A session layer that constructs its own `Volume` has to import the app's volume registry, and the registry
imports the backend: `backends::mtp` and `mtp::connection` were a genuine import cycle held together by four lines of
wiring. Neither module could then be understood or moved alone, and MTP can't become its own crate while it imports the
app. This is the shape FTP, S3, and SFTP should copy: **the wiring knows the backend, the backend never knows the
registry.**

**Gotcha: the attach must complete before the event loop starts, and the hook must not break that.** `connect()`
attaches every storage and only then calls `start_event_loop`. Everything the loop reaches routes through the volume
registry (open listings are looked up by volume id; the per-volume index routes by device id), so an event that arrived
before the volumes existed would have nothing to land on and the update would be dropped. The registrar adds an
indirection but not a delay: `attach_storage_volume` is a direct synchronous call. ❌ Never spawn it, never make it
async. Pinned by `connect_attaches_a_volume_for_every_storage_and_disconnect_detaches_them`
(`file_system/volume/backends/mtp_test.rs`), which asserts registration with no polling at all, so a scheduled attach
fails it.

**Gotcha: a test that connects a device must install the registrar.** `setup_virtual_mtp_device` does it, mirroring
startup. Without it a `connect()` opens the device and leaves the sidebar empty, and the failure looks like a volume bug
rather than missing wiring.

## Cancel propagation wiring

Long MTP operations bail at the next per-USB-roundtrip boundary when the caller's write-op intent flips to
`Stopped` / `RollingBack`.

- `WriteOperationState.backend_cancel` (a `CancellationToken`) is created per write op alongside `intent`.
  `cancel_write_operation` and `cancel_all_write_operations` flip both together so any cancel path stops the wire
  activity.
- `MtpVolume`'s three cancel-aware methods open an `MtpCancelBridge` for the call: mtp-rs polls its own
  `Arc<AtomicBool>`-backed `CancelToken`, so a task parked on `cancelled()` mirrors ours into it and retires when the
  bridge drops (its guard cancels a child token, so the mirror ends on every exit — clean, cancelled, or errored).
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
class-cancel path (see "Transfer cancellation" in `~/projects-git/vdavid/mtp-rs/AGENTS.md`).

### Hardware caveats

Some Android devices (Pixel 6/7-era firmware observed) may still leave the session degraded after a flurry of ops even
when cancel is clean on our side. This is hardware-side and unfixable in software; the settled-state gate (see
`file_system/write_operations/DETAILS.md` § "Settle contract") ensures the user doesn't issue the next op until our side
is fully quiet, which avoids provoking the bug in practice.

## Dependencies

- `mtp_rs`: MTP session, object listing, file transfer, and hotplug events (`mtp::watch_devices()`).
- `futures_util`: `StreamExt` for the hotplug stream.
- `cmdr_fs`: `FileEntry`, `CopyScanResult`, `ListingProgress`, the `mtp_ids` volume-id helpers.
- `crate::file_system`: the listing cache and `compute_diff`. ❌ Not `MtpVolume` or the volume manager, by the decision
  above.
