# ADB (app side)

The app half of the Android-over-ADB backend: the cached `host:track-devices` list, the `DeviceVolumeProvider` that
puts a device in the sidebar, lazy connect on the first `adb://` navigation, eject, and the IPC commands. The wire and
the `Volume` are `crates/cmdr-adb/` (read its `CLAUDE.md`). Same split as `mtp/volume_wiring.rs` and
`network/sftp_volume_wiring.rs`: the wiring knows the backend and the registry, neither knows the wiring.

## Module map

- `device_provider.rs`: `AdbDevices` (the list the tracker last pushed + connected volumes by serial, one process-wide
  `RwLock`) and `AdbDeviceProvider`, ADB's `device_volumes::DeviceVolumeProvider`.
- `volume_wiring.rs`: `install_device_provider` and `start_adb_tracker` (once each, from `lib.rs` setup),
  `connect_adb_device` (dial, `register_if_absent`, remember), `volume_id_for_path` (what
  `commands/volumes.rs::resolve_path_to_volume` calls for an `adb://` path).
- `commands.rs`: `list_adb_devices`, `connect_adb_device`, `get_adb_install_status`, `recheck_adb_install`, and
  `AdbConnectOutcomeError`, the typed IPC mirror of `AdbConnectError`.

## Must-knows

- **❗ Path scheme is `adb://<serial>[/device path]`; volume id `adb:<serial>`** (`cmdr_fs::volume::adb_volume_id`).
  ❌ Never build or split either by hand: `device_path` / `serial_of_path` here, `adb-path-utils.ts` on the frontend.
- **❗ A device is listed before it's connected.** `entries()` lists every `Ready` device from the cache; the volume is
  dialed on the first `adb://<serial>` navigation. ❌ Never dial from `entries()`: the listing runs on every
  `volumes-changed`.
- **❗ The tracker callback (`apply_device_list`) is synchronous and unregisters inline.** It runs on the runtime
  inside `cmdr_adb::track_devices`; a serial that left the list gets `note_device_gone` + `unregister`, which retires
  its volume. ❌ Never spawn from it, or a pane keeps a dead volume until the task gets scheduled.
- **❗ Eject retires, never detaches.** `adb` has no per-client detach; `AdbDeviceProvider::eject` forgets and
  unregisters the volume, the device stays in the cached list, and the next navigation re-dials it.
- **❗ Hotplug and connect both ride `volumes-changed`** through `device_volumes::notify_devices_changed("adb")`.
  ❌ No ADB-named event; the channel is backend-neutral on purpose.
- **❗ Connect errors cross IPC as `AdbConnectOutcomeError`**, a typed mirror the frontend words at the call site.
  ❌ Never a sentence.
- **❗ `"adb"` has a row in `MAX_CONCURRENT_OPERATIONS_SOURCES`** (`file_system/backend_settings.rs`) answering the
  constant 1; a namespace without one silently gets 2.
- **❗ No `adb` binary STOPS the tracker** (at `debug`), rather than retrying: nothing to reconnect to, and a retry
  would warn every 15 s all session on every machine without Android tooling. `recheck_adb_install` is the only way
  back, and the only path allowed to retry `adb start-server`. ❌ Nothing may poll it: one attempt per human action.
- **❗ Suites here run against the crate's fake server** (`cmdr_adb::testing::FakeAdbServer`, behind the `testing`
  feature), ❌ never a real `adb`. Which side a cell lives on: `crates/cmdr-adb/DETAILS.md`.

Flows, the provider's answers, what's not wired yet: `DETAILS.md`.
