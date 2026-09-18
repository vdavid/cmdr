//! Which PHYSICAL disk a mount sits on, and which BSD units that disk carries.
//!
//! Cmdr's own eject works per physical disk: an APFS volume's whole disk is the
//! synthesized container, not the hardware, so ejecting by volume alone can power a
//! disk down while a sibling container stays mounted. Resolution walks IOKit's
//! `IOService` plane from the volume's own media up to the topmost WHOLE media,
//! which IS the hardware, and back down for every whole media on it (each
//! synthesized APFS container). Those units are what
//! `volumes::disk_units::mounted_volumes_on` turns into the disk's mounted volumes.
//!
//! ❌ No filesystem access and no path parsing: the mount table names the volume's
//! BSD node, IOKit answers in typed properties (`kIOMediaWholeKey`, `kIOBSDUnitKey`),
//! and the disk's identity is its registry entry ID.

use std::ffi::CString;

use objc2_core_foundation::{CFBoolean, CFDictionary, CFNumber, CFRetained, CFString, CFType};
use objc2_disk_arbitration::DASession;
use objc2_io_kit::{
    IOBSDNameMatching, IOIteratorNext, IOObjectRelease, IOObjectRetain, IORegistryEntryCreateCFProperty,
    IORegistryEntryCreateIterator, IORegistryEntryGetParentEntry, IORegistryEntryGetRegistryEntryID,
    IOServiceGetMatchingService, io_object_t, io_registry_entry_t, kIORegistryIterateRecursively, kIOServicePlane,
};

use super::in_flight::DiskKey;
use crate::volumes::disk_units::{self, MountedVolume};

/// How far up the `IOService` plane a media's ancestors are walked before the walk
/// gives up. An APFS volume reaches its hardware in four (volume → container →
/// container scheme → store partition → physical whole); the bound is only so a
/// registry that answers strangely can't spin.
const MAX_ANCESTORS: usize = 32;

/// `kIOMediaWholeKey` (`IOKit/storage/IOMedia.h`): whether a media is a whole disk
/// rather than one of its partitions. Spelled out because `objc2-io-kit`'s generated
/// surface stops at `IOKitLib` and doesn't carry the storage-family keys.
const MEDIA_WHOLE_KEY: &str = "Whole";

/// `kIOBSDUnitKey` (`IOKit/IOBSD.h`): the `N` of every `diskNsM` on the media.
const BSD_UNIT_KEY: &str = "BSD Unit";

/// The physical disk a volume sits on.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct DiskTarget {
    /// The disk itself, by its whole media's registry entry ID.
    pub(super) key: DiskKey,
    /// The BSD unit of the physical whole disk, plus one per synthesized container on
    /// it. Every mounted volume of the disk has one of these as its media's unit.
    pub(super) units: Vec<u32>,
}

/// What a mount path resolved to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Resolution {
    /// The path has left the mount table: its eject's goal is already met.
    Gone,
    /// It's mounted, but no physical disk backs it (a macFUSE mount, a share). Today's
    /// per-volume teardown is the honest answer.
    NoDisk,
    /// The physical disk under it.
    Disk(DiskTarget),
}

/// The physical disk under `mount_path`.
///
/// Reads the non-blocking mount table for the volume's BSD node, then IOKit. ❌ No
/// `statfs` and no `realpath`: `DADiskCreateFromVolumePath` makes both, so a disk
/// image backed by a file on a hung share would block resolution for good.
pub(super) fn resolve(mount_path: &str) -> Resolution {
    let Some(bsd_name) = disk_units::bsd_name_at(std::path::Path::new(mount_path)) else {
        return if crate::volumes::is_mount_point(mount_path) == Some(false) {
            Resolution::Gone
        } else {
            Resolution::NoDisk
        };
    };
    let Some(media) = media_for(&bsd_name) else {
        return Resolution::NoDisk;
    };
    let physical = topmost_whole(media);
    // `media` came from `IOServiceGetMatchingService`, which hands the caller the
    // release; `topmost_whole` retains whatever it keeps.
    IOObjectRelease(media);
    let Some(physical) = physical else {
        return Resolution::NoDisk;
    };
    let target = entry_id(physical).map(|key| DiskTarget {
        key: DiskKey(key),
        units: units_on(physical),
    });
    // `physical` is the object `topmost_whole` handed over, and nothing holds it now.
    IOObjectRelease(physical);
    match target {
        Some(target) => Resolution::Disk(target),
        None => Resolution::NoDisk,
    }
}

/// What a read of a disk's mounted volumes found.
///
/// ❗ "Couldn't read" is its own answer, ❌ never an empty list. A silent
/// DiskArbitration read as "nothing is mounted on this disk any more" would let an
/// eject report success over a drive still powered on, and let a flight stop only the
/// volume that was clicked and then unmount the disk under a sibling's live watcher —
/// the FSKit wedge the pre-stop exists to avoid. Every caller fails closed on it.
#[derive(Debug)]
pub(super) enum DiskMounts {
    /// What's mounted on the disk, which may legitimately be nothing.
    Read(Vec<MountedVolume>),
    /// DiskArbitration wouldn't answer, so nobody knows what's on the disk.
    Unreadable,
}

/// Every volume mounted on `units` right now, from the mount table plus one
/// DiskArbitration lookup per device-backed mount.
///
/// Its own session each call: a `DASession` isn't `Send`, and a flight's future
/// crosses `await` points. Creating one is a local object allocation, and the lookups
/// are the same MIG calls the approver's group read makes.
///
/// ❗ EITHER half of the read can fail, and both answer `Unreadable`: the DiskArbitration
/// session, and the kernel mount table the units are matched against.
pub(super) fn mounted_volumes_on_disk(units: &[u32]) -> DiskMounts {
    // SAFETY: `DASessionCreate` with the default allocator answers under the Create
    // rule, which `CFRetained` balances; NULL means the daemon couldn't be reached.
    let Some(session) = (unsafe { DASession::new(None) }) else {
        log::warn!(target: "eject", "DiskArbitration wouldn't open a session, so what's mounted on the disk is unknown");
        return DiskMounts::Unreadable;
    };
    let Some(mounted) = disk_units::mounted_volumes_on(&session, units) else {
        log::warn!(target: "eject", "The kernel mount table wouldn't answer, so what's mounted on the disk is unknown");
        return DiskMounts::Unreadable;
    };
    DiskMounts::Read(mounted)
}

/// The IOKit media object for the BSD node `bsd_name`, `None` when IOKit doesn't know
/// it. The caller releases it.
fn media_for(bsd_name: &str) -> Option<io_object_t> {
    let name = CString::new(bsd_name).ok()?;
    // SAFETY: `name` points at a live NUL-terminated C string that outlives the call,
    // and the dictionary it answers is handed straight to `IOServiceGetMatchingService`,
    // which consumes the one reference it holds.
    let matching = unsafe { IOBSDNameMatching(main_port(), 0, name.as_ptr()) }?;
    // SAFETY: a `CFMutableDictionary` IS a `CFDictionary`; the lookup only reads it.
    let matching = unsafe { CFRetained::cast_unchecked::<CFDictionary>(matching) };
    // SAFETY: the dictionary is live and its reference is consumed by the call.
    let media = unsafe { IOServiceGetMatchingService(main_port(), Some(matching)) };
    (media != 0).then_some(media)
}

/// The default IOKit main port.
///
/// ❗ The literal 0, ❌ never `kIOMainPortDefault`: that global arrived in macOS 12, dyld binds it
/// before `main`, and v0.46.0 died on the 10.15 floor with `Symbol not found: _kIOMainPortDefault`
/// before any of our code ran. `IOKitLib.h` calls the constant "a synonym for NULL", and NULL is
/// what every IOKit entry point documents as "use the default", so 0 says the same thing on every
/// version (`sysinfo` and `licensing::device_id` take the same route). Enforced by
/// `desktop-macos-symbol-floor`.
fn main_port() -> libc::mach_port_t {
    0
}

/// The topmost WHOLE media at or above `media`: the hardware, rather than a
/// synthesized APFS container. The caller releases it.
///
/// An APFS volume's ancestors are its container (whole, synthesized), the container
/// scheme, the physical store partition (not whole), and the physical disk (whole),
/// so the LAST whole media the walk sees is the one to eject.
fn topmost_whole(media: io_object_t) -> Option<io_object_t> {
    let mut whole = is_whole_media(media).then(|| retained(media)).flatten();
    let mut entry = retained(media)?;
    for _ in 0..MAX_ANCESTORS {
        let mut parent: io_registry_entry_t = 0;
        // SAFETY: `entry` is a live registry entry, the plane name is IOKit's own
        // NUL-terminated constant, and `parent` is a valid out pointer.
        let found = unsafe { IORegistryEntryGetParentEntry(entry, plane(), &raw mut parent) };
        IOObjectRelease(entry);
        if found != 0 || parent == 0 {
            return whole;
        }
        if is_whole_media(parent) {
            if let Some(previous) = whole.replace(parent) {
                // A lower whole media the walk has moved past, retained here.
                IOObjectRelease(previous);
            }
            let Some(next) = retained(parent) else { return whole };
            entry = next;
        } else {
            entry = parent;
        }
    }
    // allowed-pluralize-noun: a log line, and `MAX_ANCESTORS` is a compile-time bound well above one
    log::warn!(target: "eject", "An IOKit media has more than {MAX_ANCESTORS} ancestors; taking the highest whole disk found so far");
    whole
}

/// Every whole media's BSD unit at or below `physical`: the disk itself, plus each
/// synthesized container on it.
fn units_on(physical: io_registry_entry_t) -> Vec<u32> {
    let mut units: Vec<u32> = bsd_unit(physical).into_iter().collect();
    let mut iterator: io_object_t = 0;
    // SAFETY: `physical` is a live registry entry, the plane name is IOKit's own
    // NUL-terminated constant, and `iterator` is a valid out pointer. The iterator is
    // released below.
    let created =
        unsafe { IORegistryEntryCreateIterator(physical, plane(), kIORegistryIterateRecursively, &raw mut iterator) };
    if created != 0 || iterator == 0 {
        return units;
    }
    loop {
        // Each entry the iterator answers is the caller's to release.
        let child = IOIteratorNext(iterator);
        if child == 0 {
            break;
        }
        if is_whole_media(child)
            && let Some(unit) = bsd_unit(child)
            && !units.contains(&unit)
        {
            units.push(unit);
        }
        IOObjectRelease(child);
    }
    IOObjectRelease(iterator);
    units
}

/// The plane every walk here runs in.
fn plane() -> *mut [std::ffi::c_char; 128] {
    kIOServicePlane.as_ptr().cast_mut().cast()
}

/// Retains `entry` for one more owner, `None` when IOKit wouldn't.
fn retained(entry: io_object_t) -> Option<io_object_t> {
    // `IOObjectRetain` adds the reference the caller releases.
    let retained = IOObjectRetain(entry);
    (retained == 0).then_some(entry)
}

/// Whether the entry is a media that IS a whole disk, from IOKit's own `Whole`
/// property. ❌ Never from its class name or its BSD name's shape.
fn is_whole_media(entry: io_registry_entry_t) -> bool {
    property(entry, MEDIA_WHOLE_KEY)
        .and_then(|value| value.downcast::<CFBoolean>().ok())
        .is_some_and(|whole| whole.as_bool())
}

/// The entry's BSD unit, `None` for one that isn't a media.
fn bsd_unit(entry: io_registry_entry_t) -> Option<u32> {
    let value = property(entry, BSD_UNIT_KEY)?;
    u32::try_from(value.downcast::<CFNumber>().ok()?.as_i64()?).ok()
}

/// One registry property, `None` when the entry doesn't carry it.
fn property(entry: io_registry_entry_t, key: &str) -> Option<CFRetained<CFType>> {
    let key = CFString::from_str(key);
    // SAFETY: `entry` is a live registry entry and `key` a live `CFString`; the value
    // comes back under the Create rule, which `CFRetained` balances.
    unsafe { IORegistryEntryCreateCFProperty(entry, Some(&key), None, 0) }
}

/// The disk's identity: its whole media's registry entry ID.
fn entry_id(entry: io_registry_entry_t) -> Option<u64> {
    let mut id: u64 = 0;
    // SAFETY: `entry` is a live registry entry and `id` a valid out pointer.
    let read = unsafe { IORegistryEntryGetRegistryEntryID(entry, &raw mut id) };
    (read == 0).then_some(id)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_path_that_isnt_a_mount_at_all_has_no_disk_to_resolve() {
        // The eject already answered `Ok` for a root that left the table; reaching here
        // with one is a race, and it must not read as a disk.
        let missing = "/Volumes/cmdr-test-never-mounted-9f3a";
        assert_eq!(resolve(missing), Resolution::Gone);
    }

    #[test]
    fn the_boot_volume_resolves_to_a_physical_disk_carrying_its_container() {
        // `/` is an APFS volume: its whole disk is the synthesized container, and the
        // walk has to reach past that to the hardware, whose units include both.
        let Resolution::Disk(target) = resolve("/") else {
            panic!("the boot volume sits on a physical disk");
        };
        assert!(target.key.0 != 0, "the disk carries a registry entry ID");
        assert!(
            target.units.len() >= 2,
            "the physical disk and its synthesized APFS container are both units, got {:?}",
            target.units
        );

        // And the resolution is what finds the boot volume among the disk's mounts.
        let DiskMounts::Read(mounted) = mounted_volumes_on_disk(&target.units) else {
            panic!("DiskArbitration answers for the boot disk");
        };
        assert!(
            mounted.iter().any(|volume| volume.path == std::path::Path::new("/")),
            "the disk's mounted volumes include the boot volume, got {mounted:?}"
        );
    }
}
