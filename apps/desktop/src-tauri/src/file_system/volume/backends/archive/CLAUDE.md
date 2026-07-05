# Archive backend (zip, tar, 7z)

Two layers: a **read core** (parse → synthetic tree; streaming decompress) and `ArchiveVolume`, the `Volume` built on
it. The core is **decoupled from the `Volume` trait** (archive-native `ArchiveIndex` / `ArchiveNode` / `ArchiveError`);
`volume.rs` alone maps them onto `FileEntry` / `VolumeError`. Keep the core submodules `Volume`-free.

Formats: **zip** browses + extracts + **writes**; **tar / tar.gz / tar.xz / tar.bz2 / tar.zst / 7z** browse + extract,
**read-only**. Only parsing and the per-entry read handle differ per format; the rest is shared.

## Module map

- `volume.rs`: `ArchiveVolume` + `VolumeByteSource` — the only file that touches the `Volume` trait.
- Read core (`Volume`-free): `index.rs` (tree, query surface, `EntryStore` dispatch, DoS caps), `format.rs`
  (`ArchiveFormat`, `format_for_name`, `is_sequential`), `zip.rs` / `tar.rs` / `sevenz.rs` (per-format parse +
  producer), `source.rs` (`ArchiveByteSource` seam + sources), `read.rs`, `name.rs` (`sanitize_entry_name`), `cache.rs`,
  `watch.rs`.
- `boundary.rs`: SHARED boundary detector + per-format magic (used by `VolumeManager::resolve` and
  `commands/volumes.rs`; two copies would drift).
- `mutator.rs`: the WRITE side (ZIP ONLY, temp+rename), `Volume`-free.

Depth, rationale, and the full test list: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here.

## Must-knows

- **Zip Slip: `sanitize_entry_name` is the single choke point every entry passes before entering the tree; don't
  bypass it.** No `Accepted` inner path escapes its root. Don't swap in rc-zip's coarser `Entry::sanitized_name`.
- **Format is decided by NAME SUFFIX (`format_for_name`, the single source of truth), then confirmed by per-format
  magic.** Longest-suffix wins: `.tar.gz` is a gzip tar, a bare `.gz` is not an archive.
- **Only zip is WRITABLE.** The write chokepoint is `write_operations::archive_edit::ensure_zip_writable` (non-zip →
  typed `ReadOnlyDevice`, untouched). Don't route a non-zip archive to the mutator.
- **We drive rc-zip's sans-IO fsm directly, NOT `rc-zip-tokio`** (which borrows its handle and decompresses on the
  executor).
- **Decompression runs on `spawn_blocking`; reads are chunked, never whole-entry buffered** — don't add a whole-entry
  `Vec` to the read path.
- **Compressed tar and 7z are SEQUENTIAL-access** (`Volume::extraction_is_sequential`); plain `.tar` and zip random.
- **Local vs remote byte source is picked by `parent.supports_local_fs_access()`, NOT by whether the path opens
  locally** — a direct-SMB parent must read through the parent, never its possibly-hung OS mount.
- **Encryption: browsing works, extraction doesn't** (`has_encrypted_entries()` gates up front). Filename encoding is
  rc-zip's job — consume the decoded `entry.name`, don't re-decode.
- **The index cache key is `(path, size, mtime)`** (external edits auto-invalidate); `index_for_local` is blocking, call
  it from `spawn_blocking`.

## Zip mutation (`mutator.rs`)

- **Edits go through `mutator.rs` + the write-ops `ArchiveEditOperation` driver, NOT `ArchiveVolume`'s mutation
  methods** — routing is path-based and backend-side, so those stay `NotSupported` and nothing calls them.
- **Temp+rename is the ONLY strategy; never `ZipWriter::new_append`** (it corrupts the archive on cancel). The original
  is byte-for-byte intact until the final atomic rename.
- **An edit that would RETAIN an encrypted entry is refused** (`zip`'s raw copy drops the PKWARE flag → silent
  corruption). Deleting an encrypted entry is fine.

## `ArchiveVolume` (the `Volume` layer)

- **Read-only: every mutation method returns `NotSupported`, including `create_directory_all`** (overridden — the trait
  default falsely returns `Ok` on an existing dir).
- **`lane_key()` and `get_space_info()` delegate to the PARENT volume, never the archive** — the parent owns the
  serialization lane and real disk cost, and this dodges a false `available = 0` disk-full block.
- **This layer is headless: it never registers itself.** `VolumeManager::resolve` mints it on demand and routes
  `.zip`-crossing paths here (async — a remote `.zip` is confirmed through the parent, not `std::fs`).
- **Live watch (`watch.rs`): refresh via `refresh_archive_listings` with the PARENT DRIVE id + full `/…/foo.zip/inner`
  path, never the archive id or `notify_directory_changed`.** Watches the `.zip`'s parent DIRECTORY (survives temp+rename
  inode swaps).
