# Volume backends

Per-backend `Volume` implementations (`LocalPosixVolume`, `MtpVolume`, `SmbVolume`, the SMB watcher, `InMemoryVolume`).
The trait shape, capability matrix, streaming patterns, and "Building a new volume" checklist live in the parent
[`volume/CLAUDE.md`](../CLAUDE.md) and [`volume/DETAILS.md`](../DETAILS.md).

## Module map

- `local_posix.rs`: `LocalPosixVolume`, real filesystem; delegates listing/indexing/watching to `file_system::listing`
  and `indexing`, copy scanning via `walkdir`, space info via `libc::statvfs` FFI.
- `mtp.rs`: `MtpVolume`, MTP device storage; direct async MTP calls, `MtpReadStream` for streaming. Gated
  `#[cfg(any(target_os = "macos", target_os = "linux"))]`.
- `smb.rs`: `SmbVolume`, direct async smb2. Split session storage, `AtomicU8` connection state, cached
  `SmbConnectionParams` for reconnect, global `AppHandle` for `smb-connection-changed` events. Same cfg gate.
- `smb_watcher.rs`: background SMB change watcher on a dedicated smb2 session (separate TCP connection).
- `in_memory.rs`: `InMemoryVolume`, `RwLock<HashMap>` store for tests + stress tests.

## Must-knows

- **`SmbVolume`'s background watcher runs on a dedicated smb2 session, NOT a clone of the main connection.** Stacking
  CHANGE_NOTIFY long-polls on the same TCP session as heavy writes wedges Samba (pinned by
  `smb_integration_concurrent_streaming_writes_no_deadlock`). See [DETAILS.md](DETAILS.md) § "Per-backend decisions".
- **The SMB watcher doesn't reconnect itself; it bails on connection errors.** Don't give it its own reconnect-with-
  backoff loop: two state machines tracking "is the session alive" diverge and swallow real disconnects.
  `do_attempt_reconnect` (driven by the FE backoff) is the single source of truth and respawns the watcher.
- **`SmbVolume::write_from_stream` uses a cloned `Connection` + owned `FileWriter`; never a borrowed `FileWriter<'a>`
  that holds the client mutex across the upload.** That brief-clone-then-long-hold shape is the QNAP deadlock
  reproducer. The client mutex is held only for `clone_session()`, never across I/O.
- **`write_from_stream` error paths must `abort()` then delete the partial.** Dropping a `FileWriter` without
  `finish()`/`abort()` leaks the SMB handle (`Drop` only logs, never CLOSEs), so a fresh-session delete hits a sharing
  violation and the partial lingers (corrupt bytes at the user's destination name). Don't refactor the owned-writer
  error sites into a post-block catch-all that loses the writer. See [DETAILS.md](DETAILS.md).
- **`LocalPosixVolume::write_from_stream` `sync_data`s each file (+ best-effort parent-dir fsync) before returning.**
  Every cross-volume copy/move landing on local disk flows through this one method; a bare `flush()` leaves bytes only
  in the page cache, so an eject/sleep loses data (on a move, from both sides). Don't drop the fsync.
- **`MtpVolume::get_metadata` lists the entire parent directory** (MTP has no single-file stat). Fine for infrequent
  `notify_mutation` use; avoid it in hot paths.
- **`MtpReadStream::Drop` spawns a detached cancel task.** mtp-rs's `ReceiveStream` panics on drop if not consumed or
  cancelled (USB session corruption guard). Safe because the stream always lives in an async context.
- **SMB watcher filenames need normalizing**: backslashes to forward slashes, and NFC (from server) to NFD (macOS
  mount paths) before cache lookups. See [DETAILS.md](DETAILS.md) § "Gotchas".
- **SMB auto-upgrade is gated on `network.directSmbConnection`** and is a no-op when no SMB mounts are present (so it
  fires no macOS Local Network prompt). See [DETAILS.md](DETAILS.md) § "SMB auto-upgrade lifecycle".

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
