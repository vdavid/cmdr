//! Which volumes sit on which whole disk, and how to read a DiskArbitration description.
//!
//! An unmount request for a whole disk reaches every volume on it as its own ask, linked by BSD
//! UNIT (`diskarbitrationd/DAQueue.c:974`), so the first ask has to let go of the whole unit's
//! group. Mapping a mount to its unit takes the non-blocking mount table plus one DiskArbitration
//! lookup per device-backed mount: ❌ no filesystem access, so a hung mount can't stall it.

use std::path::{Path, PathBuf};

use super::mounts::{MountSource, mount_sources};

/// A mounted volume of a whole disk.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MountedVolume {
    /// Its BSD node, `diskNsM`.
    pub(crate) bsd_name: String,
    /// The BSD unit of its whole disk: the `N` of every `diskNsM` on it.
    pub(crate) whole_unit: u32,
    pub(crate) volume_uuid: Option<String>,
    pub(crate) path: PathBuf,
}

/// What DiskArbitration says about one BSD node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NodeFacts {
    pub(crate) whole_unit: u32,
    pub(crate) volume_uuid: Option<String>,
}

/// The BSD node a mount source names: `None` for a source that isn't a device node (an SMB share's
/// `//user@host/share`, `devfs`, an automounter `map` entry).
fn bsd_name_of(mount_from: &str) -> Option<&str> {
    mount_from
        .strip_prefix("/dev/")
        .filter(|node| !node.is_empty() && !node.contains('/'))
}

/// Every mounted volume of the whole disks `whole_units`, from `mounts` and `facts` (asked once per
/// device-backed mount). Pure.
fn volumes_on(
    mounts: &[MountSource],
    whole_units: &[u32],
    mut facts: impl FnMut(&str) -> Option<NodeFacts>,
) -> Vec<MountedVolume> {
    mounts
        .iter()
        .filter_map(|mount| {
            let bsd_name = bsd_name_of(&mount.mount_from)?;
            let facts = facts(bsd_name)?;
            whole_units.contains(&facts.whole_unit).then(|| MountedVolume {
                bsd_name: bsd_name.to_string(),
                whole_unit: facts.whole_unit,
                volume_uuid: facts.volume_uuid,
                path: mount.mount_point.clone(),
            })
        })
        .collect()
}

/// Every mounted volume of `whole_units` from a mount-table read, `facts` asked once per
/// device-backed mount.
///
/// ❗ **`None` in is `None` out**: a table nobody could read is ❌ never an empty disk. Folding the
/// two is the collapse this subsystem has now shipped three times (a failed listing read as a
/// deleted row, a timed-out probe read as an absent file, a silent DiskArbitration read as an empty
/// disk), and here it would let a whole-disk unmount past a sibling's live FSEvents watcher. Pure,
/// so both arms are pinned without a mount table.
fn volumes_from_read(
    mounts: Option<Vec<MountSource>>,
    whole_units: &[u32],
    facts: impl FnMut(&str) -> Option<NodeFacts>,
) -> Option<Vec<MountedVolume>> {
    Some(volumes_on(&mounts?, whole_units, facts))
}

/// The BSD node mounted exactly at `path`, `None` when nothing is, when what is isn't device-backed
/// (a share, a macFUSE mount), or when the mount table wouldn't answer. What an eject reads before
/// it asks IOKit which physical disk the volume sits on.
///
/// The three fold together safely HERE and only here: `disk_target::resolve` answers `NoDisk` for
/// all of them (its `Gone` arm needs `is_mount_point` to say `Some(false)`), which tears down the
/// one volume rather than a disk nobody could describe.
pub(crate) fn bsd_name_at(path: &Path) -> Option<String> {
    let mounts = mount_sources()?;
    let mount = mounts.iter().find(|mount| mount.mount_point == path)?;
    bsd_name_of(&mount.mount_from).map(ToString::to_string)
}

/// Whether `bsd_name` is still mounted at `path`, from the non-blocking mount table. What a resume
/// asks before it starts an index again.
///
/// A table that wouldn't answer is `false`: a resume deliberately fails CLOSED, since not starting
/// an index costs a rescan later and starting one on a drive that's leaving is the wedge.
pub(crate) fn is_volume_mounted_at(bsd_name: &str, path: &Path) -> bool {
    mount_sources().is_some_and(|mounts| {
        mounts
            .iter()
            .any(|mount| mount.mount_point == path && bsd_name_of(&mount.mount_from) == Some(bsd_name))
    })
}

#[cfg(target_os = "macos")]
mod macos {
    use std::ffi::CString;
    use std::path::PathBuf;
    use std::ptr::NonNull;

    use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType, CFURL, CFUUID};
    use objc2_disk_arbitration::{
        DADisk, DASession, kDADiskDescriptionMediaBSDUnitKey, kDADiskDescriptionMediaWholeKey,
        kDADiskDescriptionVolumePathKey, kDADiskDescriptionVolumeUUIDKey,
    };

    use super::{MountedVolume, NodeFacts, mount_sources, volumes_from_read};

    /// A disk description, with the keys this module reads typed.
    pub(crate) type Description = CFRetained<CFDictionary<CFString, CFType>>;

    /// Every mounted volume of the whole disks `whole_units`, asking `session` what disk each
    /// device-backed mount belongs to. One MIG call to `diskarbitrationd` per such mount and ❌ no
    /// filesystem access, so it's safe on the approver's queue; still, it runs only when an ask
    /// needs a group.
    ///
    /// ❗ `None` when the mount table wouldn't answer, ❌ never an empty disk. Both callers fail
    /// closed on it: the approver dissents, and the eject reads `DiskMounts::Unreadable`.
    pub(crate) fn mounted_volumes_on(session: &DASession, whole_units: &[u32]) -> Option<Vec<MountedVolume>> {
        volumes_from_read(mount_sources(), whole_units, |bsd_name| node_facts(session, bsd_name))
    }

    /// What DiskArbitration says about the BSD node `bsd_name`, `None` when it doesn't know it.
    fn node_facts(session: &DASession, bsd_name: &str) -> Option<NodeFacts> {
        let name = CString::new(bsd_name).ok()?;
        let name = NonNull::new(name.as_ptr().cast_mut())?;
        // SAFETY: `name` points at a live NUL-terminated C string that outlives the call, and
        // `session` is live. The disk comes back under the Create rule, which `CFRetained` balances.
        let disk = unsafe { DADisk::from_bsd_name(None, session, name) }?;
        let description = description(&disk)?;
        Some(NodeFacts {
            whole_unit: whole_unit(&description)?,
            volume_uuid: volume_uuid(&description),
        })
    }

    /// A disk's description. A disk from a callback carries a FROZEN copy (nothing refreshes it), so
    /// ❌ never read presence from it: that comes from the mount table.
    pub(crate) fn description(disk: &DADisk) -> Option<Description> {
        // SAFETY: `disk` is live for the call, and `DADiskCopyDescription` answers under the Create
        // rule, which `CFRetained` balances.
        let description = unsafe { disk.description() }?;
        // SAFETY: a disk description is a `CFDictionary` whose keys are the `CFString` constants
        // DiskArbitration publishes and whose values are CoreFoundation objects, which is what this
        // narrows the untyped dictionary to. Each read below re-checks the value's own type.
        Some(unsafe { CFRetained::cast_unchecked(description) })
    }

    /// A disk's BSD node, `diskNsM`.
    pub(crate) fn bsd_name(disk: &DADisk) -> Option<String> {
        // SAFETY: `disk` is live for the call, and `DADiskGetBSDName` answers a NUL-terminated
        // string owned by the disk, read before this borrow ends.
        let name = unsafe { disk.bsd_name() };
        if name.is_null() {
            return None;
        }
        // SAFETY: the pointer is non-null and NUL-terminated, and `disk` outlives the read.
        Some(unsafe { std::ffi::CStr::from_ptr(name) }.to_string_lossy().into_owned())
    }

    /// The BSD unit of the disk's whole disk.
    pub(crate) fn whole_unit(description: &Description) -> Option<u32> {
        // SAFETY: the key is an `extern "C"` `&'static CFString` DiskArbitration constant.
        let value = description.get(unsafe { kDADiskDescriptionMediaBSDUnitKey })?;
        u32::try_from(value.downcast::<CFNumber>().ok()?.as_i64()?).ok()
    }

    /// Whether the disk IS a whole disk, rather than one of its volumes.
    pub(crate) fn is_whole(description: &Description) -> bool {
        // SAFETY: the key is an `extern "C"` `&'static CFString` DiskArbitration constant.
        description
            .get(unsafe { kDADiskDescriptionMediaWholeKey })
            .and_then(|value| value.downcast::<CFBoolean>().ok())
            .is_some_and(|whole| whole.as_bool())
    }

    /// The volume's UUID, `None` for a disk with no mountable volume.
    pub(crate) fn volume_uuid(description: &Description) -> Option<String> {
        // SAFETY: the key is an `extern "C"` `&'static CFString` DiskArbitration constant.
        let value = description.get(unsafe { kDADiskDescriptionVolumeUUIDKey })?;
        let uuid = value.downcast::<CFUUID>().ok()?;
        Some(CFUUID::new_string(None, Some(&uuid))?.to_string())
    }

    /// Where the volume is mounted, `None` once it isn't.
    pub(crate) fn volume_path(description: &Description) -> Option<PathBuf> {
        // SAFETY: the key is an `extern "C"` `&'static CFString` DiskArbitration constant.
        let value = description.get(unsafe { kDADiskDescriptionVolumePathKey })?;
        value.downcast::<CFURL>().ok()?.to_file_path()
    }
}

#[cfg(target_os = "macos")]
pub(crate) use macos::{bsd_name, description, is_whole, mounted_volumes_on, volume_path, volume_uuid, whole_unit};

#[cfg(test)]
mod tests {
    use super::*;

    fn mount(mount_point: &str, mount_from: &str) -> MountSource {
        MountSource {
            mount_point: PathBuf::from(mount_point),
            mount_from: mount_from.to_string(),
        }
    }

    fn facts(whole_unit: u32) -> Option<NodeFacts> {
        Some(NodeFacts {
            whole_unit,
            volume_uuid: Some(format!("uuid-{whole_unit}")),
        })
    }

    #[test]
    fn only_device_backed_mounts_name_a_bsd_node() {
        assert_eq!(bsd_name_of("/dev/disk5s1"), Some("disk5s1"));
        assert_eq!(bsd_name_of("//david@nas/share"), None);
        assert_eq!(bsd_name_of("devfs"), None);
        assert_eq!(bsd_name_of("map auto_home"), None);
        assert_eq!(bsd_name_of("/dev/"), None);
    }

    #[test]
    fn a_whole_disks_group_is_every_mounted_volume_that_shares_its_unit() {
        let mounts = [
            mount("/Volumes/A", "/dev/disk7s2"),
            mount("/Volumes/B", "/dev/disk7s3"),
            mount("/Volumes/Elsewhere", "/dev/disk9s1"),
            mount("/Volumes/naspi", "//david@nas/naspi"),
        ];

        let group = volumes_on(&mounts, &[7], |bsd_name| match bsd_name {
            "disk7s2" | "disk7s3" => facts(7),
            "disk9s1" => facts(9),
            _ => None,
        });

        assert_eq!(
            group.iter().map(|volume| volume.path.as_path()).collect::<Vec<_>>(),
            [Path::new("/Volumes/A"), Path::new("/Volumes/B")],
            "the sibling partition is in the group, the other disk and the share are not"
        );
        assert_eq!(group[0].bsd_name, "disk7s2");
        assert_eq!(group[0].volume_uuid.as_deref(), Some("uuid-7"));
    }

    #[test]
    fn a_node_diskarbitration_doesnt_know_is_left_out() {
        let mounts = [mount("/Volumes/Gone", "/dev/disk7s2")];
        assert!(volumes_on(&mounts, &[7], |_| None).is_empty());
    }

    #[test]
    fn a_mount_table_nobody_could_read_is_not_an_empty_disk() {
        // The regression fence for the collapse this whole area exists to prevent. An unreadable
        // table must stay unreadable all the way up, so the eject can call it `DiskMounts::Unreadable`
        // and the approver can dissent; ❌ the moment it becomes `Some([])` both read "nothing is on
        // this disk" and a sibling gets unmounted under its live watcher.
        assert_eq!(volumes_from_read(None, &[7], |_| facts(7)), None);

        // A table that DID answer and holds nothing for this disk is a different, legitimate answer.
        assert_eq!(
            volumes_from_read(Some(Vec::new()), &[7], |_| facts(7)),
            Some(Vec::new())
        );
    }

    #[test]
    fn the_boot_volume_is_mounted_at_its_own_root() {
        // The real mount table: `/` is always there, and a made-up node never is.
        assert!(!is_volume_mounted_at("disk-that-cannot-exist", Path::new("/")));
    }

    #[test]
    fn a_mounted_path_names_its_bsd_node_and_an_unmounted_one_names_nothing() {
        let root = bsd_name_at(Path::new("/")).expect("the boot volume is device-backed");
        assert!(is_volume_mounted_at(&root, Path::new("/")), "and it's the node listed");
        assert_eq!(bsd_name_at(Path::new("/Volumes/cmdr-test-never-mounted-2c71")), None);
    }
}
