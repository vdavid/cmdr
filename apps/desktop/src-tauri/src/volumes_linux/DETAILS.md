# Volumes (Linux): details

Decision rationale. `CLAUDE.md` holds the must-knows.

## What each location category covers

`Favorite` (user-editable, from the `favorites/` store, existence-checked), `MainVolume` (root `/`), `AttachedVolume`
(real filesystems from `/proc/mounts`), `CloudDrive` (`~/Dropbox`, `~/Google Drive`, `~/.local/share/Nextcloud`,
`~/OneDrive`), `Network` (GVFS SMB shares under `/run/user/<uid>/gvfs/smb-share:*`).

## Dependencies

`linux_mounts` (`/proc/mounts` parsing + fstype lookup), `dirs`, `libc` (`statvfs`, `poll`), `notify` (inotify on the
GVFS directory), and `crate::file_system::volume::{manager::get_volume_manager, LocalPosixVolume}`.

## One command module

`commands/volumes.rs` serves macOS and Linux alike. The genuine platform difference is this module versus `volumes/`,
reached through one `#[cfg]`'d `platform` alias, and the only other divergence is `NETWORK_FS_TYPE` (`smbfs` / `cifs`),
which the synthetic `network` volume reports.

❌ Don't add a second, Linux-only command module beside it. A file existing only to be a registration target reads
like a Linux implementation and invites someone to put Linux behavior in it, which then has to be kept in step with the
macOS twin by hand. Linux-only behavior belongs behind the `platform` alias, in THIS module.

The pair of hand-maintained command modules this replaced had drifted: both `list_volumes` and `get_volume_space` ran
their blocking work straight on the async thread with no deadline here, so one wedged CIFS mount or a stuck
`gvfsd-fuse` held the IPC handler for as long as the kernel did. macOS had wrapped both in `blocking_with_timeout_flag`
for months. Every test in the shared module now runs on both platforms, which is what keeps them in step.

The shared `confirm_archive_boundary` call in `resolve_path_to_volume` is NOT one of those fixes, despite looking like
one. macOS needs it because `statfs` fails outright on `…/bundle.zip/docs/readme.txt`; `get_mount_point` here is a
longest-prefix match over `/proc/mounts` that never fails, so Linux already resolved such a path to the containing
drive (verified by running the macOS test against the pre-merge Linux module in the Docker lane, where it passed).
It's shared so the contract is explicit: a stat-based fast path added here later would otherwise silently start
answering `None` for archive-inner paths.

## Decisions

**Decision**: two watchers, one on the mount table (§ "Watching the mount table"), one inotify watch on
`/run/user/<uid>/gvfs/`.
**Why**: GVFS SMB shares don't appear in `/proc/mounts`. GVFS uses a single FUSE mount for the whole `gvfs/` directory;
individual SMB shares are subdirectories of that FUSE mount, so a share mount/unmount is a directory create/remove,
invisible to `/proc/mounts`. Watching both sources is the only way to detect all volume changes.

## Watching the mount table

A dedicated `mount-table-watch` thread blocks in `poll()` on an open `/proc/self/mounts`, asking for `POLLPRI`. The
kernel reports `POLLPRI | POLLERR` once per change to the process's mount namespace and nothing in between, so the
thread re-reads `/proc/mounts` and diffs it (`check_for_mount_changes`) only when something mounted or unmounted. The
change signal is per namespace, so the watch sees the same table every `/proc/mounts` read does.

The second descriptor in that `poll()` is one end of a `UnixStream` pair, and it's the stop signal: dropping
`MountTableWatch` closes the other end, the thread wakes on the hang-up and returns, and the drop joins it.
`stop_volume_watcher` does that from `app_lifecycle::stop_background_services`. The thread also returns, logged, when
`poll` fails with anything but `EINTR` or the table descriptor reports `POLLHUP` / `POLLNVAL`: both descriptors are owned
for the thread's whole life, so neither is transient, and retrying would spin.

**❌ Don't go back to inotify on `/proc/mounts`.** A mount raises no inotify event there, so that watch only woke on
`IN_OPEN` (which `notify` subscribes to), and the handler's own read is an open: each check queued the next. It woke
19,246 times in 500 ms with no mount change (verified in the `rust:latest` container the Linux Rust lane uses, running
`watching_the_mount_table_doesnt_wake_itself` against the inotify version, 2026-09-12). In the app that meant reading the
table about a thousand times a second, all the time. When a stray descriptor close in the Linux E2E run made those reads
fail, each failure unregistered every volume and re-registered it on the next good read, and that churn kept the
bad descriptor number in circulation until glibc aborted on a netlink socket.

**An unreadable table is `None`, never empty.** `linux_mounts::parse_proc_mounts` returns `Option`, and
`check_mount_table` skips the diff on `None`, so a failed read can't report `/` as unmounted. `list_locations` lists no
attached volumes for that call, and the lookups (`get_mount_point`, `volume_uuid_for_mount`, `get_smb_mount_info`,
`is_mount_point`) answer `None`.

`a_real_mount_wakes_the_watch` covers the wake side by mounting a tmpfs, which needs `CAP_SYS_ADMIN`, so it's
`#[ignore]`d: run it with `--run-ignored=ignored-only` from a `docker exec --privileged` into the Linux test container.

**Decision**: filter virtual filesystems by an explicit fstype allowlist (proc, sysfs, devpts, tmpfs, cgroup/cgroup2,
devtmpfs, and similar), not by mount-path patterns.
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

## Another account's FUSE mounts

**Decision**: a FUSE mount (`fuse`, `fuse.*`, `fuseblk`) whose options carry a `user_id=` other than this process's uid
and no `allow_other` is another account's own, and hidden: `mounts::is_private_to_another_user`. `get_mounted_volumes`
gives it no row, `registrable_mount_roots` keeps it out of the registry sweep, and the watcher's `real_mounts` leaves
it out of the diff, so it never fires `volume-mounted` or `volume-unmounted`. `mount_roots` still lists it, because
the index cuts its boot scan at every mount, one it can't enter included.

**Why**: the twin of macOS `volumes/DETAILS.md` § "Another account's mounts", which has the report behind it (a second
account's cloud drive got a row nobody on this account could open) and what each consumer does. The signal differs:
`/proc/mounts` has no owner column, but a FUSE row prints `user_id=` and `allow_other` in its options, and those two
ARE the kernel's access rule: `allow_other` "overrides the security measure restricting file access to the user
mounting the filesystem", and that measure covers root's processes too unless the `allow_sys_admin_access` module option
is set (read from the kernel's [FUSE overview](https://docs.kernel.org/filesystems/fuse/fuse.html), 2026-09-30; not
reproduced on a two-account machine). So there's no disk exception and no root exception here, unlike macOS: a
`fuseblk` NTFS disk that every account can use is one mounted with `allow_other`, and that's what the rule reads. One
known miss: Cmdr run as root on a machine with `allow_sys_admin_access` could enter a mount this hides.

**What it leaves alone**: every non-FUSE mount. A CIFS `uid=` names who owns the files, which says nothing about who
may enter, and a kernel filesystem's access is ordinary permissions on its root.

## A CIFS mount source can name a directory inside the share

`mount -t cifs //server/share/sub /mnt/x` records the whole path in the `/proc/mounts` device field, exactly as macOS
records a DFS sub-mount, so `parse_smb_mount_source` here splits it the same way its macOS twin does: `share` is the
first segment, `SmbMountInfo::subpath` is everything below it. Same field, same meaning, so a share reached this way
derives the share's own volume ID and the backend addresses it through the same anchor. The rule and the incident
behind it live once, in `volumes/DETAILS.md` § "A mount can sit inside its share".

The authority (`user:password@host:port`, a bracketed IPv6 host) goes through the twin's own splitter,
`cmdr_fs::volume::smb_mount_source::split_authority`, so the two platforms can't disagree about a host, a port, or a
username (`volumes/DETAILS.md` § "SMB mount sources are percent-escaped").

Two differences from the macOS twin, both pre-existing: the segments are taken verbatim (percent-decoding is a macOS
mount-source concern, `volumes/DETAILS.md` § "SMB mount sources are percent-escaped"), and GVFS shares don't come
through this parser at all but through `parse_gvfs_smb_dirname`, which carries no subpath.

## A path inside a GVFS share resolves to the share

GVFS serves every share through ONE FUSE mount at `/run/user/<uid>/gvfs`, each share a subdirectory, so the
longest-prefix walk over `/proc/mounts` in `resolve_path_volume_fast` lands on that FUSE root for every share: a volume
with no row and no registration. A path under `/run/user/<uid>/gvfs/smb-share:…` (`smb::gvfs_share_root`, pure, no
syscall) answers the share's own row instead, built by the same `smb::gvfs_share_location` discovery lists it with, so
the two can't name one share two ways. ❗ This is what lets a favorite on a GVFS share remember the share's id (the add
gate resolves through here); before, it stored the FUSE root's path id and read "unplugged drive" from the first
listing (caught by `test/e2e-playwright/favorites-offline-smb.spec.ts`, Linux Docker lane, 2026-10-10).

Two GVFS gaps this doesn't close, both visible to that spec: a GVFS mount saves no share place (#348,
`network/DETAILS.md` § "Saved SMB shares"), so an unmounted share's favorite reads `forgotten` rather than `connects`;
and in the Docker lane the GVFS watcher's inotify sees nothing inside the FUSE mount, so an unmount from outside Cmdr
pushes no `volumes-changed` until something else does.

## One volume ID publishes one mount root

**Decision**: `get_mounted_volumes` collapses mounts that share a volume ID through
`cmdr_fs::volume::canonical_root::collapse_by_volume_id`, and `list_locations` dedupes on ID as well as path (favorites on ID only) through the shared
`cmdr_fs::volume::published_locations::dedupe_locations`. macOS
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
