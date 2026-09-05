# `cmdr-mtp` details

Read this before any non-trivial work here: editing, planning, reorganizing, or advising. `CLAUDE.md` holds the
must-knows; this is the depth. The session layer's own locks, caches, recovery, and event wiring live one level down in
`src/connection/DETAILS.md`, and this document doesn't restate them.

## Where the boundary runs, and why

A backend has two faces, and MTP's split cleanly along them.

**The file-ops face** is the `Volume` trait plus the `VolumeHost` seams: `MtpVolume` implements one and asks everything
else through the other. That half needed no argument, because a `Volume` impl already names nothing but `cmdr-fs`.

**The lifecycle face** is the session layer, and it's where the whole retrofit happened: opening a device, keeping its
PTP session, polling its interrupt endpoint, recovering a session the phone dropped, and telling somebody a storage
arrived. Every one of those used to reach sideways into the app.

What stayed up there is what genuinely needs the app:

- **The hotplug policy** (`mtp/watcher.rs`) — the `MTP_ENABLED` gate, the `KNOWN_DEVICES` diff, auto-connect, and the
  ptpcamerad calls. It owns a decision about when to connect, not a protocol. ADB's tracker is the same shape.
- **Every word and every event** (`mtp/events.rs`) — the `tauri_specta` payload structs the frontend subscribes to, and
  the single match that maps this crate's `MtpDeviceEvent` onto them.
- **Registering a volume** (`mtp/volume_wiring.rs`) — the `MtpVolumeRegistrar` this crate calls, and the
  `DeviceVolumeProvider` the picker folds over.
- **ptpcamerad's lifecycle** (`mtp/macos_workaround.rs`) — `launchctl disable`, the restore on quit, and the user-facing
  notice.

The one thing that crossed the other way is `connection/usb_owner.rs`: `connect()` asks who is holding a device it
couldn't open, which is `ioreg` and a parser with no app in it, so it lives beside the failure that asks for it. The
SUPPRESSION half of ptpcamerad stayed app-side, because that half is a policy.

## The public surface is capped

`index-crate-isolation` holds this crate to **20 root promises, 2 public modules, and 13 public items inside them**, set
on 2026-09-05 to exactly what it exposed the day the extraction finished — no headroom, so the first addition has to be
argued for.

**A backend's API is the `Volume` trait it implements**, which is `cmdr-fs`'s promise rather than this crate's, so
`MtpVolume`'s trait methods aren't counted. What IS counted serves one of four audiences, and a new item should name
which:

- **Driving a device** (`commands/mtp.rs`, `mtp/watcher.rs`, `mtp/volume_wiring.rs`): `MtpConnectionManager`,
  `DeviceWatch` (whether to poll), `MtpDeleteScope` (which of the two deletes), `MtpDisconnectReason` (the toggle or the
  USB stack), and `MtpVolumeRegistrar` with `no_device_events` beside it for a manager reporting nowhere.
- **What a call answers with**: `ConnectedDeviceInfo`, `MtpObjectInfo`, `MtpConnectionError`, `ResolvedMtpObject` (the
  index watch path's one question), plus `MtpDeviceInfo` and `MtpStorageInfo`, the two types that cross IPC and carry
  `specta::Type`.
- **Reporting lifecycle**: `MtpDeviceEvent` and the `MtpDeviceEvents` trait the app implements.
- **Finding a device at all**: `list_mtp_devices`, `watch_devices`, and `HotplugEvent`.

Those 17, plus the `MtpVolume` re-export the registrar hands the app and the two `pub mod` declarations (a module name
is itself a promise), are the 20.

Two public modules is the whole tree a host can name a path into: `volume` (for `MtpVolume` and, gated, `testing`) and
`virtual_device`. **❗ `connection` is PRIVATE**, even though the session layer is most of this crate: every name the
app uses is a root re-export, so nothing reaches the caches, the priority gate, the event loop, or the reset recovery by
module path. `MtpReadSession`, `map_mtp_error`, `MTP_READ_WINDOW`, and the whole `errors` module stay `pub(crate)` for
the same reason.

`virtual_device` costs 11 of the 13 subsystem items, which reads oddly for a fixture, and `volume` costs the other two
(`MtpVolume` and its constructor). It isn't only a fixture: the E2E harness and a `CMDR_VIRTUAL_MTP=1` dev session both
drive it from the app, and it sits behind the `virtual-device` feature rather than `testing`, so the counter (which
skips `testing` / `cfg(test)` gates outright) sees it. That is deliberate: a fake phone the E2E build ships is worth
measuring, unlike a fixture only a test target compiles.

Seven gated items sit outside all three numbers: `RecordingMtpDeviceEvents`, the root `testing` module,
`volume_read_stream_to_chunk_stream`, and the four fixture items in `virtual_device` (`volume::testing` isn't counted at
all — the counter reads `pub mod` only at the root). So the app's MTP suites can never become a reason to widen the cap,
and nothing measures the fixture surface's own growth — keep it a fixture surface by reading § "Which side a test lives
on", not by watching a number.

## What `MtpVolume` does differently

A phone answers a different set of questions than a filesystem, so five places in the `Volume` impl deviate from what
every other backend does, and each deviation is load-bearing.

**A copy scan groups by parent.** `scan_for_copy_batch_with_boundary` is overridden because MTP has no single-file stat:
`get_metadata(path)` lists the parent and searches by name. A naive scan calling it per path would re-list
`/DCIM/Camera` (15k entries, ~17 s over USB) for every selected photo. The override groups the input paths by parent,
calls `list_directory(parent, on_progress)` once per unique parent, and indexes the entries by name for O(1) lookups.
**The fresh-listing oracle layers on top**: before listing a parent it asks `ListingHost::authoritative_listing`, and a
hit replaces the listing call entirely, so no USB I/O is paid for that parent. A miss falls through to the
one-listing-per-parent path, so a cold cache is no slower. The decision is per parent, and one batch can mix
watcher-fresh and cold ones.

**❗ `get_metadata` is expensive, always.** It lists the entire parent directory and searches by name, because MTP has
no stat. `notify_mutation` pays it after each self-mutation (create, delete, rename), which is fine because those are
infrequent. ❌ Never call it in a hot path.

**A dropped read stream is safe, and needs no `Drop` impl.** `MtpReadStream` reads in bounded
`GetPartialObject64(offset, MTP_READ_WINDOW)` windows (`src/connection/DETAILS.md` § "Bounded-window reads" owns the
windowing and offset accounting). Between windows nothing is in flight — no held `FileDownload`, no pinned PTP session —
so a cancel, pause, or drop has nothing to abort or drain, and `cancel_and_release` is the trait's default no-op. A
stream dropped WHILE a window read is in flight is still safe: mtp-rs's `TransactionScope` flags the pipe and the next
op drains it under the operation lock, one ~300 ms self-heal. ❌ Don't re-add a `Drop` cancel here; with no held
`FileDownload`, mtp-rs's unconsumed-drop panic can't apply.

**A conflict scan settles a missing destination through `get_metadata`, not a `NotFound` arm.** Every other backend
reads `VolumeError::NotFound` from the destination listing as "nothing clashes" and answers an empty list. MTP can't:
`resolve_path_to_handle` is cache-only, so a path nobody has browsed to fails as a generic `IoError` ("path not in
cache"), which is honest, because it means UNKNOWN rather than absent. Reading every listing failure as absence would
let a disconnected device pass for an empty folder and clear the copy to run. `get_metadata` settles it by listing the
PARENT, so only a confirmed-absent destination reads as empty and every other failure stays the caller's to see. It
costs one extra parent listing, on the error path only.

### The no-clobber rename is check-then-act

`MtpVolume::rename` earns the `force == false` refusal by asking `exists(to)` and then moving. Every other backend
claims the name with a primitive the other end refuses (`renamex_np(RENAME_EXCL)`, an SFTP `create_new` placeholder or
plain `SSH_FXP_RENAME`, WebDAV's `Overwrite: F`, SMB's `ReplaceIfExists == false`), so MTP is the only one whose refusal
has a window in it. The conformance cell is `volume::conformance_test::rename_honors_the_shared_no_clobber_contract`.

**Decision: leave the window open and say so, rather than build machinery around it. Why:** MTP offers nothing tighter
to build on (verified on `mtp-rs` 0.32.0, source read, 2026-09-02). A same-directory rename is
`SetObjectPropValue(0x9804)` on `ObjectFileName(0xDC07)`, and a cross-directory move is `MoveObject(0x1019)` with params
`[handle, storage_id, parent]`; neither takes an overwrite or exclusive flag, and PTP's response-code enum has no
collision code to read one out of (`StoreFull`, `AccessDenied`, `InvalidParameter`, and no `ObjectAlreadyExists`). The
protocol also permits two siblings with the same name, so a device asked to collide doesn't refuse: it complies, and the
user ends up with a duplicate. ❌ Don't reach for a lock or a retry loop here. Cmdr isn't the only writer — the phone's
own apps and MTP's other clients mutate the same storage — so a lock this side would buy nothing and read like a
guarantee.

## Two features, two different axes

- **`testing`** means "this is a test build". It publishes `volume::testing` (the `list_directory` call counter and the
  read-window override) and the virtual-device fixture items, and pulls in `tempfile` for the fixture's scratch root.
  `tempfile` is a NORMAL optional dependency rather than a dev one because the fixture lives in the LIB and a lib target
  can't see dev-dependencies — the same shape `blake3` has in `cmdr-smb`.
- **`virtual-device`** means "a fake phone can exist", and forwards to `mtp-rs/virtual-device`. The app turns it on as
  `virtual-mtp`, for the Playwright lane and for a dev session.

They're independent on purpose. The E2E build wants a fake phone and no fixtures; this crate's own suites want both. The
gate on a fixture item is therefore `any(test, feature = "testing")` INSIDE a `virtual-device` module, which reads
awkwardly and is correct.

The self dev-dependency (`cmdr-mtp = { path = ".", features = ["testing", "virtual-device"] }`) is what turns both on
for every dev target and leaves them off for the lib, so a shipped build carries no fixtures.

## Which side a test lives on

Split by what a cell **asserts**, never by what it connects to — the rule `crates/cmdr-smb/DETAILS.md` § "Which side a
test lives on" states in full. Both sides drive the same virtual device through the same fixtures.

**Here**, if the assertion is about this backend:

- `volume/volume_impl_test.rs` — what `MtpVolume` declares through the trait: the capability answers a caller routes on,
  and the watch-coverage gate the app's fresh-listing oracle reads.
- `volume/conformance_test.rs` — the shared `cmdr_fs::volume::conformance` promises, answered by a device rather than an
  in-process double. `volume/delete_test.rs` is its third sibling: MTP is the one backend that has to IMPLEMENT
  non-recursion rather than inherit it, and `MtpDeleteScope` needs enough scaffolding to earn a file.
- `volume/read_range_test.rs` — both read paths, sharing one fixture: the DIRECT `read_range` (a ranged read costs one
  device operation, not three) and the WINDOWED `open_read_stream` the copy engine and native drag-out go through.
- `volume/streams_test.rs` — the `VolumeReadStream` → chunk-stream adapter, with no device behind it.
- `volume/host_seam_test.rs` — the PACE of what this backend tells the listing seam, which no type can hold: one call
  per mutation, none per directory entry. It seeds 40 files, then walks them with a listing, a stat, a stream, a ranged
  read, and both scans, asserting `RecordingListings::change_count` doesn't move.
- `volume/path_test.rs`, `connection/manager_test.rs`, `connection/path_cache_sync_test.rs`, `connection/upload_test.rs`
  (what an upload leaves on the device when its source stops early), and `connection/host_seam_test.rs` (the analytics
  counter and the registrar, which is where `handle_device_disconnected` is reachable).
- `volume/read_bench.rs` — the hardware benchmark, `#[ignore]`d. ❗ On macOS the operator holds `ptpcamerad` off in
  another terminal first: suppressing a system daemon is a host policy, which is why `macos_workaround` is app-side and
  this file doesn't call it.

**In the app**, if the assertion is about what the APP does with a phone, and the cell sits beside the app code it
asserts on rather than beside the backend: `mtp/volume_wiring_test.rs` (that `volume_wiring` really registers, and that
the attach runs inline on the connecting thread), `file_system/volume/mtp_scan_oracle_tests.rs` (the app's fresh-listing
oracle), `file_system/write_operations/mtp_archive_test.rs` (archive routing, which this crate knows nothing about), and
`file_system/write_operations/transfer/volume/rename_merge_mtp_tests.rs` plus `delete/volume_cancel_tests.rs` (the
transfer and delete pipelines).

Two consequences that bit during the extraction:

**A cell that reaches the backend's internals belongs here, and a visibility widening is not the fix.** `to_mtp_path`
and the `device_id` / `storage_id` fields are `pub(super)`; the cells asserting on them moved in as
`volume/path_test.rs`. `read_range_test.rs` drives `invalidate_storage_cache`, which is `pub(crate)` and stays that way.

**A cell that reached the app's volume registry was usually asserting on the registrar seam.** Three of them — a device
lost under the event loop, and the two session-reset cells checking that a volume survives the recovery — now assert
through `connection::testing`'s recording registrar. The registry was only ever an observation point for "did the seam
fire", and the seam says the same thing with no app in the room.

### The fixtures both sides share

`testing` (behind the `testing` feature plus `virtual-device`) is the one door: it registers a fake phone, connects,
primes the root listing, and takes it all away again, parametrized by the manager. ❗ Every entry point takes the
manager, because that's the ONLY thing that differs across the boundary — a cell here wants a detached host that answers
nothing, an app cell wants the real wiring so the listing cache, the index, and the volume registry see what the device
reports. The app's `mtp/test_support.rs` shadows each one with a no-argument version over its parked manager, the shape
`smb_test_support.rs` has.

❗ Seed a device's backing dir BEFORE connecting it. The connect primes the root listing, so a file written after that
is invisible until something invalidates the cache; `rescan_virtual_device` is for a device that's already open.

`connection::testing` stays `#[cfg(test)] pub(crate)` and is NOT published: a shared manager over a detached host plus
the recording registrar, both process-wide the way the app's parked manager is. Every cell reaching them holds
`virtual_device_test_lock` for its whole span, and under `cargo nextest` each cell is its own process anyway.

### Three properties a cell must not break

`setup_virtual_mtp_device()` hands back a `VirtualDeviceFixture` owning a **fresh temp backing root**, registered with
the **watcher off**. Breaking one of these shows up as suite flake rather than as a failure that names itself:

- **Per-test root.** `setup_virtual_mtp_device_at` WIPES its root, so any two cells sharing one delete each other's
  fixtures mid-run. ❌ Never point a cell at `MTP_FIXTURE_ROOT`; that's the E2E and dev startup root.
- **Watcher off.** Each device's backing-dir watch is a real FSEvents / inotify watch. Several concurrent test processes
  each holding one starve delivery and push these cells past nextest's 8 s cap. A cell syncs the object tree explicitly
  with `rescan_virtual_device()` instead, so nothing needs the watcher. Only the E2E path arms it.
- **Lock + unregister.** Every virtual device registers under the same serial (`cmdr-e2e-virtual`), so they share ONE
  Cmdr device id: `resolve_device_location_id` matches the FIRST registration with that id, and `connect()` is
  idempotent per device id. Under `cargo nextest` (process per cell) that's harmless; under plain `cargo test` two cells
  would silently share one connection pointed at the wrong backing dir. `virtual_device_test_lock()` covers it, and
  `unregister_virtual_mtp_device(location_id)` on teardown stops a finished cell's registration from answering for the
  next one. Hold the guard across register → connect → use → disconnect → unregister;
  `src/connection/path_cache_sync_test.rs` is the reference shape.

There is deliberately NO nextest `virtual-mtp` test-group: with no shared resource left, serializing would only hide the
next real race.

### Building the fixture's device config

**Build `VirtualDeviceConfig` with `..Default::default()`** and state only the fields this fixture actually cares about,
so a new field upstream doesn't break the build (`supports_partial_object_64` did exactly that once). ❌ Don't re-expand
the literal to name every field. The defaults model a modern Android device (`supports_rename` and
`supports_partial_object_64` both true), which matches the Pixel 9 this fixture stands in for; set
`supports_partial_object_64: false` explicitly to exercise mtp-rs's 32-bit `GetPartialObject` fallback, the PTP-camera
path.

## Running the suites

```
cargo nextest run --workspace --features cmdr/virtual-mtp -E 'package(cmdr-mtp) + test(mtp)'
```

Why the feature is package-qualified, why `cargo test` is not a substitute, and which door each side reaches a device
through: `docs/tooling/virtual-mtp.md` § "Running the Rust suites against it".

## Dependencies

- `mtp-rs`: the PTP session, object listing, transfers, hotplug events, and the virtual device. Pinned to the version
  the app carried before this crate existed, so nothing about the wire behavior moved with the code.
- `cmdr-fs`: the `Volume` trait, `FileEntry`, `UsbSpeed`, the `mtp_ids` helpers, the host seams, and poison-free
  locking. The only crate below this one.
- `specta` at the exact version the app pins, for the four types that cross IPC. Two `specta` crates in one graph would
  make these `Type` impls stop satisfying `tauri-specta`.
- `tokio` WITHOUT `rt-multi-thread`: the runtime is the app's, handed down through `VolumeHost::runtime`. A backend that
  could build its own would.
- `futures-util` and `bytes` for the upload stream; `tokio-util` for `CancellationToken` and its `DropGuard`.
