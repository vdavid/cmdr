# Volumes

macOS volume and location discovery, plus live mount/unmount watching via `NSWorkspace` notifications. Distinct from
`file_system/volume/` (the `Volume` trait + `VolumeManager`). Linux twin: `volumes_linux/`.

## Module map

Discovery is split across theme submodules, all re-exported from `mod.rs` so `crate::volumes::X` stays stable:

- **`mod.rs`**: `LocationInfo`/`LocationCategory`, consts, orchestrators, re-exports.
- **`fs_type.rs`** / **`nsurl.rs`**: non-blocking `statfs` primitives / blocking NSURL enrichment.
- **`smb.rs`** / **`cloud.rs`**: SMB parsing + `volume_id_for_mount` / cloud-drive discovery + resolution.
- **`mounts.rs`**: attached-volume enumeration via `getfsstat` (hung-mount guard).
- **`watcher.rs`**: `NSWorkspace` mount/unmount observer; emits `volume-mounted`/`volume-unmounted`, calls `emit_volumes_changed()`.

## Must-knows

- **SMB mounts use `smb_volume_id(server, port, share)`, never `path_to_id`.** `path_to_id` lowercases the mount path,
  so two shares whose case-folded names collide (a NAS `Public`, a Docker `public`) both produce `volumespublic`,
  cross-contaminating `lastUsedPaths`, tab `volumeId` fields, and per-volume state. `volume_id_for_mount(mount_path)` is
  the dispatch helper: `smb_volume_id` for SMB (detected via `get_smb_mount_info` statfs), `path_to_id` otherwise. Use
  it at every site deriving an ID from a path (`get_attached_volumes`, `resolve_path_volume_fast`,
  `watcher::register_volume_with_manager`, the Linux twins).
- **The unmount path can't use `volume_id_for_mount`.** After `NSWorkspaceDidUnmount`, `statfs` on the gone path can't
  recover SMB info, so the helper falls back to `path_to_id` (wrong ID). Use `VolumeManager::find_by_root(volume_path)`
  instead (looks up by `Volume::root()`). See `handle_volume_unmounted`.
- **`resolve_path_volume_fast()` checks cloud-drive prefixes BEFORE `statfs`.** Cloud drives are plain folders on the
  data volume, so `statfs` resolves any path inside them to `/`, mis-highlighting "Macintosh HD" in the switcher. The
  prefix test (`match_cloud_drive_root`, pure) covers deep subfolders and is free for non-cloud paths. `get_cloud_drives()`
  and the resolver share `cloud_volume_info()` so IDs/categories can't drift.
- **Don't add launch-time `NSWorkspace` icon/LaunchServices lookups, or `read_dir`/metadata on TCC-protected paths,
  without the FDA gate** (`crate::fda_gate::is_fda_pending_runtime()`). While pending, `get_icon_for_path()` returns
  `None` and `get_cloud_drives()` returns empty; both re-emit fully after the FDA decision. Skipping the gate stacks
  5-10 macOS TCC popups during onboarding. See `fda_gate.rs` and `lib/onboarding/CLAUDE.md` § "FDA gate".
- **Detect SMB volumes via `is_smb_fs_type()`, never raw `"smbfs"`/`"cifs"` comparisons.** The helper handles macOS
  (`smbfs`) and Linux (`cifs`) in one place.
- **Volume discovery must never block on a hung mount** (a wedged NAS once froze launch). Enumerate via
  `getfsstat(MNT_NOWAIT)` (`enumerate_mounts`), not NSFileManager; run blocking NSURL/statfs/NSWorkspace/DiskArbitration
  enrichment for LOCAL mounts only (`build_attached_location`; network mounts via `is_network_fs_type` come from the
  snapshot); never discover on the main thread (`init_volume_manager` spawns `volume-init`). DETAILS § "Hung mounts".
- **`is_read_only` (`MNT_RDONLY`) and `is_disk_image` (DiskArbitration, `disk_image.rs`) are set in BOTH
  `get_attached_volumes` and `resolve_path_volume_fast`; set them in both or they drift.** Gate the disk-image probe to
  local mounts (`!is_smb_fs_type`): it resolves the path, so a hung mount would stall it. Read-only is not a disk-image
  proxy (a writable `.dmg` is read-write).
- **`LocationInfo` enrichment with `VolumeManager` data lives only in `enrich_smb_connection_state`**, shared by three
  callers (`list_volumes` IPC, `volume_broadcast`, MCP `cmdr://state`); new enrichment fields go there once.
- **`append_mtp_volumes` is duplicated** across `commands/volumes.rs` and `volume_broadcast.rs` (plus Linux twins). Both
  must set every MTP-derived `LocationInfo` field (like `usb_speed`); set a new field in BOTH, or the bootstrap produces
  volumes missing it until a later push.
- **`start_volume_watcher` is idempotent** via `OnceLock` (`APP_HANDLE`, `OBSERVER_INSTALLED`); the observer block runs
  on the main thread (keep it cheap, no blocking I/O). `get_main_volume`/`get_attached_volumes`/`get_volume_space` wrap
  bodies in `objc2::rc::autoreleasepool` (they run in `spawn_blocking`; else the per-call objc objects leak).

## Location IDs (two cross-file sync points)

`DEFAULT_VOLUME_ID = "root"`; `ICLOUD_VOLUME_ID = "cloud-icloud"` is the only hardcoded cloud-drive ID (others derive
from the `~/Library/CloudStorage/<provider>` dir name). Both IDs and the provider mapping are mirrored in
`friendly_error.rs` (which `crate::volumes` can't reach, being macOS-only): it matches the `ICLOUD_VOLUME_ID` literal
under a sync-point comment, and `parse_cloud_provider_name`'s provider list must stay in sync with
`friendly_error::enrich_with_provider`'s separate one. The `LocationCategory` catalog: `DETAILS.md`.

Full details (decisions, edge cases, the `Retained::cast_unchecked` contract): `DETAILS.md`.
