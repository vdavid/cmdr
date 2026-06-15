# Volumes (Linux)

Linux volume and location discovery, plus live mount/unmount watching via inotify. Mirrors macOS `volumes/mod.rs`'s JSON
shape (`LocationInfo`, `LocationCategory`, `VolumeSpaceInfo`). Distinct from `file_system/volume/`.

## Key files

- **`mod.rs`**: the types plus `list_locations()`, `get_volume_space()`, `get_mounted_volumes()`, cloud-drive detection,
  GVFS SMB share detection. Mount enumeration via `linux_mounts::parse_proc_mounts()`. `list_locations()` aggregates all
  categories in order and dedups by path.
- **`watcher.rs`**: two inotify watchers (see must-knows). Diffs against known state, registers/unregisters with
  `VolumeManager`, emits `volume-mounted` / `volume-unmounted` Tauri events.

Location categories: `Favorite` (user-editable, from the `favorites/` store, existence-checked), `MainVolume` (root
`/`), `AttachedVolume` (real filesystems from `/proc/mounts`), `CloudDrive` (`~/Dropbox`, `~/Google Drive`,
`~/.local/share/Nextcloud`, `~/OneDrive`), `Network` (GVFS SMB shares under `/run/user/<uid>/gvfs/smb-share:*`).

## Must-knows

- **Two separate inotify watchers: `/proc/mounts` AND `/run/user/<uid>/gvfs/`.** GVFS SMB shares never appear in
  `/proc/mounts` (the whole `gvfs/` dir is one FUSE mount; each share is a subdirectory), so a share mount/unmount is a
  directory create/remove invisible to `/proc/mounts`. Watching both is the only way to catch all volume changes.
- **Virtual filesystems are filtered by an explicit fstype allowlist, NOT by mount path.** The list is duplicated:
  `VIRTUAL_FS_TYPES` in `mod.rs` and `get_real_mounts` in `watcher.rs` (the watcher doesn't import the constant). Keep
  both in sync, or the watcher emits spurious mount/unmount events for the type added to only one. (proc, sysfs, devpts,
  tmpfs, cgroup/cgroup2, devtmpfs, and similar.)
- **Hidden mounts (`/snap/`, `/boot/`, `/run/user/`) are filtered by path prefix, not fstype**, because snap loopback
  mounts are `squashfs` and EFI is `vfat`, both valid real types you can't exclude by type without hiding legitimate
  volumes (a mounted ISO).
- **GVFS network mounts: `supports_trash: false`, `is_ejectable: true`.** GVFS FUSE mounts don't implement the
  FreeDesktop trash spec (`gio trash` silently fails), so the UI must offer "delete", not "move to trash". Ejectable
  because users expect to disconnect an SMB share.
- **Removable detection is path-based** (`/run/media/$USER/` or `/media/$USER/` → `is_ejectable`). `get_username()` falls
  back `$USER` → `$LOGNAME` → empty; empty makes everything non-ejectable, a safe default (wrongly unmountable is worse
  than wrongly non-ejectable).
- **`is_submount()` filters bind mounts nested under a real mount**, so dev `node_modules` / build-dir bind mounts don't
  clutter the sidebar as separate volumes.

## Dependencies

`linux_mounts` (`/proc/mounts` parsing + fstype lookup), `dirs`, `libc` (`statvfs`), `notify` (inotify), and
`crate::file_system::{get_volume_manager, volume::LocalPosixVolume}`.

Full details (decision rationale): [DETAILS.md](DETAILS.md).
