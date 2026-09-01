# Android over ADB: a real-filesystem backend beside MTP

**Problem.** MTP shows the curated media tree a phone chooses to expose. Developers want the real filesystem: `/data/local/tmp`, `/sdcard` as the kernel sees it, app sandboxes on a rooted or debuggable device, and the speed of the `adb sync` protocol instead of PTP object handles. `adb` is already on every Android developer's machine.

**Shape.** A device-anchored `Volume`, the same shape MTP has (`rerooted` → `None`, `lane_key` = serial, `max_concurrent_ops` = 1), built as a crate the way SFTP was (`crates/cmdr-adb`, no `tauri`, no user-facing words). It talks to the **ADB server** on `127.0.0.1:5037`, never to USB itself.

**The new seam it needs.** `volume_listing::complete` hardcoded MTP as the only source of device volumes, and nothing outside `mtp/` could say "a device appeared". This development adds `device_volumes.rs`: a provider registry the listing folds over, plus the one function a provider calls on hotplug (`volume_broadcast::emit_volumes_changed`). MTP becomes the first provider; ADB the second. `host:track-devices` is the push channel ADB gets that MTP never had.

## Wire contract (what the crate implements)

The protocol the crate speaks (host framing, `host:track-devices`, the sync service's `STAT`/`STA2`, `LIST`/`LIS2`, `RECV`/`RCV2`, `SEND`/`SND2` packets, `shell,v2,raw:` framing and the verbs, the typed errors) is canonical in `crates/cmdr-adb/DETAILS.md` § "The wire contract" and § "The error policy", with the AOSP evidence anchor. Nothing here restates it.

## Volume contract

- **Identity**: `volume_id` = `adb:<serial>`; root `adb://<serial>` shows the device's `/`. Paths relative to the root are absolute device paths; `root_anchored` is idempotent so a pane's `adb://<serial>/sdcard/DCIM` and a dest box's `/sdcard/DCIM` land on the same file. ❌ Never anchor an out-of-root path; refuse it.
- **Device-anchored answers**: `rerooted` → `None` (one volume per device, the pane's path is inside it); `lane_key` = `LaneKey::Device(serial)` or the device-shaped variant the trait offers; `max_concurrent_ops` = 1 (one sync socket at a time is what a phone tolerates; `host:transport` sockets are cheap but the device's `adbd` serializes I/O anyway).
- **Capabilities**: `supports_export` true, `is_writable` true (a read-only mount answers `ReadOnly` per path when the shell says so), `supports_streaming` true, `can_watch_listings` false (`listing_watch_coverage` → `None`; the pane refreshes on `notify_mutation`), `supports_local_fs_access` false, `paths_are_os_visible` false, `operations_are_local` false, `create_directory_errors_on_existing_dir` false (`mkdir -p`).
- **Streams**: `open_read_stream` is one `RECV` socket per file, chunk by chunk; `write_from_stream` is one `SEND` socket per file into the staging name, then `mv`. ❌ Never collect a file into a `Vec<u8>`.
- **Mutation**: every write path calls `notify_mutation`; there is no watcher.
- **Liveness**: like SFTP, operations are the detector; `track-devices` additionally retires the volume when its serial leaves the list.
- **Space**: `df -k` on the volume root, polled at `space_poll_interval` = 30 s.

## App wiring

- `device_volumes.rs` (new): `DeviceVolumeProvider` trait + registry + `append_device_volumes` + `device_volume_for_path`. `volume_listing::complete` folds over it. `mtp/` registers `MtpDeviceProvider`; `adb/` (new app module) registers `AdbDeviceProvider`, owns the tracker task, and calls `volume_broadcast::emit_volumes_changed` on every device-list change.
- `apps/desktop/src-tauri/src/adb/`: the app-side half (tracker task, `AdbDeviceProvider`, lazy connect on first navigation, IPC commands). The crate is reached from there directly; there is no `backends/adb.rs` re-export.
- `file_system/backend_settings.rs`: `"adb"` reads the constant 1.
- Eject: an ADB entry has nothing to unmount; "eject" retires and unregisters the volume, and the device stays listed while `adb` still sees it (`adb` has no per-client detach). Documented in `adb/DETAILS.md`.
- IPC: `list_adb_devices`, `connect_adb_device(serial)`; `adb-devices-changed` rides the existing `volumes-changed`.
- Frontend: recognize `adb://` next to `mtp://` in the path helpers and `volume-capabilities.ts`; category `MobileDevice`, `fs_type: "adb"`; the switcher shows the device's model name with an "ADB" suffix when the same phone is also listed over MTP.

## Cost to finish (after this development)

- [ ] Real-device pass: authorize prompt, `unauthorized` → `device` transition mid-session, a 2 GB `RECV`/`SEND`, a `/data` listing on a non-rooted phone (expect `PermissionDenied` with the path).
- [ ] `sendrecv_v2` compression flags (brotli/lz4/zstd): off on purpose; measure before enabling.
- [ ] Wireless debugging (`adb pair`) is out of scope: the server owns pairing, the backend sees the device the same way.
- [ ] A settings switch for the `adb` binary path when it's not on `PATH` / in `$ANDROID_HOME`, and a `fileOperations.adbEnabled` toggle beside the MTP one.
- [ ] `crates/cmdr-index` path routing for `adb://` (`indexing/paths/routing.rs`, `IndexVolumeKind`): until then an ADB volume isn't indexed.
- [ ] Frontend connect flow: nothing calls `connectAdbDevice` / `adbConnectErrorMessage` yet (the pane's `adb://` navigation connects lazily on the Rust side; a device picker and the `unauthorized` prompt need a surface). After `pnpm bindings:regen`, delete the `TODO(bindings-shim)` block in `tauri-commands/adb.ts`.
- [ ] `go_to_path` scheme short-circuit and MCP `select_volume` via `volume_listing::complete`, so `adb://` paths resolve the way `mtp://` ones do.
- [ ] Full `pnpm check` and `pnpm check --include-slow` locally; this branch was built in a cloud environment where the crate suites ran but the app-side lanes couldn't all be exercised.
