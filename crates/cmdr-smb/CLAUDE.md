# `cmdr-smb`

Everything Cmdr says to an SMB server: the `SmbVolume` backend and the protocol layer under it. No `tauri`, no app.
Discovery, the keychain, mounts, upgrades, and every human-facing word stay in the app's `network/`.

## Module map

- `src/volume/`: the backend — `mod.rs` (structs), `volume_impl.rs` (the whole `impl Volume`), and one module per
  concern (`paths`, `query`, `mutation`, `session`/`reconnect`/`state`, `scan`/`scan_pool`, `streams`, `mapping`,
  `foreground_yield`, `watcher/`, `testing` for the Docker fixtures).
- `src/{types,errors,connection}.rs`: share-listing vocabulary, `smb2::Error` classification, the address builder.
  Re-exported at the root, so callers write `cmdr_smb::`.

## Backend must-knows

- **Everything the backend asks the app goes through the `VolumeHost`** taken in `connect_smb_volume`, kept on
  `SmbVolumeInner` (seams: `crates/cmdr-fs/src/volume/host/CLAUDE.md`). Background work spawns onto `host.runtime()`.
- **The watcher runs on a DEDICATED session** (stacked CHANGE_NOTIFY long-polls wedge Samba) **and never reconnects
  itself**: on death it kicks the ONE reconnect path (`spawn_watcher_death_reconnect`), which respawns it and resumes
  the index. ❌ No second loop, no cancel on pane close.
- **`SmbVolume` is a per-mount-root instance over a shared `Arc<SmbVolumeInner>`**; share-scoped background work reads
  `SmbVolumeInner::self_handle()`, ❌ never the volume id (the SUCCESSOR's after a swap).
- **A replaced volume is SUPERSEDED, never unmounted**: `on_superseded` retires the id-scoped parts and leaves `state` /
  `tree` / `client` alone for transfers still holding an `Arc` (tearing it down once killed a live NAS copy). ❌ A
  promotion calls neither hook on the instance it replaces: both act on the SHARED session.
- **`paths_are_os_visible()` tracks the MOUNT, not the backend kind** (latched off by `note_root_mount_gone`). ❌ Never
  hardcode `true`: smb2 browses on past a dead mount, so the drag it breaks fails silently.
- **`write_from_stream` drives an OWNED `FileWriter` on a cloned `Connection`**, ❌ never one borrowed while the client
  mutex is held across the upload (the QNAP deadlock). Error paths `abort()` then delete the partial, or corrupt bytes
  stay.
- **Streaming-write progress reports `FileWriter::bytes_written()`** (server-confirmed), ❌ never bytes handed to the
  pipeline: `write_chunk` returns on ACCEPTANCE.
- **A read that knows its size sends `read_file_compound_sized`**: unsized it charges credits for a whole `max_read`
  (130 for a 4 MB file), which parked seven of ten slots on a 300 GB copy. ❌ Don't tune `max_concurrent_ops`'s credit
  clamp; it divides a constant, so it's inert.
- **`scan_recursive` asks its `ScanBoundary` per entry, `dir()` BEFORE the listing** (`DETAILS.md` § "Scanning", which
  also says what the batch scan owns and what it borrows from `cmdr_fs::volume::scan_walk`).
- **Bulk work draws on the refcounted pool of extra sessions** (`scan_pool.rs`); a dead member retries on a sibling, ❌
  never moving the MAIN volume's connection state.
- **smb2 bounds every wait itself**: ❌ no timeout layer of ours, never a missed keepalive read as death.
- **`to_smb_path` matches the root by COMPONENT and `NotFound`s anything outside it**; guessing sent real requests
  somewhere wrong.
- **Watcher filenames need NFC→NFD normalizing and ❌ nothing else**: smb2 already decodes separators, so a `\` in a
  filename is part of its NAME; re-normalizing loses the entry.

## Crate must-knows

- **Verify with `cargo check -p cmdr-smb --all-targets`.** ❌ Nothing here may name the app, and the public surface is
  capped (`index-crate-isolation`).
- ❌ **No user-facing prose here**: an error `message` is a log diagnostic; the host renders what humans read.
- ❌ Never gate behavior on `cfg(test)`; use `any(test, feature = "testing")`, or it flips silently when a consumer
  compiles this crate.

Reconnect and scan-pool lifecycles, `rerooted`, credits and copy concurrency, the `specta` pin, NFC share names, test
placement, decisions, and suites: `DETAILS.md`. Read it first.
