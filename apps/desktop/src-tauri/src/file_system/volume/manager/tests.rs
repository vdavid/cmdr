//! The registry proper: registering, replacing, and unregistering by id, the
//! default volume, identity conflicts, and concurrent access.

use super::super::InMemoryVolume;
use super::*;
use test_support::RetiringVolume;

#[test]
fn test_new_creates_empty_manager() {
    let manager = VolumeManager::new();
    assert_eq!(manager.count(), 0);
    assert!(manager.default_volume().is_none());
}

#[test]
fn test_register_and_get() {
    let manager = VolumeManager::new();
    let volume = Arc::new(InMemoryVolume::new("Test Volume"));

    manager.register("test", volume.clone());

    let retrieved = manager.get("test").unwrap();
    assert_eq!(retrieved.name(), "Test Volume");
}

#[test]
fn test_unregister() {
    let manager = VolumeManager::new();
    let volume = Arc::new(InMemoryVolume::new("Test Volume"));

    manager.register("test", volume);
    assert_eq!(manager.count(), 1);

    manager.unregister("test");
    assert_eq!(manager.count(), 0);
    assert!(manager.get("test").is_none());
}

#[test]
fn test_set_default() {
    let manager = VolumeManager::new();
    let volume = Arc::new(InMemoryVolume::new("Test Volume"));

    manager.register("test", volume);
    assert!(manager.set_default("test"));

    let default = manager.default_volume().unwrap();
    assert_eq!(default.name(), "Test Volume");
}

#[test]
fn test_set_default_nonexistent_returns_false() {
    let manager = VolumeManager::new();
    assert!(!manager.set_default("nonexistent"));
}

#[test]
fn test_unregister_clears_default() {
    let manager = VolumeManager::new();
    let volume = Arc::new(InMemoryVolume::new("Test Volume"));

    manager.register("test", volume);
    manager.set_default("test");
    assert!(manager.default_volume().is_some());

    manager.unregister("test");
    assert!(manager.default_volume().is_none());
}

#[test]
fn test_find_by_root_returns_registered_entry() {
    let manager = VolumeManager::new();
    let volume = Arc::new(InMemoryVolume::new("Test Volume"));
    manager.register("test-id", volume);

    let (id, v) = manager.find_by_root(Path::new("/")).expect("InMemoryVolume root is /");
    assert_eq!(id, "test-id");
    assert_eq!(v.name(), "Test Volume");
}

#[test]
fn test_find_by_root_returns_none_for_unknown_root() {
    let manager = VolumeManager::new();
    manager.register("test-id", Arc::new(InMemoryVolume::new("Test")));
    assert!(manager.find_by_root(Path::new("/nonexistent/path")).is_none());
}

#[test]
fn test_list_volumes() {
    let manager = VolumeManager::new();
    manager.register("vol1", Arc::new(InMemoryVolume::new("Volume One")));
    manager.register("vol2", Arc::new(InMemoryVolume::new("Volume Two")));

    let list = manager.list_volumes();
    assert_eq!(list.len(), 2);
    assert!(list.iter().any(|(id, name)| id == "vol1" && name == "Volume One"));
    assert!(list.iter().any(|(id, name)| id == "vol2" && name == "Volume Two"));
}

#[test]
fn test_register_if_absent_new_volume() {
    let manager = VolumeManager::new();
    let volume = Arc::new(InMemoryVolume::new("Test Volume"));

    assert!(manager.register_if_absent("test", volume));
    assert_eq!(manager.count(), 1);
    assert_eq!(manager.get("test").unwrap().name(), "Test Volume");
}

#[test]
fn test_register_if_absent_existing_volume_keeps_original() {
    let manager = VolumeManager::new();
    let original = Arc::new(InMemoryVolume::new("Original"));
    let replacement = Arc::new(InMemoryVolume::new("Replacement"));

    manager.register("test", original);
    assert!(!manager.register_if_absent("test", replacement));

    // Original should be kept
    assert_eq!(manager.get("test").unwrap().name(), "Original");
}

#[test]
fn re_registering_the_same_mount_is_not_an_identity_conflict() {
    // The SMB upgrade swaps an OS-mounted `LocalPosixVolume` for a direct
    // `SmbVolume` at the same root. Routine, and it must stay quiet.
    let existing: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("OS mount").with_root("/Volumes/naspi"));
    let incoming: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Direct SMB").with_root("/Volumes/naspi"));
    assert!(!is_identity_conflict(existing.root(), incoming.root()));
}

#[test]
fn a_root_nested_in_the_other_is_a_quiet_double_mount() {
    // macOS's update volume and its snapshot mount: one filesystem, mounted twice.
    let update = Path::new("/System/Volumes/Update");
    let snapshot = Path::new("/System/Volumes/Update/mnt1");
    assert_eq!(identity_conflict_level(update, snapshot), log::Level::Info);
    assert_eq!(identity_conflict_level(snapshot, update), log::Level::Info);
}

#[test]
fn sibling_roots_under_one_id_stay_a_warning() {
    // Could be a clone, so a human should see it. `/Volumes/naspi-1` shares a
    // string prefix with `/Volumes/naspi` but isn't inside it.
    assert_eq!(
        identity_conflict_level(Path::new("/Volumes/naspi"), Path::new("/Volumes/naspi-1")),
        log::Level::Warn
    );
}

#[test]
fn one_id_over_two_roots_is_an_identity_conflict() {
    // Two different disks under one ID would cross-wire their index, saved
    // paths, and operation routing. IDs are built so this can't happen from a
    // derivation; if it ever does (a cloned volume, a double mount), it has to
    // be loud rather than silent.
    let existing: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Disk A").with_root("/Volumes/A"));
    let incoming: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Disk B").with_root("/Volumes/B"));
    assert!(is_identity_conflict(existing.root(), incoming.root()));
}

#[test]
fn an_identity_conflict_keeps_the_incumbent_whatever_the_order() {
    // One share mounted at two paths registers twice under one ID. Letting
    // the last writer win made registration ORDER decide where the volume
    // was rooted, so a saved path under the first mount reached a backend
    // rooted at the second and every listing under it failed.
    for (first_root, second_root) in [
        ("/Volumes/naspi", "/Volumes/naspi-1"),
        ("/Volumes/naspi-1", "/Volumes/naspi"),
    ] {
        let manager = VolumeManager::new();
        manager.register(
            "smb-share",
            Arc::new(InMemoryVolume::new("First").with_root(first_root)),
        );
        manager.register(
            "smb-share",
            Arc::new(InMemoryVolume::new("Second").with_root(second_root)),
        );

        let registered = manager.get("smb-share").expect("registered above");
        assert_eq!(registered.name(), "First", "the incumbent keeps the ID");
        assert_eq!(registered.root(), Path::new(first_root));
    }
}

#[test]
fn a_second_mount_of_one_share_is_recorded_as_a_sibling_root() {
    // macOS mounts the same share twice and both mounts derive one ID. The
    // incumbent stays active (that's what keeps a saved `/Volumes/naspi/…`
    // path working), but the second root has to stay FINDABLE: the unmount
    // path looks a gone mount up by root, and a sibling it can't see is a
    // sibling it can't promote to.
    let manager = VolumeManager::new();
    manager.register(
        "smb-share",
        Arc::new(InMemoryVolume::new("First").with_root("/Volumes/naspi")),
    );
    manager.register(
        "smb-share",
        Arc::new(InMemoryVolume::new("Second").with_root("/Volumes/naspi-1")),
    );

    let active = manager.get("smb-share").expect("registered above");
    assert_eq!(active.root(), Path::new("/Volumes/naspi"), "the incumbent stays active");

    let (id, _) = manager
        .find_by_root(Path::new("/Volumes/naspi-1"))
        .expect("the sibling root is a known root of this volume");
    assert_eq!(id, "smb-share");
}

#[test]
fn replacing_the_volume_at_the_same_root_still_wins() {
    // The SMB upgrade swaps an OS-mounted `LocalPosixVolume` for a direct
    // `SmbVolume` at the same root, and the manual "Connect directly" and
    // reconnect paths do the same. Not an identity conflict, so it replaces.
    let manager = VolumeManager::new();
    manager.register(
        "smb-share",
        Arc::new(InMemoryVolume::new("OS mount").with_root("/Volumes/naspi")),
    );
    manager.register(
        "smb-share",
        Arc::new(InMemoryVolume::new("Direct SMB").with_root("/Volumes/naspi")),
    );

    assert_eq!(manager.get("smb-share").expect("registered above").name(), "Direct SMB");
}

#[test]
fn test_multiple_volumes() {
    let manager = VolumeManager::new();

    manager.register("root", Arc::new(InMemoryVolume::new("Macintosh HD")));
    manager.register("dropbox", Arc::new(InMemoryVolume::new("Dropbox")));
    manager.register("gdrive", Arc::new(InMemoryVolume::new("Google Drive")));

    assert_eq!(manager.count(), 3);

    manager.set_default("root");
    assert_eq!(manager.default_volume().unwrap().name(), "Macintosh HD");

    // Switch default
    manager.set_default("dropbox");
    assert_eq!(manager.default_volume().unwrap().name(), "Dropbox");
}

#[test]
fn test_concurrent_registration() {
    use std::thread;

    let manager = Arc::new(VolumeManager::new());
    let mut handles = vec![];

    // Spawn 10 threads that each register a volume
    for i in 0..10 {
        let manager_clone = Arc::clone(&manager);
        handles.push(thread::spawn(move || {
            let volume = Arc::new(InMemoryVolume::new(format!("Volume {}", i)));
            manager_clone.register(&format!("vol_{}", i), volume);
        }));
    }

    // Wait for all threads
    for handle in handles {
        handle.join().unwrap();
    }

    // All 10 should be registered
    assert_eq!(manager.count(), 10);
}

#[test]
fn test_concurrent_reads() {
    use std::thread;

    let manager = Arc::new(VolumeManager::new());

    // Pre-register volumes
    for i in 0..5 {
        manager.register(
            &format!("vol_{}", i),
            Arc::new(InMemoryVolume::new(format!("Volume {}", i))),
        );
    }
    manager.set_default("vol_0");

    let mut handles = vec![];

    // Spawn 20 threads that concurrently read
    for _ in 0..20 {
        let manager_clone = Arc::clone(&manager);
        handles.push(thread::spawn(move || {
            for _ in 0..100 {
                let _ = manager_clone.get("vol_0");
                let _ = manager_clone.default_volume();
                let _ = manager_clone.list_volumes();
                let _ = manager_clone.count();
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Should still have correct state
    assert_eq!(manager.count(), 5);
    assert_eq!(manager.default_volume().unwrap().name(), "Volume 0");
}

#[test]
fn test_concurrent_read_write() {
    use std::thread;

    let manager = Arc::new(VolumeManager::new());
    manager.register("permanent", Arc::new(InMemoryVolume::new("Permanent")));

    let mut handles = vec![];

    // Readers
    for _ in 0..5 {
        let manager_clone = Arc::clone(&manager);
        handles.push(thread::spawn(move || {
            for _ in 0..50 {
                let _ = manager_clone.get("permanent");
                let _ = manager_clone.list_volumes();
                thread::yield_now();
            }
        }));
    }

    // Writers
    for i in 0..5 {
        let manager_clone = Arc::clone(&manager);
        handles.push(thread::spawn(move || {
            for j in 0..10 {
                let vol_id = format!("temp_{}_{}", i, j);
                manager_clone.register(&vol_id, Arc::new(InMemoryVolume::new(&vol_id)));
                thread::yield_now();
                manager_clone.unregister(&vol_id);
            }
        }));
    }

    for handle in handles {
        handle.join().unwrap();
    }

    // Permanent volume should still exist
    assert!(manager.get("permanent").is_some());
}

// ── Retirement ──────────────────────────────────────────────
//
// A volume's background work (a watcher, a reconnect backoff loop) outlives
// any one call, so it has to keep asking whether the registry still serves
// the volume it was spawned for. Only the registry knows, so the registry
// writes the answer down. See `cmdr_fs::volume::Retirement`.

#[test]
fn unregistering_a_volume_retires_it() {
    let manager = VolumeManager::new();
    let (volume, is_retired) = RetiringVolume::at("/Volumes/naspi");
    manager.register("naspi", volume);
    assert!(!is_retired(), "a registered volume is live");

    manager.unregister("naspi");

    assert!(
        is_retired(),
        "nothing routes to this volume any more, so its background work must stand down"
    );
}

/// Replacing a volume is NOT removing it, and the difference decides whether
/// a healthy share keeps its watcher. The registry replaces an entry for two
/// reasons: an upgrade (a fresh volume over a fresh session, whose
/// predecessor is retired by the caller through `Volume::on_superseded`), and
/// a re-root onto a surviving mount (the SAME share, deliberately still
/// live). Retiring on every replace would stand the re-rooted share down.
#[test]
fn replacing_a_volume_at_its_own_root_retires_nobody() {
    let manager = VolumeManager::new();
    let (incumbent, incumbent_retired) = RetiringVolume::at("/Volumes/naspi");
    let (successor, successor_retired) = RetiringVolume::at("/Volumes/naspi");
    manager.register("naspi", incumbent);

    manager.register("naspi", successor);

    assert!(
        !incumbent_retired() && !successor_retired(),
        "who retires on a hand-over is the hand-over's business, not the registry's"
    );
}

/// A backend with nothing running between calls answers `None`, and the
/// registry has to cope rather than assume every volume keeps a flag.
#[test]
fn a_volume_that_keeps_no_flag_unregisters_fine() {
    let manager = VolumeManager::new();
    manager.register("test", Arc::new(InMemoryVolume::new("Test Volume")));

    manager.unregister("test");

    assert_eq!(manager.count(), 0);
}
