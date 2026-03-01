# Volumes (Linux)

Linux volume and location discovery, plus live mount/unmount watching via inotify on `/proc/mounts`.

## Key files

| File | Purpose |
|---|---|
| `mod.rs` | `LocationInfo`, `LocationCategory`, `VolumeSpaceInfo` types (mirrors macOS `volumes/mod.rs` JSON shape). `list_locations()`, `get_volume_space()`, `get_mounted_volumes()`, cloud drive detection. Uses `linux_mounts::parse_proc_mounts()` for mount enumeration. |
| `watcher.rs` | `notify` (inotify) watcher on `/proc/mounts`. Detects mount/unmount by diffing real mounts against `KNOWN_MOUNTS`. Registers/unregisters with `VolumeManager`. Emits `volume-mounted` / `volume-unmounted` Tauri events. |

## Location categories

```
Favorite       — Home, ~/Desktop, ~/Documents, ~/Downloads (only if they exist)
MainVolume     — root "/" filesystem
AttachedVolume — real filesystems from /proc/mounts (filters out virtual: proc, sysfs, tmpfs, etc.)
CloudDrive     — ~/Dropbox, ~/Google Drive, ~/.local/share/Nextcloud, ~/OneDrive
Network        — not yet implemented
```

`list_locations()` aggregates all categories in order and deduplicates by path.

## Virtual filesystem filtering

Both `mod.rs` and `watcher.rs` share the same list of virtual FS types to exclude: proc, sysfs, devpts, tmpfs, cgroup, cgroup2, devtmpfs, hugetlbfs, mqueue, debugfs, tracefs, securityfs, pstore, configfs, fusectl, binfmt_misc, autofs, efivarfs, ramfs, rpc_pipefs, nfsd, nsfs, bpf.

## Removable volume detection

Mounts under `/run/media/$USER/` or `/media/$USER/` are marked as ejectable (`is_ejectable: true`).

## Dependencies

- `linux_mounts` (from `file_system/linux_mounts.rs`) — `/proc/mounts` parsing and fs type lookup
- `dirs` — home directory detection
- `libc` — `statvfs` for volume space info
- `notify` — inotify-based watcher on `/proc/mounts`
- Internal: `crate::file_system::{get_volume_manager, volume::LocalPosixVolume}`
