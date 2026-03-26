# Volumes (Linux)

Linux volume and location discovery, plus live mount/unmount watching via inotify on `/proc/mounts`.

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | `LocationInfo`, `LocationCategory`, `VolumeSpaceInfo` types (mirrors macOS `volumes/mod.rs` JSON shape). `list_locations()`, `get_volume_space()`, `get_mounted_volumes()`, cloud drive detection, GVFS SMB share detection. Uses `linux_mounts::parse_proc_mounts()` for mount enumeration. |
| `watcher.rs` | Two inotify watchers: `/proc/mounts` for standard mounts, `/run/user/<uid>/gvfs/` for GVFS SMB shares. Detects mount/unmount by diffing against known state. Registers/unregisters with `VolumeManager`. Emits `volume-mounted` / `volume-unmounted` Tauri events. |

## Location categories

```
Favorite       — Home, ~/Desktop, ~/Documents, ~/Downloads (only if they exist)
MainVolume     — root "/" filesystem
AttachedVolume — real filesystems from /proc/mounts (filters out virtual: proc, sysfs, tmpfs, etc.)
CloudDrive     — ~/Dropbox, ~/Google Drive, ~/.local/share/Nextcloud, ~/OneDrive
Network        — GVFS SMB shares under /run/user/<uid>/gvfs/smb-share:* (ejectable, no trash)
```

`list_locations()` aggregates all categories in order and deduplicates by path.

## Virtual filesystem filtering

Both `mod.rs` and `watcher.rs` share the same list of virtual FS types to exclude: proc, sysfs, devpts, tmpfs, cgroup, cgroup2, devtmpfs, hugetlbfs, mqueue, debugfs, tracefs, securityfs, pstore, configfs, fusectl, binfmt_misc, autofs, efivarfs, ramfs, rpc_pipefs, nfsd, nsfs, bpf.

## Removable volume detection

Mounts under `/run/media/$USER/` or `/media/$USER/` are marked as ejectable (`is_ejectable: true`).

## Key decisions

**Decision**: Two separate inotify watchers — one for `/proc/mounts`, one for `/run/user/<uid>/gvfs/`
**Why**: GVFS SMB shares don't appear in `/proc/mounts` at all. GVFS uses a single FUSE mount for the entire `gvfs/` directory; individual SMB shares are just subdirectories of that FUSE mount. So mount/unmount of an SMB share is a directory create/remove in the GVFS dir, invisible to `/proc/mounts`. Watching both sources is the only way to detect all volume changes.

**Decision**: Filter virtual filesystems by an explicit allowlist of types to exclude, not by mount path patterns
**Why**: Filtering by path (e.g., skip `/proc`, `/sys`) misses virtual filesystems mounted at unusual locations (bind mounts, containers) and is fragile against distro differences. Filtering by `fstype` is definitive — `tmpfs` is always `tmpfs` regardless of where it's mounted.

**Decision**: Use `/proc/self/mountinfo` (not `statfs()`) for filesystem type detection and network mount classification
**Why**: `statfs()` collapses all FUSE mounts to a single `FUSE_SUPER_MAGIC`, making it impossible to distinguish `sshfs` from `ntfs-3g`. It also blocks for minutes on hung NFS mounts and triggers automounts as a side effect. `/proc/self/mountinfo` is a single file read that correctly identifies FUSE-based network mounts (for example, `fuse.sshfs`, `fuse.rclone`) via fstype substrings, never blocks, and doesn't trigger automounts. This data is reused by both volume discovery and copy strategy (network FS detection routes to chunked copy).

**Decision**: Detect removable volumes by mount path (`/run/media/$USER/` or `/media/$USER/`), not by querying udev
**Why**: udev queries require the `udev` crate or shelling out to `udevadm`, adding a dependency and complexity. The FreeDesktop standard specifies that `udisks2` mounts removable media under `/run/media/$USER/`, so path-based detection is reliable on all modern distros. The path convention is simpler and sufficient.

**Decision**: GVFS network mounts have `supports_trash: false` and `is_ejectable: true`
**Why**: GVFS FUSE mounts don't implement the FreeDesktop trash specification — `gio trash` silently fails. Marking them non-trashable ensures the UI offers "delete" instead of "move to trash". They're ejectable because users expect to be able to disconnect from an SMB share (GVFS supports unmounting via `gio mount -u`).

**Decision**: Use `is_submount()` to filter bind mounts nested under another real mount
**Why**: Development setups commonly bind-mount `node_modules` or build directories as separate ext4 partitions to improve performance. Without this filter, every bind mount shows up as a separate "volume" in the sidebar, cluttering it with implementation details of the build system.

## Gotchas

**Gotcha**: The virtual filesystem type list is duplicated between `mod.rs` (`VIRTUAL_FS_TYPES`) and `watcher.rs` (`get_real_mounts`)
**Why**: `watcher.rs` independently filters mounts for its diff logic and doesn't import the constant from `mod.rs`. Both lists must stay in sync — if a new virtual fs type is added to one but not the other, the watcher will emit spurious mount/unmount events for virtual filesystems.

**Gotcha**: Hidden mount filtering uses path prefixes (`/snap/`, `/boot/`, `/run/user/`), not filesystem types
**Why**: Snap loopback mounts are `squashfs`, which is a perfectly valid real filesystem type — you can't filter them by fstype without also hiding legitimate squashfs volumes (e.g., a mounted ISO). EFI partitions are `vfat`, same story. Path-based filtering is the only way to distinguish "system internal mount" from "user-facing volume" for these cases.

**Gotcha**: `get_username()` falls back from `$USER` to `$LOGNAME` to empty string
**Why**: Some environments (systemd services, cron, containers) don't set `$USER`. `$LOGNAME` is the POSIX-specified alternative. If neither is set, removable detection returns `false` for all mounts — safe default, since wrongly marking something as non-ejectable is harmless, but wrongly marking a system mount as ejectable could let users unmount it.

## Dependencies

- `linux_mounts` (from `file_system/linux_mounts.rs`) — `/proc/mounts` parsing and fs type lookup
- `dirs` — home directory detection
- `libc` — `statvfs` for volume space info
- `notify` — inotify-based watcher on `/proc/mounts`
- Internal: `crate::file_system::{get_volume_manager, volume::LocalPosixVolume}`
