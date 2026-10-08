# `cmdr-smb`

Everything Cmdr says to an SMB server: the `SmbVolume` backend and the protocol layer under it. No `tauri`; discovery,
the keychain, mounts, and every human-facing word stay in the app's `network/`.

## Module map

- `src/volume/`: the backend — `mod.rs` (structs), `volume_impl.rs` (the whole `impl Volume`), and one module per
  concern (`streams`, `session`/`reconnect`, `scan_pool`, `watcher/`, `testing` for the Docker fixtures, and so on).
- `src/{types,errors,connection}.rs`: share-listing vocabulary, `smb2::Error` classification, the address builder.
  Re-exported at the root (`cmdr_smb::`).

## Backend must-knows

- **Everything the backend asks the app goes through the `VolumeHost`** (seams:
  `crates/cmdr-fs/src/volume/host/CLAUDE.md`). Background work spawns onto `host.runtime()`.
- **The watcher runs on a DEDICATED session** (stacked CHANGE_NOTIFY long-polls wedge Samba) **and never reconnects
  itself**: on death it kicks the ONE reconnect path (`spawn_watcher_death_reconnect`). ❌ No second loop, no cancel on
  pane close.
- **`SmbVolume` is a per-mount-root instance over a shared `Arc<SmbVolumeInner>`**; share-scoped background work reads
  `SmbVolumeInner::self_handle()`, ❌ never the volume id (the SUCCESSOR's after a swap).
- **A replaced volume is SUPERSEDED, never unmounted**: `on_superseded` retires the id-scoped parts and leaves the
  session alone for transfers still holding an `Arc` (tearing it down once killed a live NAS copy). ❌ A promotion calls
  neither hook on the replaced instance (both act on the SHARED session).
- **`paths_are_os_visible()` tracks the MOUNT, not the backend kind** (latched off by `note_root_mount_gone`). ❌ Never
  hardcode `true`: smb2 browses on past a dead mount, so the drag it breaks fails silently.
- **`write_from_stream` drives an OWNED `FileWriter` on a cloned `Connection`** (❌ never borrowed under the client
  mutex: the QNAP deadlock); on error, `abort()` then delete the partial. Progress is the server-confirmed
  `bytes_written()`.
- **A copy's date stamps the writer's OWN handle before `finish()`**; ❌ never by path while it's open (its close
  re-stamps).
- **Unknown-length writes always stream**; success includes `finish`'s server-confirmed count.
- **SMB force-rename deletes first**, so its atomic-replace capability stays false; publishers use tracked
  aside/restore.
- **One-frame fast paths stop at smb2's quick limits**: a hinted read at `quick_read_limit()` (sized via
  `read_file_compound_sized`), the write promise at `quick_write_limit()`. ❌ A refused one-frame write to a non-scratch
  name never streams (`one_frame_write_limit`).
- **A streamed read ends at its last byte** (the CLOSE is already out): ❌ don't wait for `None` to drop `chunk_tx`.
- **`scan_recursive` asks its `ScanBoundary` per entry, `dir()` BEFORE the listing** (`DETAILS.md` § "Scanning").
- **Bulk work draws on the refcounted pool of extra sessions** (`scan_pool.rs`); a dead member retries on a sibling, ❌
  never moving the MAIN volume's connection state.
- **smb2 bounds every wait itself**: ❌ no timeout layer of ours.
- **Path conversion matches whole COMPONENTS both ways** (`to_smb_path` / `to_display_path` join and strip
  `share_root`). ❌ Build a volume through `MountAnchor`, never a bare mount path: an anchored mount that loses its
  anchor addresses the share's top (ERR-48RZX).
- **SMB names are opaque bytes** (NFC and NFD mix, ERR-VETBX): ❌ never normalize a share path, wire or watcher key (a
  `\` in a watcher filename is NAME). A foreign path gets exact only via `find_stored_spelling`, ❌ never a fold inside
  an operation (a delete could hit a look-alike). New names compose in the app's write layer (`composes_new_names`), ❌
  never here.

## Crate must-knows

- **Verify with `cargo check -p cmdr-smb --all-targets`.** ❌ Nothing here may name the app, and the public surface is
  capped (`index-crate-isolation`).
- ❌ **No user-facing prose here**: an error `message` is a log diagnostic.
- ❌ Never gate behavior on `cfg(test)`; use `any(test, feature = "testing")`, or it flips silently in a consumer's
  build.

Everything else (lifecycles, anchored mounts, credits, tests, decisions): `DETAILS.md`. Read it first.
