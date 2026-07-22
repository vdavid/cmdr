# Indexing path arithmetic

Pure, lock-free helpers that map between the filesystem's absolute paths and each volume's index path space. This area
is the canonical owner of `IndexPathSpace` and the read-side path transforms.

## Must-knows

- **Three path spaces, one trap.** The same path string lives in three spaces in the local pipeline: `resolve_path`
  wants the **index-relative** path; `read_dir` / `symlink_metadata` / `Path::exists` want the **absolute FS** path;
  `emit_dir_updated` / the FE `index-dir-updated` payload want the **absolute** path (to match pane paths). Apply the
  mount-relative strip ONLY at each `IndexPathSpace::resolve_abs` argument. Every path SET (`affected_paths`,
  `pending_paths`, `new_dir_paths`) and every dedup key stays ABSOLUTE (via `absolute()`). Strip at set insertion and
  you break the FS reads and the FE emit; omit it and you break resolution. Wrong space ⇒ silently dropped live events
  or a false-complete scan.
- **`IndexPathSpace` is built once per scan/loop** (`for_volume(kind, root, inodes_trustworthy)`): boot disk passes
  through (absolute == index-relative after firmlink normalization), a `mount_rooted()` volume strips its mount root via
  the SAME `transports::smb::watch::index_relative_path` the SMB read/write sides use — never a second copy.
- **`absolute(raw)` firmlink-normalizes for the boot disk, is identity for a mount-rooted drive.** Firmlink semantics
  are boot-disk-only; they must NOT touch virtual SMB/MTP paths.
- **`index_read_path` is the read-side mirror**: pass-through for `root`, mount-relative strip for SMB, `mtp://` scheme
  strip for MTP. `None` ⇒ the path isn't in this volume's index ⇒ the caller skips (like an unindexed volume), never
  mis-roots it at `ROOT_ID`.
- **`trust_inode` nulls the inode on a FAT/exFAT drive** (`inodes_trustworthy == false`): a derived, unstable inode
  must never reach the index and drive the local rename pre-pass into a false `MoveEntryV2`. See
  [`../transports`](../transports/CLAUDE.md) for where the flag is resolved.
- **Route by what's REGISTERED, never a path/id substring.** `volume_id_for_local_path` fast-rejects with
  `is_on_mounted_external_volume` so a cloud-drive folder in the home dir stays on `root` and keeps its sizes.

## Module map

- `routing.rs` — `volume_id_for_local_path` (path → owning volume), `IndexPathSpace`, `index_read_path[_pure]` +
  `mtp_index_relative_path`, `exclusion_scope_for_volume`.
- `firmlinks.rs` — parse `/usr/share/firmlinks`, normalize to canonical form (`/System/Volumes/Data/...` → `/...`,
  `/tmp` → `/private/tmp`).
- `path_prefix.rs` — component-aware absolute-path prefix tests (`/a/bc` is never a child of `/a/b`).

Owned elsewhere: the exclusion policy (`should_exclude`, `ExclusionScope`, pseudo-filesystem detection) lives in
[`../scanner`](../scanner/CLAUDE.md); the SQLite `resolve_path` in [`../store`](../store/CLAUDE.md); the write-side
mount transforms in [`../transports`](../transports/CLAUDE.md).

`IndexPathSpace`, the routing tiers, and firmlink normalization: [DETAILS.md](DETAILS.md). Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
