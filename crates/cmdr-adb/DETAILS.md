# `cmdr-adb` details

The depth behind `CLAUDE.md`: the shape of the crate, the wire contract it implements, what the `Volume` impl answers
and why, the error policy, where a test lives, and what is deliberately not done yet. The app-side half (the tracker
task, the device provider, eject, the IPC commands) is `apps/desktop/src-tauri/src/adb/DETAILS.md`; the design that
started this development is `docs/specs/android-adb-backend.md`.

## Why a second Android backend

MTP shows the curated media tree a phone chooses to expose, through PTP object handles. Developers want the real
filesystem: `/data/local/tmp`, `/sdcard` as the kernel sees it, an app sandbox on a rooted or debuggable device, and the
throughput of the `adb sync` protocol. `adb` is already on every Android developer's machine, so this backend adds no
dependency of its own: it is a TCP client of a server that is already running.

## The connection model

Cmdr talks to the **ADB server** (`127.0.0.1:5037`, or `$ANDROID_ADB_SERVER_PORT`), never to the device. The server owns
USB, authorization, and wireless pairing; a client only ever asks it for a socket to a device. That has three
consequences the code is shaped around:

- **Starting the server is a one-shot.** `server::AdbEndpoint::connect` answers a refused loopback connect with one
  `adb -P <port> start-server` per process, through the binary `server::locate_adb_binary` finds (`$ADB`, `$PATH`,
  `$ANDROID_HOME/platform-tools`, `$ANDROID_SDK_ROOT/platform-tools`, `~/Library/Android/sdk/platform-tools`, then the
  Homebrew and `/usr/local` bins). No binary is `AdbConnectError::AdbNotInstalled`; a second refusal after a start is
  `ServerUnreachable`, and both are the user's to look at. `AdbEndpoint::at(addr)` (fixtures, a forwarded port) never
  starts anything.
- **One socket per operation, no shared session.** Every sync operation opens its own `sync:` socket and closes it with
  the operation. A shared session behind a mutex deadlocks a same-volume copy (the source stream holds it while the
  destination write waits) and parks every listing while a transfer is paused mid-file. The server multiplexes sockets
  and `adbd` serializes I/O on its side anyway, so nothing is gained by holding one. What bounds concurrent transfers is
  `max_concurrent_ops`, below.
- **A connect is four phases under one budget and one `CancellationToken`** (`volume::connect_adb_volume`): the device
  list (a serial that isn't there is `DeviceGone`, one in `unauthorized` is `Unauthorized`), the feature probe (no
  `shell_v2` is `DeviceTooOld`), then the hello (one `STAT` of `/`). A cancel ends the attempt where it stands and
  leaves nothing behind: no volume, no socket. `CONNECT_BUDGET` is 10 s, all phases together.

## The wire contract

This section is the canonical account of what the crate implements; the spec points here. The protocol is what
`adb/SERVICES.TXT`, `adb/SYNC.TXT`, and `adb/protocol.txt` in AOSP describe (verified against platform-tools 35 by
reading those files and exercising a fake server built from them, 2026-09-01).

**Host framing** (`transport.rs`, the ONLY module that knows this): a request is four ASCII hex digits of length then
the payload (`000Chost:version`). The answer is `OKAY` or `FAIL`; `FAIL` is followed by a four-hex-length message.
`host:version` and `host:devices-l` answer `OKAY` then one four-hex-length payload. `host:transport:<serial>` answers
`OKAY` and binds the socket to that device; the next request on it is a device service (`sync:`, `shell,v2,raw:<cmd>`).
`host-serial:<serial>:features` returns the device's comma-separated feature list.

**Hotplug** (`devices.rs`): `host:track-devices` answers `OKAY` and then, for the life of the socket, a four-hex-length
device list on every change (the SHORT `host:devices` format, `serial\tstate` per line; states are `device`,
`unauthorized`, `offline`, `recovery`, `sideload`, `bootloader`, `no permissions`, `connecting`, `authorizing`). Only
`device` (`AdbDeviceState::Ready`) is usable; the enum carries the rest as typed values so the app can word them.
`track_devices` refetches `host:devices-l` on every push (the long format carries `product`, `model`, `device`,
`transport_id`), hands the callback the full list, and when the socket drops reconnects with backoff (1 s doubling,
capped at 15 s), redelivering the list after each reconnect so a listener catches up.

**Sync service** (`sync.rs`): after `sync:` → `OKAY`, binary little-endian packets `[id: 4 ASCII][arg: u32 LE]`.

- `STAT` + len + path → `STAT` + mode + size + mtime (all u32; mode 0 = doesn't exist). `STA2` (feature `stat_v2`) →
  `STA2` + error(u32) + dev(u64) + ino(u64) + mode(u32) + nlink(u32) + uid + gid + size(u64) + atime + mtime + ctime
  (i64 each). `STA2` is preferred: it carries errno and 64-bit sizes, which is what makes the error policy possible.
- `LIST` + len + path → repeated `DENT` + mode + size + mtime + namelen + name, then `DONE` + 16 zero bytes. `LIS2`
  (feature `ls_v2`) → repeated `DNT2` + error + dev + ino + mode + nlink + uid + gid + size + atime + mtime + ctime +
  namelen + name, then `DONE`.
- `RECV` + len + path → repeated `DATA` + len + bytes (≤ 64 KiB each), then `DONE` + 0; or `FAIL` + len + message.
  `RCV2` (feature `sendrecv_v2`) sends `RCV2` + len + path, then `RCV2` + flags(u32) before data. There is no ranged
  read on the wire: `open_read_stream_at_offset` reads and discards up to the offset (§ "Known gaps").
- `SEND` + len + `path,mode` → then repeated `DATA` + len + bytes, then `DONE` + mtime(u32) → `OKAY` + 0 or `FAIL` +
  len + message. `SND2` (feature `sendrecv_v2`) sends `SND2` + len + path then `SND2` + mode(u32) + flags(u32), then the
  same data stream. `SEND` truncates on open, which is why every write here stages (§ "The `Volume` answers").

**Shell** (`shell.rs`): `shell,v2,raw:<cmd>` (feature `shell_v2`) frames stdout, stderr, and the exit code as packets
`[id: u8][len: u32 LE][payload]` with ids `0` stdin, `1` stdout, `2` stderr, `3` exit (payload one byte). A device
without `shell_v2` (pre-Android 7, 2016) is refused with `AdbConnectError::DeviceTooOld` rather than guessed at: the
legacy `shell:` service has no exit code, and inferring one from output would be string-matching control flow. Every
argument is single-quoted (`'` → `'\''`). Verbs (`volume/writes.rs`): `mkdir -p`, `rmdir` for a directory and `rm -f`
for anything else (strictly one node, so `delete` never recurses), `mv -f`, `cp -f` for `copy_within`, and
`df -k <path>` (space info; parsed as data, never as an error signal). Shell reference: `adb/shell_protocol.h`
(platform-tools 35).

## The `Volume` answers, and why

The volume is device-anchored, the same shape MTP has, and every answer below follows from that.

- **Identity**: `volume_id` = `cmdr_fs::volume::adb_volume_id(serial)` (`adb:<serial>`); the root is the device's `/`.
  Paths relative to the root ARE device paths, so nothing translates between two spellings of one tree. `root_anchored`
  is idempotent: a pane's `adb://<serial>/sdcard/DCIM` and a dest box's `/sdcard/DCIM` land on the same file. ❌ Never
  anchor an out-of-root path; refuse it.
- **`rerooted` → `None`.** One volume per device; the pane's path is inside it. A device has no second root to offer.
- **`lane_key` → the serial.** Two panes on one phone contend on one `adbd`, so they share a lane and the operation
  manager serializes their writes.
- **`max_concurrent_ops` reads `settings()` per batch dispatch**, and the app's `MAX_CONCURRENT_OPERATIONS_SOURCES`
  table has an `"adb"` row answering the constant 1. ❗ A namespace with no row silently gets a cautious 2, which is why
  the row exists even though it is not a user-facing knob. `adbd` serializes I/O per device, so a second concurrent
  transfer only adds contention.
- **`supports_export` → true, `is_writable` → true, `supports_streaming` → true.** Every read and write path is
  implemented; the conformance assertions hold each declaration to what the device accepts. A read-only mount answers
  `ReadOnly` per path when the shell's `EROFS` says so, not volume-wide.
- **`can_watch_listings` → false, `listing_watch_coverage` → `None`.** There is no watcher, so ❌ nothing here may claim
  an authoritative listing; the pane stays honest through `notify_mutation`, called once per changed directory by every
  mutation, `write_from_stream` included.
- **`supports_local_fs_access`, `paths_are_os_visible`, `operations_are_local` → false; `local_path` → `None`.** Nothing
  on the host can open a device path.
- **`create_directory_errors_on_existing_dir` → false.** `mkdir -p` is the verb, and it is idempotent by design.
- **Streams**: `open_read_stream` is one `RECV` socket per file, chunk by chunk; `write_from_stream` is one `SEND`
  socket per file into a staging sibling (`<dir>/<name>.cmdr-tmp-<pid>-<n>`, the house `STAGING_TEMP_MARKER`, so a
  leftover is filtered from every pane), then `mv -f` via the shell; any failure removes the staging name. ❌ Never
  collect a file into a `Vec<u8>`.
- **Two accepted TOCTOU windows**: `create_file` and a `force = false` rename each `stat` the destination first and
  refuse on a hit. Neither can be atomic on this protocol: `SEND` truncates unconditionally and `mv -n` exits 0 whether
  it moved or not (verified on Android 14 `toybox 0.8.9`, 2026-09-01). The window is one round trip on a device only
  this host writes to, and `conformance_test.rs` holds the refusal itself.
- **Liveness**: operations are the detector (there is no keepalive), so every wire-touching delegator classifies a
  `DeviceGone` into `VolumeError::DeviceDisconnected` and emits the transition once (`state.rs`). `track-devices`
  additionally retires the volume when its serial leaves the list, which is the push channel MTP never had.
- **Space**: `df -k` on the volume root, polled at `space_poll_interval` = 30 s. A `df` that fails is `NotSupported`
  ("can't tell"), ❌ never a guessed number.

## The error policy

Typed only. `AdbError` is the transport's failure shape (`Io`, `Refused`, `Protocol`, `DeviceGone`, `Timeout`,
`Cancelled`); `AdbConnectError` is why a connect didn't produce a volume (`AdbNotInstalled`, `ServerUnreachable`,
`DeviceGone`, `Unauthorized`, `DeviceTooOld`, `TimedOut`, `Cancelled`, `Transport`). The one variant carrying the
server's text (`Refused`) is a log diagnostic; ❌ branching on it is what `error-string-match` forbids.

**The device's errno is the classifier.** `STA2` and `DNT2` carry a Linux errno, and `errors::volume_error_from_errno`
maps it to the `Volume` vocabulary for an operation on `path` in one place: `ENOENT` → `NotFound(path)`,
`EACCES`/`EPERM` → `PermissionDenied(path)`, `EEXIST` → `AlreadyExists(path)`, `EISDIR` → `IsADirectory(path)`,
`ENAMETOOLONG` → `InvalidName(path)`, `ENOTEMPTY` → an `IoError` carrying the HOST's `ENOTEMPTY` number (the device
numbers it 39, macOS 66; the app's classifier re-dispatches on `raw_os_error`, so the translation happens here, as
`cmdr-sftp` does it), `EROFS` → `ReadOnly(path)`, `ENOSPC` → `StorageFull`. Anything else is an `IoError` with the raw
number. `volume_error_from_adb` maps the transport's shapes (`DeviceGone` → `DeviceDisconnected`, `Timeout` →
`ConnectionTimeout`, `Cancelled` → `Cancelled`); a `FAIL` text from the sync service lands as an unclassified `IoError`
with the text in `message` for the log.

**A shell failure is classified by a follow-up probe, never by stderr.** `shell,v2` gives an exit code, which says "no"
and nothing else; `mkdir`, `rm`, and `mv` print their reason to stderr in `toybox`'s wording, which is for people and
may be localized. So a non-zero exit is read through what the sync service says is at the path and its parent
(`AdbVolume::classify_failed_verb` in `writes.rs`): parent missing → `NotFound(parent)`; parent there but not writable
(`test -w`) → `PermissionDenied(path)`; anything else → an `IoError` carrying stderr for the technical-details panel.
The probe classifies, ❌ never guards: asked before, it is a TOCTOU window (the two the backend accepts on purpose are
listed under the `Volume` answers).

**What a variant carries.** `VolumeError::NotFound` and `PermissionDenied` are defined to carry the PATH
(`crates/cmdr-fs/src/volume/types.rs`), and the transfer layer forwards it straight into what the frontend renders as
the missing file's name. The mapper takes the path it is mapping a failure for, so a pathless `NotFound` is not
constructible here. Held by `conformance::assert_not_found_carries_the_path`.

## Which side a test lives on

A cell lives with whatever it **asserts**, never with whatever it connects to.

- **Here**: the framing, the sync and shell codecs, the errno table, path anchoring, the connect phases and calling one
  off, the state transitions, and the shared `cmdr_fs::volume::conformance` assertions. They run against the **fake ADB
  server** in `src/testing.rs` (`FakeAdbServer`): a loopback `TcpListener` speaking the host framing, `host:transport`,
  `host-serial:<serial>:features`, `sync:` (both v1 and v2 verbs), and `shell,v2,raw:` over an in-memory `FakeTree`,
  plus `host:track-devices` with `push_devices` for scripted hotplug and `drop_connections` / `stop` for faults.
  `volume/testing.rs` holds the volume-level fixtures on top of it. No `adb` binary, no device, no Docker: every cell
  runs in the unit lane.
- **App-side** (`apps/desktop/src-tauri/src/adb/`): anything driving `write_operations`, the volume registry,
  `volume_listing::complete`, or the listing cache. ❌ Don't widen this crate's public surface to keep a test on that
  side; move the test instead. ❗ A green suite here is not evidence that a copy works: `supports_export` and the
  free-space pre-flight are read by the engine, so the cells that would catch them live with the engine.
- **`#[cfg(any(test, feature = "testing"))]`** widens `testing` and `volume::testing` to `pub` for the app's suites; the
  crate's own `dev-dependencies` self-entry turns the feature on for every dev target and leaves it off for the lib, so
  a shipped build carries no fixture. ❌ Never gate a fixture on `cfg(test)` alone.
- **A real-device pass is pending** (§ "Known gaps"). The fake server implements what the AOSP docs say; the documented
  differences between the docs and a phone's `adbd` are what that pass is for.

## Known gaps and follow-ups

- **No ranged read on the wire.** `RECV` has no offset; `open_read_stream_at_offset` reads and discards up to it. A
  resumed pane read on a phone is rare, and a resume from the middle of a 2 GB file re-reads the first half. If it
  matters, `shell dd` with `skip=`/`bs=` is the fallback, at the cost of the sync service's throughput.
- **`sendrecv_v2` compression flags** (brotli, lz4, zstd) are off on purpose; measure before enabling, since the device
  does the compressing.
- **Wireless debugging** (`adb pair`) is out of scope: the server owns pairing, and a paired device appears in
  `track-devices` like any other.
- **Real-device pass pending**: the authorize prompt, an `unauthorized` → `device` transition mid-session, a 2 GB `RECV`
  / `SEND`, and a `/data` listing on a non-rooted phone (expect `PermissionDenied` carrying the path).
- **A settings switch for the `adb` binary path**, for machines where it is neither on `PATH` nor under `$ANDROID_HOME`;
  `$ADB` is the only override today.

## The public surface

The crate is in `guardedIndexCrates`, so nothing here may name `cmdr`, `tauri`, or `tauri-specta`. The root re-exports
are the app's whole vocabulary: `AdbVolume`, `connect_adb_volume`, `AdbConnectionParams`, `AdbEndpoint`, `AdbDevice`,
`AdbDeviceState`, `DeviceTracker`, `list_devices`, `track_devices`, `DeviceFeatures`, `AdbError`, and `AdbConnectError`.
`errors`, `features`, `params`, and `devices` are `pub(crate)`; ❗ keep them that way, since a `pub mod` promises
everything `pub` inside it.
