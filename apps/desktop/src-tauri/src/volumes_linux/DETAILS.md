# Volumes (Linux): details

Decision rationale. `CLAUDE.md` holds the must-knows.

## Decisions

**Decision**: two separate inotify watchers, one for `/proc/mounts`, one for `/run/user/<uid>/gvfs/`.
**Why**: GVFS SMB shares don't appear in `/proc/mounts`. GVFS uses a single FUSE mount for the whole `gvfs/` directory;
individual SMB shares are subdirectories of that FUSE mount, so a share mount/unmount is a directory create/remove,
invisible to `/proc/mounts`. Watching both sources is the only way to detect all volume changes.

**Decision**: filter virtual filesystems by an explicit fstype allowlist, not by mount-path patterns.
**Why**: filtering by path (skip `/proc`, `/sys`) misses virtual filesystems mounted at unusual locations (bind mounts,
containers) and is fragile across distros. Filtering by `fstype` is definitive: `tmpfs` is always `tmpfs` regardless of
mount point.

**Decision**: read `/proc/mounts` (parsed by `linux_mounts`) for fstype detection and network-mount classification, not
`statfs()`.
**Why**: `statfs()` collapses all FUSE mounts to a single `FUSE_SUPER_MAGIC`, so it can't distinguish `sshfs` from
`ntfs-3g`. It also blocks for minutes on hung NFS mounts and triggers automounts as a side effect. A `/proc/mounts` read
correctly identifies FUSE-based network mounts (`fuse.sshfs`, `fuse.rclone`) via fstype substrings, never blocks, and
doesn't trigger automounts. Reused by both volume discovery and copy strategy (network FS → chunked copy). Unknown
`fuse.*` subtypes are treated as network conservatively (chunked copy is the safe default).

**Decision**: detect removable volumes by mount path (`/run/media/$USER/` or `/media/$USER/`), not by querying udev.
**Why**: udev queries need the `udev` crate or shelling out to `udevadm`, adding a dependency. The FreeDesktop standard
has `udisks2` mount removable media under `/run/media/$USER/`, so path-based detection is reliable on modern distros and
simpler.

**Decision**: GVFS network mounts have `supports_trash: false` and `is_ejectable: true`.
**Why**: GVFS FUSE mounts don't implement the FreeDesktop trash spec (`gio trash` silently fails), so the UI must offer
"delete" instead of "move to trash". They're ejectable because users expect to disconnect from an SMB share (GVFS
unmounts via `gio mount -u`).

**Decision**: `is_submount()` filters bind mounts nested under another real mount.
**Why**: dev setups commonly bind-mount `node_modules` or build dirs as separate partitions for performance. Without the
filter, every bind mount shows as a separate "volume" in the sidebar, cluttering it with build-system internals.
