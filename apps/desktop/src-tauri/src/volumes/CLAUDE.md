# Volumes

macOS volume and location discovery, plus live mount/unmount watching via `NSWorkspace` notifications.

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | `LocationInfo` type and `VolumeInfo` type alias (`pub use LocationInfo as VolumeInfo` for backwards compatibility), `LocationCategory` enum, `SmbConnectionState` enum. `list_locations()`, `get_volume_space()`, `parse_cloud_provider_name()`, `get_mount_point()` (statfs-based mount resolution with APFS firmlink normalization), `resolve_path_volume_fast()` (builds `VolumeInfo` from statfs without enumerating volumes), and private helpers using `objc2`/`objc2_foundation`. |
| `watcher.rs` | `NSWorkspace` mount/unmount observer. Subscribes to `NSWorkspaceDidMountNotification` and `NSWorkspaceDidUnmountNotification`, extracts the volume path from `NSWorkspaceVolumeURLKey`, and dispatches to `handle_volume_mounted` / `handle_volume_unmounted`. Those register/unregister with `VolumeManager` (via `register_volume_with_manager` / `unregister_volume_from_manager`, coupling to `file_system::get_volume_manager()`), emit per-volume `volume-mounted` / `volume-unmounted` Tauri events (`DualPaneExplorer` uses `volume-unmounted` with the volume path to redirect panes off ejected volumes), and trigger `volume_broadcast::emit_volumes_changed()`. |

## Location categories

```
Favorite:       hardcoded: /Applications, ~/Desktop, ~/Documents, ~/Downloads
MainVolume:     root volume at "/"
AttachedVolume: /Volumes/* (skips /System, Preboot, Recovery, CloudStorage)
CloudDrive:     iCloud at ~/Library/Mobile Documents/…, providers at ~/Library/CloudStorage/
```

The `LocationCategory::Network` enum variant exists for future use but is currently unconstructed; the OS-level `/Network` browseable location doesn't surface as a sidebar entry yet.

`list_locations()` aggregates all categories in order and deduplicates by path using a `HashSet<String>`.

## Global state in `watcher.rs`

```rust
APP_HANDLE:         OnceLock<AppHandle>  // app handle for emitting events
OBSERVER_INSTALLED: OnceLock<()>         // idempotency gate
```

`start_volume_watcher` is idempotent (second call returns early). The observer `RcBlock` closures aren't kept in our own static; `addObserverForName:object:queue:usingBlock:` retains the block for the lifetime of the registration, and we never remove the observer. Same pattern as `file_system/open_with.rs`.

## Volume IDs: `path_to_id` and `smb_volume_id`

`path_to_id` (keep only alphanumeric + `-`, lowercase, `/` → `"root"`) is `pub(crate)` in
`file_system/volume/mod.rs` and re-exported from `volumes/mod.rs`. The constant `DEFAULT_VOLUME_ID = "root"` is
defined alongside it.

**SMB mounts use `smb_volume_id(server, port, share)` instead of `path_to_id`.** Path-shape IDs lowercase the mount
path, so two SMB shares with the same case-folded name on different servers (a NAS sharing `Public`, a Docker
container sharing `public`) collided on `volumespublic`. The collision cross-contaminated `lastUsedPaths`, tab
`volumeId` fields, and per-volume state, and surfaced as wrong-cased paths flowing into `SmbVolume::list_directory`
with the server replying `STATUS_OBJECT_PATH_NOT_FOUND`. Keying SMB by `(server, port, share)` (server and share
lowercased to match DNS / SMB protocol case-insensitivity, server dots replaced by `-`) puts each mount in its own
slot and removes the entire class of collision.

`volume_id_for_mount(mount_path)` is the dispatch helper: it returns `smb_volume_id(...)` for SMB mounts (detected
via `get_smb_mount_info`'s statfs read) and `path_to_id` for everything else. Use this at every site that derives
an ID from a path: `get_attached_volumes`, `resolve_path_volume_fast`, `volumes/watcher.rs::register_volume_with_manager`,
and the linux twins.

**The unmount path can't use `volume_id_for_mount` directly.** Once `NSWorkspaceDidUnmount` fires, `statfs` on the
gone-volume path no longer recovers SMB info, so the helper falls back to `path_to_id` and produces the wrong ID.
Use `VolumeManager::find_by_root(volume_path)` instead — it looks up by `Volume::root()` and returns whatever ID
the volume was registered under. See `handle_volume_unmounted`.

`ICLOUD_VOLUME_ID = "cloud-icloud"` is also exported from `mod.rs`. iCloud Drive is the only cloud-drive entry that gets a hardcoded ID (others are derived from their `~/Library/CloudStorage/<provider>` directory names). Cross-module callers should use the constant; `file_system/volume/friendly_error.rs` matches the literal string with a sync-point comment because `crate::volumes` is macOS-only and can't be imported from the cross-platform `friendly_error` module.

## Volume space

`get_volume_space(path)` uses `NSURLVolumeTotalCapacityKey` and `NSURLVolumeAvailableCapacityForImportantUsageKey` (falls back to `NSURLVolumeAvailableCapacityKey`). Returns `None` for non-existent paths.

## FDA gate

The local `get_icon_for_path()` wrapper short-circuits to `None` while `crate::fda_gate::is_fda_pending_runtime()` is `true`. `get_cloud_drives()` returns an empty `Vec` under the same condition because enumerating `~/Library/CloudStorage` is FDA-gated. Both fire only at first launch; after the user clicks Allow or Deny on the in-app FDA modal, the gate clears and `volumes-changed` re-emits with full data.

**Don't add new launch-time `NSWorkspace` icon/LaunchServices lookups, or `read_dir`/metadata calls on TCC-protected paths, without checking the gate.** It's the difference between zero TCC prompts during onboarding and a stack of five-to-ten popups (MediaLibrary, AppData, Desktop, Documents, Downloads, ...). See [`src-tauri/src/fda_gate.rs`](../fda_gate.rs) for the helpers and [`src/lib/onboarding/CLAUDE.md`](../../../src/lib/onboarding/CLAUDE.md) § "FDA gate" for the user-side flow.

## Cloud provider detection

`parse_cloud_provider_name(dir_name)` maps `~/Library/CloudStorage/` directory names to friendly labels:

| Directory prefix | Display name |
|---|---|
| `Dropbox` | Dropbox |
| `GoogleDrive` | Google Drive |
| `OneDrive` (+ `Business`) | OneDrive / OneDrive for Business |
| `Box` | Box |
| `pCloud` | pCloud |
| anything else | first `-`-delimited segment |

**Note**: Error enrichment in `file_system/volume/friendly_error.rs` has its own provider detection (`enrich_with_provider`) using the same path-prefix matching strategy but for a different purpose (error suggestions vs display names). Keep the two lists in sync when adding new providers.

## Gotchas

**Gotcha**: Use `is_smb_fs_type()` to detect SMB volumes, never raw `"smbfs"` / `"cifs"` string comparisons
**Why**: The helper in `mod.rs` handles both macOS (`smbfs`) and Linux (`cifs`) in one place. Raw string comparisons scatter platform knowledge and are easy to get wrong.

**Decision**: `LocationInfo` enrichment with `VolumeManager` data lives in one place: `volumes::enrich_smb_connection_state`
**Why**: Previously duplicated across `commands/volumes.rs` (for `list_volumes` IPC) and `volume_broadcast.rs` (for `volumes-changed` push) with a "must stay in sync" gotcha. With MCP's `cmdr://state` becoming a third caller, the sync risk grew to three sites. The helper now lives in `volumes::mod.rs` and all three call it. New enrichment fields go there once, not three times.

**Gotcha**: `append_mtp_volumes` is duplicated across `commands/volumes.rs` and `volume_broadcast.rs` (and their Linux counterparts), and both populate the MTP-only `usb_speed` field from `ConnectedDeviceInfo::device::usb_speed`
**Why**: Same two-site sync problem as SMB enrichment, but for MTP-derived volume fields. Originally only `volume_broadcast.rs` set `usb_speed` from the device info; the `list_volumes` IPC variant hardcoded `None`, so the bootstrap call (used until the `volumes-changed` event lands) produced volumes with a missing dot until a later push refreshed them. Any new MTP-derived `LocationInfo` field needs to be set in BOTH `append_mtp_volumes` copies. The Linux variant lives in `commands/volumes_linux.rs` and mirrors the macOS one.

## Key decisions

**Decision**: SMB volume IDs are keyed by `(server, port, share)`, not by mount path
**Why**: `path_to_id("/Volumes/Public")` and `path_to_id("/Volumes/public")` both produce `volumespublic`, so a NAS's
`Public` share and a Docker container's `public` share collided on one ID. The collision cross-contaminated every
per-volume store: `lastUsedPaths`, persisted tab `volumeId` fields, the in-memory `VolumeManager`, and any future
per-volume preference. The user-visible bug was a wrong-case path leaking from a stale `lastUsedPaths` entry into
`SmbVolume::list_directory`, where the case-sensitive `strip_prefix` against `mount_path` failed and the smb2 path
was constructed as `Volumes\Public` (relative under the share root), producing `STATUS_OBJECT_PATH_NOT_FOUND` from
Samba. `smb_volume_id(server, port, share)` removes the entire collision class. Server is lowercased (DNS is
case-insensitive); share is lowercased (SMB share names are case-insensitive per Windows / Samba default); dots in
the server (IPs) are replaced by `-` so the ID stays in `[a-z0-9-]`. Statfs is the canonical source for both the
watcher and the mount-time `register_smb_volume`, so they always agree. The unmount path looks up by
`VolumeManager::find_by_root` because statfs no longer recovers SMB info once the mount is gone.

**Decision**: Gate launch-time icon fetches on the FDA decision (`crate::fda_gate::is_fda_pending_runtime()`)
**Why**: `NSWorkspace.iconForFile:` resolution touches LaunchServices and several adjacent TCC services beyond the input path itself. On a fresh prod install with FDA off, calling it for `/Applications`, `~/Desktop`, `~/Documents`, `~/Downloads`, the iCloud root, and per-provider cloud-storage paths stacked **5–10 macOS native permission popups** (MediaLibrary, AppData, Desktop, Documents, Downloads, ...) on top of the in-app FDA modal, exactly the onboarding-flood UX the modal is supposed to replace. Returning `icon: None` from `get_icon_for_path()` while the gate is pending eliminates the entire class. The frontend already falls back to a generic folder icon when `icon` is missing, so the sidebar still shows favorite/volume entries during onboarding; they just look generic for the few seconds before the user clicks Allow or Deny. See `commands/indexing.rs::start_indexing_after_fda_decision` for the gate-clear + re-emit on the deny path; the allow path requires a restart, so re-entering `setup()` sets the gate to `false` via the OS probe.

**Decision**: Use `NSWorkspace` notifications (`NSWorkspaceDidMountNotification` / `NSWorkspaceDidUnmountNotification`) instead of an FSEvents watcher on `/Volumes`
**Why**: FSEvents fires when the kernel writes a directory entry under `/Volumes`, which races the mount: `statfs` on the new mount point still returns the root filesystem's `fsid` until the OS finishes mounting. The previous implementation papered over this with a `spawn_mount_settle_watcher` that polled `fsid` for up to 10 s, but slow drives behind USB-C/Thunderbolt docks can take longer; if the poll timed out, `get_attached_volumes` filtered the volume out and only an app restart surfaced it. `NSWorkspace` notifications are posted by `diskarbitrationd` *after* the mount is fully settled and `NSFileManager` metadata is ready, so there's no race to paper over and the volume always shows up. They also carry the volume URL directly in `userInfo[NSWorkspaceVolumeURLKey]` with no diffing or polling needed. DiskArbitration would also work but requires a CFRunLoop scheduled separately from Tokio; `NSWorkspace` rides on the AppKit runloop Tauri already runs.

**Decision**: Use `OnceLock` for `APP_HANDLE` and `OBSERVER_INSTALLED`
**Why**: `start_volume_watcher` must be idempotent; calling it twice (e.g., if app setup runs again) must not double-subscribe. `OnceLock::set` failing on the second call is the idempotency gate. `LazyLock` would initialize eagerly, which doesn't work because the `AppHandle` isn't available at static-init time.

**Decision**: Use `NSURLVolumeAvailableCapacityForImportantUsageKey` with fallback to `NSURLVolumeAvailableCapacityKey`
**Why**: The "ForImportantUsage" key accounts for purgeable space (iCloud, APFS snapshots): it reports how much space the OS would make available if needed, which matches what Finder shows. The plain key reports only physically free blocks, which can be misleadingly low on APFS volumes with purgeable data. The fallback handles older macOS versions where the key doesn't exist.

**Decision**: `supports_trash` defaults to `true` for unknown filesystem types
**Why**: Optimistic default. Most local filesystems support trash; the only exceptions are network mounts and FAT-family formats, which are explicitly listed. If an unknown fs type doesn't support trash, the operation fails gracefully at trash time, better than pessimistically disabling trash for a filesystem that actually supports it.

**Decision**: Use `libc::statfs` for filesystem type detection instead of `NSURLVolumeLocalizedFormatDescriptionKey`
**Why**: The NSURL key returns a human-readable string like "APFS (Case-sensitive)" which is locale-dependent and not machine-parseable. `statfs.f_fstypename` returns a stable machine identifier ("apfs", "smbfs", "nfs") that can be matched against a known list of network/non-trash-capable filesystems.

## Gotchas

**Gotcha**: `VolumeInfo` is a type alias for `LocationInfo`, not a separate type
**Why**: The frontend sends/receives `VolumeInfo`, but locations also cover favorites and cloud drives (not just volumes). The alias keeps IPC compatibility without a frontend migration.

**Gotcha**: Watcher registers/unregisters volumes with `VolumeManager` directly, creating tight coupling to `file_system::get_volume_manager()`
**Why**: When a volume mounts, it must be immediately available for file operations. Emitting just a Tauri event and letting the frontend trigger registration would introduce a race window where operations fail because the volume isn't registered yet. Direct registration ensures atomicity: by the time the frontend receives `volume-mounted`, the volume is already usable.

**Gotcha**: `get_main_volume`, `get_attached_volumes`, and `get_volume_space` wrap their bodies in `objc2::rc::autoreleasepool`
**Why**: These functions are called from `spawn_blocking` threads (via `blocking_with_timeout_flag` in commands). Without an autorelease pool, the `NSFileManager`, `NSURL`, `NSString`, and `NSNumber` objects created per call accumulate in a default pool that is never drained, causing memory leaks over hours.

**Gotcha**: The observer block in `watcher.rs::install_observers` runs on the main thread
**Why**: With `queue: nil` passed to `addObserverForName:object:queue:usingBlock:`, AppKit dispatches the block on the same thread that posted the notification. `diskarbitrationd` posts on the main thread, so the block runs there. Keep the body cheap: `register_volume_with_manager` is microseconds, `try_upgrade_smb_mount` and `volume_broadcast::emit_volumes_changed` both `tauri::async_runtime::spawn`, and `app.emit` is non-blocking. Don't add any blocking I/O here without moving it onto a background task.

**Gotcha**: `userInfo` is downcast with `Retained::cast_unchecked` to `NSDictionary<NSString, NSURL>`
**Why**: AppKit documents the value under `NSWorkspaceVolumeURLKey` as an `NSURL`. The unchecked cast trades a runtime type check for a hard contract on Apple's side. If Apple ever changed this, the next `objectForKey` access would either return `None` (best case) or be unsound. A safer alternative is `cast::<NSDictionary>` plus a `downcast::<NSURL>` per value, but that costs an Objective-C `isKindOfClass:` call per notification. Today we lean on the documented contract; revisit if a future macOS version breaks it.

## Dependencies

External: `dirs`, `objc2`, `objc2_foundation`, `objc2_app_kit` (`NSWorkspace`), `block2` (`RcBlock` for the observer callbacks)
Internal: `crate::file_system::{get_volume_manager, volume::LocalPosixVolume}`, `crate::icons::get_icon_for_path`
