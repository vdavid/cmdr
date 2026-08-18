# Volumes (Linux)

Linux volume and location discovery, plus live mount/unmount watching via inotify. Mirrors macOS `volumes/mod.rs`'s JSON
shape (`LocationInfo`, `LocationCategory`, `VolumeSpaceInfo`). Distinct from `file_system/volume/`.

## Key files

Same file names as macOS `volumes/`, so a rule you know on one platform sits where you expect on the other. `mod.rs`
re-exports every submodule item, keeping `crate::volumes_linux::X` paths stable.

- **`mod.rs`**: model types, `DEFAULT_VOLUME_ID`, orchestrators (`list_locations`, `get_favorites`, `get_main_volume`,
  `resolve_path_volume_fast`).
- **`mounts.rs`**: `get_mounted_volumes` and the filters for which `/proc/mounts` rows are user-facing.
- **`fs_type.rs`**: trash support, `VIRTUAL_FS_TYPES`, `get_mount_point`, `get_volume_space` (`statvfs`).
- **`ids.rs`**: `volume_id_for_mount` and its `/dev/disk/by-uuid` lookup. **`cloud.rs`**: cloud-sync dirs.
- **`smb.rs`**: CIFS mount-source and GVFS dirname parsing, `get_network_mounts`, plus
  `enrich_from_volume_registry` (the macOS twin's capability half; keep them in step, the frontend doesn't branch on
  platform).
- **`watcher.rs`**: two inotify watchers (see must-knows). Diffs known state, registers/unregisters with
  `VolumeManager`, emits `volume-mounted` / `volume-unmounted` Tauri events.

## Must-knows

- **❌ Never derive a volume ID yourself; call `volume_id_for_mount`.** It's the twin of macOS `volumes::ids` and owns
  the same ladder: CIFS and GVFS SMB key on `(server, port, share)`, everything else on its filesystem UUID
  (`/dev/disk/by-uuid`, matched against the `/proc/mounts` device), falling back to the mount path. An ID keys the index
  DB, `lastUsedPaths`, tab state, and operation routing, so one that loses information sends reads and deletes to the
  wrong disk. Mint IDs only through `cmdr_fs::volume::ids`; the rationale lives in macOS `volumes/DETAILS.md` § "A
  volume ID is derived from the volume's IDENTITY".
- **One volume ID publishes ONE location, at ONE canonical root.** `get_mounted_volumes` collapses double mounts
  (`cmdr_fs::volume::canonical_root::collapse_by_volume_id`, shared with macOS: ❌ never copy the rule back in here),
  and `list_locations` dedupes on ID, ❌ never on path alone. `is_submount` doesn't cover this: it only catches a bind
  mount nested UNDER another volume. Collapsing is display-only, so it moves no pane and drops no root the registry
  knows. DETAILS § "One volume ID publishes one mount root".
- **Two separate inotify watchers: `/proc/mounts` AND `/run/user/<uid>/gvfs/`.** GVFS SMB shares never appear in
  `/proc/mounts` (the whole `gvfs/` dir is one FUSE mount; each share is a subdirectory), so a share mount/unmount is a
  directory create/remove invisible to `/proc/mounts`. Watching both is the only way to catch all volume changes.
- **Virtual filesystems are filtered by an explicit fstype allowlist, NOT by mount path.** The list is duplicated:
  `VIRTUAL_FS_TYPES` in `fs_type.rs` and `get_real_mounts` in `watcher.rs` (the watcher doesn't import the constant). Keep
  both in sync, or the watcher emits spurious mount/unmount events for the type added to only one. (proc, sysfs, devpts,
  tmpfs, cgroup/cgroup2, devtmpfs, and similar.)
- **Hidden mounts (`/snap/`, `/boot/`, `/run/user/`) are filtered by path prefix, not fstype**, because snap loopback
  mounts are `squashfs` and EFI is `vfat`, both valid real types you can't exclude by type without hiding legitimate
  volumes (a mounted ISO).
- **GVFS network mounts: `supports_trash: false`, `is_ejectable: true`.** GVFS FUSE mounts don't implement the
  FreeDesktop trash spec (`gio trash` silently fails), so the UI must offer "delete", not "move to trash". Ejectable
  because users expect to disconnect an SMB share.
- **Removable detection is path-based** (`/run/media/$USER/` or `/media/$USER/` → `is_ejectable`). `get_username()`
  falls back `$USER` → `$LOGNAME` → empty; empty makes everything non-ejectable, the safe default.
- **`is_submount()` filters bind mounts nested under a real mount**, so dev `node_modules` / build-dir bind mounts don't
  clutter the sidebar as separate volumes.
- **The volume IPC commands aren't per-platform**: `commands/volumes.rs` serves both through one `platform` alias, and
  `commands::volumes_linux` is a `pub use` of it, not a module. Linux-only behavior goes behind the alias. DETAILS §
  "One command module".

Full details (decision rationale): `DETAILS.md`.
