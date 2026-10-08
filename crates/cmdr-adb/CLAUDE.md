# `cmdr-adb`

Everything Cmdr says to an Android device over ADB: one `Volume` per attached device, rooted at `adb://<serial>`, spoken
to the ADB **server** on loopback (never USB itself). The device-side twin of `cmdr-sftp`. No `tauri`, no user-facing
words.

## Module map

- `server.rs` (endpoint + the one `start-server` attempt), `transport.rs` (the ONLY module that knows the wire framing),
  `devices.rs` (`host:devices-l` + the `host:track-devices` hotplug stream), `features.rs` (read once per session),
  `sync.rs` (`STAT`/`LIST`/`RECV`/`SEND`), `shell.rs` (`mkdir`/`rm`/`mv`/`cp`/`df`), `errors.rs`, `params.rs`,
  `testing/` (the fake ADB server), `fuzzing.rs` (the fuzz targets' entry points, `fuzzing` feature).
- `volume/`: the `Volume` impl by job: `paths`, `query`, `streams`, `writes`, `mapping`, `state`, `volume_impl`,
  `testing`, `index_scope` (which directories a drive-index walk descends), plus `scan` and `mutation`, which are just
  this backend's `ScanSource` / `PatchSource` impls.

## Must-knows

- **❗ Host framing (hex lengths, `OKAY`/`FAIL`) lives only in `transport.rs`**; `sync.rs` / `shell.rs` decode the
  device's packets through its raw helpers. **Every device-chosen length is capped BEFORE its buffer exists**
  (`MAX_DATA_CHUNK`, `MAX_FRAME_PAYLOAD`), or a hostile phone allocates 4 GiB. DETAILS § "Wire contract".
- **❗ The peer is the server, not the phone.** `adb` owns USB and pairing; a refused loopback connect earns exactly one
  `adb start-server` per process, ❌ never a retry loop that spawns processes.
- **❗ `shell_v2` is required; a device without it is `AdbConnectError::DeviceTooOld`.** Legacy `shell:` has no exit
  code, and output is never parsed for one.
- **❗ A shell failure is classified by a follow-up stat of the path and its parent, ❌ never by stderr.** The exit code
  says "no"; the sync service says why. `DETAILS.md` § "The error policy".
- **❗ `NotFound` / `PermissionDenied` carry the PATH**, ❌ never the device's wording: the frontend renders it as the
  missing file's name. `errors::volume_error_from_errno` takes the path for that reason.
- **❗ Paths are `adb://<serial>/…` both ways; `volume/paths.rs` is the ONLY translation.** ❌ A bare `/sdcard` or a
  `..` above `/` is refused, never anchored.
- **❌ Never collect a file into a `Vec<u8>`.** `RECV`/`SEND` stream chunkwise; `read_range` runs bounded, quoted
  device-side `dd`.
- **❗ Every write lands under a staging name (`<name>.cmdr-tmp-<pid>-<n>`) and is `mv -f`ed into place.** `SEND`
  accepts known or unknown length but truncates on open, so direct writes stage.
- **❗ Every mutation calls `notify_mutation`.** There is no watcher; `can_watch_listings` is `false` and stays so.
- **❗ The walk and the listing patch come from `cmdr_fs::volume::{scan_walk, patching}`**, ❌ never a copy here. But ❌
  do NOT adopt `MakesDirectories`: the shell's native `mkdir -p` is one verb at any depth, where that walk costs a
  request per level. `DETAILS.md` has the reasoning.
- **❗ One sync socket per operation, ❌ no shared session behind a mutex**: a same-volume copy would deadlock and a
  paused transfer would park every listing. `max_concurrent_ops` (1, the app's `"adb"` settings row) is what bounds
  transfers.
- **❗ `host:track-devices` is the hotplug channel AND the retirement signal**: `track_devices` refetches on every push
  and reconnects with backoff, but ENDS on `AdbNotInstalled` (a retry would warn all session on every machine without
  Android tooling); the app retires a departed serial's volume and revives a stopped tracker through
  `forget_start_attempt`. Operations remain the liveness detector in between.
- **❗ Features are read once at connect** (`DeviceFeatures::fetch`). ❌ Never re-probe at a call site.
- **❗ Report transitions, never states** (`volume/state.rs`); a retired volume reports nothing.
- **❌ Never `cfg(test)`-gate a fixture; use `any(test, feature = "testing")`.** The app's ADB suites share
  `testing::FakeAdbServer` and `volume::testing`.

The wire contract, the `Volume` answers, the error policy, testing, and known gaps: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
