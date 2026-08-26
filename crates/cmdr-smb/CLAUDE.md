# `cmdr-smb`

Everything Cmdr says to an SMB server: the `SmbVolume` backend and the protocol layer under it. No `tauri`, no app.
Discovery, the keychain, mounts, the upgrade passes, and every word a human reads stay in the app's `network/`.

## Module map

- `src/volume/`: the backend. `mod.rs` (the two structs and the shared prelude), `volume_impl.rs` (the whole
  `impl Volume`: capability flags, one-line delegators for the rest), then one module per concern: `paths`, `query`,
  `mutation`, `session` + `reconnect` + `state`, `scan` + `scan_pool`, `streams`, `mapping`, `foreground_yield`,
  `watcher/`, `testing` (the Docker fixtures).
- `src/{types,errors,connection}.rs`: the share-listing vocabulary, `smb2::Error` classification, the address builder.
  Re-exported at the root, so callers write `cmdr_smb::`.

## Backend must-knows

- **Everything the backend asks the app goes through the `VolumeHost`** it takes in `connect_smb_volume` and keeps on
  `SmbVolumeInner` (seams: `crates/cmdr-fs/src/volume/host/CLAUDE.md`). Background work spawns onto `host.runtime()`.
- **The watcher runs on a DEDICATED session** (stacked CHANGE_NOTIFY long-polls wedge Samba) **and never reconnects
  itself**: on death it kicks the ONE reconnect path (`spawn_watcher_death_reconnect`), which respawns it AND resumes
  the index. ❌ No second reconnect loop, and never cancel it on a pane close.
- **`SmbVolume` is a per-mount-root instance over a shared `Arc<SmbVolumeInner>`**; `rerooted` moves a share to another
  mount for one allocation, and share-scoped background work reads `SmbVolumeInner::self_handle()`, never the volume id
  (which answers with the SUCCESSOR after a swap).
- **A replaced volume is SUPERSEDED, never unmounted**: `on_superseded` retires the id-scoped parts and leaves `state` /
  `tree` / `client` alone for the transfers still holding an `Arc` (tearing it down once killed a live NAS copy). ❌ A
  promotion must never call either hook on the instance it replaces: both act on the SHARED session.
- **`paths_are_os_visible()` tracks the MOUNT, not the backend kind** (latched off by `note_root_mount_gone`). ❌ Never
  hardcode it `true`: smb2 keeps browsing a share whose mount is gone, so the drag it breaks fails silently.
- **`write_from_stream` drives an OWNED `FileWriter` on a cloned `Connection`**, ❌ never one borrowed while the client
  mutex is held across the upload (the QNAP deadlock). Error paths `abort()` then delete the partial, or corrupt bytes
  linger.
- **Streaming-write progress reports `FileWriter::bytes_written()`** (server-confirmed), ❌ never bytes handed to the
  pipeline: `write_chunk` returns on ACCEPTANCE, so on a slow link the two diverge by `concurrency x window`. Size any
  test meant to reach this path off `negotiated_max_write()`, or it silently takes the compound path.
- **Bulk work draws on the refcounted pool of extra sessions** (`scan_pool.rs`); a dead member retries on a sibling and
  ❌ never moves the MAIN volume's connection state.
- **smb2 bounds every wait itself**: ❌ no timeout layer of ours, and never read a missed keepalive as death.
- **`to_smb_path` matches the root by COMPONENT and `NotFound`s anything outside it**; guessing sent real requests to
  the wrong place.
- **Watcher filenames need NFC→NFD normalizing and ❌ nothing else**: smb2 already decodes separators, so a `\` in a
  filename is part of its NAME; re-normalizing loses the entry.

## Crate must-knows

- **`cargo check -p cmdr-smb --all-targets` is the whole verification loop**, because nothing here may name the app:
  `index-crate-isolation` forbids `tauri` / `tauri-specta` / `cmdr` in this crate's tree.
- **`specta` is pinned to the app's exact version.** Two `specta` crates in one graph make these `Type` impls stop
  satisfying `tauri-specta`, and the app's command signatures collect them transitively.
- **A test that reads `.inner` belongs HERE**, and `volume::testing` ❌ never hands `.inner` out. Which side a cell
  lives on: `DETAILS.md`.
- **`#![deny(missing_docs)]` holds**, and ❌ no user-facing prose: a `message: String` on `ShareListError` is a log
  diagnostic, and the host renders every word a human reads.
- ❌ Never gate behavior on `cfg(test)`; use `any(test, feature = "testing")`, or it flips silently the moment a
  consumer compiles this crate as a dependency.

Reconnect and scan-pool lifecycles, re-rooting, the decisions, and the suites: `DETAILS.md`. Read it first.
