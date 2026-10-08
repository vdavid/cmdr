//! `move_root`'s cells: a drive renamed while it's mounted.

use std::path::Path;
use std::sync::Arc;

use super::RootMove;
use crate::file_system::volume::manager::VolumeManager;
use crate::file_system::volume::manager::test_support::RetiringVolume;
use crate::file_system::volume::{InMemoryVolume, Volume};

const ID: &str = "vol-uuid-1234";
const OLD: &str = "/Volumes/Old";
const NEW: &str = "/Volumes/New";

/// The headline case: the drive keeps its id and serves it from the new root, and
/// nothing finds it by the old one any more.
#[test]
fn a_renamed_drive_keeps_its_id_at_its_new_root() {
    let manager = VolumeManager::new();
    let (volume, is_retired) = RetiringVolume::at(OLD);
    manager.register(ID, volume as Arc<dyn Volume>);

    assert_eq!(
        manager.move_root(Path::new(OLD), Path::new(NEW)),
        RootMove::Moved { id: ID.to_string() }
    );

    let serving = manager.get(ID).expect("still registered under its id");
    assert_eq!(serving.root(), Path::new(NEW), "it serves from the new root");
    assert_eq!(manager.known_roots(ID), vec![Path::new(NEW).to_path_buf()]);
    assert!(manager.find_by_root(Path::new(OLD)).is_none(), "the old root is gone");
    assert_eq!(
        manager.mount_id_for_path("/Volumes/New/photos").as_deref(),
        Some(ID),
        "a path under the new root routes to the drive"
    );
    assert!(!is_retired(), "the same mount goes on, so nothing is retired");
}

/// A spare mount of the same filesystem moving leaves the active root alone.
#[test]
fn a_renamed_sibling_root_moves_in_the_set_only() {
    let manager = VolumeManager::new();
    let (volume, _) = RetiringVolume::at("/Volumes/Active");
    manager.register(ID, volume as Arc<dyn Volume>);
    manager.register(ID, Arc::new(InMemoryVolume::new("Spare").with_root(OLD)));

    assert_eq!(
        manager.move_root(Path::new(OLD), Path::new(NEW)),
        RootMove::SiblingMoved { id: ID.to_string() }
    );
    assert_eq!(
        manager.get(ID).expect("registered").root(),
        Path::new("/Volumes/Active"),
        "the active root stays"
    );
    assert_eq!(
        manager.find_by_root(Path::new(NEW)).map(|(id, _)| id).as_deref(),
        Some(ID),
        "the spare is findable at its new root"
    );
    assert!(manager.find_by_root(Path::new(OLD)).is_none());
}

/// A backend that can't follow stays where it was, and the root it's anchored to
/// is marked stale so nothing promotes back onto it.
#[test]
fn a_backend_that_cant_reroot_stays_anchored_to_the_old_root() {
    let manager = VolumeManager::new();
    manager.register(ID, Arc::new(InMemoryVolume::new("Fixed").with_root(OLD)));

    assert_eq!(
        manager.move_root(Path::new(OLD), Path::new(NEW)),
        RootMove::BackendCantReroot { id: ID.to_string() }
    );
    assert_eq!(manager.get(ID).expect("still registered").root(), Path::new(OLD));
}

/// A root nobody registered is nobody's to move.
#[test]
fn an_unknown_root_moves_nothing() {
    let manager = VolumeManager::new();
    assert_eq!(manager.move_root(Path::new(OLD), Path::new(NEW)), RootMove::Unknown);
}
