# `cmdr-archive`

Presents an archive file as a browsable folder. **The first storage backend to live in its own crate, so it's the worked
example**: writing `cmdr-ftp`? Read `DETAILS.md` § "The pattern, for the next backend crate" before this.

Two layers: a `Volume`-free **reading core** (`src/read/CLAUDE.md`) that parses an archive into a synthetic tree, and
**`ArchiveVolume`** (`src/volume.rs`), the only place its archive-native types map onto `FileEntry` / `VolumeError`. Zip
browses, extracts, and **writes**; tar (every codec), 7z, and OOXML are **read-only**.

## Module map

- `src/volume.rs`: `ArchiveVolume` + `VolumeByteSource` — the only module that touches the `Volume` trait.
- `src/boundary.rs`: the SHARED boundary detector + per-format magic, called by the host's routing and its volume
  commands alike.
- `src/read/CLAUDE.md` (the reading engine: Zip Slip, DoS caps, sans-IO fsm, codecs), `src/mutation/CLAUDE.md` (the
  zip-only temp+rename write side), `src/watch/CLAUDE.md` (the live content watch).
- `src/test_fixtures.rs`: fixture builders, `pub` under the `testing` feature so the HOST's archive tests use them too.

## Crate must-knows

- **`cargo check -p cmdr-archive` is the whole verification loop**, and nothing here may name the app:
  `index-crate-isolation` keeps `tauri` / `cmdr` out of the dependency tree and caps the public surface, so ❌ don't
  reach for `pub` as a compile fix. `#![deny(missing_docs)]` holds too.
- **Everything the app answers arrives through the `VolumeHost` given to `ArchiveVolume::new`** (today only the watch,
  via `host.runtime()` and `host.listings()`). ❌ Never `tokio::spawn`, and never a static of your own.
  `crates/cmdr-fs/src/volume/host/CLAUDE.md` is the seam list.
- **❌ Never gate behavior on `cfg(test)`; use `any(test, feature = "testing")`.** `cfg(test)` is set only while THIS
  crate builds its own test target, so a consumer's test build silently gets the production arm. Bitten three times.

## Routing must-knows

- **Format is decided by NAME SUFFIX (`format_for_name`, the single source of truth), then confirmed by per-format magic
  (`boundary.rs`).** Longest-suffix wins: `.tar.gz` is a gzip tar, a bare `.gz` is not an archive. ❌ Don't fork a
  second detector; the host shares this one.
- **This backend is headless: it never registers itself.** The host mints an `ArchiveVolume` on demand, routes
  archive-crossing paths to it, and LRU-caps it. Every read site re-resolves, so eviction is safe.
- **Only `ArchiveFormat::Zip` is WRITABLE** — the host refuses every other format, typed and untouched, before the
  [mutator](src/mutation/CLAUDE.md) sees it. `Ooxml` (`.docx`/`.jar`/…) exists to ride that refusal: those ARE zips, so
  sharing `Zip` would let the mutator rewrite a user's document. ❌ Never fold it in. DETAILS § "document container".

## `ArchiveVolume` must-knows (`src/volume.rs`)

- **Read-only at this layer: every mutation method returns `NotSupported`**, `create_directory_all` included — it's
  overridden because the trait default falsely answers `Ok` for a dir that already exists. Edits route path-based to the
  mutator, never through these methods.
- **`lane_key()`, `get_space_info()`, and `get_space_info_at()` delegate to the PARENT volume, never the archive** — the
  parent owns the serialization lane and the real disk cost (asked at the `.zip`'s own path, so an SD card answers for
  itself), and this dodges a false `available = 0` disk-full block.
- **Local vs remote byte source is picked by `parent.supports_local_fs_access()`, NOT by whether the path opens
  locally** — a direct-SMB parent must read through the parent, never its possibly-hung OS mount.

Depth, rationale, routing, remote-backed archives, and the capability flags: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
