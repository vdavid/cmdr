# `cmdr-adb` details

The depth behind `CLAUDE.md`: the shape of the crate, the wire contract it implements, what the `Volume` impl answers
and why, the error policy, where a test lives, and what is deliberately not done yet. The app-side half (the tracker
task, the device provider, eject, the IPC commands) is `apps/desktop/src-tauri/src/adb/DETAILS.md`.

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
  `adb -P <port> start-server` per process, through the binary `server::locate_adb_binary` finds: the path the app
  configured (`set_adb_binary_override`, from `fileOperations.adbBinaryPath`) first, then `$ADB`, `$PATH`,
  `$ANDROID_HOME/platform-tools`, `$ANDROID_SDK_ROOT/platform-tools`, `~/Library/Android/sdk/platform-tools`, then the
  Homebrew and `/usr/local` bins. ❗ The override wins only while it is RUNNABLE: a path that went stale (an SDK moved,
  a typo saved in Settings) falls through to the search rather than making every device vanish. No binary is
  `AdbConnectError::AdbNotInstalled`; a second refusal after a start is `ServerUnreachable`, and both are the user's to
  look at. `AdbEndpoint::at(addr)` (fixtures, a forwarded port) never starts anything.
- **One socket per operation, no shared session.** Every sync operation opens its own `sync:` socket and closes it with
  the operation. A shared session behind a mutex deadlocks a same-volume copy (the source stream holds it while the
  destination write waits) and parks every listing while a transfer is paused mid-file. The server multiplexes sockets
  and `adbd` serializes I/O on its side anyway, so nothing is gained by holding one. What bounds concurrent transfers is
  `max_concurrent_ops`, below.
- **A connect is four phases under one budget and one `CancellationToken`** (`volume::connect_adb_volume`): the device
  list (a serial that isn't there is `DeviceGone`, one in `unauthorized` is `Unauthorized`), the feature probe (no
  `shell_v2` is `DeviceTooOld`), then the hello (one `STAT` of `/`). A cancel ends the attempt where it stands and
  leaves nothing behind: no volume, no socket. `CONNECT_BUDGET` is 10 s, all phases together.
- **A generation with no session behind it.** There's no session to rebuild, so `Volume::connection_generation` counts
  the times a reconnect's hello brought a device that was DOWN back (`do_attempt_reconnect` →
  `ConnectionReporter::revive`, packed with the state in one atomic); a hello that finds the device already up, a failed
  hello, and a loss move nothing. `noting` reads it before each operation, and `note_lost_session` marks the device gone
  only while that generation is still current (`ConnectionReporter::lose`), so an operation that started before the
  device came back can't mark it gone again, and a transfer whose operation failed then sees the device live on a newer
  generation and runs it again (`cmdr-operations`'s `transfer/volume_outage.rs`). The tracker's `note_device_gone` and a
  failed hello are verdicts about the device NOW, so they move the state on whatever generation is current. Pinned by
  `volume/reconnect_test.rs`, against the fake server.

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
capped at 15 s), redelivering the list after each reconnect so a listener catches up. ❗ `AdbNotInstalled` ENDS the loop
instead: no binary means no server to reconnect to, and a machine with no Android tooling would otherwise carry a
warning every 15 s for the whole session. `forget_start_attempt` plus a fresh `track_devices` is how a caller revives it
after the user installs the tools.

**`FileEntry.permissions` is the whole `st_mode`**, file-type bits included, as a local listing has it
(`volume/mapping.rs`): a walker skips a FIFO, socket, or device off those bits rather than opening it, and copies carry
only the low nine bits anyway.

**Sync service** (`sync.rs`): after `sync:` → `OKAY`, binary little-endian packets `[id: 4 ASCII][arg: u32 LE]`.

- `STAT` + len + path → `STAT` + mode + size + mtime (all u32; mode 0 = doesn't exist). `STA2` (feature `stat_v2`) →
  `STA2` + error(u32) + dev(u64) + ino(u64) + mode(u32) + nlink(u32) + uid + gid + size(u64) + atime + mtime + ctime
  (i64 each). `STA2` is preferred: it carries errno and 64-bit sizes, which is what makes the error policy possible. ❗
  A v1 size is the low 32 bits of an `lstat` (wraps past 4 GiB; a symlink's is its target path's length), so
  `SyncStat::size_is_exact` is `false` there and a read stream opened over `STAT` declares `StreamLength::Unknown`,
  reading to `DONE`. Declaring it `Known` failed a fine copy at the size check (#538); `STA2` devices keep the strict
  check.
- `LIST` + len + path → repeated `DENT` + mode + size + mtime + namelen + name, then `DONE` + 16 zero bytes. `LIS2`
  (feature `ls_v2`) → repeated `DNT2` + error + dev + ino + mode + nlink + uid + gid + size + atime + mtime + ctime +
  namelen + name, then `DONE`.
- `RECV` + len + path → repeated `DATA` + len + bytes (≤ 64 KiB each), then `DONE` + 0; or `FAIL` + len + message.
  `RCV2` (feature `sendrecv_v2`) sends `RCV2` + len + path, then `RCV2` + flags(u32) before data. There is no ranged
  read on the wire: `open_read_stream_at_offset` reads and discards up to the offset (§ "Known gaps").
- `SEND` + len + `path,mode` → then repeated `DATA` + len + bytes, then `DONE` + mtime(u32) → `OKAY` + 0 or `FAIL` +
  len + message. `SND2` (feature `sendrecv_v2`) sends `SND2` + len + path then `SND2` + mode(u32) + flags(u32), then the
  same data stream. `SEND` truncates on open, which is why only a name the caller owns is written in place (§ "The
  `Volume` answers").

**Compression** (`lz4.rs`, on when the device advertises `sendrecv_v2_lz4`). `sendrecv_v2` alone doesn't promise it:
each algorithm is its own feature, and a device that doesn't know a flag answers `FAIL` + `unknown flags`, so the flag
goes out only on that word and everything else stays plain `SND2`/`RCV2` or `SEND`/`RECV`, with no retry. Flag bits:
brotli 1, lz4 2, zstd 4, dry run `0x80000000`; one compression bit at most. Under the flag, the file is ONE LZ4 frame
across the transfer's `DATA` payloads, cut at arbitrary bytes; the packet framing and the `DONE` are otherwise the same.
adbd writes linked 64 KiB blocks with no checksums (`LZ4F_compressBegin` with null preferences). The reader takes any
valid frame (block mode, up to the format's 4 MiB blocks, the optional checksums and content size) and refuses a
dictionary or a second frame. (All verified against `client/file_sync_client.cpp`, `daemon/file_sync_service.cpp`,
`compression_utils.h`, and `file_sync_protocol.h` in AOSP `packages/modules/adb` main, 2026-10-30.) Why LZ4 and not the
other two: `docs/notes/adb-sendrecv-compression-2026-10-30.md`.

- ❗ **A `DONE` before the frame's end mark is `AdbError::Protocol`**, never a clean end: that's a truncated file, and
  on a stream of unknown length nothing downstream would catch it. `lz4_flex`'s `FrameDecoder` takes a frame cut at a
  block boundary as finished, one of the two reasons the reader is ours (the other: it's a blocking `Read` that can't
  resume a partial block over an async socket).
- ❗ **A push owes a whole frame even for an empty file.** adbd's decoder reports `Done` only after the end mark, so a
  `DONE` without one spins its decode loop forever. `Lz4Writer::finish` always emits a header and an end mark.
- **A compression bomb** (255:1 at best, so one 64 KiB packet decodes to 16 MiB) stays bounded three ways: the size word
  is refused past the frame's declared maximum before a byte of the block is buffered, a block that decodes past that
  maximum fails inside the codec at the end of its buffer, and decoded bytes leave at most `MAX_DATA_CHUNK` per
  `recv_chunk`, so the next block decodes only when the consumer has room. Peak per stream: one block in, one block out,
  a 64 KiB window, and the read stream's channel. The total a device sends is unbounded, exactly as on an uncompressed
  pull.

**Every length word is the device's to choose, so each is bounded before its buffer exists.** A sync payload (name,
`DATA`, `FAIL` message) past `MAX_DATA_CHUNK` (64 KiB) and a shell frame past `shell::MAX_FRAME_PAYLOAD` (1 MiB, adbd's
own buffer) are `AdbError::Protocol`, and so is an LZ4 block past its frame's maximum. Before the caps, one hostile
`DNT2` or stderr frame allocated 4 GiB; the `adb_sync` and `adb_shell` fuzz targets (`fuzz/DETAILS.md`) found both
within a few hundred runs, and `sync_test.rs` / `shell_test.rs` pin them. `adb_lz4` holds the frame reader to the same.

**A listed name is one path segment or it's dropped.** The device names each `DENT`/`DNT2` entry, and the volume joins
the name onto the listed folder, so an empty name, `.`, `..`, or one holding a `/` would list a path naming some other
file. `push_entry` drops them, as `cmdr-nfs` does; `adb_sync` asserts it.

**Shell** (`shell.rs`): `shell,v2,raw:<cmd>` (feature `shell_v2`) frames stdout, stderr, and the exit code as packets
`[id: u8][len: u32 LE][payload]` with ids `0` stdin, `1` stdout, `2` stderr, `3` exit (payload one byte). A device
without `shell_v2` (pre-Android 7, 2016) is refused with `AdbConnectError::DeviceTooOld` rather than guessed at: the
legacy `shell:` service has no exit code, and inferring one from output would be string-matching control flow. Every
argument is single-quoted (`'` → `'\''`). Verbs (`volume/writes.rs`): `mkdir -p`, `rmdir` for a directory and `rm -f`
for anything else, a link included (strictly one node, so `delete` never recurses; the kind comes from the non-following
`entry_kind_impl`, since `rmdir` refuses a link to a folder), `mv -f`, `cp -f` for `copy_within`, and `df -k <path>`
(space info; parsed as data, never as an error signal). `read_range` uses
`dd if=<path> bs=65536 skip=<block> count=<blocks>`: Toybox 0.8.9–0.8.13 lacks GNU byte flags, but its block skip calls
`lseek` on regular files and falls back to discard only on unseekable inputs (verified by reading each release's
`toys/pending/dd.c`, 2026-09-29). The offset aligns down, the host trims the prefix, and `shell::run_bounded` rejects
stdout beyond the requested range plus one block before allocation. Shell reference: `adb/shell_protocol.h`
(platform-tools 35).

## The `Volume` answers, and why

The volume is device-anchored, the same shape MTP has, and every answer below follows from that.

- **Identity**: `volume_id` = `cmdr_fs::volume::adb_volume_id(serial)` (`adb-<slug>-<digest>`). The root is
  `adb_app_root(serial)`, `adb://<serial>` with the serial verbatim (the server keys on its exact case, while the id's
  slug is folded), spelled without a trailing slash because the device provider's row and the frontend's
  `constructAdbPath` spell it that way. The device's whole `/` hangs under it: `adb://R58M1/sdcard/DCIM` is
  `/sdcard/DCIM`.
- **Paths**: `volume/paths.rs` is the one translation, through `cmdr_fs::volume::remote_paths::RemoteRoot` (shared with
  SFTP and WebDAV): `to_device_path` going down, `to_app_path` coming back. Every `FileEntry.path` a listing or stat
  hands out, and every listing-cache patch path, carries the prefix, because a pane passes an entry's path straight back
  and a scheme-free `/sdcard/x` resolves to the Mac's boot disk. For the same reason a bare device-absolute path is
  refused rather than anchored: the app sites that take a volume-relative path (a transfer dialog's `/sdcard`) run it
  through `root_anchored` against `Volume::root` first, which lands it on `adb://<serial>/sdcard`, idempotently for a
  pane's already-prefixed path. ❌ Never anchor an out-of-root path; refuse it. `RemoteRoot` absorbs a `..` at `/`,
  which is right for a server rooted deeper but silent on a volume whose root IS `/`, so `paths.rs` counts the climb
  first and refuses `adb://<serial>/sdcard/../../etc`. Internally the device path is what the wire, the shell, and an
  error from the device carry; ❗ `probe` and `follow` take it directly, since handing one back through
  `get_metadata_impl` would be a bare path that the translation refuses.
- **Copies keep the source's date and mode** (the contract: `crates/cmdr-operations/src/transfer/volume/DETAILS.md` §
  "Copies keep the source's metadata"). A read stream reports the mtime of the `STAT`/`STA2` its open already does
  (`SyncStat::modified_at`, `None` before 1970 to match the listing); a push hands the stream's date to `DONE`, which
  must carry one, so a dateless source lands dated now, as `adb push` does. ❗ `DONE`'s mtime is a u32 on both verb sets
  (`SND2` only adds flags), so `done_mtime_word` clamps past 2106-02-07 and drops sub-seconds; the staging `mv -f` keeps
  the date. The same `stat` gives the stream its mode (`VolumeReadStream::metadata`), so a file copied onto local disk
  lands with it, folded through the umask. Both verb sets have a cell
  (`a_copy_keeps_the_source_date[_over_the_v1_verbs]`).
- **A push sends the source's mode**: `SEND`'s mode word is the stream's mode folded under the user's umask
  (`cmdr_fs::metadata::routed_mode`: never wider, no setuid), or `adb push`'s `0660` for a source that reports none
  (`streams.rs::send_mode`). Zero cost: the word goes out either way. ❗ `/sdcard` ignores it (FUSE over the shared
  storage hands every file the same mode), so `METADATA_CAPS` does NOT claim `MODE` in `stores`: the loss note keeps
  saying permissions stayed behind, true on `/sdcard` where nearly every copy lands, and a false alarm only on
  `/data/local/tmp` or a rooted device's own filesystems, which keep it. A per-path cap would need the trait to ask by
  path. Pinned on the fake (which keeps what it's sent, like `/data/local/tmp`) by
  `a_copy_sends_the_sources_mode_and_a_modeless_one_lands_as_adb_push_would`; the `/sdcard` claim is unverified on a
  real device.
- **`rerooted` → `None`.** One volume per device; the pane's path is inside it. A device has no second root to offer.
- **`lane_key` → the serial.** Two panes on one phone contend on one `adbd`, so they share a lane and the operation
  manager serializes their writes.
- **`max_concurrent_ops` reads `settings()` per batch dispatch**, and the app's `MAX_CONCURRENT_OPERATIONS_SOURCES`
  table has an `"adb"` row answering the constant 1. ❗ A namespace with no row silently gets a cautious 2, which is why
  the row exists even though it is not a user-facing knob. `adbd` serializes I/O per device, so a second concurrent
  transfer only adds contention.
- **`max_concurrent_scan_listings` → 8.** A drive-index walk keeps at most that many `LIST`s in flight on the phone,
  under whatever the walk's own pacing allows. Why eight (measured on a real phone), and how the walk applies it:
  `crates/cmdr-index/src/indexing/network_scanner/DETAILS.md` § "A backend's own ceiling".
- **`index_walk` → the phone's storage, once** (`src/volume/index_scope.rs`, whose module doc is the canonical why). A
  drive-index walk descends `/`, the `/sdcard` link (walked as the folder it points at), `/storage`, and every SD card:
  whatever `/storage` lists besides `emulated` and `self`, ❌ never a hardcoded card name. Every other directory keeps
  its row unwalked, `/proc`, `/sys`, `/data`, and the two second paths onto primary storage included. ❗ Primary storage
  is indexed under `/sdcard/…` only, so a pane on `/storage/emulated/0/…` shows no folder sizes.
- **`supports_export` → true, `is_writable` → true, `supports_streaming` → true, unknown-length writes → true.** Every
  read and write path is implemented; the conformance assertions hold each declaration to what the device accepts. The
  conflict scan is `scan_walk::scan_conflicts`, which lists the destination through this backend's own `scan_list` and
  matches with `conflicts_against`, so a conflict dialog gets the same shape here as anywhere; a destination that isn't
  there yet answers an empty list rather than `NotFound`, per `Volume::scan_for_conflicts`. A read-only mount answers
  `ReadOnly` per path when the shell's `EROFS` says so, not volume-wide.
- **`can_watch_listings` → false, `listing_watch_coverage` → `None`.** There is no watcher, so ❌ nothing here may claim
  an authoritative listing; the pane, and the phone's drive index behind the app's listing host, stay honest through
  `notify_mutation`, called once per changed directory by every mutation, `write_from_stream` included. The patch itself
  is `cmdr_fs::volume::patching`: this backend implements `PatchSource` (`volume/mutation.rs`) and owes nothing else.
- **The tree walk is `cmdr_fs::volume::scan_walk`**, reached by implementing `ScanSource` (`volume/scan.rs`): one `STAT`
  for a stat, one `LIST` for a listing, and the walk's arithmetic, batch loop, and conflict matcher come with it. ❗ It
  counts a symlinked directory as the one entry it is rather than walking it, which matters more here than on any other
  backend: `/sdcard` links to `/storage/self/primary`, which links to `/storage/emulated/0`, so following links would
  count the same bytes three times and a link aimed at an ancestor would never terminate.
- **❌ `MakesDirectories` is deliberately NOT implemented**, though the other stat-and-listing backends use it. That
  walk exists for protocols whose create-directory verb makes exactly one level (WebDAV's `MKCOL`), so it spends one
  request per missing ancestor. The device shell has native `mkdir -p`: `create_directory_all` is one probe plus one
  verb whatever the depth, and adopting the shared walk would trade that for a round trip per level.
- **`supports_local_fs_access`, `paths_are_os_visible`, `operations_are_local` → false; `local_path` → `None`.** Nothing
  on the host can open a device path.
- **`create_directory_errors_on_existing_dir` → false.** `mkdir -p` is the verb, and it is idempotent by design.
- **Streams**: `open_read_stream` is one `RECV` socket per file, chunk by chunk; `read_range` is one bounded device-side
  `dd` window rather than an archive-sized prefix transfer; `write_from_stream` is one `SEND` socket per file. Under
  `CreateOrReplace` it goes straight to `dest`, a name the caller owns (the transfer engine's own `.cmdr-tmp-*`, which
  it lands itself), and any failure removes what's there. Under `CreateNew` it goes into a staging sibling
  (`<dir>/<name>.cmdr-tmp-<uuid>` from `cmdr_fs::staging::temp_sibling_name`: the house marker, so a leftover is
  filtered from every pane, with the name shortened when the whole wouldn't fit the house 143-byte cap), then `mv -f`
  via the shell, and any failure removes the staging name. Both `Known` and `Unknown` lengths stream through the same
  path and report bytes actually sent with the caller's explicit expected length. ❌ Never collect a whole file into a
  `Vec<u8>`.
- **Two accepted TOCTOU windows**: `create_file` and a `force = false` rename each `stat` the destination first and
  refuse on a hit. Neither can be atomic on this protocol: `SEND` truncates unconditionally and `mv -n` exits 0 whether
  it moved or not (verified on Android 14 `toybox 0.8.9`, 2026-09-01). The window is one round trip on a device only
  this host writes to, and `conformance_test.rs` holds the refusal itself.
- **A forced rename onto a folder `rmdir`s it first**, so only an EMPTY one gives way (`Volume::rename`), and the source
  lands AS it. A bare `mv -f` onto a folder moves the source INTO it, replacing a same-named file inside, and `mv -T`
  only exists from toybox 0.8.3 (Android 11; read from toybox's `cp.c` per release tag, 2026-10-20). A link to a folder
  never gives way (its `rmdir` fails). The window between the `rmdir` and the `mv` would bury the source in a folder
  recreated in between: misplaced, never lost. The fake shell's `mv` moves into a folder the same way, so a regression
  shows in `conformance_test.rs`. ❗ A destination that's the source RE-CASED is never cleared: casefolded `/sdcard`
  takes it for the source, so the `rmdir` would take the source itself. "Re-cased" is Unicode's full case fold over NFD
  (`writes.rs::is_the_source_recased`, `unicase` + `unicode-normalization`), the way ext4/f2fs casefold compares names,
  so `Ö`/`ö` and `ß`/`SS` count; an ASCII-only check let a non-ASCII re-casing `rmdir` an empty source (#512).
- **Liveness**: operations are the detector (there is no keepalive), so every wire-touching delegator classifies a
  `DeviceGone` into `VolumeError::DeviceDisconnected` and emits the transition once (`state.rs`). `track-devices`
  additionally retires the volume when its serial leaves the list, which is the push channel MTP never had.
- **Space**: two answers, both `df -k`. `get_space_info` is the phone's figure, asked of the shared storage (`/sdcard`),
  and is what the pane's indicator and the poller show, polled at `space_poll_interval` = 30 s. Before a volume exists,
  `shell::shared_storage_space` asks the same thing for the app's switcher row, dialing nothing.
  `get_space_info_at(path)` asks about `path` itself, so an SD card answers for what's on it; the transfer pre-flight
  asks this of the destination folder. ❌ Never ask about the device root: `/` is a read-only system image reporting 0
  free, which put "0 bytes" in the pane and refused every copy onto a phone (observed on a Pixel 9 Pro XL, the dev log's
  `volume-space-changed: adb-… (0 avail)` and `InsufficientSpace { available: 0 }`, 2026-09-10). A `df` without figures
  reads like any failed verb, through a follow-up stat and ❌ never its stderr (`AdbVolume::df_space`): a path that
  isn't there (`ENOENT`, or a mode of 0 on the v1 verbs) answers for the nearest folder above it that is, because the
  copy pre-flight asks about a destination folder the copy will create, and anything but `NotSupported` fails the
  dialog's preview (`copy.rs::dest_space_if_known`). ❗ That stays a plain space answer, even when the climb lands on
  `/` and its 0 free: whether a folder takes writes is `write_access_at`'s answer (below), which the transfer pre-flight
  asks first, ❌ not a special case here. Anything else is `NotSupported` ("can't tell"), ❌ never a guessed number: a
  `df` failing on a path that exists, or a path under a file, where stat answers `ENOTDIR` and nothing could land
  (verified on Pixel 9 Pro XL, Android 17, `adb shell stat /sdcard/x.png/New`, 2026-09-10; the fake's `FakeTree::stat`
  answers the same). What `df -k` prints on a phone (verified on Pixel 9 Pro XL, Android 17, toybox 0.8.13,
  `adb shell df -k`, 2026-09-10): the last column is the MOUNT POINT, so `/sdcard`, `/storage/emulated/0`, and every
  folder under them report `/storage/emulated` (and `/data` its bind mount `/data/user/0`); toybox sizes the columns per
  invocation, so `shell::parse_df_k` reads the first three numbers after the header and ignores spacing; `/` reports
  `Available 0`; a missing path still prints the header on stdout, puts its reason on stderr, and exits 1, so the exit
  code decides and stderr is never read. `testing::pixel_captures` holds those captures verbatim, `FakeTree::new` mounts
  the Pixel's own rows, and `shell_test.rs` holds the fake's `df` to them byte for byte.
- **Write access**: `write_access_at(path)` runs `test -w` on the nearest folder at or above `path` that the sync
  service finds, the same climb the space answer makes. Exit 0 is `Writable`. Exit 1 is
  `Unwritable { reason: Unexplained }`, ❌ never `ReadOnlyFilesystem` or `NoPermission`: the exit code can't tell them
  apart (a phone's `/` is both a read-only image and root's), and telling them apart would need the mount flags in
  `/proc/mounts`, which no capture here holds yet. A transport failure or any other exit is `Unknown`. The transfer
  pre-flight asks this before it measures space, so a copy onto `/` is refused as not writable rather than as out of
  room (`crates/cmdr-operations/src/transfer/volume/DETAILS.md` § "A destination folder that takes no writes").
  `FakeTree::new` and `FakeTree::android_layout` mount `/` read-only (`FakeMount::read_only`), so a write under it
  answers `EROFS` and `test -w` exits 1 there, as the kernel does under a read-only mount. The mount is matched after
  following links (`FakeTree::writes_refused_at`), so a write through the layout's `/sdcard` link lands on shared
  storage rather than under `/`. `classify_failed_verb` relies on the same verb, whose exit 1 on `/` is verified (§
  "Verified on a real phone").

## The error policy

Typed only. `AdbError` is the transport's failure shape (`Io`, `Refused`, `Protocol`, `DeviceGone`, `Timeout`,
`Cancelled`); `AdbConnectError` is why a connect didn't produce a volume (`AdbNotInstalled`, `ServerUnreachable`,
`DeviceGone`, `Unauthorized`, `DeviceTooOld`, `TimedOut`, `Cancelled`, `Transport`). The one variant carrying the
server's text (`Refused`) is a log diagnostic; ❌ branching on it is what `error-string-match` forbids.

**The device's errno is the classifier.** `STA2` and `DNT2` carry a Linux errno, and `errors::volume_error_from_errno`
maps it to the `Volume` vocabulary for an operation on `path` in one place: `ENOENT` → `NotFound(path)`,
`EACCES`/`EPERM` → `PermissionDenied { path, raw_os_error: None }` (❗ `None`: the errno is the device's, and carrying
it would earn macOS-only "unlock it in Finder" advice), `EEXIST` → `AlreadyExists(path)`, `EISDIR` →
`IsADirectory(path)`, `ENAMETOOLONG` → `InvalidName(path)`, `EROFS` → `ReadOnly(path)`, `ENOSPC` → `StorageFull`.
Anything else goes through `DEVICE_ERRNO_CLASSES`, the Linux-numbered errno → `OsErrorClass` table, and lands as
`VolumeError::io_of_class`: an `IoError` carrying the HOST's code for that class (the device numbers `ENOTEMPTY` 39,
macOS 66; the app's classifiers read the class off `raw_os_error`, so the translation happens here, as `cmdr-sftp` does
it). ❗ An errno the table doesn't name carries `raw_os_error: None`, ❌ never the device's number: Linux 70 (`ECOMM`)
is macOS's `ESTALE`, 57 (`EBADSLT`) its `ENOTCONN`, 60 (`ENOSTR`) its `ETIMEDOUT`, and 35 (`EDEADLK`) its `EAGAIN`, so a
raw pass-through got a rare device error retried as a dropped link or flagged as a stale mount. The number stays in
`message` for the log. ❗ The table is literal numbers, ❌ never `libc::` constants (those are the host's); a Linux-only
test checks every row against the OS's own table. `volume_error_from_adb` maps the transport's shapes (`DeviceGone` →
`DeviceDisconnected`, `Timeout` → `ConnectionTimeout`, `Cancelled` → `Cancelled`); a `FAIL` text from the sync service
lands as an unclassified `IoError` with the text in `message` for the log.

**A shell failure is classified by a follow-up probe, never by stderr.** `shell,v2` gives an exit code, which says "no"
and nothing else; `mkdir`, `rm`, and `mv` print their reason to stderr in `toybox`'s wording, which is for people and
may be localized. So a non-zero exit is read through what the sync service says is at the path and its parent
(`AdbVolume::classify_failed_verb` in `writes.rs`): parent missing → `NotFound(parent)`; parent there but not writable
(`test -w`) → `PermissionDenied { path, .. }`; anything else → an `IoError` carrying stderr for the technical-details
panel. The probe classifies, ❌ never guards: asked before, it is a TOCTOU window (the two the backend accepts on
purpose are listed under the `Volume` answers).

**A guard stat that fails refuses; it never reads as an empty spot.** The stats that decide whether a verb may clear or
claim a name (a `force = false` rename, `create_file`, the `CreateNew` landing, `create_directory_all`'s `Created`
promise, `delete`'s pick of `rmdir` or `rm -f`) go through `AdbVolume::occupant`, which answers `Nothing` only for
`ENOENT`, `ENOTDIR`, or the v1 verbs' mode 0, and returns any other failure as the error. The classifier keeps the
forgiving `probe`, where "can't tell" only picks which refusal to report. Why it matters on a real phone: the sync
service stats in `adbd`'s SELinux domain while `mv` runs as the shell user, so a path the sync `STAT` is denied on could
read as free and then be replaced (#539; the SELinux trigger is reasoned from the domains, not traced on a device). The
v1 `STAT` zeroes the mode on any failure, so a device without `stat_v2` can't tell a denied stat from a missing file;
that gap stays. The fake models the denial with `FakeTree::deny_sync_stat`, and `conformance_test.rs` holds the three
guards to it.

**An empty listing asks whether the folder could be opened at all.** `adbd`'s `LIST`/`LIS2` answers `DONE` straight away
when `opendir` fails, so a folder it can't read, an empty folder, and a missing path all list as nothing, and it never
sends `.`/`..` (verified on Pixel 9 Pro XL, Android 17 `CP3A.261005.005`, adb 37.0.0, raw sync `LIST` of `/data`, an
empty folder, and `/sdcard/nope-xyz`, 2026-10-30, #836). `list_directory_impl`'s stat before the `LIST` separates
"missing", but `STAT` succeeds on `/data` (`drwxrwx--x system`), which then read as an empty folder. So a listing that
comes back EMPTY runs one `test -r '<p>' && test -x '<p>'` (`shell::run_chain`, one round trip, only for empty
listings): exit 1 re-stats the folder and answers `PermissionDenied { path, raw_os_error: None }`, or `NotFound` if it
went away meanwhile; any other exit can't tell and leaves the listing empty. `-x` counts too: a folder with `r` but no
`x` opens, but `adbd` can't stat what's inside and drops every entry. A copy scan (`scan_list`) asks too, or a copy
would quietly make an empty folder. **❌ The drive-index walk (`list_directory_for_scan`, `EmptyListing::Believe`)
doesn't:** a phone holds hundreds of empty folders, and one shell round trip each took a walk of 2,213 folders from 4 s
to about 22 s (Pixel 9 Pro XL, dev build, 2026-10-30). The price is a refused folder indexed as empty, with size 0,
which misleads nobody the way a pane would. The fake models refusal with `FakeTree::deny_read`.

**`create_directory_all` names a file in the way** as `NotADirectory(device_path)`, the contract every backend keeps
(`conformance::assert_create_directory_all_refuses_a_file_in_the_way`). `mkdir -p` says "File exists" or "Not a
directory" only on stderr, so both halves are the probe's: the leaf stat that already decides `AlreadyExisted` refuses a
leaf that isn't a folder, and an ancestor is looked for only after the verb has refused, walking up to the nearest thing
that exists (`file_above`). The probe follows links, so a path through a link to a folder is created into, which on a
phone is most paths: `/sdcard` is one. The fake's `mkdir -p` resolves links and refuses a file the way a kernel does
(`testing/tree.rs`), or neither cell would mean anything.

**What a variant carries.** `VolumeError::NotFound` and `PermissionDenied` are defined to carry the PATH
(`crates/cmdr-fs/src/volume/types.rs`), and the transfer layer forwards it straight into what the frontend renders as
the missing file's name. The mapper takes the path it is mapping a failure for, so a pathless `NotFound` is not
constructible here. Held by `conformance::assert_not_found_carries_the_path`.

## Which side a test lives on

A cell lives with whatever it **asserts**, never with whatever it connects to.

- **Here**: the framing, the sync and shell codecs, the errno table, path anchoring, the connect phases and calling one
  off, the state transitions, and the shared `cmdr_fs::volume::conformance` assertions. They run against the **fake ADB
  server** in `crates/cmdr-adb/src/testing/` (`FakeAdbServer` in `server.rs`, the filesystem model in `tree.rs`, the
  shell verbs in `shell.rs`, a real phone's `df -k` output in `pixel_captures.rs`): a loopback `TcpListener` speaking
  the host framing, `host:transport`, `host-serial:<serial>:features`, `sync:` (both v1 and v2 verbs; `SEND` creates its
  file at open, as a device's does, so an upload that stops short leaves a torn file for the writer's cleanup to remove,
  and a cell can watch that cleanup fail; `RCV2`/`SND2` honor the LZ4 flag, refuse it where `set_features` doesn't list
  `sendrecv_v2_lz4`, and record each flags word for `transfer_flags`, so every volume-level cell runs compressed and a
  fallback cell means something), and `shell,v2,raw:` over an in-memory `FakeTree`. The tree resolves links as the
  kernel does: a read follows every link along the path (`canonical`, up to `MAX_LINK_HOPS`, then `ELOOP`), `STAT`
  leaves the last one unfollowed, `LIST` of a link lists its target (with no `.`/`..`, and nothing at all for a
  `deny_read` folder, as a phone answers), and a write stays literal; `FakeTree::android_layout` builds the `/sdcard` →
  `/storage/self/primary` → `/storage/emulated/0` chain the index cells walk (a phone's own, verified on Pixel 9 Pro XL,
  Android 17, `adb shell readlink /sdcard /storage/self/primary`, 2026-09-11), plus `host:track-devices` with
  `push_devices` for scripted hotplug, `drop_connections` / `stop` for faults, `hold_answers` / `release_answers` to
  hold a dial provably in flight, `set_device_round_trip` to model the USB hop before every device service, and
  `requests` (every service request, in order) for counting dials or round trips, or proving none happened.
  `volume/testing.rs` holds the volume-level fixtures on top of it. No `adb` binary, no device, no Docker: every cell
  runs in the unit lane.
- **App-side** (`apps/desktop/src-tauri/src/adb/`): anything driving `cmdr-operations`, the volume registry,
  `volume_listing::complete`, or the listing cache. The transfer engine's cells sit beside the SFTP suite, in
  `apps/desktop/src-tauri/src/operations_host/backend_suites/adb_transfer_test.rs`; the list of what exists is
  `apps/desktop/src-tauri/src/adb/DETAILS.md` § "Testing". ❌ Don't widen this crate's public surface to keep a test on
  that side; move the test instead. ❗ A green suite here is not evidence that a copy works: `supports_export` and the
  free-space pre-flight are read by the engine, so the cells that would catch them live with the engine.
- **`#[cfg(any(test, feature = "testing"))]`** widens `testing` and `volume::testing` to `pub` for the app's suites; the
  crate's own `dev-dependencies` self-entry turns the feature on for every dev target and leaves it off for the lib, so
  a shipped build carries no fixture. ❌ Never gate a fixture on `cfg(test)` alone.
- **Codec cells that need exact bytes, not a conversation**, script the peer: `AdbConnection::scripted` plays a fixed
  byte string and discards what we write (`AdbConnection` holds any `AsyncRead + AsyncWrite` stream for this). The
  `fuzzing` feature's `fuzzing` module drives the sync and shell readers the same way for the fuzz targets.
- **The fake implements what the AOSP docs say**, so a phone can still surprise it. What a real one did: § "Verified on
  a real phone".

## Known gaps and follow-ups

- **No ranged read in the sync service.** `RECV` has no offset; `open_read_stream_at_offset` reads and discards up to
  it. A resumed pane read on a phone is rare, and a resume from the middle of a 2 GB file re-reads the first half. If it
  matters, route resumable streams through the bounded `read_range` primitive instead.
- **`sendrecv_v2` compression is LZ4 only.** brotli and zstd stay off: both cost real speed pulling photos for little or
  no gain elsewhere (`docs/notes/adb-sendrecv-compression-2026-10-30.md`). LZ4 has no opt-out and no per-file choice;
  the numbers show it neutral on already-compressed data, from one idle phone on AC power, so a phone on battery is the
  open question.
- **Bulk writes run at `adb push` speed; small files pay in round trips.** `SEND` frames at the protocol's 64 KiB
  maximum with no per-chunk round trip, so one large file lands as fast as the CLI does (about 28 MB/s through Cmdr
  against 31 to 35 MB/s for `adb push`, same cable, same minute). A copy of many small files is another story: 200 files
  of 20 KB took 52 s (about 260 ms each) where `adb push` of the folder took under 10 s for those plus 300 MB. A device
  shell round trip is about 60 ms, and every stat opens its own sync socket. (Measured on a Pixel 9 Pro XL over USB, dev
  build, 2026-09-30, BEFORE the fix below.)
  - **What a file costs now: seven device round trips**, down from nine. The engine's probe of the name, the `SEND` onto
    its temp, the patch's stat of that temp, the landing rename's stat of the free name, its `mv`, the rename patch's
    stat, and the engine's read-back. The two gone (#339): `write_from_stream` used to stage the engine's temp AGAIN (a
    `<name>.cmdr-tmp-<uuid>.cmdr-tmp-<uuid>` `mv -f`ed onto it), and `rename` stat'ed its source before every `mv`. The
    caller says the name is its own with `WriteMode::CreateOrReplace` (the house meaning: SFTP, SMB, WebDAV, and local
    disk all write that mode in place), so nothing is inferred from the name; a missing source is asked about only once
    `mv` fails, or before an empty folder gives way to it. Pinned by
    `apps/desktop/src-tauri/src/operations_host/backend_suites/adb_small_files_test.rs` (one `mv` per file, a round-trip
    budget).
  - **Measured on the fake, not a phone**: 20 files of 20 KB, the fake waiting 30 ms before every device service
    (`FakeAdbServer::set_device_round_trip`, a model of the USB hop), three runs each: 304, 311, and 364 ms per file
    (9.30 round trips) before, 240, 256, and 260 ms (7.30) after. With no modeled wait both are a few ms per file.
    (`CMDR_ADB_BENCH=1 cargo nextest run -p cmdr --no-capture adb_small_files_test::bench`, M-series Mac, 2026-11-03.)
    On the Pixel, one shell `mv` (about 60 ms) and one sync stat less per file; the inner stage alone measured about 135
    ms there, so expect a saving of that order. ❗ Unverified on a device.
  - **What's left**: the two patch stats (a phone has no watcher, so the patch is what keeps a pane honest, and the
    index reads it too) and the engine's own probe and read-back. Batching them is engine-side work.
- **Wireless debugging** (`adb pair`) is out of scope: the server owns pairing, and a paired device appears in
  `track-devices` like any other.

## Verified on a real phone

Pixel 9 Pro XL, Android 17 (`CP3A.261005.005`), adb 37.0.0, macOS 27.0.1, dev build, 2026-10-30:

- **Authorizing**: revoking USB debugging drops the phone's whole USB session. The row waits while unauthorized, and
  tapping Allow brings ADB back with nothing else pressed. Android falls back to "No data transfer", so MTP stays away
  until something sets File transfer.
- **2 GB each way**: `RECV` and `SEND` both byte-identical by SHA-256, with the modified date kept.
- **Compressed transfers** against a real `adbd`: round trips from 0 bytes to 20 MiB, including the empty file. Numbers:
  `docs/notes/adb-sendrecv-compression-2026-10-30.md`.
- **Refusals**: `/data` answers `PermissionDenied` carrying the path, and `test -w /` exits 1 where a readable folder
  exits 0.
- **Drive-index walk of `/sdcard`**: 23,553 entries and 2,205 folders in 5.1 s, 4 s on a rescan. Pane listings stay
  responsive under the eight-listing ceiling: a median of 50 ms during a walk against 36 ms idle. Unplugging mid-walk
  ends it with a typed disconnect, and the in-flight listings aren't held against their folders.

## The public surface

The crate is in `guardedIndexCrates`, so nothing here may name `cmdr`, `tauri`, or `tauri-specta`. The root re-exports
are the app's whole vocabulary: `AdbVolume`, `connect_adb_volume`, `AdbConnectionParams`, `AdbEndpoint`, `AdbDevice`,
`AdbDeviceState`, `DeviceTracker`, `list_devices`, `track_devices`, `DeviceFeatures`, `AdbError`, `AdbConnectError`,
`locate_adb_binary`, `set_adb_binary_override`, and `forget_start_attempt`. `errors`, `features`, `params`, and
`devices` are `pub(crate)`; ❗ keep them that way, since a `pub mod` promises everything `pub` inside it.
