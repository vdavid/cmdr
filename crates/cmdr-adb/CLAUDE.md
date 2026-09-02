# `cmdr-adb`

Everything Cmdr says to an Android device over ADB: one `Volume` per attached device, rooted at the device's real `/`,
spoken to the ADB **server** on loopback (never USB itself). The device-side twin of `cmdr-sftp`. No `tauri`, no
user-facing words.

## Module map

- `server.rs` (endpoint + the one `start-server` attempt), `transport.rs` (the ONLY module that knows the wire framing),
  `devices.rs` (`host:devices-l` + the `host:track-devices` hotplug stream), `features.rs` (read once per session),
  `sync.rs` (`STAT`/`LIST`/`RECV`/`SEND`), `shell.rs` (`mkdir`/`rm`/`mv`/`cp`/`df`), `errors.rs`, `params.rs`,
  `testing/` (the fake ADB server: `tree` the filesystem model, `server` the listener and wire, `shell` the device-shell
  verbs).
- `volume/`: the `Volume` impl by job: `paths`, `query`, `streams`, `writes`, `scan`, `mutation`, `mapping`, `state`,
  `volume_impl`, `testing`.

## Must-knows

- **❗ Keep the framing in `transport.rs`.** Hex-length requests, `OKAY`/`FAIL`, sync packets: one module, so a protocol
  change stays one file's problem.
- **❗ The peer is the server, not the phone.** `adb` owns USB and pairing; a refused loopback connect earns exactly one
  `adb start-server` per process, ❌ never a retry loop that spawns processes.
- **❗ `shell_v2` is required; a device without it is `AdbConnectError::DeviceTooOld`.** Legacy `shell:` has no exit
  code, and inferring one from output would be string-matching control flow.
- **❗ A shell failure is classified by a follow-up stat of the path and its parent, ❌ never by stderr.** The exit code
  says "no"; the sync service says why. `DETAILS.md` § "The error policy".
- **❗ `NotFound` / `PermissionDenied` carry the PATH**, ❌ never the device's wording: the frontend renders it as the
  missing file's name. `errors::volume_error_from_errno` takes the path for that reason.
- **❌ Never anchor an out-of-root path; refuse it.** `root_anchored` is idempotent; a pane's `adb://<serial>/sdcard`
  and a dest box's `/sdcard` are the same file.
- **❌ Never collect a file into a `Vec<u8>`.** `RECV` and `SEND` are one socket per file, chunk by chunk.
- **❗ Every write lands under a staging name (`<name>.cmdr-tmp-<pid>-<n>`) and is `mv -f`ed into place.** `SEND`
  truncates on open, so a direct write is a torn file the moment the cable pulls.
- **❗ Every mutation calls `notify_mutation`.** There is no watcher; `can_watch_listings` is `false` and stays so.
- **❗ One sync socket per operation, ❌ no shared session behind a mutex**: a same-volume copy would deadlock and a
  paused transfer would park every listing. `max_concurrent_ops` (1, the app's `"adb"` settings row) is what bounds
  transfers.
- **❗ `host:track-devices` is the hotplug channel AND the retirement signal**: `track_devices` refetches the long list
  on every push and reconnects with backoff, but ENDS on `AdbNotInstalled` (no binary, nothing to reconnect to, and a
  retry would warn all session on every machine without Android tooling); the app retires the volume of a serial that
  left, and revives a stopped tracker through `forget_start_attempt`. Operations remain the liveness detector in
  between, like SFTP.
- **❗ Features are read once at connect** (`DeviceFeatures::fetch`). ❌ Never re-probe at a call site.
- **❗ Report transitions, never states** (`volume/state.rs`); a retired volume reports nothing.
- **❌ Never `cfg(test)`-gate a fixture; use `any(test, feature = "testing")`.** The app's ADB suites share
  `testing::FakeAdbServer` and `volume::testing`.

The wire contract, the `Volume` answers and why, the error policy, the testing story, and the known gaps: `DETAILS.md`.
Read it first.
