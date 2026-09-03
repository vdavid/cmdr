# `cmdr-smb`

Everything Cmdr says to an SMB server: the `SmbVolume` backend and the protocol layer under it. No `tauri`, no app.
Discovery, the keychain, mounts, upgrades, and every human-facing word stay in the app's `network/`.

## Module map

- `src/volume/`: the backend. `mod.rs` (the structs), `volume_impl.rs` (the whole `impl Volume`), then one module per
  concern: `paths`, `query`, `mutation`, `session` + `reconnect` + `state`, `scan` + `scan_pool`, `streams`, `mapping`,
  `foreground_yield`, `watcher/`, `testing` (the Docker fixtures).
- `src/{types,errors,connection}.rs`: share-listing vocabulary, `smb2::Error` classification, the address builder.
  Re-exported at the root, so callers write `cmdr_smb::`.

## Backend must-knows

- **Everything the backend asks the app goes through the `VolumeHost`** taken in `connect_smb_volume`, kept on
  `SmbVolumeInner` (seams: `crates/cmdr-fs/src/volume/host/CLAUDE.md`). Background work spawns onto `host.runtime()`.
- **The watcher runs on a DEDICATED session** (stacked CHANGE_NOTIFY long-polls wedge Samba) **and never reconnects
  itself**: on death it kicks the ONE reconnect path (`spawn_watcher_death_reconnect`), which respawns it AND resumes
  the index. ❌ No second loop, no cancel on pane close.
- **`SmbVolume` is a per-mount-root instance over a shared `Arc<SmbVolumeInner>`**; `rerooted` moves a share to another
  mount for one allocation, and share-scoped background work reads `SmbVolumeInner::self_handle()`, never the volume id
  (the SUCCESSOR's after a swap).
- **A replaced volume is SUPERSEDED, never unmounted**: `on_superseded` retires the id-scoped parts and leaves `state` /
  `tree` / `client` alone for transfers still holding an `Arc` (tearing it down once killed a live NAS copy). ❌ A
  promotion must never call either hook on the instance it replaces: both act on the SHARED session.
- **`paths_are_os_visible()` tracks the MOUNT, not the backend kind** (latched off by `note_root_mount_gone`). ❌ Never
  hardcode `true`: smb2 browses on past a dead mount, so the drag it breaks fails silently.
- **`write_from_stream` drives an OWNED `FileWriter` on a cloned `Connection`**, ❌ never one borrowed while the client
  mutex is held across the upload (the QNAP deadlock). Error paths `abort()` then delete the partial, or corrupt bytes
  stay.
- **Streaming-write progress reports `FileWriter::bytes_written()`** (server-confirmed), ❌ never bytes handed to the
  pipeline: `write_chunk` returns on ACCEPTANCE, so a slow link diverges by `concurrency x window`. Size any test
  reaching this path off `negotiated_max_write()`, or it silently takes the compound path.
- **A read that knows its size sends `read_file_compound_sized`**: unsized, it charges credits for a whole `max_read`
  (130 for a 4 MB file), which parked seven of ten slots on a 300 GB copy. `max_concurrent_ops` also clamps by
  `credit_capacity_for`, but ❌ that clamp is INERT and tuning it won't help: it divides a constant, not the grant.
- **The batch scan keeps its own oracle short-circuit** (the watcher earns the `authoritative_listing` shortcut); the
  conflict matcher and batch fold are `cmdr_fs::volume::scan_walk`'s, so every backend hands a conflict dialog the same
  shape. **`scan_recursive` asks its `ScanBoundary` per entry, `dir()` BEFORE the listing** — `DETAILS.md` § "Scanning".
- **Bulk work draws on the refcounted pool of extra sessions** (`scan_pool.rs`); a dead member retries on a sibling, ❌
  never moving the MAIN volume's connection state.
- **smb2 bounds every wait itself**: ❌ no timeout layer of ours, never a missed keepalive read as death.
- **`to_smb_path` matches the root by COMPONENT and `NotFound`s anything outside it**; guessing sent real requests
  somewhere wrong.
- **Watcher filenames need NFC→NFD normalizing and ❌ nothing else**: smb2 already decodes separators, so a `\` in a
  filename is part of its NAME; re-normalizing loses the entry.
- **`SmbConnectionParams` carries NFC names; build it with `new`, ❌ never a struct literal off a raw `statfs` name**: a
  decomposed share is answered `STATUS_BAD_NETWORK_NAME`.

## Crate must-knows

- **`cargo check -p cmdr-smb --all-targets` is the whole verification loop**: nothing here may name the app
  (`index-crate-isolation` forbids `tauri` / `tauri-specta` / `cmdr`).
- **`specta` is pinned to the app's exact version.** Two `specta` crates in one graph break these `Type` impls for
  `tauri-specta`, which the app's commands collect transitively.
- **A test reading `.inner` belongs HERE**; `volume::testing` ❌ never hands it out. Which side a cell lives on:
  `DETAILS.md`.
- **`#![deny(missing_docs)]` holds**, ❌ no user-facing prose: `ShareListError`'s `message` is a log diagnostic; the
  host renders what humans read.
- ❌ Never gate behavior on `cfg(test)`; use `any(test, feature = "testing")`, or it flips silently when a consumer
  compiles this crate.

Reconnect and scan-pool lifecycles, re-rooting, decisions, and suites: `DETAILS.md`. Read it first.
