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

Two public modules is the whole tree a host can name a path into: `volume` (for `MtpVolume` and, gated, `testing`) and
`virtual_device`. **❗ `connection` is PRIVATE**, even though the session layer is most of this crate: every name the
app uses is a root re-export, so nothing reaches the caches, the priority gate, the event loop, or the reset recovery by
module path. `MtpReadSession`, `map_mtp_error`, `MTP_READ_WINDOW`, and the whole `errors` module stay `pub(crate)` for
the same reason.

`virtual_device` costs 10 of the 13 subsystem items, which reads oddly for a fixture. It isn't only one: the E2E harness
and a `CMDR_VIRTUAL_MTP=1` dev session both drive it from the app, and it sits behind the `virtual-device` feature
rather than `testing`, so the counter (which skips `testing` / `cfg(test)` gates outright) sees it. That is deliberate:
a fake phone the E2E build ships is worth measuring, unlike a fixture only a test target compiles.

Gated items sit outside all three numbers: `volume::testing`, `volume_read_stream_to_chunk_stream`, and the four fixture
items in `virtual_device`. So the app's MTP suites can never become a reason to widen the cap, and nothing measures the
fixture surface's own growth — keep it a fixture surface by reading § "Which side a test lives on", not by watching a
number.

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
test lives on" states in full. Two consequences that bit during the extraction:

**A cell that reaches the backend's internals belongs here, and a visibility widening is not the fix.** `to_mtp_path`
and the `device_id` / `storage_id` fields are `pub(super)`; the cells asserting on them moved in as
`volume/path_test.rs`. `read_range_test.rs` drives `invalidate_storage_cache`, which is `pub(crate)` and stays that way.

**A cell that reached the app's volume registry was usually asserting on the registrar seam.** Three of them — a device
lost under the event loop, and the two session-reset cells checking that a volume survives the recovery — now assert
through `connection::testing`'s recording registrar. The registry was only ever an observation point for "did the seam
fire", and the seam says the same thing with no app in the room. The app keeps the cells that assert the app's own half:
that `volume_wiring` really registers, and that the attach runs inline on the connecting thread.

`connection::testing` is `#[cfg(test)] pub(crate)`: a shared manager over a detached host plus that recording registrar,
both process-wide the way the app's parked manager is. Every cell reaching them holds `virtual_device_test_lock` for its
whole span, and under `cargo nextest` each cell is its own process anyway.

## Running the suites

```
cargo nextest run --workspace --features cmdr/virtual-mtp -E 'package(cmdr-mtp) + test(mtp)'
```

The feature is spelled `cmdr/virtual-mtp` rather than bare, so it resolves the same way the shared `desktop-rust-tests`
lane resolves it and no rebuild is triggered by the flip. Plain `cargo test` fails the virtual-device cells: every fake
phone registers under one serial, so two cells in one process share a connection, and `virtual_device_test_lock` is what
covers that. ❌ Never drop the lock from a virtual-device cell.

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
