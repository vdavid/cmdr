//! `replace_root_in_place`'s cells: a saved place whose root was edited while
//! it was connected.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use super::RootReplacement;
use crate::file_system::volume::manager::VolumeManager;
use crate::file_system::volume::manager::test_support::RetiringVolume;
use crate::file_system::volume::{InMemoryVolume, Volume};
use crate::ignore_poison::IgnorePoison;

const ID: &str = "sftp-nas.local-22-ada";
const NARROW: &str = "sftp://ada@nas.local:22/srv/data/tmp";
const WIDE: &str = "sftp://ada@nas.local:22/srv/data";

/// A registry serving `ID` at `root`, the volume's retirement reader, and the
/// volume itself so a cell can build its successor the way the wiring does.
fn connected_at(root: &str) -> (VolumeManager, Arc<RetiringVolume>, impl Fn() -> bool) {
    let manager = VolumeManager::new();
    let (volume, is_retired) = RetiringVolume::at(root);
    manager.register(ID, serving(&volume));
    (manager, volume, is_retired)
}

/// The handle the registry holds for `volume`, which is the one a caller names
/// as the instance it checked against.
fn serving(volume: &Arc<RetiringVolume>) -> Arc<dyn Volume> {
    Arc::clone(volume) as Arc<dyn Volume>
}

/// The successor a live edit builds: another instance sharing the incumbent's
/// state, at another root.
fn successor(volume: &RetiringVolume, root: &str) -> Arc<dyn Volume> {
    volume
        .rerooted(Path::new(root))
        .expect("a RetiringVolume always re-roots")
}

/// The case plain `register` refuses: a different root under a taken id. Here
/// the new root has to WIN, because a person asked for it.
#[test]
fn a_replaced_root_is_the_active_one() {
    let (manager, volume, _) = connected_at(NARROW);

    let outcome = manager.replace_root_in_place(ID, &serving(&volume), successor(&volume, WIDE));

    let RootReplacement::Replaced { previous } = outcome else {
        panic!("the id is registered, so the replace must happen");
    };
    assert_eq!(
        previous.root(),
        Path::new(NARROW),
        "it hands back the instance it replaced"
    );
    assert_eq!(manager.get(ID).expect("still registered").root(), Path::new(WIDE));
    assert_eq!(manager.known_roots(ID), vec![PathBuf::from(WIDE)]);
}

/// ❗ The old root leaves the root set, so a lookup by root or by path stops
/// matching it. Kept as a fallback, a path under the old root would keep
/// resolving to this volume after the place moved away from it.
#[test]
fn the_old_root_stops_resolving_after_a_replace() {
    let (manager, volume, _) = connected_at(WIDE);

    let _ = manager.replace_root_in_place(ID, &serving(&volume), successor(&volume, NARROW));

    assert!(
        manager.find_by_root(Path::new(WIDE)).is_none(),
        "the old root is no longer a root of this volume"
    );
    assert_eq!(
        manager.find_by_root(Path::new(NARROW)).map(|(id, _)| id),
        Some(ID.to_string())
    );
    assert_eq!(
        manager.mount_id_for_path("sftp://ada@nas.local:22/srv/data/other"),
        None,
        "a path the narrowed root no longer covers resolves to nothing"
    );
    assert_eq!(
        manager.mount_id_for_path("sftp://ada@nas.local:22/srv/data/tmp/a.txt"),
        Some(ID.to_string())
    );
}

/// ❗ **Retires nobody.** The successor shares the incumbent's connection, its
/// reconnect loop, and its retirement flag; retiring the incumbent stands the
/// live place down.
#[test]
fn replacing_a_root_in_place_retires_nobody() {
    let (manager, volume, is_retired) = connected_at(NARROW);

    let _ = manager.replace_root_in_place(ID, &serving(&volume), successor(&volume, WIDE));

    assert!(!is_retired(), "the place is still connected, at its new root");
}

/// Announced the way `register` announces a replace: once, with the id, after
/// the swap, so a listener asking back finds the successor.
#[test]
fn a_replace_announces_the_volume_like_register_does() {
    let (manager, volume, _) = connected_at(NARROW);
    let heard = Arc::new(Mutex::new(Vec::<(String, Option<PathBuf>)>::new()));
    let recorder = Arc::clone(&heard);
    let registry = Arc::new(manager);
    let asked = Arc::downgrade(&registry);
    registry.on_volume_arrival(move |id| {
        let root = asked
            .upgrade()
            .and_then(|registry| registry.get(id))
            .map(|volume| volume.root().to_path_buf());
        recorder.lock_ignore_poison().push((id.to_string(), root));
    });

    let _ = registry.replace_root_in_place(ID, &serving(&volume), successor(&volume, WIDE));

    assert_eq!(
        heard.lock_ignore_poison().clone(),
        vec![(ID.to_string(), Some(PathBuf::from(WIDE)))]
    );
}

/// ❗ A compare-and-swap. The place disconnected and reconnected while its edit
/// was checked, so a fresh instance over a new connection serves the id. The
/// successor was built over the old, closed one, so the fresh instance stays and
/// nothing is announced.
#[test]
fn a_replace_against_an_instance_a_reconnect_displaced_replaces_nothing() {
    let (manager, checked, _) = connected_at(NARROW);
    let (fresh, _) = RetiringVolume::at(NARROW);
    manager.register(ID, serving(&fresh));
    let heard = Arc::new(Mutex::new(0usize));
    let recorder = Arc::clone(&heard);
    manager.on_volume_arrival(move |_| *recorder.lock_ignore_poison() += 1);

    let outcome = manager.replace_root_in_place(ID, &serving(&checked), successor(&checked, WIDE));

    assert!(matches!(outcome, RootReplacement::Superseded));
    let served = manager.get(ID).expect("still registered");
    assert!(Arc::ptr_eq(&served, &serving(&fresh)), "the reconnected instance stays");
    assert_eq!(manager.known_roots(ID), vec![PathBuf::from(NARROW)]);
    assert_eq!(*heard.lock_ignore_poison(), 0, "nothing arrived");
}

/// An id nobody registered is a typed refusal, and ❗ the volume is NOT
/// registered as a side effect: this replaces, it never adds.
#[test]
fn replacing_the_root_of_an_unregistered_id_registers_nothing() {
    let manager = VolumeManager::new();
    let heard = Arc::new(Mutex::new(0usize));
    let recorder = Arc::clone(&heard);
    manager.on_volume_arrival(move |_| *recorder.lock_ignore_poison() += 1);
    let never_registered: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("gone").with_root(NARROW));

    let outcome = manager.replace_root_in_place(
        ID,
        &never_registered,
        Arc::new(InMemoryVolume::new("stray").with_root(WIDE)),
    );

    assert!(matches!(outcome, RootReplacement::NotRegistered));
    assert!(manager.get(ID).is_none());
    assert_eq!(manager.count(), 0);
    assert_eq!(*heard.lock_ignore_poison(), 0, "nothing arrived");
}
