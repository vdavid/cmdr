# Volume backends

Per-backend `Volume` impls. Trait shape, capability matrix, streaming patterns, and the "Building a new volume"
checklist: `../CLAUDE.md` + `../DETAILS.md`.

## Module map

- `local_posix.rs`, `archive.rs` (re-export of `crates/cmdr-archive`: zip/tar/7z), `mtp/` (macOS and Linux only),
  `smb/` + `smb_watcher.rs`. Both remote backends are split by concern, with the whole `impl Volume` in `volume_impl`
  (a trait impl can't span files). `InMemoryVolume` rides with the trait in `cmdr-fs`.

## SMB must-knows

- **The watcher runs on a DEDICATED session, never a clone of the main connection**: stacked CHANGE_NOTIFY long-polls
  wedge Samba.
- **The watcher never reconnects itself; on death it kicks the ONE reconnect path** (`spawn_watcher_death_reconnect` →
  `do_attempt_reconnect`), which respawns it AND resumes the index. ❌ No second reconnect loop. It's canceled by
  `on_unmount` / `do_attempt_reconnect`, never by a pane close.
- **Background bulk work uses the refcounted pool of extra sessions** (`smb/scan_pool.rs`; ksmbd serializes per
  connection, 4 ≈ 3.8×). Dead members retry on siblings, never the MAIN session.
- **A replaced volume is SUPERSEDED, never unmounted**: `on_superseded` retires the id-scoped parts and leaves
  `state` / `tree` / `client` alone for the transfers still holding an `Arc`. Tearing the session down here once killed
  a live NAS copy. ❌ Don't reinstate it.
- **`write_from_stream` uses a cloned `Connection` + owned `FileWriter`**, never a borrowed one holding the client
  mutex across the upload (the QNAP deadlock). Error paths must `abort()` then delete the partial, or the handle leaks
  and corrupt bytes linger at the user's destination name.
- **An unreachable request fails instead of hanging** (20 s to the socket, then 30 s of server silence; the watcher's
  session is probed too). ❌ Never read a missed keepalive as death.
- **`to_smb_path` matches the root by COMPONENT and `NotFound`s anything outside it**; guessing sent real requests to
  the wrong place. Post-mutation cache patches take `display_path_for`.
- **Watcher filenames need NFC→NFD normalizing and ❌ nothing else**: smb2 ≥ 0.18 already decodes separators, so a `\`
  in a filename is part of its NAME, and re-normalizing loses that entry forever.
- **Auto-upgrade is gated on `network.directSmbConnection`** and no-ops with no SMB mounts (no macOS Local Network
  prompt). Drive INDEXING lives in `src/indexing/`, not here.

## Local and MTP must-knows

- **`list_directory` must feed `on_progress`; ❌ never `_on_progress`.** It's the pane's only "Loaded N files..."
  signal, so dropping it strands a big folder on "Opening folder...". `../../listing/DETAILS.md` § "Local listing
  progress".
- **`LocalPosixVolume::write_from_stream` `sync_data`s each file** (+ best-effort parent-dir fsync) before returning:
  every cross-volume copy landing on local disk flows through it, and a bare `flush()` loses data on eject or sleep.
- **`MtpVolume::get_metadata` lists the entire parent directory** (MTP has no single-file stat). Avoid in hot paths.
- **`MtpReadStream` reads in bounded windows, freeing the session between them**; don't re-add a `Drop`/cancel.
  **`read_range` takes `read_range_direct`, NOT a read session**: archive extraction issues one per 256 KiB.

Per-backend decisions, the scan pool, supersede-vs-unmount, and the SMB auto-upgrade / reconnect lifecycles:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
