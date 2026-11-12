//! The kernel mount table, read without blocking on any mount: one
//! `getfsstat(MNT_NOWAIT)` snapshot, and the mount-point question asked of it.
//! `volumes/` builds the switcher's rows from the same snapshot.

use std::path::Path;

use super::statfs::statfs_string;

/// One entry from the kernel mount table, as returned by `getfsstat(MNT_NOWAIT)`.
///
/// Every field comes straight out of `statfs` with no follow-up syscall, so
/// building one never talks to the backing filesystem. That is the whole reason
/// discovery uses `getfsstat` instead of NSFileManager's volume enumeration: the
/// enumeration `getattrlist`s every mount, which blocks 30s–forever on a hung
/// network mount and froze the app at launch. See `apps/desktop/src-tauri/src/volumes/DETAILS.md` § "Hung mounts".
pub struct MountEntry {
    /// Mount point, e.g. `/Volumes/testshare` (`f_mntonname`).
    pub mount_point: String,
    /// Filesystem type, e.g. `apfs`, `exfat`, `smbfs` (`f_fstypename`).
    pub fs_type: String,
    /// Mount source, e.g. `//test_user_3@192.168.50.10/testshare` for SMB (`f_mntfromname`).
    pub mount_from: String,
    /// Whether the mount carries the `MNT_RDONLY` flag.
    pub is_read_only: bool,
    /// Whether the OS says this mount belongs in a file manager's sidebar, i.e.
    /// it does NOT carry `MNT_DONTBROWSE`. Finder's own rule, and the one that
    /// separates real drives from the plumbing (`devfs`, `/System/Volumes/*`,
    /// `/Volumes/Recovery`, autofs triggers) without naming any of them.
    pub is_browsable: bool,
    /// The mounted filesystem's identity (`f_fsid`, its two words packed high then
    /// low). Renaming a mounted volume moves the mount point and keeps this.
    pub fsid: u64,
    /// Who mounted it (`f_owner`).
    pub mounted_by: MountedBy,
}

/// Who mounted a filesystem: the mount-table row's `f_owner`, which `mount` prints
/// as "mounted by `<name>`" for every row whose owner isn't root.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MountedBy {
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
    pub(crate) fn from_owner(owner: libc::uid_t, this_user: libc::uid_t) -> Self {
        if owner == this_user {
            Self::ThisUser
        } else if owner == 0 {
            Self::Root
        } else {
            Self::AnotherUser
        }
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
/// `volumes::has_mount_identity`, `volumes::mount_sources`) or fold it deliberately.
pub fn enumerate_mounts() -> Option<Vec<MountEntry>> {
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
                mount_point: statfs_string(&s.f_mntonname),
                fs_type: statfs_string(&s.f_fstypename),
                mount_from: statfs_string(&s.f_mntfromname),
                is_read_only: (s.f_flags & libc::MNT_RDONLY as u32) != 0,
                is_browsable: (s.f_flags & libc::MNT_DONTBROWSE as u32) == 0,
                fsid: packed_fsid(s),
                mounted_by: MountedBy::from_owner(s.f_owner, this_user),
            })
            .collect(),
    )
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
pub fn is_mount_point(path: &str) -> Option<bool> {
    let mounts = enumerate_mounts()?;
    let path = Path::new(path);
    Some(mounts.iter().any(|m| Path::new(&m.mount_point) == path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_owner_is_this_user_root_or_someone_else() {
        assert_eq!(MountedBy::from_owner(501, 501), MountedBy::ThisUser);
        assert_eq!(MountedBy::from_owner(0, 501), MountedBy::Root);
        assert_eq!(MountedBy::from_owner(502, 501), MountedBy::AnotherUser);
        // Cmdr run as root mounts as root, and that's still its own.
        assert_eq!(MountedBy::from_owner(0, 0), MountedBy::ThisUser);
    }

    #[test]
    fn enumerate_mounts_finds_the_boot_volume() {
        // getfsstat should always return at least the root mount on a live system,
        // and it must never block (this test would hang if it did).
        let mounts = enumerate_mounts().expect("getfsstat answers on a live system");
        assert!(!mounts.is_empty(), "getfsstat returned no mounts");
        assert!(mounts.iter().any(|m| m.mount_point == "/"), "root mount missing");
    }

    #[test]
    fn is_mount_point_answers_from_the_mount_table() {
        assert_eq!(is_mount_point("/"), Some(true), "the boot volume is a mount point");
        // A folder ON a mount is not one: an eject can't mistake it for a live mount.
        assert_eq!(is_mount_point("/usr/bin"), Some(false));
    }
}
