# ADB (app side)

The app half of the Android-over-ADB backend: the cached `host:track-devices` list, the `DeviceVolumeProvider` that
puts a device in the switcher, lazy connect on the first `adb://` navigation, the two settings, eject, and the IPC
commands. The wire and the `Volume` are `crates/cmdr-adb/`. As in `mtp/volume_wiring.rs` and
`network/sftp_volume_wiring.rs`, the wiring knows the backend and the registry; neither knows the wiring.

## Module map

- `device_provider.rs`: the cached state (`AdbDevices`: the tracker's last list plus connected volumes by serial, one
  process-wide `RwLock`), `readiness_of` (which states become a row), and `AdbDeviceProvider`.
- `volume_wiring.rs`: tracker lifecycle, settings, `connect_adb_device` / `cancel_connect`, and `volume_id_for_path`
  (what `commands/volumes.rs::resolve_path_to_volume` calls).
- `commands.rs`: the IPC pass-throughs, plus `AdbConnectOutcomeError`, the typed mirror of `AdbConnectError`.

## Must-knows

- **❗ Path scheme is `adb://<serial>[/device path]`, minted by `cmdr_fs::volume::adb_app_root` (the volume's `root()`
  too); the id is `adb_volume_id`.** ❌ Never build or split either by hand: `device_path` / `serial_of_path` here,
  `adb-path-utils.ts` on the frontend.
- **❗ A device is listed before it's connected, and before it's authorized**, carrying a `device_readiness`; only
  `recovery` / `bootloader` / `sideload` / an unreadable state word are left out. ❌ Never dial from `entries()`: the
  listing runs on every `volumes-changed`.
- **❗ Readiness is PRESENCE; `connection_state` stays `None` on a device row.** A phone waiting for its "Allow USB
  debugging?" tap has no session, and the reconnect backoff would dial nothing forever.
- **❗ A dial is cancelable and the attempt id is the CALLER's**, filed before the wire is touched so a pane can arm
  its cancel button first. A navigation's own dial files under `adb-navigation:<serial>`.
- **❗ At most one wire dial per serial; later callers JOIN it.** A cancel answers its own attempt at once; the wire
  dial goes only when no joined attempt still wants it. ❌ Never register or remember a volume outside that one dial,
  or the registry and the provider hold different volumes.
- **❗ The tracker callback (`apply_device_list`) is synchronous and unregisters inline.** ❌ Never spawn from it, or a
  pane keeps a dead volume until the task gets scheduled.
- **❗ Turning `fileOperations.adbEnabled` off empties the cached list, not only the tracker**, or the last list stays
  frozen on screen with its volumes registered. Both settings travel together through `set_adb_settings`.
- **❗ No `adb` binary STOPS the tracker** rather than retrying. Only a human action restarts it: a
  `recheck_adb_install` click, or the setting going back on; both clear the start-attempt memory, so `adb start-server`
  runs once per action. ❌ Nothing may poll the status.
- **❗ `"adb"` has a row in `MAX_CONCURRENT_OPERATIONS_SOURCES`** (`file_system/backend_settings.rs`) answering 1; a
  namespace without one silently gets 2.
- **❗ Suites here drive the crate's fake server**, ❌ never a real `adb`.

Flows, the provider's answers, eject, the settings, and the deliberate non-goals: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
