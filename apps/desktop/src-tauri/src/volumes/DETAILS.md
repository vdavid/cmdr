# Volumes details

Depth and rationale. `CLAUDE.md` holds the must-knows; the decision detail lives here.

## `list_locations()`

Aggregates all `LocationCategory` entries in order and deduplicates by path using a `HashSet<String>`. The OS-level
`/Network` browseable location doesn't surface as a sidebar entry yet, so `LocationCategory::Network` is currently
unconstructed.

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
