# `cmdr-archive`

Presents an archive file as a browsable folder. **The first storage backend to live in its own crate, so it's the worked
example**: writing `cmdr-ftp`? Read `DETAILS.md` § "The pattern, for the next backend crate" before this.

Two layers: a **reading core** (`src/read/CLAUDE.md`, parse → synthetic tree, streaming decompress, `Volume`-free) and
**`ArchiveVolume`** (`src/volume.rs`), the `Volume` built on it. The core speaks archive-native types (`ArchiveIndex` /
`ArchiveNode` / `ArchiveError`); `volume.rs` alone maps them onto `FileEntry` / `VolumeError`.

Formats: **zip** browses + extracts + **writes**; **tar / tar.gz / tar.xz / tar.bz2 / tar.zst / 7z** browse + extract,
**read-only**.

## Module map

- `src/volume.rs`: `ArchiveVolume` + `VolumeByteSource` — the only module that touches the `Volume` trait.
- `src/boundary.rs`: the SHARED boundary detector + per-format magic (the host's routing and its volume commands both
  call it; two copies would drift).
- `src/read/CLAUDE.md`: the `Volume`-free reading engine (all formats). Zip Slip, DoS caps, sans-IO fsm, codecs.
- `src/mutation/CLAUDE.md`: the zip-only temp+rename write side.
- `src/watch/CLAUDE.md`: the live content watch on the backing file.
- `src/test_fixtures.rs`: fixture builders, `pub` under the `testing` feature so the HOST's archive tests use them too.

Depth, rationale, routing, and remote-backed archives: `DETAILS.md`. Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.

## Crate must-knows

- **`cargo check -p cmdr-archive` is the whole verification loop.** Nothing here may name the app. Enforced by
  `index-crate-isolation`, which forbids `tauri` / `tauri-specta` / `cmdr` anywhere in this crate's dependency tree and
  caps its public surface — so ❌ don't reach for `pub` as a compile fix.
- **Everything the app answers arrives through the `VolumeHost` given to `ArchiveVolume::new`.** Today only the watch
  uses it (`host.runtime()` to spawn, `host.listings()` to refresh). ❌ Never `tokio::spawn`, and never a static of your
  own. `crates/cmdr-fs/src/volume/host/CLAUDE.md` is the seam list.
- **❌ Never gate behavior on `cfg(test)`; use `any(test, feature = "testing")`.** `cfg(test)` is set only while THIS
  crate builds its own test target, so a consumer's test build silently gets the production arm. This project has been
  bitten three times.
- **`#![deny(missing_docs)]` holds.** A new `pub` item, field, or enum variant needs a doc comment.

## Routing must-knows

- **Format is decided by NAME SUFFIX (`format_for_name`, the single source of truth), then confirmed by per-format magic
  (`boundary.rs`).** Longest-suffix wins: `.tar.gz` is a gzip tar, a bare `.gz` is not an archive. The host's routing
  shares `boundary.rs`; don't fork a second detector.
- **This backend is headless: it never registers itself.** The host mints an `ArchiveVolume` on demand, routes
  archive-crossing paths to it, and LRU-caps it. Every read site re-resolves, so eviction is safe.
- **Only zip is WRITABLE** — the host refuses a non-zip target, typed and untouched, before the
  [mutator](src/mutation/CLAUDE.md) sees it.

## `ArchiveVolume` must-knows (`src/volume.rs`)

- **Read-only at this layer: every mutation method returns `NotSupported`, including `create_directory_all`**
  (overridden — the trait default falsely returns `Ok` on an existing dir). Edits route path-based to the mutator, never
  through these methods.
- **`lane_key()` and `get_space_info()` delegate to the PARENT volume, never the archive** — the parent owns the
  serialization lane and real disk cost, and this dodges a false `available = 0` disk-full block.
- **Local vs remote byte source is picked by `parent.supports_local_fs_access()`, NOT by whether the path opens
  locally** — a direct-SMB parent must read through the parent, never its possibly-hung OS mount.
- **`listing_watch_coverage` reflects the live [content watch](src/watch/CLAUDE.md)** — covered only while the local
  content watch is established (never for a remote parent), and capped by the CEILING the caller armed it with, so an
  archive on an OS-mounted share reports `ThisMachineOnly`. `can_watch_listings` stays `false` (a generic FSEvents
  dir-watcher can't watch an archive-inner path).
