# Volumes details

Depth and rationale. `CLAUDE.md` holds the must-knows; the decision detail lives here.

## Location categories

`LocationCategory` variants:

- **Favorite**: user-editable, from the `favorites/` store.
- **MainVolume**: root volume at `/`.
- **AttachedVolume**: `/Volumes/*` (skips System, Preboot, Recovery, CloudStorage).
- **CloudDrive**: iCloud at `~/Library/Mobile Documents/…`, providers at `~/Library/CloudStorage/`.
- **Network**: variant exists but is currently unconstructed.

`parse_cloud_provider_name` maps `~/Library/CloudStorage/` dir prefixes to display names (Dropbox, GoogleDrive→Google
Drive, OneDrive/Business, Box, pCloud, else the first `-`-segment). The `ICLOUD_VOLUME_ID` / provider-list sync points
with `friendly_error.rs` are called out in `CLAUDE.md`.

## `list_locations()`

Aggregates all `LocationCategory` entries in order and deduplicates by path using a `HashSet<String>`. The OS-level
`/Network` browseable location doesn't surface as a sidebar entry yet, so `LocationCategory::Network` is currently
unconstructed.

## Hung mounts

**The problem.** A network mount (SMB, NFS, …) can wedge so that every metadata syscall on it blocks in the kernel for
30s–forever (uninterruptible — even `SIGKILL` won't land until the mount is force-unmounted). Volume discovery is riddled
with such syscalls, and a single dead mount used to take the whole app down at launch: `init_volume_manager` ran
`get_attached_volumes` synchronously on the main thread (inside the Tauri `setup` closure), and NSFileManager's
`mountedVolumeURLsIncludingResourceValuesForKeys` `getattrlist`s every mount to build the URL array. On a wedged
`/Volumes/naspi` the main thread stuck in `__getattrlist` for 90s+ and the webview never recovered (its startup IPC piled
up behind the frozen process). The MCP `cmdr://state` resource hit the same wall through `list_locations`: reads took a
flat ~30s (one smbfs kernel timeout). (Incident: live NAS QA, 2026-07-13.)

**The fix — three layers.**

1. **Non-blocking enumeration.** `get_attached_volumes` enumerates via `getfsstat(MNT_NOWAIT)` (`enumerate_mounts`), not
   NSFileManager. `MNT_NOWAIT` returns the kernel's cached mount table (mount point, fs type, `MNT_RDONLY` flag, and the
   `f_mntfromname` SMB source) without ever round-tripping to a filesystem, so a wedged mount can't stall it — this is
   the difference between `df -n` and plain `df`. `getfsstat` was verified non-blocking on the exact wedged NAS state
   from the incident. Because fs type and read-only come straight from the snapshot, three former per-volume `statfs`
   calls (`get_fs_type`, `read_only_from_statfs`, `get_smb_mount_info`) are gone from this path.
2. **Skip blocking enrichment for network mounts.** `build_attached_location` runs the blocking NSURL / NSWorkspace /
   DiskArbitration enrichment (`resolve_local`) ONLY for local mounts. Network mounts (`is_network_fs_type`) derive
   everything from the getfsstat snapshot: id/name from `f_mntfromname` (SMB → "share on server"), `is_ejectable = false`
   (cosmetically moot — the eject affordance keys on `smbConnectionState` and `eject.rs` forces it true for SMB), no icon,
   never a disk image. So a dead network mount contributes its entry and never blocks discovery of the healthy volumes
   beside it.
3. **Off-main + timeout-guarded callers.** `init_volume_manager` registers root synchronously (cheap, `/` never hangs)
   and spawns attached/cloud discovery on the `volume-init` helper thread, then re-emits `volumes-changed`. Every caller
   of `list_locations` is wrapped in a ~2s `spawn_blocking` timeout (`volume_broadcast::do_emit`, the MCP
   `snapshot_volumes`, the `list_volumes` IPC via `blocking_with_timeout_flag`), so the remaining unguarded blocking
   paths inside `list_locations` — `get_favorites` and `get_cloud_drives`, which still `statfs`/icon per item and would
   hang on a favorite or cloud folder that lives on a wedged mount — degrade to a bounded 2s partial result instead of an
   infinite stall. `get_main_volume` no longer enumerates: it builds root directly from `/`.

**Follow-up.** `get_favorites` and `get_cloud_drives` still do unguarded per-item `statfs`/icon; a favorite pointing at a
hung mount makes `list_locations` time out (2s) and drop the healthy volumes with it. Fully fixing "one dead mount never
hides the others" here needs per-item timeouts for those two, tracked separately.

## Global state in `watcher.rs`

- `APP_HANDLE: OnceLock<AppHandle>`: app handle for emitting events.
- `OBSERVER_INSTALLED: OnceLock<()>`: idempotency gate.

The observer `RcBlock` closures aren't kept in our own static; `addObserverForName:object:queue:usingBlock:` retains the
block for the lifetime of the registration, and we never remove the observer. Same pattern as
`file_system/open_with.rs`. `DualPaneExplorer` uses the `volume-unmounted` event (carrying the volume path) to redirect
panes off ejected volumes.

## Volume space

`get_volume_space(path)` uses `NSURLVolumeTotalCapacityKey` and `NSURLVolumeAvailableCapacityForImportantUsageKey`
(falls back to `NSURLVolumeAvailableCapacityKey`). Returns `None` for non-existent paths.

## Key decisions

**Decision**: Detect mounted disk images (`.dmg`) via DiskArbitration's `DADeviceModel`, set on
`LocationInfo::is_disk_image` (see `disk_image.rs`).
**Why**: Disk images are transient install-style mounts, so the UI suppresses their index affordances and free-space
bars (the frontend reads `isDiskImage`). The reliable signal is DiskArbitration: `DADeviceModel == "Disk Image"` for any
`hdiutil`-attached image (verified on macOS 15.5, 2026-06-27). Read-only is NOT a usable proxy — a writable APFS `.dmg`
reports `is_read_only == false`, and conversely a locked SD card is read-only but not an image — so the two flags are
independent. `fs_type`/`f_mntfromname` don't disambiguate either (a `.dmg` can be APFS/HFS and present a normal
`/dev/diskNsM` source). The DA call is synchronous (no run loop) and cheap next to the per-volume NSURL/icon work, but it
resolves the volume path, so callers gate it to local (non-SMB) mounts to keep a hung network mount from stalling it.
Both `get_attached_volumes` (the switcher list) and `resolve_path_volume_fast` (highlight + transfer-source) set the flag
so they can't drift.

**Decision**: Populate `is_read_only` for attached volumes from the `statfs` `MNT_RDONLY` flag (`read_only_from_statfs`).
**Why**: It powers the 🔒 indicator and the copy/move write guard for ANY read-only mount (a read-only `.dmg`, a locked
SD card, an optical disc), not just MTP locked storage. The frontend guard machinery (`file-operation-commands.ts`,
`transfer-entry.ts`) already keys on `isReadOnly`, so populating the flag activates it with no frontend change; backend
`validate_destination_writable` (via `libc::access`) is the second line of defense.

**Decision**: SMB volume IDs are keyed by `(server, port, share)`, not by mount path.
**Why**: `path_to_id("/Volumes/Public")` and `path_to_id("/Volumes/public")` both produce `volumespublic`, so a NAS's
`Public` share and a Docker container's `public` share collided on one ID. The collision cross-contaminated every
per-volume store (`lastUsedPaths`, persisted tab `volumeId` fields, the in-memory `VolumeManager`, future per-volume
prefs). The user-visible bug: a wrong-case path leaking from a stale `lastUsedPaths` entry into
`SmbVolume::list_directory`, where the case-sensitive `strip_prefix` against `mount_path` failed and the smb2 path was
built as `Volumes\Public` (relative under the share root), producing `STATUS_OBJECT_PATH_NOT_FOUND` from Samba.
`smb_volume_id(server, port, share)` removes the entire class: server lowercased (DNS case-insensitive), share
lowercased (SMB share names case-insensitive per Windows/Samba default), server dots replaced by `-` so the ID stays in
`[a-z0-9-]`. Statfs is the canonical source for both the watcher and mount-time `register_smb_volume`, so they agree.
The unmount path looks up by `VolumeManager::find_by_root` because statfs no longer recovers SMB info once the mount is
gone.

**Decision**: Gate launch-time icon fetches on the FDA decision (`crate::fda_gate::is_fda_pending_runtime()`).
**Why**: `NSWorkspace.iconForFile:` resolution touches LaunchServices and several adjacent TCC services beyond the input
path. On a fresh prod install with FDA off, calling it for `/Applications`, `~/Desktop`, `~/Documents`, `~/Downloads`,
the iCloud root, and per-provider cloud-storage paths stacked 5-10 macOS native permission popups (MediaLibrary,
AppData, Desktop, Documents, Downloads, …) on top of the in-app FDA modal, exactly the onboarding flood the modal is
meant to replace. Returning `icon: None` from `get_icon_for_path()` while the gate is pending eliminates the class; the
frontend falls back to a generic folder icon, so the sidebar still shows favorite/volume entries (just generic for the
few seconds before the user decides). See `commands/indexing.rs::start_indexing_after_fda_decision` for the gate-clear +
re-emit on the deny path; the allow path requires a restart, so re-entering `setup()` sets the gate to `false` via the
OS probe.

**Decision**: Use `NSWorkspace` notifications, not an FSEvents watcher on `/Volumes`.
**Why**: FSEvents fires when the kernel writes a directory entry under `/Volumes`, which races the mount: `statfs` on the
new mount point still returns the root filesystem's `fsid` until the OS finishes mounting. Polling `fsid` to settle
times out on slow drives behind USB-C/Thunderbolt docks, and a timeout would filter the volume out until an app
restart. `NSWorkspace` notifications are posted by `diskarbitrationd` after the mount is fully settled and
`NSFileManager` metadata is ready, so there's no race, and they carry the volume URL directly in
`userInfo[NSWorkspaceVolumeURLKey]` (no diffing or polling). DiskArbitration would work too but needs a CFRunLoop
scheduled separately from Tokio; `NSWorkspace` rides on the AppKit runloop Tauri already runs.

**Decision**: Use `OnceLock` for `APP_HANDLE` and `OBSERVER_INSTALLED`.
**Why**: `start_volume_watcher` must be idempotent; `OnceLock::set` failing on the second call is the gate. `LazyLock`
would initialize eagerly, which doesn't work because the `AppHandle` isn't available at static-init time.

**Decision**: Use `NSURLVolumeAvailableCapacityForImportantUsageKey` with fallback to `NSURLVolumeAvailableCapacityKey`.
**Why**: The "ForImportantUsage" key accounts for purgeable space (iCloud, APFS snapshots), matching what Finder shows.
The plain key reports only physically free blocks, misleadingly low on APFS volumes with purgeable data. The fallback
handles older macOS versions lacking the key.

**Decision**: `supports_trash` defaults to `true` for unknown filesystem types.
**Why**: Optimistic default. Most local filesystems support trash; the exceptions (network mounts, FAT-family) are
explicitly listed. If an unknown fs type doesn't support trash, the op fails gracefully at trash time, better than
pessimistically disabling trash for a filesystem that supports it.

**Decision**: Use `libc::statfs` for filesystem type detection, not `NSURLVolumeLocalizedFormatDescriptionKey`.
**Why**: The NSURL key returns a locale-dependent human string ("APFS (Case-sensitive)"). `statfs.f_fstypename` returns
a stable machine identifier ("apfs", "smbfs", "nfs") that matches against the known network/non-trash list.

## Gotchas

**Gotcha**: `VolumeInfo` is a type alias for `LocationInfo`, not a separate type.
**Why**: The frontend sends/receives `VolumeInfo`, but locations also cover favorites and cloud drives. The alias keeps
IPC compatibility without a frontend migration.

**Gotcha**: The watcher registers/unregisters volumes with `VolumeManager` directly (tight coupling to
`file_system::get_volume_manager()`).
**Why**: A mounting volume must be immediately available for file operations. Emitting only a Tauri event and letting the
frontend trigger registration would open a race window where ops fail because the volume isn't registered yet. Direct
registration ensures that by the time the frontend gets `volume-mounted`, the volume is usable.

**Gotcha**: `get_main_volume`, `get_attached_volumes`, and `get_volume_space` wrap their bodies in
`objc2::rc::autoreleasepool`.
**Why**: Called from `spawn_blocking` threads. Without a pool, the per-call `NSFileManager`/`NSURL`/`NSString`/`NSNumber`
objects accumulate in a default pool that's never drained, leaking memory over hours.

**Gotcha**: The observer block in `watcher.rs::install_observers` runs on the main thread.
**Why**: With `queue: nil`, AppKit dispatches the block on the thread that posted the notification, and
`diskarbitrationd` posts on the main thread. Keep the body cheap: `register_volume_with_manager` is microseconds,
`try_upgrade_smb_mount` and `emit_volumes_changed` both `tauri::async_runtime::spawn`, and `app.emit` is non-blocking.
Don't add blocking I/O here without moving it onto a background task.

**Gotcha**: `userInfo` is downcast with `Retained::cast_unchecked` to `NSDictionary<NSString, NSURL>`.
**Why**: AppKit documents the value under `NSWorkspaceVolumeURLKey` as an `NSURL`. The unchecked cast trades a runtime
type check for a hard contract on Apple's side. A safer alternative (`cast::<NSDictionary>` plus a per-value
`downcast::<NSURL>`) costs an `isKindOfClass:` call per notification. We lean on the documented contract; revisit if a
future macOS version breaks it.

## Dependencies

- External: `dirs`, `objc2`, `objc2_foundation`, `objc2_app_kit` (`NSWorkspace`), `block2` (`RcBlock`).
- Internal: `crate::file_system::{get_volume_manager, volume::LocalPosixVolume}`, `crate::icons::get_icon_for_path`.
