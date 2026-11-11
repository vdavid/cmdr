# Volumes

Volume and location discovery, plus live mount/unmount watching: one model for every OS, behind a platform seam.
Distinct from `file_system/volume/` (the `Volume` trait, `VolumeManager`). This doc covers the seam and the macOS arm;
Linux's: `platform/linux/CLAUDE.md`.

## Module map

`model.rs`: the model every OS publishes (`LocationInfo`, `SmbMountInfo`). `platform/mod.rs`: the pinned fn set.
`platform/macos/`: `mod.rs` (orchestrators), `ids.rs`, `fs_type.rs` (non-blocking `statfs`), `nsurl.rs` (blocking
enrichment), `mounts.rs` (`getfsstat` rows), `smb.rs`, `cloud.rs`, `disk_image.rs`, `watcher.rs` + `lifecycle.rs` (the
`NSWorkspace` observer, its handlers), `disk_units.rs`, `unmount_approver/`. `platform/windows.rs`: no discovery yet.

## Must-knows

- **❌ Never derive or parse a volume ID yourself; call `ids::volume_id_for`** (or `volume_id_for_mount` given only a
  path): an ID keys the index DB, `lastUsedPaths`, tabs, and routing, so a lossy one sends deletes to the wrong disk.
  Only the scheme prefix means anything: ❌ never match the slug or rebuild one from parts.
- **One volume ID publishes ONE location at ONE canonical root**: mounts sharing an ID collapse to the shortest path via
  `canonical_root::collapse_by_volume_id` (❌ never re-copy it), and `list_locations`
  dedupes on ID, ❌ never path alone, and favorites ❌ never on path (#349).
- **What this module publishes is the SWITCHER's list, ❌ never the registry's**: `mount_registration` sweeps
  `registrable_mount_roots`: every mount but another account's own, which only the index still sees (`mount_roots`).
  A row filtered out here must never mean a path can't be opened.
- **A mount earns a row by NOT carrying `MNT_DONTBROWSE`, or by a recognized provider** (`provider.is_some()`, ❌ never
  `is_cloud_storage()`), ❌ never by location.
- **A cloud provider's own MOUNT is a `CloudDrive` row with `is_cloud_mount: true`** (`is_cloud_provider_mount`), which
  groups it under CLOUD and drops the index affordances. ❌ Never ask it about a non-mount path: a
  `~/Library/CloudStorage` folder names a provider too, and the index reads those at local speed.
- **Discovery must never block on a hung mount** (a wedged NAS once froze launch): `getfsstat(MNT_NOWAIT)`, ❌ never
  NSFileManager or per-mount `statfs`; blocking NSURL / NSWorkspace / DiskArbitration enrichment for LOCAL mounts
  only, never on the main thread. **❗ A mount table that wouldn't answer is its OWN answer, ❌ never an empty one**, or an unmount takes a
  sibling down under its live watcher.
- **Callers ask `crate::volumes`, ❌ never an arm or a `cfg`'d alias.** Detect SMB with `is_smb_fs_type()`, ❌ never raw `"smbfs"` / `"cifs"`.
- **An SMB share is ONE path segment; everything below it is a directory INSIDE the share** (`SmbMountInfo::subpath`),
  the SAME volume as its share. ❌ Never split a mount source on the first `/`: a DFS sub-mount records
  `//user@domain/SYSVOL/domain` (wrong share, second ID, ERR-48RZX).
- **Four fields drift unless BOTH `get_attached_volumes` and `resolve_path_volume_fast` set them**:
  `mount_is_read_only`, `is_disk_image`, `is_cloud_mount`, and the category. ❌ Read-only is no disk-image proxy: a
  writable `.dmg` is read-write.
- **Only `enrich_from_volume_registry` copies registry state onto a `LocationInfo`** (`capabilities` +
  `connection_state`), once per arm, ❌ never a discovery constructor. `volume_listing::complete` assembles the
  published list, owning its order and the only `append_device_volumes` call.
- **Wrap every objc-touching `spawn_blocking` body in `objc2::rc::autoreleasepool`**, or the objects leak. Keep
  `watcher.rs`’s observer block cheap: main thread, no I/O.
- **Six more rules each own a `DETAILS.md` section to read first**: which mounts get a row, another account's mounts,
  cloud-drive prefixes ahead of `statfs` in `resolve_path_volume_fast`, the unmount path's `remove_root` (deriving an ID
  for a gone mount lands on the wrong one), the FDA gate in front of launch-time icon / LaunchServices / TCC reads (or
  onboarding stacks popups), and the DiskArbitration unmount approver (❌ no filesystem, SQLite, or second pre-unmount
  hook on the ask path).

Decisions, edge cases, the servers arm: `DETAILS.md`. Read it before non-trivial work.
