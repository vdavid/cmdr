# ADB (app side): details

The app side of `crates/cmdr-adb/`: how a device reaches the sidebar, when a volume is dialed, what an eject does,
what the frontend calls, and what is not wired yet. The wire contract, the `Volume` answers, and the error policy are
the crate's (`crates/cmdr-adb/DETAILS.md`); the design that started this is `docs/specs/android-adb-backend.md`. The
seam both device backends register through is `device_volumes.rs`, whose module doc is canonical for the trait.

## Where each thing lives

- **The crate** owns the protocol, the `Volume`, and the `track_devices` subscription with its reconnect backoff. It
  never names the registry, the listing, or `tauri`.
- **This module** owns the cached device state, the provider, the connect wiring, eject, and the commands. It is the
  only place that knows both the crate and the app.
- **`device_volumes.rs`** owns the provider registry, `append_device_volumes` (what `volume_listing::complete` folds
  over), `provider_for_volume_id` (what `eject.rs` asks before answering `EjectAction::DeviceDisconnect`),
  `device_volume_for_path` (what path resolution asks), and `notify_devices_changed`, the one push channel (it emits
  `volumes-changed`).
- **`commands/volumes.rs::resolve_path_to_volume`** is where an `adb://` path turns into a dial: it calls
  `volume_wiring::volume_id_for_path` before the generic device-provider lookup.

## Flows

**Startup** (`lib.rs` setup): `install_device_provider` registers `AdbDeviceProvider` beside `MtpDeviceProvider`,
then `start_adb_tracker` starts the one `host:track-devices` subscription for the process (a `OnceLock`; a second call
is a no-op). The tracker talks only to the local server socket, never to USB. With no `adb` installed it logs at
`debug` and idles, so a machine without the platform tools sees nothing and pays nothing.

**Hotplug**: every push from `cmdr_adb::track_devices` (the full `host:devices-l` list, refetched by the crate on each
short-format push) lands in `device_provider::apply_device_list`, synchronously on the runtime. It stores the list,
and for every serial that left and had a volume, calls the volume's `note_device_gone` (the crate emits the
`Disconnected` transition once) and `VolumeManager::unregister` (which retires it). Then
`notify_devices_changed("adb")` → `volumes-changed` → the frontend refetches the list. When the server goes away the
crate reconnects with backoff (1 s doubling to 15 s) and redelivers the list, so a change missed while it was down is
caught up.

**Connect**: `connect_adb_device(serial)` answers an already-dialed volume's id without a second dial; otherwise
`cmdr_adb::connect_adb_volume(params, host, cancel)` runs the crate's four phases, the volume goes in through
`VolumeManager::register_if_absent` (never `register`: no OS mount can pre-register the id, and a repeated connect
must not retire a volume a pane is using), is remembered by serial, and `notify_devices_changed("adb")` lets
`volume_listing::complete` enrich the entry with its capabilities. Errors cross IPC as `AdbConnectOutcomeError`, a
typed mirror of `AdbConnectError` (`AdbNotInstalled`, `ServerUnreachable`, `DeviceGone`, `Unauthorized`,
`DeviceTooOld`, `TimedOut`, `Cancelled`, `Transport`); the frontend words each one in `adb-connect-errors.ts`. The
cancel token handed to the crate is a fresh one today (§ "Not wired yet").

**Eject**: `eject.rs` asks `provider_for_volume_id`, gets this provider, and answers
`EjectAction::DeviceDisconnect { provider: "adb", volume_id }`. `AdbDeviceProvider::eject` forgets the volume and
unregisters it; nothing is sent to the phone (`adb` has no per-client detach). The device stays in the cached list, so
it is listed again on the next `volumes-changed` and re-dialed on the next navigation. A device the user wants gone
for good is unplugged, or revoked on the phone.

## The provider's answers

`AdbDeviceProvider` answers from the cache, never the wire:

- `id`: `"adb"`.
- `entries()`: one entry per device in state `Ready` (`device` on the wire), dialed or not: id `adb:<serial>`, path
  `adb://<serial>`, `fs_type: "adb"`, name = `AdbDevice::display_name()` (the model, falling back to the serial),
  `mount_is_read_only: false`, `usb_speed: None`. `unauthorized`, `offline`, `recovery`, and the rest are not listed;
  `list_adb_devices` still returns them with their typed state, so a device switcher can say "tap Allow on the phone".
- `owns_volume_id`: any cached serial's id matches.
- `space_for_path`: the connected volume's `get_space_info` (`df -k` on the device), `None` until it is dialed.
- `eject`: above.

## IPC and frontend

- `list_adb_devices() -> Vec<AdbDevice>`: the cached list, typed states included.
- `connect_adb_device(serial) -> Result<volume_id, AdbConnectOutcomeError>`.
- Frontend: `src/lib/adb/` (`adb-path-utils.ts` for the `adb://` scheme beside `mtp://`, `adb-volume-label.ts`,
  `adb-connect-errors.ts`) and `tauri-commands/adb.ts`. The frontend is a passive consumer of `volumes-changed`, the
  posture `src/lib/mtp/CLAUDE.md` describes for MTP; its one active step is the connect a navigation triggers.

## Testing

Suites here drive `cmdr_adb::testing::FakeAdbServer` (the crate's `testing` feature is on for the app's dev targets):
the tracker's diff and inline retirement, the provider's listing answers, eject, `resolve_path_to_volume` on an
`adb://` path, and the transfer engine through the registry. A cell asserting on the protocol belongs in the crate:
`crates/cmdr-adb/DETAILS.md` § "Which side a test lives on".

## Not wired yet

- An " (ADB)" name suffix when the same phone is also listed over MTP (`entries()` names the model alone).
- Index routing for `adb:` volume ids, `go_to_path`, and the MCP `select_volume` tool don't answer for an `adb://`
  path.
- A settings toggle (the MTP twin is `fileOperations.mtpEnabled`) and the `adb` binary path setting.
- The connect isn't cancelable from the pane: `connect_adb_device` hands the crate a fresh `CancellationToken`. The
  crate honors one; the four lines that wire a cancel button are SFTP's (`crates/cmdr-sftp/DETAILS.md` § "Wiring the
  cancel button").
- The real-device pass and the crate's own deferrals: `crates/cmdr-adb/DETAILS.md` § "Known gaps and follow-ups".
