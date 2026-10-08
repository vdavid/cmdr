//! Which mounts the boot-disk index stops at, and that every gate asking
//! `should_exclude` stops there too.

use std::sync::Arc;

use super::*;
use crate::indexing::host::volumes::{self, FakeVolumeProvider, MountIdentity};
use crate::indexing::scanner::exclusions::excluded_by_boot_prefixes;
use crate::indexing::scanner::{ExclusionScope, should_exclude};

fn set_of(points: &[&str]) -> BootTreeMounts {
    BootTreeMounts::from_mount_table(points.iter().map(|p| (*p).to_string()), excluded_by_boot_prefixes)
}

fn paths(set: &BootTreeMounts) -> Vec<&str> {
    set.mounts.iter().map(|mount| mount.path.as_str()).collect()
}

/// A host whose mount table lists `points`, installed for the rest of the test.
fn install_mount_table(points: &[&str]) -> (volumes::TestProviderGuard, Arc<FakeVolumeProvider>) {
    let provider = FakeVolumeProvider::shared();
    for (i, point) in points.iter().enumerate() {
        provider.mount(*point, MountIdentity::from_raw(i as u64 + 1));
    }
    (volumes::install_for_test(provider.clone()), provider)
}

/// The boot disk itself, and every mount the prefix policy already keeps the boot
/// scan out of, stay out of the set: `/System/Volumes/Data` is where `/Users` lives,
/// so cutting it would empty `root`'s index.
#[test]
#[cfg(target_os = "macos")]
fn the_boot_disks_own_volumes_are_not_inside_its_tree() {
    let set = set_of(&[
        "/",
        "/System/Volumes/Data",
        "/System/Volumes/VM",
        "/System/Volumes/Data/home",
        "/Volumes/naspi",
        "/dev",
        "/Users/someone/pCloud Drive",
        "/Users/someone/Library/Developer/CoreDevice/DeviceFS",
    ]);

    assert_eq!(
        paths(&set),
        vec![
            "/Users/someone/Library/Developer/CoreDevice/DeviceFS",
            "/Users/someone/pCloud Drive",
        ]
    );
}

/// The mount table can spell a home-folder mount through the Data volume. The set
/// holds it firmlink-normalized, which is the space every gate asks in, and keeps
/// the table's spelling for the registry lookup routing makes.
#[test]
#[cfg(target_os = "macos")]
fn a_mount_listed_through_the_data_volume_is_normalized() {
    let set = set_of(&["/System/Volumes/Data/Users/someone/mnt/nfs"]);

    let mount = set.covering("/Users/someone/mnt/nfs/docs").expect("covered");
    assert_eq!(mount.path, "/Users/someone/mnt/nfs");
    assert_eq!(mount.raw, "/System/Volumes/Data/Users/someone/mnt/nfs");
}

/// Component-aware, and the innermost mount wins, so a disk image mounted inside an
/// sshfs mount answers for its own paths.
#[test]
fn covering_is_component_aware_and_innermost_first() {
    let set = set_of(&["/home/someone/mnt/outer", "/home/someone/mnt/outer/inner"]);

    assert_eq!(
        set.covering("/home/someone/mnt/outer/inner/a.txt")
            .map(|m| m.path.as_str()),
        Some("/home/someone/mnt/outer/inner")
    );
    assert_eq!(
        set.covering("/home/someone/mnt/outer/b.txt").map(|m| m.path.as_str()),
        Some("/home/someone/mnt/outer")
    );
    assert_eq!(
        set.covering("/home/someone/mnt/outer-sibling"),
        None,
        "`/a/bc` is not under `/a/b`"
    );
    assert_eq!(
        set.covering("/home/someone/mnt"),
        None,
        "the mount point's parent is the boot disk's"
    );
}

/// ❗ The rule itself, at the one gate every walk, reconcile, verification, and
/// enrichment path asks: a filesystem mounted inside the boot tree, and everything
/// under it, is not the boot disk's.
#[test]
fn should_exclude_stops_the_boot_disk_at_a_mount_inside_its_tree() {
    let _serialized = crate::indexing::handle::test_lock();
    let (_installed, _provider) = install_mount_table(&["/", "/Users/statustest/mnt/share"]);
    let boot = ExclusionScope::boot_disk();

    assert!(
        should_exclude("/Users/statustest/mnt/share", &boot),
        "the mount point gets no row"
    );
    assert!(should_exclude("/Users/statustest/mnt/share/docs/a.txt", &boot));
    assert!(
        !should_exclude("/Users/statustest/mnt", &boot),
        "its parent is still the boot disk's"
    );
    assert!(!should_exclude("/Users/statustest/mnt/share-notes", &boot));
    assert!(
        !should_exclude(
            "/Users/statustest/mnt/share/docs",
            &ExclusionScope::mount_rooted("/Users/statustest/mnt/share")
        ),
        "the mount's OWN index walks all of it"
    );
}

/// A mount that appears is picked up without anyone calling anything: the next ask
/// after the table changes reads it (the fake bumps the table generation; the real
/// host is re-read once a second).
#[test]
fn a_new_mount_is_seen_by_the_next_ask() {
    let _serialized = crate::indexing::handle::test_lock();
    let (_installed, provider) = install_mount_table(&["/"]);
    let boot = ExclusionScope::boot_disk();
    assert!(!should_exclude("/Users/statustest/mnt/late/a.txt", &boot));

    provider.mount("/Users/statustest/mnt/late", MountIdentity::from_raw(9));

    assert!(should_exclude("/Users/statustest/mnt/late/a.txt", &boot));
}

/// ❗ A table that won't answer is its own answer, ❌ never an empty one: the set it
/// had stands, or every mount's rows would flow back into `root` until it answered.
#[test]
fn an_unreadable_table_keeps_the_previous_set() {
    let _serialized = crate::indexing::handle::test_lock();
    let (_installed, provider) = install_mount_table(&["/", "/Users/statustest/mnt/share"]);
    let boot = ExclusionScope::boot_disk();
    assert!(should_exclude("/Users/statustest/mnt/share/a.txt", &boot));

    provider.mark_table_unreadable();

    assert!(should_exclude("/Users/statustest/mnt/share/a.txt", &boot));
}
