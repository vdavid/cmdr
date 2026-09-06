# MTP module (the app half)

The protocol, the session layer, and the `Volume` over one storage are `crates/cmdr-mtp`; read its `CLAUDE.md` before
touching a device. What's here is what needs the app. macOS and Linux only; on Linux, USB permissions may need udev
rules (`resources/99-cmdr-mtp.rules`). The frontend (`src/lib/mtp/CLAUDE.md`) is a passive consumer of
`volumes-changed` and `mtp-device-connected` / `-disconnected`; it never orchestrates connections.

## File map

- `watcher.rs`: hotplug over `cmdr_mtp::watch_devices()`, auto-connect, the enabled gate, the ptpcamerad calls.
- `events.rs`: the seven `tauri_specta` payload structs and the adapter mapping the crate's typed `MtpDeviceEvent`s onto
  five of them. The struct name kebab-cases to the wire event name.
- `volume_wiring.rs`: registers a storage as an `MtpVolume` (twin of `network/smb_upgrade.rs`), and files MTP as a
  `DeviceVolumeProvider`. `macos_workaround.rs`: ptpcamerad suppression and restore.
- `mod.rs`: where the app parks the one manager it built (`install_connection_manager`, then `connection_manager()`),
  and the door the crate's names are re-exported through, so a call site writes `crate::mtp::…` either way.
- `test_support.rs`: how every app-side MTP cell reaches a virtual device, over the parked manager. `DETAILS.md` lists
  where those cells sit and why.

## Must-knows

- **Hotplug events are a TRIGGER, never the source of truth.** ❌ Don't auto-connect off a `HotplugEvent` payload: that
  watch is USB-only, so E2E's virtual device would never connect. Every event funnels into
  `check_for_device_changes()`, which re-enumerates and diffs `KNOWN_DEVICES`.
- **The MTP-on bit lives on the MANAGER** (`set_enabled` / `is_enabled`, default on). The app pushes the persisted
  setting in; `watcher.rs` reads it back to gate auto-connect, never the watcher loop. Key `fileOperations.mtpEnabled`.
- **Identity lives in `cmdr_fs::volume::mtp_ids`.** `device_id_for(serial, location_id)` prefers the serial (stable
  across a replug to ANY port, so the index re-matches), else the `location_id`. Volume id =
  `{device_id}:{storage_id}`, both halves OPAQUE: ❌ never `split(':').nth(1)`, ALWAYS `split_volume_id` /
  `device_id_of_volume` / `storage_id_of_volume` (rsplit on the LAST `:`; TS mirrors it with `lastIndexOf(':')`).
- **❌ The session layer never registers volumes.** `connect()` attaches storages through its `MtpVolumeRegistrar`
  (`volume_wiring::volume_registrar`), synchronously: the attach must finish before the event loop starts. ❌ The
  `volumes-changed` broadcast lives in that hook and nowhere else. New backends copy this.
- **`delete` has two scopes; only `delete_mtp_object` may recurse.** `MtpVolume::delete` passes
  `MtpDeleteScope::SingleNode`, so a folder with children is refused (`DirectoryNotEmpty`) and nothing is deleted. ❌
  Never widen a caller to `Tree`: the same-volume move's "a Skipped child keeps its only copy" guarantee IS that
  refusal.
- **macOS ptpcamerad suppression** runs before connecting and is restored when the last device leaves, on exit, or on
  MTP being disabled; a failed one falls back to the `ExclusiveAccess` dialog. ❌ `needs_ptpcamerad_suppression` keeps it
  off an all-VIRTUAL device set: an E2E run once took `ptpcamerad` down on the developer's machine.
- **Error events the frontend depends on**: `mtp-exclusive-access-error` (ptpcamerad still holds the device, carrying
  the blocking process name from `ioreg`, `None` on Linux) and `mtp-permission-error` (Linux udev rules missing →
  `MtpPermissionDialog`).

Hotplug reconciliation, the delete-scope decision, ptpcamerad mechanics, the registrar decision, and the virtual
device's E2E gating: `DETAILS.md`. Read it before any non-trivial work here.
