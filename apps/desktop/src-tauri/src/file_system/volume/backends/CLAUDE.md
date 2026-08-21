# Volume backends

Per-backend `Volume` impls. Trait shape, capabilities, streaming patterns, "Building a new volume": `../CLAUDE.md`
+ `../DETAILS.md`.

## Module map

- `local_posix.rs`, `archive.rs` (re-export of `crates/cmdr-archive`: zip/tar/7z), `mtp/`, `smb/` + `smb_watcher.rs`.
  Both remote backends split by concern: `volume_impl` (the whole `impl Volume`, since one can't span files), `streams`
  (bytes), `mapping` (errors), plus SMB's `state` + `reconnect` (share-scoped, on `SmbVolumeInner`). `InMemoryVolume`
  rides with the trait in `cmdr-fs`.

## SMB must-knows

- **The watcher runs on a DEDICATED session, never a clone of the main one**: stacked CHANGE_NOTIFY long-polls wedge
  Samba.
- **The watcher never reconnects itself; on death it kicks the ONE reconnect path** (`spawn_watcher_death_reconnect`),
  which respawns it AND resumes the index. ❌ No second reconnect loop, and ❌ never cancel it on a pane close.
- **Share-scoped background work re-upgrades `SmbVolumeInner::self_handle()` to learn it has stood down.** Resolving
  the volume id instead answers with the SUCCESSOR after a swap.
- **Background bulk work uses the refcounted pool of extra sessions** (`smb/scan_pool.rs`; ksmbd serializes per
  connection, 4 ≈ 3.8×). Dead members retry on siblings, ❌ never the MAIN one.
- **`SmbVolume` is a per-mount-root instance over a shared `Arc<SmbVolumeInner>`** carrying the session, connection
  state, and reconnect machinery; `rerooted` moves a share to another mount for one allocation. ❌ A promotion must
  never call `on_superseded` / `on_unmount` on the instance it replaces: they act on the SHARED session.
- **`paths_are_os_visible()` tracks the MOUNT, not the backend kind** (latched off by `note_root_mount_gone` when no
  live root survives). ❌ Never hardcode it `true`: smb2 keeps browsing a share whose mount is gone, so the drag it
  breaks fails silently.
- **A replaced volume is SUPERSEDED, never unmounted**: `on_superseded` retires the id-scoped parts and leaves
  `state` / `tree` / `client` alone for the transfers still holding an `Arc`. Tearing it down here once killed a live
  NAS copy.
- **`write_from_stream` uses a cloned `Connection` + owned `FileWriter`**, ❌ never a borrowed one holding the client
  mutex across the upload (the QNAP deadlock). Error paths must `abort()` then delete the partial, or corrupt bytes
  linger at the destination.
- **An unreachable request fails instead of hanging** (20 s to connect, 30 s of server silence, watcher included).
  ❌ Never read a missed keepalive as death.
- **`to_smb_path` matches the root by COMPONENT and `NotFound`s anything outside it**; guessing sent real requests to
  the wrong place. Anchoring a volume-relative path is the CALLER's job.
- **Watcher filenames need NFC→NFD normalizing and ❌ nothing else**: smb2 already decodes separators, so a `\` in a
  filename is part of its NAME; re-normalizing loses the entry.

## Local and MTP must-knows

- **`list_directory` must feed `on_progress`; ❌ never `_on_progress`.** It's the pane's only "Loaded N files…" signal,
  so dropping it strands a big folder on "Opening folder…".
- **So must a copy SCAN** (`scan_for_copy_batch_with_progress`; SMB threads a `ScanTicker` through its recursion): the
  transfer dialog's only climbing counter, AND the scan watchdog's proof the device is answering, so a silent backend is
  cut off as unresponsive (`write_operations/DETAILS.md` § "Bounding the scan").
- **`LocalPosixVolume::write_from_stream` `sync_data`s each file** (+ best-effort parent-dir fsync) before returning:
  every cross-volume copy landing on local disk flows through it, and `flush()` alone loses data on eject.
- **`MtpVolume::get_metadata` lists the whole parent directory** (MTP has no single-file stat): avoid it in hot paths.
- **`MtpReadStream` reads in bounded windows, freeing the session between them**; ❌ don't re-add a `Drop`/cancel, and
  `read_range` takes `read_range_direct`, ❌ not a read session (archive extraction issues one per 256 KiB).

Per-backend decisions, the scan pool, supersede-vs-unmount, re-rooting, and the SMB auto-upgrade / reconnect
lifecycles: `DETAILS.md`. Read it before any non-trivial work here.
