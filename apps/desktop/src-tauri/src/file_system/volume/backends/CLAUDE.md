# Volume backends

Per-backend `Volume` impls. Trait shape, capability matrix, streaming patterns, and the "Building a new volume"
checklist live in the parent `../CLAUDE.md` + `../DETAILS.md`.

## Module map

- `local_posix.rs`: `LocalPosixVolume`, real filesystem; delegates listing to `file_system::listing`, copy scan via
  `walkdir`, space via `libc::statvfs` FFI.
- `mtp.rs`: `MtpVolume`, MTP device storage; direct async MTP calls, `MtpReadStream` (bounded-window reads).
  macOS/Linux only.
- `smb/`: `SmbVolume`, direct async smb2. `mod.rs` owns the struct + `connect_smb_volume`;
  concerns split into `events`, `state`, `mapping`, `session`, `reconnect`, `streams`, `scan`, `scan_pool`, and
  `volume_impl` (the whole `impl Volume`, since a trait impl can't be split across files).
- `smb_watcher.rs`: background SMB change watcher on a dedicated smb2 session.
- `InMemoryVolume` (for tests) isn't here — it needs no host, so it rides with the trait in `cmdr-fs` and is
  re-exported as `volume::InMemoryVolume`.
- `archive/`: `ArchiveVolume` (zip/tar/7z) + reading core, zip write side, live watch. See
  `archive/CLAUDE.md`.

## Must-knows

Depth: `DETAILS.md` (§§ Per-backend decisions, Gotchas, SMB auto-upgrade / reconnect).

- **The SMB watcher runs on a dedicated smb2 session, not a clone of the main connection.** Stacking CHANGE_NOTIFY
  long-polls on the write session wedges Samba (pinned by `smb_integration_concurrent_streaming_writes_no_deadlock`).
- **Background bulk work (scan listings + media prefetch) uses a pool of extra smb2 sessions**
  (`smb/scan_pool.rs`; ksmbd serializes per connection, 4 connections ≈ 3.8×). Reads are compound-only on members;
  dead members retry on siblings, never the MAIN session; REFCOUNTED. DETAILS § "SMB scan-connection pool".
- **A replaced volume is SUPERSEDED, never unmounted.** `on_superseded` retires the id-scoped parts (watcher, scan
  pool, state events, index-resume) but leaves `state` / `tree` / `client` alone: a running transfer, stream, or
  scan still holds an `Arc` and can't move to the successor. Tearing the session down here killed a live NAS copy
  on a healthy connection. ❌ Don't reinstate it. DETAILS § "Supersede vs. unmount".
- **The SMB watcher doesn't reconnect itself; on death it kicks the one reconnect path** (`spawn_watcher_death_reconnect`
  → `do_attempt_reconnect`, bounded backoff), which respawns the watcher AND resumes the index. Don't give it its OWN
  reconnect loop (a second state machine swallows real disconnects).
- **`SmbVolume::write_from_stream` uses a cloned `Connection` + owned `FileWriter`, never a borrowed `FileWriter<'a>`
  holding the client mutex across the upload** (that shape is the QNAP deadlock reproducer).
- **An SMB request that can't reach the wire now fails instead of hanging** (smb2 0.15.0): `Error::SendTimeout` →
  `ErrorKind::TimedOut` → `VolumeError`, after 60 s, and the connection is torn down. Before that, a stuck socket froze
  every SMB op in the app permanently with no error and no log line — the 2026-07-31/08-01 transfer wedge. If a
  transfer fails this way, read `sent_age` in the smb2 diagnostics before blaming the server: `None` means we never
  asked it. Full account: `docs/notes/incidents/2026-07-31-transfer-wedge/README.md` § Resolution.
- **`write_from_stream` error paths must `abort()` then delete the partial.** Dropping a `FileWriter` without
  `finish()`/`abort()` leaks the SMB handle, so a fresh-session delete hits a sharing violation and corrupt bytes linger
  at the user's destination name. Don't collapse the owned-writer error sites into a catch-all that loses the writer.
- **`LocalPosixVolume::write_from_stream` `sync_data`s each file (+ best-effort parent-dir fsync) before returning.**
  Every cross-volume copy/move landing on local disk flows through it; a bare `flush()` leaves bytes only in the page
  cache, so an eject/sleep loses data (on a move, from both sides). Don't drop the fsync.
- **`MtpVolume::get_metadata` lists the entire parent directory** (MTP has no single-file stat). Avoid in hot paths.
- **`MtpReadStream` reads in bounded windows, freeing the session between them** (a mid-window drop self-heals via
  mtp-rs `TransactionScope`). Don't re-add a `Drop`/cancel. `mtp/connection/DETAILS.md` § "Bounded-window reads".
- **`MtpVolume::read_range` uses `read_range_direct`, NOT a read session**: one `GetPartialObject64` per call, no
  `GetStorageInfo`/`GetObjectInfo`. Archive extraction issues one per 256 KiB, so a read session would triple the USB
  round trips. Same doc, § "Ranged reads take the DIRECT path".
- **SMB watcher filenames need normalizing** (backslash→slash, NFC→NFD) before cache lookups.
- **SMB auto-upgrade is gated on `network.directSmbConnection`** and no-ops with no SMB mounts (fires no macOS Local
  Network prompt).
- **SMB drive INDEXING lives in `src/indexing/`, not here** (needs a `direct` smb2 session; an `os_mount` upgrades
  first). See `crates/cmdr-index/src/indexing/transports/DETAILS.md` § "The direct-smb2 gate".
- **The SMB watcher feeds the per-volume index; don't shorten its lifetime.** `smb_watcher.rs` also drives
  `indexing::apply_smb_change` (death/overflow ⇒ index Stale), so it lives for the whole volume lifetime (canceled only
  by `on_unmount` / `do_attempt_reconnect`), never by a pane close. See
  `crates/cmdr-index/src/indexing/transports/DETAILS.md` § "Live SMB watch → index".

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
