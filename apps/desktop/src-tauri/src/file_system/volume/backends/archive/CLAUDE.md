# Archive backend (zip, tar, 7z)

Presents an archive file as a browsable folder. Two layers: a **reading core** ([`read/`](read/CLAUDE.md), parse →
synthetic tree, streaming decompress, `Volume`-free) and **`ArchiveVolume`** (`volume.rs`), the `Volume` built on it.
The core is decoupled from the `Volume` trait (archive-native `ArchiveIndex` / `ArchiveNode` / `ArchiveError`);
`volume.rs` alone maps them onto `FileEntry` / `VolumeError`.

Formats: **zip** browses + extracts + **writes**; **tar / tar.gz / tar.xz / tar.bz2 / tar.zst / 7z** browse + extract,
**read-only**.

## Module map

- `volume.rs`: `ArchiveVolume` + `VolumeByteSource` — the only file that touches the `Volume` trait.
- `boundary.rs`: SHARED boundary detector + per-format magic (used by `VolumeManager::resolve` and
  `commands/volumes.rs`; two copies would drift).
- [`read/`](read/CLAUDE.md): the `Volume`-free reading engine (all formats). Zip Slip, DoS caps, sans-IO fsm, codecs.
- [`mutation/`](mutation/CLAUDE.md): the zip-only temp+rename write side.
- [`watch/`](watch/CLAUDE.md): the live content watch on the backing `.zip`.

Depth, rationale, routing, and remote-backed archives: [DETAILS.md](DETAILS.md). Read it before any non-trivial work
here: editing, planning, reorganizing, or advising.

## Routing must-knows

- **Format is decided by NAME SUFFIX (`format_for_name`, the single source of truth), then confirmed by per-format
  magic (`boundary.rs`).** Longest-suffix wins: `.tar.gz` is a gzip tar, a bare `.gz` is not an archive. `boundary.rs`
  is shared with `VolumeManager::resolve`; don't fork a second detector.
- **This backend is headless: it never registers itself.** `VolumeManager::resolve` mints an `ArchiveVolume` on demand,
  routes `.zip`-crossing paths here (async — a remote `.zip` is confirmed through the parent, not `std::fs`), and
  LRU-caps it. Every read site re-resolves from `(parent_id, full_path)`, so eviction is safe.
- **Only zip is WRITABLE** — the write chokepoint (`write_operations::archive_edit::ensure_zip_writable`) refuses a
  non-zip target typed and untouched before the [mutator](mutation/CLAUDE.md) sees it.

## `ArchiveVolume` must-knows (`volume.rs`)

- **Read-only at this layer: every mutation method returns `NotSupported`, including `create_directory_all`** (overridden
  — the trait default falsely returns `Ok` on an existing dir). Edits route path-based to the mutator, never through
  these methods.
- **`lane_key()` and `get_space_info()` delegate to the PARENT volume, never the archive** — the parent owns the
  serialization lane and real disk cost, and this dodges a false `available = 0` disk-full block.
- **Local vs remote byte source is picked by `parent.supports_local_fs_access()`, NOT by whether the path opens
  locally** — a direct-SMB parent must read through the parent, never its possibly-hung OS mount.
- **`listing_is_watched` reflects the live [watch](watch/CLAUDE.md)** — `true` only while the local content watch is
  established (never for a remote parent). `supports_watching` stays `false` (the generic FSEvents dir-watcher can't
  watch an archive-inner path).
