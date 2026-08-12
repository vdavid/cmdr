# Volumes

macOS volume and location discovery, plus live mount/unmount watching via `NSWorkspace` notifications. Distinct from
`file_system/volume/` (the `Volume` trait + `VolumeManager`). Linux twin: `volumes_linux/`.

## Module map

Submodules, all re-exported from `mod.rs` so `crate::volumes::X` stays stable:

- **`mod.rs`**: `LocationInfo`/`LocationCategory`, consts, orchestrators, re-exports.
- **`fs_type.rs`** / **`nsurl.rs`**: non-blocking `statfs` primitives / blocking NSURL enrichment.
- **`ids.rs`**: which identity a mount is keyed by (`volume_id_for`, `volume_id_for_mount`).
- **`smb.rs`** / **`cloud.rs`**: SMB mount parsing + connection-state enrichment / cloud-drive discovery + resolution.
- **`mounts.rs`**: attached-volume enumeration via `getfsstat` (hung-mount guard).
- **`watcher.rs`**: `NSWorkspace` mount/unmount observer; emits `volume-mounted`/`volume-unmounted`, calls `emit_volumes_changed()`.

## Must-knows

- **❌ Never derive a volume ID yourself; call `ids::volume_id_for`** (or `volume_id_for_mount` given only a path), from
  every site. An ID keys the index DB, `lastUsedPaths`, tab state, and routing, so a lossy one sends reads and deletes to
  the wrong disk. It owns the ladder: SMB → `(server, port, share)`, other network → path, local → filesystem UUID.
  DETAILS § "A volume ID is derived from the volume's IDENTITY".
- **One volume ID publishes ONE location, at ONE canonical root.** A filesystem mounted twice derives one ID from both
  mounts, so `get_attached_volumes` collapses to the shortest path and `list_locations` dedupes on ID, ❌ never on path
  alone. Publishing one location doesn't forget the others; the registry keeps them. DETAILS § "One volume ID
  publishes one mount root".
- **The unmount path can't use `volume_id_for_mount`.** Neither `statfs` nor NSURL can identify a gone mount, so it
  falls back to a path ID (the wrong one). Use `VolumeManager::remove_root(volume_path)`: it promotes a sibling mount
  and unregisters only on the last root. See `handle_volume_unmounted`.
- **`resolve_path_volume_fast()` checks cloud-drive prefixes BEFORE `statfs`.** Cloud drives are plain folders on the
  data volume, so `statfs` resolves any path inside them to `/`, mis-highlighting "Macintosh HD" in the switcher. The
  prefix test (`match_cloud_drive_root`, pure) covers deep subfolders and is free for non-cloud paths.
  `get_cloud_drives()` and the resolver share `cloud_volume_info()` so IDs/categories can't drift.
- **Don't add launch-time `NSWorkspace` icon/LaunchServices lookups, or `read_dir`/metadata on TCC-protected paths,
  without the FDA gate** (`crate::fda_gate::is_fda_pending_runtime()`). While pending, `get_icon_for_path()` returns
  `None` and `get_cloud_drives()` returns empty; both re-emit after the FDA decision. Skipping it stacks 5-10 macOS TCC
  popups during onboarding. See `fda_gate.rs` and `lib/onboarding/CLAUDE.md` § "FDA gate".
- **Detect SMB volumes via `is_smb_fs_type()`, never raw `"smbfs"`/`"cifs"` comparisons.** It handles macOS (`smbfs`)
  and Linux (`cifs`) in one place.
- **Volume discovery must never block on a hung mount** (a wedged NAS once froze launch). Enumerate via
  `getfsstat(MNT_NOWAIT)` (`enumerate_mounts`), not NSFileManager; run blocking NSURL/statfs/NSWorkspace/DiskArbitration
  enrichment for LOCAL mounts only (`build_attached_location`; network mounts via `is_network_fs_type` come from the
  snapshot, and that includes ❌ never asking one for its volume UUID); never discover on the main thread
  (`init_volume_manager` spawns `volume-init`). DETAILS § "Hung mounts".
- **`is_read_only` (`MNT_RDONLY`) and `is_disk_image` (DiskArbitration, `disk_image.rs`) are set in BOTH
  `get_attached_volumes` and `resolve_path_volume_fast`; set them in both or they drift.** Gate the disk-image probe to
  local mounts (`!is_smb_fs_type`): it resolves the path, so a hung mount would stall it. Read-only is not a disk-image
  proxy (a writable `.dmg` is read-write).
- **`LocationInfo` enrichment with `VolumeManager` data lives only in `enrich_smb_connection_state`**, shared by three
  callers (`list_volumes` IPC, `volume_broadcast`, MCP `cmdr://state`); new enrichment fields go there once.
- **`append_mtp_volumes` is duplicated** across `commands/volumes.rs` and `volume_broadcast.rs` (plus Linux twins). Set
  every MTP-derived `LocationInfo` field (like `usb_speed`) in BOTH, or the bootstrap produces volumes missing it until
  a later push.
- **`start_volume_watcher` is idempotent** via `OnceLock` (`APP_HANDLE`, `OBSERVER_INSTALLED`); the observer block runs
  on the main thread (keep it cheap, no blocking I/O). `get_main_volume`/`get_attached_volumes`/`get_volume_space` wrap
  bodies in `objc2::rc::autoreleasepool` (they run in `spawn_blocking`; else the per-call objc objects leak).

## Location IDs

`root`, `cloud-*`, and `fav-*` are the literal IDs; the rest are `{scheme}-{slug}-{digest}` from `cmdr_fs::volume::ids`
(`vol-` = UUID-keyed local, plus `path-`, `smb-`, `mtp-`). Only the scheme prefix means anything: ❌ never match on the
slug or rebuild an ID from parts. `ICLOUD_VOLUME_ID` and the cloud-provider list are mirrored in `friendly_error.rs`,
two sync points DETAILS § "Location IDs" spells out.

Full details (decisions, edge cases, the `Retained::cast_unchecked` contract): `DETAILS.md`.
