# Volumes (Linux)

Linux volume and location discovery, plus live mount/unmount watching. Mirrors macOS `volumes/mod.rs`'s JSON
shape (`LocationInfo`, `LocationCategory`, `VolumeSpaceInfo`). Distinct from `file_system/volume/`.

## Key files

Same file names as macOS `volumes/`. `mod.rs`
holds the model types, `DEFAULT_VOLUME_ID`, and the orchestrators (`list_locations`, `get_favorites`, `get_main_volume`,
`resolve_path_volume_fast`), and re-exports every submodule item so `crate::volumes_linux::X` stays stable:
`mounts.rs` (`get_mounted_volumes`, and which `/proc/mounts` rows are user-facing), `fs_type.rs` (trash support,
`VIRTUAL_FS_TYPES`, `get_mount_point`, `get_volume_space`), `ids.rs` (`volume_id_for_mount` and its
`/dev/disk/by-uuid` lookup), `cloud.rs` (cloud-sync dirs), `smb.rs` (CIFS mount-source and GVFS dirname parsing,
`get_network_mounts`, plus `enrich_from_volume_registry`; keep it in step with the macOS twin, the frontend doesn't
branch on platform), `watcher.rs` (mount-table and GVFS watchers; diffs known state, registers with `VolumeManager`, emits
`volume-mounted` / `volume-unmounted`).

## Must-knows

- **❌ Never derive a volume ID yourself; call `volume_id_for_mount`.** It's the twin of macOS `volumes::ids` and owns
  the same ladder: CIFS and GVFS SMB key on `(server, port, share)`, everything else on its filesystem UUID
  (`/dev/disk/by-uuid`, matched against the `/proc/mounts` device), falling back to the mount path. A share is ONE
  segment, so `//server/share/sub` is a mount INSIDE the share, on the share's own ID (`volumes/DETAILS.md` § "A mount
  can sit inside its share"). A lossy ID sends reads and deletes to the wrong disk. Mint IDs only through `cmdr_fs::volume::ids`; the rationale lives in macOS `volumes/DETAILS.md` § "A
  volume ID is derived from the volume's IDENTITY".
- **One volume ID publishes ONE location, at ONE canonical root.** `get_mounted_volumes` collapses double mounts
  (`cmdr_fs::volume::canonical_root::collapse_by_volume_id`, shared with macOS: ❌ never copy the rule back in here),
  and `list_locations` dedupes on ID, ❌ never on path alone, and a favorite on ID only (shared
  `published_locations::dedupe_locations`). `is_submount` doesn't cover this: it only catches a bind
  mount nested UNDER another volume. Collapsing is display-only, so it moves no pane and drops no root the registry
  knows. DETAILS § "One volume ID publishes one mount root".
- **The mount table is watched by `poll()` on `/proc/self/mounts` (`POLLPRI`); ❌ never inotify on `/proc/mounts`**:
  every open raises an event there, the handler's own read included, so it feeds itself. GVFS shares never reach the
  table, hence the inotify watch on `/run/user/<uid>/gvfs/`. DETAILS § "Watching the mount table".
- **An unreadable mount table is `None`, ❌ never empty**: read as empty, it unmounts every volume.
- **Virtual filesystems are filtered by an explicit fstype allowlist, NOT by mount path.** The list is duplicated in
  `VIRTUAL_FS_TYPES` (`fs_type.rs`) and `real_mounts` (`watcher.rs`); keep both in sync, or the watcher emits spurious
  mount/unmount events for the type added to only one.
- **Hidden mounts (`/snap/`, `/boot/`, `/run/user/`) are filtered by path prefix, not fstype**, because snap loopback
  mounts are `squashfs` and EFI is `vfat`, real types you can't exclude without hiding a mounted ISO.
- **Another account's FUSE mount (`user_id=` not ours, no `allow_other`) gets no row, registration, or watcher event**,
  yet stays in `mount_roots` for the index's cut. DETAILS § "Another account's FUSE mounts".
- **GVFS network mounts: `supports_trash: false`, `is_ejectable: true`**: GVFS FUSE has no FreeDesktop trash
  (`gio trash` silently fails), and users expect to disconnect a share. DETAILS § "Decisions".
- **Removable detection is path-based** (`/run/media/$USER/` or `/media/$USER/` → `is_ejectable`). `get_username()`
  falls back `$USER` → `$LOGNAME` → empty; empty makes everything non-ejectable, the safe default.
- **`is_submount()` filters bind mounts nested under a real mount**, so dev `node_modules` / build-dir bind mounts don't
  clutter the sidebar as separate volumes.
- **The volume IPC commands aren't per-platform**: one `commands/volumes.rs` serves both, reaching this module through
  a `#[cfg]`'d `platform` alias. Linux-only behavior goes behind that alias, ❌ never into a second command module.
  DETAILS § "One command module".

Full details (decision rationale): `DETAILS.md`.
