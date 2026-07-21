# MTP module

MTP (Media Transfer Protocol) for Android devices and PTP cameras over USB. macOS and Linux only
(`#[cfg(any(target_os = "macos", target_os = "linux"))]`). On Linux, users may need udev rules for USB permissions
(`resources/99-cmdr-mtp.rules`).

Frontend counterpart: [`apps/desktop/src/lib/mtp/CLAUDE.md`](../../../src/lib/mtp/CLAUDE.md) (connection toast, storage
picker, reactive volume state). The frontend is a passive consumer: it subscribes to `volumes-changed` and
`mtp-device-connected` / `mtp-device-disconnected`; it never orchestrates connections.

## File map

- `mod.rs`: re-exports + module doc. `types.rs`: `MtpDeviceInfo`, `MtpStorageInfo` (camelCase JSON); `usb_speed` mirrors
  `mtp_rs::UsbSpeed` via `crate::usb_speed::UsbSpeed`.
- `discovery.rs`: `list_mtp_devices()`; device IDs via `identity::device_id_for` (see Must-knows).
- `identity.rs`: device/volume id derivation (`device_id_for`) and the ONE robust parser (`split_volume_id` and friends).
- `watcher.rs`: hotplug watcher over `mtp_rs::mtp::watch_devices()`; auto-connect/disconnect; owns the `MTP_ENABLED`
  gate.
- `macos_workaround.rs` (macOS-only): ptpcamerad suppression (see below).
- `connection/`: per-device session layer (`MtpConnectionManager` singleton, connect/disconnect, event loop, list / read
  / write / mutate / bulk ops). See [`connection/CLAUDE.md`](connection/CLAUDE.md) for locks, caches, and gotchas.
- `virtual_device.rs`: virtual MTP device for E2E + dev, gated behind the `virtual-mtp` feature; dev opt-in
  `CMDR_VIRTUAL_MTP=1 pnpm dev`. See [`docs/tooling/virtual-mtp.md`](../../../../../docs/tooling/virtual-mtp.md).

## Must-knows

- **Hotplug events are a TRIGGER, never the source of truth.** ❌ Don't auto-connect off a `mtp_rs` `HotplugEvent`
  payload: that watch is USB-only, so E2E's virtual device would never connect. Every event funnels into
  `check_for_device_changes()`, which re-enumerates and diffs `KNOWN_DEVICES`. Why the initial `Arrived` burst can't
  double-connect: DETAILS.md.
- **`MTP_ENABLED` (`AtomicBool`, default `true`, in `watcher.rs`) gates all auto-connect.** The watcher loop always runs
  (`OnceLock`, no shutdown channel); `check_for_device_changes()` returns early when disabled. Setting key:
  `fileOperations.mtpEnabled` in `settings.json`, read by `settings/loader.rs` at startup.
  - `set_mtp_enabled_flag(bool)`: sets the flag, no side effects; called before `start_mtp_watcher()` so startup respects
    the persisted setting.
  - `set_mtp_enabled(bool)`: the Tauri-command path. Disabling disconnects all devices, clears `KNOWN_DEVICES`, and
    restores ptpcamerad (macOS); enabling re-runs `check_for_device_changes()`.
- **Write-capability probe.** `probe_write_capability()` creates a hidden `.cmdr_write_probe` folder to detect cameras
  that advertise write support but reject writes (`StoreReadOnly`). Timeout or non-fatal errors are treated as writable
  (benefit of the doubt).
- **macOS ptpcamerad suppression.** The watcher auto-suppresses `ptpcamerad` (`launchctl disable` + `pkill -9`) before
  connecting, restores it when all devices disconnect or on exit, and runs `ensure_ptpcamerad_enabled()` at startup for
  crash recovery. If suppression fails, the `ExclusiveAccess` dialog is the manual fallback. Disabling MTP calls
  `restore_ptpcamerad_unconditionally()`.
- **Error events the frontend depends on:** `mtp-exclusive-access-error` (ptpcamerad still holds the device; carries the
  blocking process name from `ioreg`, `None` on Linux), `mtp-permission-error` (Linux missing udev rules → frontend shows
  `MtpPermissionDialog` with the install command).
- **Identity (`identity.rs`).** Device id = `device_id_for(serial, location_id)`: `mtp-{serial}` when the device reports
  a serial (stable across a replug to ANY port, so the index re-matches), else `mtp-{location_id}` (same-port-only).
  Volume id = `{device_id}:{storage_id}` (e.g. `mtp-336592896:65537`). ❌ A serial CAN contain `:`, so NEVER parse a
  volume id with `split(':').nth(1)` / `split_once(':')` (they break on a colon in the serial): the storage id is the
  trailing numeric tail, so ALWAYS go through `identity::split_volume_id` / `device_id_of_volume` /
  `storage_id_of_volume` (rsplit on the last `:`). The TS side (`FilePane`, `mtp-path-utils`) mirrors this with
  `lastIndexOf(':')`. The device id is OPAQUE — `connect()` resolves it to a `location_id` by matching the live
  enumeration (`resolve_device_location_id`), never by decoding it.
- **Cancel propagation bails at the next per-USB-roundtrip boundary** (per-handle in `ObjectListing::next`), driven by
  an `Arc<AtomicBool>` (`WriteOperationState.backend_cancel`, `StreamingListingState.cancelled`) wrapped as an
  `mtp_rs::CancelToken`. It's the ONLY safe way to stop an MTP op early: ❌ never a `tokio::time::timeout` or a task
  abort, which drop the future mid-transaction and wedge the phone (`connection/CLAUDE.md`, enforced by
  `pnpm check mtp-dropping-timeout`). Don't switch list/delete to PTP `CancelTransaction` (rationale in DETAILS.md).

Full details (data-flow diagram, virtual-device activation gating, cancel-propagation wiring, why-not-CancelTransaction,
hardware caveats, dependencies): [DETAILS.md](DETAILS.md).
