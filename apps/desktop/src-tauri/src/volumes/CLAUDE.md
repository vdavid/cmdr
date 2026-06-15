# Volumes

macOS volume and location discovery, plus live mount/unmount watching via `NSWorkspace` notifications. Distinct from
`file_system/volume/` (the `Volume` trait + `VolumeManager`). Linux twin: `volumes_linux/`.

## Module map

- **`mod.rs`**: `LocationInfo` (+ `VolumeInfo` alias), `LocationCategory` and `SmbConnectionState` enums,
  `list_locations()`, `get_volume_space()`, `parse_cloud_provider_name()`, `get_mount_point()`,
  `resolve_path_volume_fast()`, ID helpers (`path_to_id`, `smb_volume_id`, `volume_id_for_mount`), and the
  `enrich_smb_connection_state` helper.
- **`watcher.rs`**: `NSWorkspace` mount/unmount observer. On `NSWorkspaceDidMount`/`Unmount`, extracts the volume URL
  from `userInfo[NSWorkspaceVolumeURLKey]`, registers/unregisters with `VolumeManager`, emits per-volume
  `volume-mounted` / `volume-unmounted` Tauri events, and triggers `volume_broadcast::emit_volumes_changed()`.

## Must-knows

- **SMB mounts use `smb_volume_id(server, port, share)`, never `path_to_id`.** `path_to_id` lowercases the mount path,
  so two shares whose case-folded names collide (a NAS `Public`, a Docker `public`) both produce `volumespublic`,
  cross-contaminating `lastUsedPaths`, tab `volumeId` fields, and per-volume state. `volume_id_for_mount(mount_path)` is
  the dispatch helper: `smb_volume_id` for SMB (detected via `get_smb_mount_info` statfs), `path_to_id` otherwise. Use
  it at every site deriving an ID from a path (`get_attached_volumes`, `resolve_path_volume_fast`,
  `watcher::register_volume_with_manager`, the Linux twins).
- **The unmount path can't use `volume_id_for_mount`.** Once `NSWorkspaceDidUnmount` fires, `statfs` on the gone path no
  longer recovers SMB info, so the helper falls back to `path_to_id` and produces the wrong ID. Use
  `VolumeManager::find_by_root(volume_path)` instead (looks up by `Volume::root()`). See `handle_volume_unmounted`.
- **`resolve_path_volume_fast()` checks cloud-drive prefixes BEFORE `statfs`.** Cloud drives are plain folders on the
  data volume, so `statfs` resolves any path inside them to `/`, mis-highlighting "Macintosh HD" in the switcher. The
  prefix test (`match_cloud_drive_root`, pure) covers deep subfolders and is free for non-cloud paths. `get_cloud_drives()`
  and the resolver share `cloud_volume_info()` so IDs/categories can't drift.
- **Don't add launch-time `NSWorkspace` icon/LaunchServices lookups, or `read_dir`/metadata on TCC-protected paths,
  without the FDA gate** (`crate::fda_gate::is_fda_pending_runtime()`). The local `get_icon_for_path()` wrapper returns
  `None` and `get_cloud_drives()` returns empty while the gate is pending; both re-emit with full data after the user
  decides FDA. Skipping the gate stacks 5-10 macOS TCC popups during onboarding. See `fda_gate.rs` and
  `lib/onboarding/CLAUDE.md` § "FDA gate".
- **Detect SMB volumes via `is_smb_fs_type()`, never raw `"smbfs"`/`"cifs"` comparisons.** The helper handles macOS
  (`smbfs`) and Linux (`cifs`) in one place.
- **`LocationInfo` enrichment with `VolumeManager` data lives only in `enrich_smb_connection_state`.** Three callers
  (`list_volumes` IPC, `volume_broadcast`, MCP `cmdr://state`) share it; new enrichment fields go there once.
- **`append_mtp_volumes` is duplicated** across `commands/volumes.rs` and `volume_broadcast.rs` (plus Linux twins). Both
  must set every MTP-derived `LocationInfo` field (e.g. `usb_speed`); set a new field in BOTH or the bootstrap call
  produces volumes with the field missing until a later push refreshes them.
- **`start_volume_watcher` is idempotent** via `OnceLock` (`APP_HANDLE`, `OBSERVER_INSTALLED`); the observer block runs
  on the main thread (keep it cheap, no blocking I/O). `get_main_volume`/`get_attached_volumes`/`get_volume_space` wrap
  bodies in `objc2::rc::autoreleasepool` (called from `spawn_blocking`; without it the per-call objc objects leak).

## Location categories and IDs

- **Favorite**: user-editable, from the `favorites/` store.
- **MainVolume**: root volume at `/`.
- **AttachedVolume**: `/Volumes/*` (skips System, Preboot, Recovery, CloudStorage).
- **CloudDrive**: iCloud at `~/Library/Mobile Documents/…`, providers at `~/Library/CloudStorage/`.
- **Network**: variant exists but is currently unconstructed.

`DEFAULT_VOLUME_ID = "root"`. `ICLOUD_VOLUME_ID = "cloud-icloud"` is the only hardcoded cloud-drive ID (others derive
from the `~/Library/CloudStorage/<provider>` dir name); `friendly_error.rs` matches the literal with a sync-point
comment because `crate::volumes` is macOS-only. `parse_cloud_provider_name` maps `~/Library/CloudStorage/` dir prefixes
to display names (Dropbox, GoogleDrive→Google Drive, OneDrive/Business, Box, pCloud, else first `-`-segment); keep it in
sync with `friendly_error::enrich_with_provider`'s separate provider list.

Full details (decision rationale, NSWorkspace-vs-FSEvents choice, volume-space key choice, `supports_trash` default,
the `Retained::cast_unchecked` contract): [DETAILS.md](DETAILS.md).
