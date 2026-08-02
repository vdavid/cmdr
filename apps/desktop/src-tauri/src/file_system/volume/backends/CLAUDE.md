# Volume backends

Per-backend `Volume` impls. Trait shape, capability matrix, streaming patterns, and the "Building a new volume"
checklist live in `../CLAUDE.md` + `../DETAILS.md`.

## Module map

- `local_posix.rs`, `mtp.rs` (macOS/Linux only), `archive/` (zip/tar/7z, see `archive/CLAUDE.md`).
- `smb/`: `SmbVolume` over direct async smb2, split into `events`, `state`, `mapping`, `session`, `reconnect`,
  `streams`, `scan`, `scan_pool`, `volume_impl` (the whole `impl Volume`, since a trait impl can't span files).
  `smb_watcher.rs`: background change watcher.
- `InMemoryVolume` isn't here — needing no host, it rides with the trait in `cmdr-fs`.

## SMB must-knows

- **The watcher runs on a DEDICATED session, not a clone of the main connection.** Stacking CHANGE_NOTIFY long-polls on
  the write session wedges Samba (pinned by `smb_integration_concurrent_streaming_writes_no_deadlock`).
- **The watcher never reconnects itself; on death it kicks the ONE reconnect path** (`spawn_watcher_death_reconnect` →
  `do_attempt_reconnect`), which respawns it AND resumes the index. ❌ No second reconnect loop. It also feeds
  `indexing::apply_smb_change`, so it lives for the whole volume lifetime — canceled by `on_unmount` /
  `do_attempt_reconnect`, never by a pane close.
- **Background bulk work uses a refcounted pool of extra sessions** (`smb/scan_pool.rs`; ksmbd serializes per
  connection, 4 ≈ 3.8×). Dead members retry on siblings, never the MAIN session.
- **A replaced volume is SUPERSEDED, never unmounted.** `on_superseded` retires the id-scoped parts but leaves `state` /
  `tree` / `client` alone: a running transfer still holds an `Arc`, and tearing the session down here killed a live NAS
  copy on a healthy connection. ❌ Don't reinstate it.
- **`write_from_stream` uses a cloned `Connection` + owned `FileWriter`**, never a borrowed `FileWriter<'a>` holding the
  client mutex across the upload (the QNAP deadlock reproducer). Its error paths must `abort()` then delete the partial:
  dropping a writer without `finish()`/`abort()` leaks the handle, so a fresh-session delete hits a sharing violation
  and corrupt bytes linger at the user's destination name.
- **An unreachable request fails instead of hanging**: 20 s to reach the socket (`SendTimeout`), 30 s of server silence
  after that (`Timeout`, or `ServerUnresponsive` when the whole link went quiet), both tearing the connection down. An
  ECHO keepalive (5 s, on by default) buys a proven-alive connection 6× the response deadline, so nothing slow-but-alive
  is cut off. Read `sent_age` in the smb2 diagnostics before blaming the server: `None` means we never asked it.
- **The watcher's dedicated session now discovers a dead server by itself.** CHANGE_NOTIFY counts as work outstanding,
  so that connection is probed and its long poll ends on connection-wide silence rather than waiting forever — which is
  what feeds `spawn_watcher_death_reconnect`. A watcher death is therefore cheaper than it looks: it marks the volume
  Disconnected and rebuilds the session, while in-flight transfers keep their own `Arc`s and run on.
- **Watcher filenames need normalizing** (backslash→slash, NFC→NFD) before cache lookups.
- **Auto-upgrade is gated on `network.directSmbConnection`** and no-ops with no SMB mounts (so no macOS Local Network
  prompt). Drive INDEXING lives in `src/indexing/`, not here.

## Local and MTP must-knows

- **`LocalPosixVolume::write_from_stream` `sync_data`s each file** (+ best-effort parent-dir fsync) before returning.
  Every cross-volume copy landing on local disk flows through it; a bare `flush()` leaves bytes in the page cache, so an
  eject or sleep loses data (on a move, from both sides). Don't drop the fsync.
- **`MtpVolume::get_metadata` lists the entire parent directory** (MTP has no single-file stat). Avoid in hot paths.
- **`MtpReadStream` reads in bounded windows, freeing the session between them** (a mid-window drop self-heals via
  mtp-rs `TransactionScope`); don't re-add a `Drop`/cancel. **`read_range` takes `read_range_direct`, NOT a read
  session** — archive extraction issues one per 256 KiB, so a session would triple the USB round trips.

Per-backend decisions, gotchas, the scan pool, supersede-vs-unmount, and the SMB auto-upgrade / reconnect lifecycles:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
