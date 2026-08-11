# Volume backends

Per-backend `Volume` impls. Trait shape, capability matrix, streaming patterns, and the "Building a new volume"
checklist: `../CLAUDE.md` + `../DETAILS.md`.

## Module map

- `local_posix.rs`, `archive.rs` (re-export of `crates/cmdr-archive`, zip/tar/7z), `mtp/` (macOS/Linux only, split
  like `smb/`).
- `smb/`: `SmbVolume` over direct async smb2, split by concern, with the whole `impl Volume` in `volume_impl` (a trait
  impl can't span files). `smb_watcher.rs`: background change watcher.
- `InMemoryVolume` rides with the trait in `cmdr-fs`, needing no host.

## SMB must-knows

- **The watcher runs on a DEDICATED session, not a clone of the main connection.** Stacking CHANGE_NOTIFY long-polls on
  the write session wedges Samba (pinned by `smb_integration_concurrent_streaming_writes_no_deadlock`).
- **The watcher never reconnects itself; on death it kicks the ONE reconnect path** (`spawn_watcher_death_reconnect` →
  `do_attempt_reconnect`), which respawns it AND resumes the index. ❌ No second reconnect loop. Feeding
  `indexing::apply_smb_change` too, it lives for the volume's lifetime: canceled by `on_unmount` /
  `do_attempt_reconnect`, never by a pane close.
- **Background bulk work uses a refcounted pool of extra sessions** (`smb/scan_pool.rs`; ksmbd serializes per
  connection, 4 ≈ 3.8×). Dead members retry on siblings, never the MAIN session.
- **A replaced volume is SUPERSEDED, never unmounted.** `on_superseded` retires the id-scoped parts but leaves `state` /
  `tree` / `client` alone: a running transfer still holds an `Arc`, and tearing the session down here killed a live NAS
  copy on a healthy connection. ❌ Don't reinstate it.
- **`write_from_stream` uses a cloned `Connection` + owned `FileWriter`**, never a borrowed `FileWriter<'a>` holding the
  client mutex across the upload (the QNAP deadlock reproducer). Error paths must `abort()` then delete the partial: a
  writer dropped without `finish()`/`abort()` leaks the handle, the delete then hits a sharing violation, and corrupt
  bytes linger at the user's destination name.
- **An unreachable request fails instead of hanging**: 20 s to reach the socket, then 30 s of server silence (6× on an
  ECHO-proven-alive link, so nothing slow-but-alive is cut). Both tear the connection down. The watcher's session is
  probed too, so a dead one ends its long poll instead of parking forever. The deadlines, `sent_age`, and the
  keepalive's limits as a death signal: `DETAILS.md`.
- **`to_smb_path` matches the root by COMPONENT and `NotFound`s anything outside it**; guessing sent real requests to
  the wrong place. Post-mutation cache patches take `display_path_for`, so a done write can't be reported as failed.
- **Watcher filenames need NFC→NFD normalizing** before cache lookups, and ❌ nothing else: smb2 ≥ 0.18 hands back `/`
  separators with illegal characters already decoded, so a `\` in one is part of a file's NAME. Re-normalizing it to `/`
  turns that name into a path and the lookup misses forever.
- **Auto-upgrade is gated on `network.directSmbConnection`** and no-ops with no SMB mounts (so no macOS Local Network
  prompt). Drive INDEXING lives in `src/indexing/`, not here.

## Local and MTP must-knows

- **`list_directory` must feed `on_progress`; ❌ never `_on_progress`.** It's the pane's only "Loaded N files..."
  signal, so dropping it silently strands a big folder on "Opening folder..." for its whole read. A `spawn_blocking`
  hop is no excuse; use the tally: `../../listing/DETAILS.md` § "Local listing progress".
- **`LocalPosixVolume::write_from_stream` `sync_data`s each file** (+ best-effort parent-dir fsync) before returning.
  Every cross-volume copy landing on local disk flows through it; a bare `flush()` leaves bytes in the page cache, so an
  eject or sleep loses data (on a move, from both sides).
- **`MtpVolume::get_metadata` lists the entire parent directory** (MTP has no single-file stat). Avoid in hot paths.
- **`MtpReadStream` reads in bounded windows, freeing the session between them** (a mid-window drop self-heals via
  mtp-rs `TransactionScope`); don't re-add a `Drop`/cancel. **`read_range` takes `read_range_direct`, NOT a read
  session** — archive extraction issues one per 256 KiB, so a session would triple the USB round trips.

Per-backend decisions, the scan pool, supersede-vs-unmount, and the SMB auto-upgrade / reconnect lifecycles:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
