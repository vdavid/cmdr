# Volume backends

Per-backend `Volume` implementations (`LocalPosixVolume`, `MtpVolume`, `SmbVolume`, the SMB watcher, `InMemoryVolume`).
The trait shape, capability matrix, streaming patterns, and "Building a new volume" checklist live in the parent
[`volume/CLAUDE.md`](../CLAUDE.md) and [`volume/DETAILS.md`](../DETAILS.md).

## Module map

- `local_posix.rs`: `LocalPosixVolume`, real filesystem; delegates listing/indexing/watching to `file_system::listing`
  and `indexing`, copy scanning via `walkdir`, space info via `libc::statvfs` FFI.
- `mtp.rs`: `MtpVolume`, MTP device storage; direct async MTP calls, `MtpReadStream` (bounded-window reads). macOS/Linux
  only.
- `smb.rs`: `SmbVolume`, direct async smb2. Split session storage, `AtomicU8` connection state, cached
  `SmbConnectionParams` for reconnect, global `AppHandle` for `smb-connection-changed` events. Same cfg gate.
- `smb_watcher.rs`: background SMB change watcher on a dedicated smb2 session (separate TCP connection).
- `in_memory.rs`: `InMemoryVolume`, `RwLock<HashMap>` store for tests + stress tests.
- `archive/`: `ArchiveVolume` (zip/tar/7z) + its reading core, zip write side, and live watch (subfolders). See
  [`archive/CLAUDE.md`](archive/CLAUDE.md).

## Must-knows

- **`SmbVolume`'s background watcher runs on a dedicated smb2 session, NOT a clone of the main connection.** Stacking
  CHANGE_NOTIFY long-polls on the same TCP session as heavy writes wedges Samba (pinned by
  `smb_integration_concurrent_streaming_writes_no_deadlock`). See [DETAILS.md](DETAILS.md) § "Per-backend decisions".
- **The SMB watcher doesn't reconnect itself; on death it kicks the one reconnect path.** It bails, then
  `spawn_watcher_death_reconnect` → `do_attempt_reconnect` (single source of truth) on a bounded backoff, so a background
  disconnect recovers with no pane open. Don't give the watcher its OWN reconnect loop (a second state machine swallows
  real disconnects). Reconnect respawns the watcher AND resumes the index (`resume_smb_index_if_enabled`).
  [DETAILS.md](DETAILS.md) § "Backend-autonomous reconnect and index resume".
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
- **`MtpVolume::get_metadata` lists the entire parent directory** (MTP has no single-file stat). Avoid in hot paths.
- **`MtpReadStream` reads in bounded windows, not one held-open stream.** Each `next_chunk` issues one
  `GetPartialObject64(offset, MTP_READ_WINDOW)`; the device lock is held per window, freeing the session between
  windows (foreground nav slips in). `cancel_and_release` is a no-op; a mid-window drop self-heals (mtp-rs
  `TransactionScope`). Offset/EOF rules: `mtp/connection/DETAILS.md` § "Bounded-window reads".
- **SMB watcher filenames need normalizing**: backslashes to forward slashes, and NFC (from server) to NFD (macOS
  mount paths) before cache lookups. See [DETAILS.md](DETAILS.md) § "Gotchas".
- **SMB auto-upgrade is gated on `network.directSmbConnection`** and is a no-op when no SMB mounts are present (so it
  fires no macOS Local Network prompt). See [DETAILS.md](DETAILS.md) § "SMB auto-upgrade lifecycle".
- **SMB drive INDEXING lives in `src/indexing/`, not here.** It needs a `direct` smb2 session (an `os_mount` is upgraded
  first). See [`src/indexing/DETAILS.md`](../../../indexing/DETAILS.md) § "SMB indexing and the freshness model".
- **The SMB watcher feeds the per-volume index; don't shorten its lifetime.** `smb_watcher.rs` →
  `notify_directory_changed` ALSO drives `indexing::apply_smb_change` (and `on_smb_watcher_died` / `on_smb_overflow` ⇒
  index Stale), so the index needs events for the whole volume lifetime — canceled only by `on_unmount` /
  `do_attempt_reconnect`, NOT a pane close, even with no pane open. See
  [`src/indexing/DETAILS.md`](../../../indexing/DETAILS.md) § "Live SMB watch → index".

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
