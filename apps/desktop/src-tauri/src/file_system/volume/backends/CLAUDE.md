# Volume backends

Per-backend `Volume` impls. Trait shape, capabilities, streaming patterns, "Building a new volume": `../CLAUDE.md`
+ `../DETAILS.md`.

## Module map

- `local_posix.rs`, `archive.rs` (re-export of `crates/cmdr-archive`: zip/tar/7z), `mtp/`, `smb/` + `smb_watcher.rs`;
  both remote backends split by concern (`volume_impl` = the whole `impl Volume`, `streams`, `mapping`, plus SMB's
  share-scoped `state` + `reconnect` on `SmbVolumeInner`). `InMemoryVolume` rides with the trait in `cmdr-fs`.

## SMB must-knows

- **The watcher runs on a DEDICATED session** (stacked CHANGE_NOTIFY long-polls wedge Samba) **and never reconnects
  itself**: on death it kicks the ONE reconnect path (`spawn_watcher_death_reconnect`), which respawns it AND resumes
  the index. ❌ No second reconnect loop, and ❌ never cancel it on a pane close.
- **`SmbVolume` is a per-mount-root instance over a shared `Arc<SmbVolumeInner>`**; `rerooted` moves a share to
  another mount for one allocation, and share-scoped background work reads `SmbVolumeInner::self_handle()` rather than
  the volume id, which answers with the SUCCESSOR after a swap.
- **A replaced volume is SUPERSEDED, never unmounted**: `on_superseded` retires the id-scoped parts and leaves
  `state` / `tree` / `client` alone for the transfers still holding an `Arc` (tearing it down once killed a live NAS
  copy). ❌ A promotion must never call either hook on the instance it replaces: both act on the SHARED session.
- **`paths_are_os_visible()` tracks the MOUNT, not the backend kind** (latched off by `note_root_mount_gone`). ❌ Never
  hardcode it `true`: smb2 keeps browsing a share whose mount is gone, so the drag it breaks fails silently.
- **`write_from_stream` drives an owned `FileWriter` on a cloned `Connection`**, ❌ never one borrowed while the client
  mutex is held across the upload (the QNAP deadlock). Error paths `abort()` then delete the partial, or corrupt bytes
  linger at the destination.
- **Background bulk work draws on the refcounted pool of extra sessions** (`smb/scan_pool.rs`); a dead member retries on
  a sibling and ❌ never moves the MAIN volume's connection state.
- **smb2 bounds every wait itself**: ❌ no timeout layer of ours, and ❌ never read a missed keepalive as death.
- **`to_smb_path` matches the root by COMPONENT and `NotFound`s anything outside it**; guessing sent real requests to
  the wrong place.
- **Watcher filenames need NFC→NFD normalizing and ❌ nothing else**: smb2 already decodes separators, so a `\` in a
  filename is part of its NAME; re-normalizing loses the entry.

## Local and MTP must-knows

- **Feed the progress callbacks** in `list_directory` and in a copy SCAN (`scan_for_copy_batch_with_progress`);
  ❌ never quiet one to `_on_progress`. They drive the pane's only "Loaded N files…" readout and the transfer dialog's
  only climbing counter, and the scan one is the watchdog's proof the device is answering: a silent backend gets cut
  off as unresponsive.
- **`LocalPosixVolume::write_from_stream` `sync_data`s each file** (+ best-effort parent-dir fsync) before returning:
  every cross-volume copy landing on local disk flows through it, and `flush()` alone loses data on eject.
- **MTP has no single-file stat**, so `get_metadata` lists the whole parent: avoid it in hot paths. Ranged reads and
  read sessions are canonical in `mtp/connection/CLAUDE.md`.

Per-backend decisions, the scan pool, supersede-vs-unmount, re-rooting, and the SMB auto-upgrade / reconnect
lifecycles: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
