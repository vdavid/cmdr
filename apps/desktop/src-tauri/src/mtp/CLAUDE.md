# MTP module

MTP for Android devices and PTP cameras over USB. macOS and Linux only; on Linux, USB permissions may need udev rules
(`resources/99-cmdr-mtp.rules`). The frontend (`src/lib/mtp/CLAUDE.md`) is a passive consumer of `volumes-changed` and
`mtp-device-connected` / `-disconnected`; it never orchestrates connections.

## File map

- `discovery.rs` (`list_mtp_devices()`), `watcher.rs` (hotplug over `mtp_rs::mtp::watch_devices()`, auto-connect, the
  `MTP_ENABLED` gate), `types.rs` (camelCase JSON), `macos_workaround.rs` (ptpcamerad suppression).
- `connection/`: the per-device session layer (`MtpConnectionManager` singleton, event loop, list / read / write /
  mutate / bulk ops). See its `CLAUDE.md` for locks, caches, and gotchas.
- `events.rs`: the seven `tauri_specta` payload structs and the adapter mapping the session layer's typed
  `MtpDeviceEvent`s onto five of them. The struct name kebab-cases to the wire event name.
- `volume_wiring.rs` registers a storage as an `MtpVolume` (twin of `network/smb_upgrade.rs`); `virtual_device.rs` is
  the E2E and dev device behind the `virtual-mtp` feature (`docs/tooling/virtual-mtp.md`).

## Must-knows

- **Hotplug events are a TRIGGER, never the source of truth.** ❌ Don't auto-connect off an `mtp_rs` `HotplugEvent`
  payload: that watch is USB-only, so E2E's virtual device would never connect. Every event funnels into
  `check_for_device_changes()`, which re-enumerates and diffs `KNOWN_DEVICES`.
- **`MTP_ENABLED` (`AtomicBool` in `watcher.rs`, default `true`) gates auto-connect, never the watcher loop itself.**
  Setting key `fileOperations.mtpEnabled`.
- **`delete` has two scopes; only `delete_mtp_object` may recurse.** `MtpVolume::delete` passes
  `MtpDeleteScope::SingleNode`, so a folder with children is refused (`DirectoryNotEmpty`) and nothing is deleted. ❌
  Never widen a caller to `Tree`: the same-volume move's "a Skipped child keeps its only copy" guarantee IS that
  refusal.
- **Identity lives in `cmdr_fs::volume::mtp_ids`.** `device_id_for(serial, location_id)` prefers the serial (stable
  across a replug to ANY port, so the index re-matches), else the `location_id`. Volume id =
  `{device_id}:{storage_id}`, both halves OPAQUE: ❌ never `split(':').nth(1)`, ALWAYS `split_volume_id` /
  `device_id_of_volume` / `storage_id_of_volume` (rsplit on the LAST `:`; TS mirrors it with `lastIndexOf(':')`).
- **❌ The session layer never registers volumes.** `connect()` attaches storages through the `OnceLock` registrar in
  `connection/volume_registrar.rs`, installed at startup by `volume_wiring.rs`. Keep it synchronous: the attach must
  finish before the event loop starts. New backends copy this.
- **Cancel propagation bails at the next per-USB-roundtrip boundary** (`MtpCancelBridge` bridges a
  `CancellationToken`). It's the ONLY safe early stop: ❌ never a `tokio::time::timeout` or a task abort, which drop
  the future mid-transaction and wedge the phone (`pnpm check mtp-dropping-timeout`), and ❌ never PTP
  `CancelTransaction` for list/delete.
- **macOS ptpcamerad suppression** runs before connecting, and is restored when the last device leaves, on exit, or on
  MTP being disabled; `ensure_ptpcamerad_enabled()` at startup covers a crash. A failed one falls back to the
  `ExclusiveAccess` dialog. `needs_ptpcamerad_suppression` keeps it off a device set that's only VIRTUAL: disabling a
  real macOS daemon for a fixture is how an E2E run took `ptpcamerad` down on the developer's machine.
- **The session layer holds no `AppHandle`.** It reports typed `MtpDeviceEvent`s into an `MtpDeviceEvents` sink and
  `events.rs` maps them; ❌ don't reach for an app handle inside `connection/`. The `ptpcamerad` pair stays the
  watcher's to emit.
- **Error events the frontend depends on**: `mtp-exclusive-access-error` (ptpcamerad still holds the device; carries
  the blocking process name from `ioreg`, `None` on Linux) and `mtp-permission-error` (Linux udev rules missing →
  `MtpPermissionDialog`).

Data flow, virtual-device gating, the write-capability probe, cancel wiring, why-not-`CancelTransaction`, and hardware
caveats: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
