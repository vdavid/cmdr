# Volumes (Linux): details

Decision rationale. `CLAUDE.md` holds the must-knows.

## What each location category covers

`Favorite` (user-editable, from the `favorites/` store, existence-checked), `MainVolume` (root `/`), `AttachedVolume`
(real filesystems from `/proc/mounts`), `CloudDrive` (`~/Dropbox`, `~/Google Drive`, `~/.local/share/Nextcloud`,
`~/OneDrive`), `Network` (GVFS SMB shares under `/run/user/<uid>/gvfs/smb-share:*`).

## Dependencies

`linux_mounts` (`/proc/mounts` parsing + fstype lookup), `dirs`, `libc` (`statvfs`), `notify` (inotify), and
`crate::file_system::volume::{manager::get_volume_manager, LocalPosixVolume}`.

## One command module

`commands/volumes.rs` serves macOS and Linux alike. The genuine platform difference is this module versus `volumes/`,
reached through one `#[cfg]`'d `platform` alias, and the only other divergence is `NETWORK_FS_TYPE` (`smbfs` / `cifs`),
which the synthetic `network` volume reports.

`commands::volumes_linux` still resolves, because `ipc.rs` and `ipc_collectors.rs` register the Linux command set under
that path, but it's a `pub use volumes as volumes_linux;` in `commands/mod.rs` rather than a file. A file existing only
to be a registration target reads like a Linux implementation and invites someone to put Linux behavior in it; a
one-line alias sitting next to the `pub mod volumes` it points at can't. It goes away when those registrations move to
`volumes`.

The pair of hand-maintained command modules this replaced had drifted: Linux resolved a path inside an archive to
nothing (no `confirm_archive_boundary`, so a pane deep-linked inside a `.zip` couldn't find its volume), and both
`list_volumes` and `get_volume_space` ran their blocking work straight on the async thread with no deadline, so one
wedged CIFS mount held the IPC handler. macOS had all three guards. Every test in the shared module now runs on both
platforms, which is what keeps them.

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

## One volume ID publishes one mount root

**Decision**: `get_mounted_volumes` collapses mounts that share a volume ID through
`cmdr_fs::volume::canonical_root::collapse_by_volume_id`, and `list_locations` dedupes on ID as well as path. macOS
calls the same function from `get_attached_volumes`; the rationale for the rule (and for the shortest-path tie-break)
lives once, in `volumes/DETAILS.md` § "One volume ID publishes one mount root".

**Why here too**: `is_submount()` only catches a bind mount NESTED under another volume, so a share mounted twice at
unrelated paths (`/mnt/data` and `/srv/data`), a CIFS share mounted twice, or a container mount all reach the list as
separate rows deriving one ID. The frontend's `dedupeById` net then drops one of them by arrival order, which is
alphabetical by display name and says nothing about which root is canonical.

**Why a shared function instead of a Linux copy**: the rule is a pure list transform over `(volume id, mount root)`
pairs, not platform knowledge. What IS platform-specific is deriving the ID from a mount (`/proc/mounts` plus
`/dev/disk/by-uuid` here, `getfsstat` plus the filesystem UUID on macOS), and that stays in each platform's module. The
two `LocationInfo` structs stay separate; each implements `MountRootCandidate` so the collapse asks for the two fields
it needs.

**What it does NOT do**: it never moves a pane and never makes a root unfindable. Collapsing only decides what the
switcher lists; the registry keeps every mount root it learns about (`file_system/volume/DETAILS.md` § "A volume ID owns
a set of mount roots"), and the mount watcher's `register` records a second mount as a fallback root when it arrives at
runtime. The gap it leaves, same as macOS: mounts that already existed at launch are collapsed BEFORE
`register_discovered_volumes` sees them, so only the canonical root is registered at startup.

**Testability seam**: `get_mounted_volumes_with` takes the ID derivation as a parameter because `volume_id_for_mount`
reads the LIVE `/proc/mounts` and `/dev/disk/by-uuid`, which a `mounts` fixture can't stand in for. A test that needs
two mounts to share an ID has to say so directly.
