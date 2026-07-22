# Indexing path arithmetic details

Read this before any non-trivial work in `indexing/paths/`: editing, planning, reorganizing, or advising. Must-know
invariants are in [CLAUDE.md](CLAUDE.md).

This area is pure path arithmetic, deliberately separate from the lifecycle/registry core. It is the CANONICAL owner of
`IndexPathSpace`, the read-side path transforms (`index_read_path`), path → volume routing, firmlink normalization, and
the component-aware prefix test. The read query surface ([`../read`](../read/DETAILS.md)) and the progress reporter
([`../events`](../events/DETAILS.md)) both depend on these.

## `IndexPathSpace` — the mount-relative local pipeline (`routing.rs`)

The LOCAL scan/reconcile/live-event pipeline (the guarded walker + FSEvents) had only ever run on `root`, where an
absolute FS path already equals the index-relative path (`ROOT_ID` = `/`). A `LocalExternal` drive is the first
LOCAL-scanned volume that is ALSO **mount-rooted** (`ROOT_ID` = `/Volumes/X`, like SMB/MTP). So every place the local
pipeline calls `store::resolve_path(conn, abs)` would MISS for a mount-rooted index — it walks `Volumes`→`X`→… from the
mount root. `firmlinks::normalize_path` on `event.path` is likewise boot-disk-only.

`IndexPathSpace` is the ONE seam that teaches the pipeline the mount-relative path space. It's built once per scan/loop
from the volume's kind + root + inode-trust (`IndexPathSpace::for_volume`) and threaded through `scan_completion` → the
reconciler + live loop, and through `manager::start_scan` → the scanner + local reconcile. It stores its space AS an
`ExclusionScope` (owned by [`../scanner`](../scanner/DETAILS.md)) and reads the mount root back through it, so the path
space and the exclusion gate can't disagree about where the volume begins. Operations:

- **`absolute(raw)`** — the canonical ABSOLUTE path in this volume's world: `firmlinks::normalize_path` for the boot
  disk, identity for a mount-rooted drive (no firmlink normalization). This is what every path SET holds.
- **`resolve_abs(conn, absolute)`** — resolves that absolute path to an entry id, applying the mount-relative strip for
  a mount-rooted volume (via `transports::smb::watch::index_relative_path`). `Ok(None)` for a path outside the mount
  root, which every caller already treats as skip/no-op.
- **`exclusion_scope()`** — the scope the scan/live gate uses; a mount-rooted scan skips only the per-volume tier (under
  `BootDisk` its own `/Volumes/X` subtree would be excluded and the scan would falsely complete empty). Either way the
  scope carries the volume ROOT, so the root-position pseudo-filesystem skip works on every volume.
- **`is_boot_disk()`** — read back from the scope's mount root. The shallow-`MustScanSubDirs` sweep window branches on
  this (the once-a-day window is boot-disk-only; policy owned by [`../reconcile`](../reconcile/DETAILS.md)).
- **`volume_root_string()`** — `/Volumes/X` for a mount-rooted drive, `/` for the boot disk; stored as the
  `volume_path` meta.

**The three-path-spaces discipline (the trap).** The SAME path string lives in three spaces in the live loop /
reconciler: `store::resolve_path` wants the **index-relative** path; `read_dir` / `Path::exists` / `symlink_metadata`
want the **absolute FS** path; `emit_dir_updated` / the FE `index-dir-updated` payload want the **absolute** path (to
match pane paths). So the mount-relative strip is applied ONLY at each `resolve_abs` argument — `affected_paths` /
`pending_paths` / `new_dir_paths` and every dedup key stay ABSOLUTE (via `absolute()`). Applying the strip at set
insertion breaks the FS reads and the FE emit; omitting it breaks resolution. This is why the discipline is
load-bearing, and why the mount-relative resolution tests pin both the miss (`root` space drops a `/Volumes/X` path)
and the fix (`mount_rooted` space resolves it).

**Root-only sites left as `BootDisk` / `ROOT_VOLUME_ID` (deliberate).** Journal replay (gated on `has_event_journal()`
= boot disk only), post-replay background verification, and the per-navigation verifier don't run for a `LocalExternal`
volume, so their `ROOT_VOLUME_ID` / boot-disk scope stays. Replay still threads the space (it's `root` today) so it
resolves in the same space as the live loop that follows. Write-side storage is already mount-relative:
`resolve_scan_root` maps a volume-root scan → `ROOT_ID` kind-agnostically, and the fresh scanner attributes children via
the carried `parent_id`, so a mount-rooted fresh scan naturally stores `EntryRow`s under `ROOT_ID` by mount-relative
name (mirroring SMB/MTP).

**The inode-trust axis.** `IndexPathSpace` also carries `inodes_trustworthy`, resolved once per scan from the volume's
`FilesystemKind` (via `transports::local_external::classify`; see [`../transports`](../transports/DETAILS.md)). Only a
FAT/exFAT local external drive is `false`. `trust_inode(raw)` is the single choke point every local write path funnels a
snapshot's inode through before persisting it: the raw inode on a trustworthy filesystem, `None` on FAT/exFAT. With no
stored inode the local rename pre-pass can never match, so an inode-reused delete+create can't become a false
`MoveEntryV2` that re-homes a deleted entry's `dir_stats`. The boot disk (APFS) and every trait-scanned volume (SMB/MTP,
which don't run the local pre-pass) are `true`. `for_volume` / `mount_rooted` default to trustworthy;
`with_inodes_trustworthy` carries the FAT/exFAT fact.

## `index_read_path` — the read side (`routing.rs`)

The READ side needs the same transform as the SMB/MTP write side. Enrichment and the dir-stats IPC receive
MOUNT-ABSOLUTE paths but `resolve_path` against a non-root index walks from `ROOT_ID` (the mount root). Without
stripping the mount root, an indexed SMB folder enriches to nothing — the bug that made the whole feature invisible.

`index_read_path(volume_id, abs_path)` is the live mapper; `index_read_path_pure(volume_id, abs, mount_root)` is the
pure, unit-tested decision it wraps:

- **`root`** — pass-through (its index is rooted at `/`; firmlink-normalized absolute paths are already index-rooted).
- **MTP** (id recognized by `crate::mtp::identity::is_mtp_volume_id`) — `mtp_index_relative_path` strips the
  `mtp://{device}/{storage}` scheme + segments to the inner `/path` the index stores under. The path's device+storage
  must match the volume id (a `:`-in-serial device id round-trips verbatim); a plain `/inner` path already
  storage-relative is accepted as-is; anything else ⇒ `None`.
- **SMB (non-root with a known mount root)** — strip the mount root via `transports::smb::watch::index_relative_path`.
  `None` for a path not under the mount root, or a volume with no registered mount root (drop rather than mis-root).

Firmlink normalization stays local-only — it must NOT touch virtual SMB/MTP paths. `index_read_path` is called by
`read/enrichment.rs`, `read/queries.rs`, and `events/progress_reporter.rs` (which maps firmlink-normalized hot paths
into index-relative space before the partial-aggregate send, so the same transform is single-sourced).

## Path → volume routing (`routing.rs`)

`volume_id_for_local_path(path)` resolves which index volume owns a path, four tiers in order, each mapping to the SAME
id its volume and index register under:

1. **SMB** — `transports::smb::index::smb_volume_id_for_path` (probes the mount, keys by `(server, port, share)`).
2. **MTP** — `mtp_volume_id_for_path`, the pure `mtp://` half: strip the scheme, take the first two `/`-segments, require
   the storage segment to parse as a `u32` (so a malformed `mtp://` path doesn't resolve to a bogus volume), yield
   `{device}:{storage}`.
3. **Local external mount** — `external_mount_volume_id_for_path`: fast-reject with
   `scanner::is_on_mounted_external_volume` (a pure prefix check, no registry lock) so ONLY a path under an excluded
   mount prefix (`/Volumes`, `/mnt`, `/media`) can leave `root`, then route by the `VolumeManager` registry
   (`mount_id_for_path`, the longest non-root ancestor mount). The fast-reject is the load-bearing trap-guard: a
   registered cloud-drive folder in the home dir (`~/Library/CloudStorage/…`) is a non-root registered volume too, but
   `root`'s index owns it — a naive "any registered non-root volume" prefix match would divert it to an index-less id
   and drop its sizes.
4. **Everything else** → `root` (the boot disk, plus cloud-drive folders root's index owns).

`exclusion_scope_for_volume(volume_id)` derives the read-side scope (root ⇒ boot disk; every other registered volume ⇒
mount-rooted at its registered root). An UNREGISTERED non-root id yields an empty mount root — still mount-rooted, but
no path sits at that root, so the pseudo-filesystem rule never fires (inert; `index_read_path` drops the path a moment
later anyway).

## Firmlink + `/private`-symlink normalization (`firmlinks.rs`)

Parses `/usr/share/firmlinks`, builds a prefix map, and normalizes paths to the canonical form the index stores and
every lookup uses. Converts `/System/Volumes/Data/Users/foo` → `/Users/foo`, and resolves the well-known macOS
`/private` symlinks (`/tmp` → `/private/tmp`, `/var` → `/private/var`, `/etc` → `/private/etc`) so listing paths match
the index's canonical paths.

The invariant is **canonical form everywhere**: stored keys and lookup keys are both `/private/tmp`, never `/tmp`. The
scanner does NOT follow symlinks (`follow_links(false)`), so the canonical contents come for free from walking the real
`/private`. The scanner's related `is_canonicalization_alias` skip (so the three `/private` root symlinks don't collide
on the real directory's `(parent_id, name_folded)` slot) lives with the exclusion policy in
[`../scanner`](../scanner/DETAILS.md).

## Component-aware prefix tests (`path_prefix.rs`)

Absolute-path prefix checks that respect path components, so `/a/bc` is never treated as a child of `/a/b`. Shared by
the rescan ancestor-collapse and removal-storm coalescing (owned by [`../reconcile`](../reconcile/DETAILS.md) and
[`../watch`](../watch/DETAILS.md)), and by `index_read_path`'s deepest-hot-path selection.
