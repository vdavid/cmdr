# Volume backends

Per-backend `Volume` impls. Trait shape, capability matrix, streaming patterns, and the "Building a new volume"
checklist live in the parent [`volume/CLAUDE.md`](../CLAUDE.md) + [`volume/DETAILS.md`](../DETAILS.md).

## Module map

- `local_posix.rs`: `LocalPosixVolume`, real filesystem; delegates listing/indexing/watching to `file_system::listing`
  + `indexing`, copy scan via `walkdir`, space via `libc::statvfs` FFI.
- `mtp.rs`: `MtpVolume`, MTP device storage; direct async MTP calls, `MtpReadStream` (bounded-window reads).
  macOS/Linux only.
- `smb/`: `SmbVolume`, direct async smb2, as a directory module. `mod.rs` owns the struct + `connect_smb_volume`;
  concerns split into `events`, `state`, `mapping`, `session`, `reconnect`, `streams`, `scan`, and `volume_impl` (the
  whole `impl Volume`, since a trait impl can't be split across files). Split session storage, `AtomicU8` connection
  state, cached `SmbConnectionParams` for reconnect, global `AppHandle` for `smb-connection-changed` events.
- `smb_watcher.rs`: background SMB change watcher on a dedicated smb2 session (separate TCP connection).
- `in_memory.rs`: `InMemoryVolume`, `RwLock<HashMap>` for tests + stress tests.
- `archive/`: `ArchiveVolume` (zip/tar/7z) + reading core, zip write side, live watch. See
  [`archive/CLAUDE.md`](archive/CLAUDE.md).

## Must-knows

Depth for all of these is in [DETAILS.md](DETAILS.md) (§§ Per-backend decisions, Gotchas, SMB auto-upgrade / reconnect).

- **The SMB watcher runs on a dedicated smb2 session, not a clone of the main connection.** Stacking CHANGE_NOTIFY
  long-polls on the write session wedges Samba (pinned by `smb_integration_concurrent_streaming_writes_no_deadlock`).
- **A background index scan opens a pool of extra smb2 sessions (`smb/scan_pool.rs`)** the walk lists across (cold-NAS
  scans are ksmbd per-connection-serialized; 4 connections ≈ 3.8×). Transparent to the scanner; a dead member retries on
  a sibling, never touching the MAIN session. See DETAILS § "SMB scan-connection pool".
- **The SMB watcher doesn't reconnect itself; on death it kicks the one reconnect path** (`spawn_watcher_death_reconnect`
  → `do_attempt_reconnect`, single source of truth, bounded backoff), which respawns the watcher AND resumes the index.
  Don't give it its OWN reconnect loop (a second state machine swallows real disconnects).
- **`SmbVolume::write_from_stream` uses a cloned `Connection` + owned `FileWriter`, never a borrowed `FileWriter<'a>`
  holding the client mutex across the upload** (that brief-clone-then-long-hold shape is the QNAP deadlock reproducer).
- **`write_from_stream` error paths must `abort()` then delete the partial.** Dropping a `FileWriter` without
  `finish()`/`abort()` leaks the SMB handle, so a fresh-session delete hits a sharing violation and corrupt bytes linger
  at the user's destination name. Don't collapse the owned-writer error sites into a catch-all that loses the writer.
- **`LocalPosixVolume::write_from_stream` `sync_data`s each file (+ best-effort parent-dir fsync) before returning.**
  Every cross-volume copy/move landing on local disk flows through it; a bare `flush()` leaves bytes only in the page
  cache, so an eject/sleep loses data (on a move, from both sides). Don't drop the fsync.
- **`MtpVolume::get_metadata` lists the entire parent directory** (MTP has no single-file stat). Avoid in hot paths.
- **`MtpReadStream` reads in bounded windows, freeing the session between them** (`cancel_and_release` is a no-op; a
  mid-window drop self-heals via mtp-rs `TransactionScope`). Don't re-add a `Drop`/cancel. Offset/EOF rules:
  `mtp/connection/DETAILS.md` § "Bounded-window reads".
- **`MtpVolume::read_range` uses `read_range_direct`, NOT a read session**: one `GetPartialObject64` per call, no
  `GetStorageInfo`/`GetObjectInfo`. Archive extraction issues one per 256 KiB, so re-routing it through
  `open_read_session` would triple the USB round trips. Same doc, § "Ranged reads take the DIRECT path".
- **SMB watcher filenames need normalizing** (backslash→slash, NFC→NFD) before cache lookups.
- **SMB auto-upgrade is gated on `network.directSmbConnection`** and no-ops with no SMB mounts (fires no macOS Local
  Network prompt).
- **SMB drive INDEXING lives in `src/indexing/`, not here** (needs a `direct` smb2 session; an `os_mount` upgrades
  first). See [`src/indexing/DETAILS.md`](../../../indexing/DETAILS.md) § "SMB indexing and the freshness model".
- **The SMB watcher feeds the per-volume index; don't shorten its lifetime.** `smb_watcher.rs` also drives
  `indexing::apply_smb_change` (death/overflow ⇒ index Stale), so it needs events for the whole volume lifetime,
  canceled only by `on_unmount` / `do_attempt_reconnect`, not a pane close, even with no pane open. See
  [`src/indexing/DETAILS.md`](../../../indexing/DETAILS.md) § "Live SMB watch → index".

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
