//! Attached-volume enumeration: snapshotting the kernel mount table via the
//! non-blocking `getfsstat`, filtering it to the mounts a user should see a row
//! for, and enriching only local mounts (network mounts stay non-blocking so a
//! hung mount can't stall discovery). See `DETAILS.md` § "Hung mounts" and
//! § "Which mounts get a row".

use super::{
    LocationCategory, LocationInfo, SmbMountInfo, disk_image, get_icon_for_path, get_volume_name, get_volume_uuid,
    is_network_fs_type, is_smb_fs_type, is_volume_ejectable, parse_smb_mount_source, supports_trash_for_fs_type,
    volume_id_for, volume_name_from_path,
};
use cmdr_fs::volume::canonical_root::collapse_by_volume_id;
use cmdr_fs::volume::friendly_error::Provider;
use std::path::Path;

/// One entry from the kernel mount table, as returned by `getfsstat(MNT_NOWAIT)`.
///
/// Every field comes straight out of `statfs` with no follow-up syscall, so
/// building one never talks to the backing filesystem. That is the whole reason
/// discovery uses `getfsstat` instead of NSFileManager's volume enumeration: the
/// enumeration `getattrlist`s every mount, which blocks 30s–forever on a hung
/// network mount and froze the app at launch. See `DETAILS.md` § "Hung mounts".
pub(super) struct MountEntry {
    /// Mount point, e.g. `/Volumes/naspi` (`f_mntonname`).
    mount_point: String,
    /// Filesystem type, e.g. `apfs`, `exfat`, `smbfs` (`f_fstypename`).
    fs_type: String,
    /// Mount source, e.g. `//david@192.168.1.111/naspi` for SMB (`f_mntfromname`).
    mount_from: String,
    /// Whether the mount carries the `MNT_RDONLY` flag.
    is_read_only: bool,
    /// Whether the OS says this mount belongs in a file manager's sidebar, i.e.
    /// it does NOT carry `MNT_DONTBROWSE`. Finder's own rule, and the one that
    /// separates real drives from the plumbing (`devfs`, `/System/Volumes/*`,
    /// `/Volumes/Recovery`, autofs triggers) without naming any of them.
    is_browsable: bool,
    /// The mounted filesystem's identity (`f_fsid`, its two words packed high then
    /// low). Renaming a mounted volume moves the mount point and keeps this.
    fsid: u64,
    /// Who mounted it (`f_owner`).
    mounted_by: MountedBy,
}

/// Who mounted a filesystem: the mount-table row's `f_owner`, which `mount` prints
/// as "mounted by `<name>`" for every row whose owner isn't root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MountedBy {
    /// `f_owner` 0: the system's own mounts, and anything mounted as root.
    Root,
    /// The account Cmdr runs as.
    ThisUser,
    /// Another account on this Mac, for example one still logged in behind fast
    /// user switching.
    AnotherUser,
}

impl MountedBy {
    /// Pure, so the three answers are testable without a second account.
    fn from_owner(owner: libc::uid_t, this_user: libc::uid_t) -> Self {
        if owner == this_user {
            Self::ThisUser
        } else if owner == 0 {
            Self::Root
        } else {
            Self::AnotherUser
        }
    }
}

impl MountEntry {
    /// Whether this is another account's own mount, which this account can't open:
    /// a cloud client's or FUSE layer's drive that someone else on this Mac mounted.
    ///
    /// Two exceptions keep everything a second account CAN use:
    ///
    /// - **A disk** (mounted from `/dev/…`): a drive plugged in while someone else
    ///   was logged in is "mounted by" them, and it's still everyone's.
    /// - **A network mount** (`is_network_fs_type`): whether another account's SMB,
    ///   NFS, WebDAV, or AFP mount is usable from this one is unverified, so those
    ///   stay as they were. `DETAILS.md` § "Another account's mounts" says what
    ///   would widen the rule.
    ///
    /// ❗ Read off the snapshot row, ❌ never an access probe: that's a syscall per
    /// mount that can hang on a dead share, and the drive this rule exists for
    /// answered an ambiguous "No such file or directory".
    fn is_private_to_another_user(&self) -> bool {
        self.mounted_by == MountedBy::AnotherUser
            && !self.mount_from.starts_with("/dev/")
            && !is_network_fs_type(Some(&self.fs_type))
    }
}

/// Snapshot the kernel mount table without blocking on any mount.
///
/// `getfsstat(MNT_NOWAIT)` returns cached mount metadata and never round-trips to
/// a filesystem, so a wedged network mount can't stall it — unlike `MNT_WAIT`,
/// which is what makes plain `df` hang.
///
/// ❗ **`None` when the syscall wouldn't answer, ❌ never an empty list.** A live
/// system always lists `/`, so an empty table is a failed read wearing a positive
/// answer's clothes: every caller above reads "nothing is mounted", and the one that
/// groups a disk's volumes then lets an unmount take a sibling down under its live
/// FSEvents watcher. Callers name the third answer (`is_mount_point`,
/// `has_mount_identity`, `mount_sources`) or fold it deliberately.
pub(super) fn enumerate_mounts() -> Option<Vec<MountEntry>> {
    // First pass: ask how many mounts exist (null buffer writes nothing).
    // SAFETY: `getfsstat(NULL, 0, flags)` is the documented count query; with a
    // null buffer and zero size the kernel only returns the mount count.
    let count = unsafe { libc::getfsstat(std::ptr::null_mut(), 0, libc::MNT_NOWAIT) };
    if count <= 0 {
        return None;
    }

    // A few slots of slack in case a mount appears between the two calls.
    let capacity = count as usize + 4;
    let mut buf: Vec<libc::statfs> = Vec::with_capacity(capacity);
    let bufsize = (capacity * size_of::<libc::statfs>()) as libc::c_int;
    // SAFETY: `buf` has room for `capacity` `statfs` records and `bufsize` matches
    // that byte length, so the kernel fills at most `capacity` records and returns
    // how many it wrote.
    let filled = unsafe { libc::getfsstat(buf.as_mut_ptr(), bufsize, libc::MNT_NOWAIT) };
    if filled <= 0 {
        return None;
    }
    // SAFETY: the kernel initialized `filled` records; clamp to our capacity in
    // case the mount table grew past the slack between the two calls.
    unsafe { buf.set_len((filled as usize).min(capacity)) };

    // SAFETY: `getuid` takes no arguments, touches no memory, and can't fail.
    let this_user = unsafe { libc::getuid() };

    Some(
        buf.iter()
            .map(|s| MountEntry {
                mount_point: cstr_field_to_string(&s.f_mntonname),
                fs_type: cstr_field_to_string(&s.f_fstypename),
                mount_from: cstr_field_to_string(&s.f_mntfromname),
                is_read_only: (s.f_flags & libc::MNT_RDONLY as u32) != 0,
                is_browsable: (s.f_flags & libc::MNT_DONTBROWSE as u32) == 0,
                fsid: packed_fsid(s),
                mounted_by: MountedBy::from_owner(s.f_owner, this_user),
            })
            .collect(),
    )
}

/// Convert a NUL-terminated `c_char` array from `statfs` into a `String`
/// (UTF-8 lossy, since mount points and volume names can be non-ASCII).
fn cstr_field_to_string(field: &[libc::c_char]) -> String {
    let bytes: Vec<u8> = field.iter().take_while(|&&c| c != 0).map(|&c| c as u8).collect();
    String::from_utf8_lossy(&bytes).into_owned()
}

/// A `statfs` record's `f_fsid`, its two words packed high then low into one `u64`.
fn packed_fsid(stat: &libc::statfs) -> u64 {
    // SAFETY: libc declares `fsid_t` `#[repr(C)]` around exactly one `[i32; 2]` and
    // only keeps that field private, so reading the value as the array reads the field.
    let [high, low]: [i32; 2] = unsafe { std::mem::transmute(stat.f_fsid) };
    (u64::from(high.cast_unsigned()) << 32) | u64::from(low.cast_unsigned())
}

/// Whether `path` is a mount point in the kernel mount table. Reads the same
/// non-blocking `getfsstat` snapshot discovery does, so a hung mount can't stall
/// it. `None` when the table couldn't be read (a live system always lists `/`).
pub(crate) fn is_mount_point(path: &str) -> Option<bool> {
    let mounts = enumerate_mounts()?;
    let path = Path::new(path);
    Some(mounts.iter().any(|m| Path::new(&m.mount_point) == path))
}

/// The identity of the filesystem mounted exactly at `path`, from the same
/// non-blocking table: `None` when nothing is mounted there or the table couldn't be
/// read. With mounts stacked on one path, the last one listed is the one a lookup
/// reaches.
pub(crate) fn mount_identity_at(path: &str) -> Option<u64> {
    let path = Path::new(path);
    enumerate_mounts()?
        .into_iter()
        .rev()
        .find(|m| Path::new(&m.mount_point) == path)
        .map(|m| m.fsid)
}

/// The filesystem type and source of the mount `path` lies on: the deepest mount
/// point that's a whole-component prefix of it, from the same non-blocking table, so
/// a hung mount can't stall it. Lexical: a symlink on the way isn't followed. `None`
/// when the table couldn't be read. With mounts stacked on one path, the last one
/// listed is the one a lookup reaches.
pub(crate) fn mount_type_and_source_for(path: &Path) -> Option<(String, String)> {
    mount_under(&enumerate_mounts()?, path).map(|m| (m.fs_type.clone(), m.mount_from.clone()))
}

/// The filesystem type of the mount `path` lies on, from a table already read: lexical, so it
/// can't hang on the mount it names. What favorites discovery asks before it decides whether a
/// folder may be stat'd at all.
pub(super) fn fs_type_under<'a>(mounts: &'a [MountEntry], path: &Path) -> Option<&'a str> {
    mount_under(mounts, path).map(|mount| mount.fs_type.as_str())
}

/// A browsable mount-table row for a sibling module's test, which can't name the private fields.
#[cfg(test)]
pub(super) fn mount_entry_for_test(mount_point: &str, fs_type: &str) -> MountEntry {
    MountEntry {
        mount_point: mount_point.to_string(),
        fs_type: fs_type.to_string(),
        mount_from: String::new(),
        is_read_only: false,
        is_browsable: true,
        fsid: 0,
        mounted_by: MountedBy::ThisUser,
    }
}

/// [`mount_type_and_source_for`] over a table already read.
fn mount_under<'a>(mounts: &'a [MountEntry], path: &Path) -> Option<&'a MountEntry> {
    // `max_by_key` keeps the LAST of equal keys, so a stacked mount wins.
    mounts
        .iter()
        .filter(|m| path.starts_with(&m.mount_point))
        .max_by_key(|m| m.mount_point.len())
}

/// Every mount point the kernel currently lists, from the same non-blocking snapshot.
///
/// What the INDEX cuts its boot-tree scan at, so it's every row, another account's
/// mounts included: a scan has to stop at a mount it can't enter too. ❗ `None` when
/// the table couldn't be read, ❌ never an empty list.
pub(crate) fn mount_roots() -> Option<Vec<String>> {
    Some(enumerate_mounts()?.into_iter().map(|mount| mount.mount_point).collect())
}

/// The mount points the volume REGISTRY sweeps (`file_system::volume::mount_registration`):
/// every row except another account's own mounts.
///
/// A different question from what the switcher shows: resolution can mint an ID for any mount
/// this account can reach, so registration has to cover all of those, browsable or not. ❗ `None`
/// when the table couldn't be read, ❌ never an empty list, or the sweep would read a machine with
/// no mounts and register nothing.
pub(crate) fn registrable_mount_roots() -> Option<Vec<String>> {
    Some(registrable_roots(enumerate_mounts()?))
}

/// [`registrable_mount_roots`] over a table already read.
fn registrable_roots(mounts: Vec<MountEntry>) -> Vec<String> {
    mounts
        .into_iter()
        .filter(|mount| !mount.is_private_to_another_user())
        .map(|mount| mount.mount_point)
        .collect()
}

/// Whether the mount at `path` is another account's own
/// ([`MountEntry::is_private_to_another_user`]), from the same non-blocking table. `false` when
/// nothing is mounted there or the table couldn't be read: the mount watcher asks this to decide
/// whether to stay quiet about a mount, and an unknown one is announced as it always was.
pub(crate) fn is_private_to_another_user(path: &str) -> bool {
    enumerate_mounts().is_some_and(|mounts| private_to_another_user_at(&mounts, path))
}

/// [`is_private_to_another_user`] over a table already read. With mounts stacked on one path,
/// the last one listed is the one a lookup reaches.
fn private_to_another_user_at(mounts: &[MountEntry], path: &str) -> bool {
    let path = Path::new(path);
    mounts
        .iter()
        .rev()
        .find(|mount| Path::new(&mount.mount_point) == path)
        .is_some_and(MountEntry::is_private_to_another_user)
}

/// A mount's point and the source it was mounted from (`f_mntfromname`), for code that maps
/// mounts to the disks under them. Straight from the non-blocking snapshot: no syscall per entry.
pub(crate) struct MountSource {
    /// Mount point, e.g. `/Volumes/naspi`.
    pub(crate) mount_point: std::path::PathBuf,
    /// Mount source, e.g. `/dev/disk5s1` for a local disk.
    pub(crate) mount_from: String,
}

/// Every mount's point and source, from the same non-blocking `getfsstat` snapshot discovery reads.
///
/// ❗ `None` when the table couldn't be read, ❌ never an empty list: this is what maps mounts to the
/// disks under them, so an empty answer reads as "nothing is on this disk" and lets a whole-disk
/// unmount past a sibling nobody stopped.
pub(crate) fn mount_sources() -> Option<Vec<MountSource>> {
    Some(
        enumerate_mounts()?
            .into_iter()
            .map(|mount| MountSource {
                mount_point: std::path::PathBuf::from(mount.mount_point),
                mount_from: mount.mount_from,
            })
            .collect(),
    )
}

/// Every SMB mount the kernel lists, as `(mount point, what the mount source says)`,
/// from the same non-blocking `getfsstat` snapshot: no syscall per entry, so a hung
/// share can't stall it and nothing goes out on the network.
///
/// The SMB upgrade paths ask this, ❌ never the volume registry: the registry fills on
/// a background thread that `statfs`es every mount, so at launch it lags the kernel
/// by seconds, and a pass that asked it read "no SMB mounts" with four of them up.
/// `None` when the table couldn't be read.
pub(crate) fn smb_mounts() -> Option<Vec<(String, SmbMountInfo)>> {
    Some(
        enumerate_mounts()?
            .into_iter()
            .filter_map(|mount| {
                let info = smb_info(&mount)?;
                Some((mount.mount_point, info))
            })
            .collect(),
    )
}

/// Whether any filesystem in the kernel mount table has identity `fsid`: `None` when
/// the table couldn't be read. The index asks this, ❌ never [`is_mount_point`], to
/// tell a drive that's gone from one a rename moved.
pub(crate) fn has_mount_identity(fsid: u64) -> Option<bool> {
    let mounts = enumerate_mounts()?;
    Some(mounts.iter().any(|m| m.fsid == fsid))
}

/// Whether a mount should surface as a drive in the switcher.
///
/// ❗ A DISPLAY question, and the strict one. The volume REGISTRY sweeps the whole
/// mount table regardless (`file_system::volume::mount_registration`), because
/// resolution can mint an ID for any row of it; a mount this drops is still
/// openable, it just doesn't get a row of its own.
///
/// Two ways in, and the first is the OS's own answer:
///
/// 1. **Browsable**: the mount doesn't carry `MNT_DONTBROWSE`, which is exactly
///    what keeps `devfs`, `/System/Volumes/*`, `/Volumes/Recovery`, and autofs
///    triggers out of Finder's sidebar. Reading the flag beats the `/Volumes/`
///    prefix test it replaces: that one both let `/Volumes/Recovery` through (a
///    name check caught it) and hid every drive mounted anywhere else.
/// 2. **Served by a provider we recognize** (`provider` is `Some`), browsable or
///    not, wherever it's mounted: a cloud client or FUSE layer may mark its
///    mount unbrowsable and add its own Finder shortcut instead. An allowlist,
///    so it fails closed: Xcode's `DeviceFS` is an unbrowsable system mount
///    inside `$HOME`, and the next one Apple adds stays out untouched.
///
/// ❗ Admission asks `provider.is_some()`, ❌ never `is_cloud_storage()`: that's
/// the separate CLOUD-or-VOLUMES question, and a FUSE container is a drive we
/// recognize that isn't cloud storage.
///
/// Never the boot volume (it has its own row), never a dot-prefixed mount, never
/// a `~/Library/CloudStorage` path, which the cloud-drive arm publishes and would
/// otherwise appear twice, and never another account's own mount
/// ([`MountEntry::is_private_to_another_user`]), recognized provider or not: a row
/// for it is a drive this account can't open. Pure, so it's unit-testable.
fn is_user_facing_mount(mount: &MountEntry, provider: Option<Provider>) -> bool {
    let path = Path::new(&mount.mount_point);
    if path == Path::new("/") {
        return false;
    }
    if mount.mount_point.contains("/Library/CloudStorage") {
        return false;
    }
    // Hidden mount (leading dot on the last component), e.g. `/Volumes/.timemachine`.
    if let Some(name) = path.file_name().and_then(|n| n.to_str())
        && name.starts_with('.')
    {
        return false;
    }
    if mount.is_private_to_another_user() {
        return false;
    }
    mount.is_browsable || provider.is_some()
}

/// Metadata for a LOCAL mount, resolved via blocking macOS APIs (NSURL +
/// DiskArbitration + NSWorkspace). Only computed for local mounts — network
/// mounts skip it so a hung mount never blocks discovery.
struct LocalVolumeMeta {
    name: String,
    is_ejectable: bool,
    icon: Option<String>,
    is_disk_image: bool,
    /// The volume's filesystem UUID, what its ID keys on. Gathered here because
    /// this struct is exactly the "blocking, local mounts only" seam the NSURL
    /// lookup belongs behind.
    uuid: Option<String>,
}

/// The SMB mount info for a network mount, or `None` if it isn't an SMB mount we
/// can parse. Read straight out of the `getfsstat` snapshot, no syscall.
fn smb_info(mount: &MountEntry) -> Option<SmbMountInfo> {
    is_smb_fs_type(Some(&mount.fs_type))
        .then(|| parse_smb_mount_source(&mount.mount_from))
        .flatten()
}

/// The display name for a network mount, from the non-blocking `f_mntfromname`.
/// SMB mounts read as "share on server"; other network mounts fall back to the
/// path. No syscalls: everything comes from the `getfsstat` snapshot. Pure.
fn network_name(mount: &MountEntry) -> String {
    match smb_info(mount) {
        Some(info) => {
            // With the port off 445, as the saved row names it: two servers on one
            // machine would otherwise read identically.
            let server = crate::network::smb_server_address::friendly_server_name(&info.server);
            let display = crate::network::server_identity::smb_server(&server, info.port);
            format!("{} on {}", info.share, display)
        }
        None => volume_name_from_path(&mount.mount_point),
    }
}

/// Classify one mount-table entry into a switcher [`LocationInfo`], or `None` if
/// it isn't a user-facing attached volume.
///
/// Network mounts (SMB, NFS, WebDAV, …) are built purely from the non-blocking
/// `statfs` data already in `mount`; `resolve_local` is NOT called for them. That
/// is the guarantee a hung network mount can't block discovery of the other
/// volumes. Local mounts call `resolve_local` for their name, ejectability, icon,
/// and disk-image status (safe: local disks don't hang). Splitting the blocking
/// enrichment behind a closure also keeps the classification unit-testable.
fn build_attached_location(
    mount: &MountEntry,
    resolve_local: impl FnOnce(&str) -> LocalVolumeMeta,
) -> Option<LocationInfo> {
    let path = mount.mount_point.as_str();
    let fs_type = mount.fs_type.clone();
    let provider = super::mount_provider(path, &fs_type);
    if !is_user_facing_mount(mount, provider) {
        return None;
    }
    let supports_trash = supports_trash_for_fs_type(Some(&fs_type));

    let (id, name, is_ejectable, icon, is_disk_image) = if is_network_fs_type(Some(&fs_type)) {
        // Network mount: derive everything from the non-blocking snapshot. Never
        // touch NSURL / NSWorkspace / DiskArbitration here — those are exactly the
        // calls that hang on a dead mount (which is also why the ID gets no UUID
        // to key on). `is_ejectable` is cosmetically moot for network mounts (the
        // eject affordance keys on `connectionState`, and the eject flow forces
        // it true), so a safe `false` costs nothing.
        let id = volume_id_for(path, Some(&fs_type), smb_info(mount).as_ref(), None);
        (id, network_name(mount), false, None, false)
    } else {
        // Local mount: safe to run the blocking enrichment.
        let meta = resolve_local(path);
        (
            volume_id_for(path, Some(&fs_type), None, meta.uuid.as_deref()),
            meta.name,
            meta.is_ejectable,
            meta.icon,
            meta.is_disk_image,
        )
    };

    // A mount a cloud provider serves is grouped with the other cloud drives and
    // kept out of the index affordances, whether it sits in `/Volumes` or in the
    // home folder. `is_cloud_mount` travels as its own typed field: ❌ never
    // re-derive it downstream from the category or the fs type.
    let is_cloud_mount = provider.is_some_and(|provider| provider.is_cloud_storage());
    let category = match is_cloud_mount {
        true => LocationCategory::CloudDrive,
        false => LocationCategory::AttachedVolume,
    };

    Some(LocationInfo {
        id,
        name,
        path: path.to_string(),
        category,
        icon,
        is_ejectable,
        fs_type: Some(fs_type),
        supports_trash,
        mount_is_read_only: mount.is_read_only,
        is_disk_image,
        is_cloud_mount,
        connection_state: None,
        pinned: None,
        landing_path: None,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
        favorite_shortcut: None,
        favorite_target: None,
        // An SMB share is called by its own name, whatever `/Volumes` dir it got.
        root_label: smb_info(mount).map(|info| info.share),
        mount_account: smb_info(mount).and_then(|info| info.username),
    })
}

/// Get attached volumes (external drives, USB, network mounts, etc.), from a fresh
/// snapshot of the mount table. [`attached_volumes_in`] has the rules.
pub fn get_attached_volumes() -> Vec<LocationInfo> {
    // A table nobody could read means no rows this pass. The switcher keeps what it has and the
    // next discovery asks again; ❌ nothing downstream may read this list as "the disk is empty".
    attached_volumes_in(&enumerate_mounts().unwrap_or_default())
}

/// The attached volumes in a `getfsstat(MNT_NOWAIT)` snapshot already taken
/// (`list_locations` shares one with favorites discovery, so a listing reads the
/// table once). Enriches only LOCAL mounts through blocking macOS APIs: a hung
/// network mount contributes its getfsstat-derived entry and never blocks the
/// others. See `DETAILS.md` § "Hung mounts".
pub(super) fn attached_volumes_in(mounts: &[MountEntry]) -> Vec<LocationInfo> {
    use objc2::rc::autoreleasepool;
    use objc2_foundation::{NSString, NSURL};

    // Drain autoreleased ObjC objects from the per-local-mount NSURL enrichment.
    // Called from spawn_blocking / helper threads that lack AppKit's pool.
    autoreleasepool(|_| {
        let discovered: Vec<LocationInfo> = mounts
            .iter()
            .filter_map(|mount| {
                build_attached_location(mount, |path| {
                    let url = NSURL::fileURLWithPath(&NSString::from_str(path));
                    LocalVolumeMeta {
                        name: get_volume_name(&url, path),
                        is_ejectable: is_volume_ejectable(&url, path),
                        icon: get_icon_for_path(path),
                        is_disk_image: disk_image::is_disk_image_mount(path),
                        uuid: get_volume_uuid(&url),
                    }
                })
            })
            .collect();

        let mut volumes = collapse_by_volume_id(discovered);
        // Sort alphabetically
        volumes.sort_by_key(|a| a.name.to_lowercase());
        volumes
    })
}

#[cfg(test)]
#[path = "mounts_tests.rs"]
mod tests;
