//! What the share's background work does when it stops being the live volume.
//!
//! The watcher and the watcher-death reconnect loop both hold a
//! `SelfHandle<SmbVolumeInner>` and re-ask it every iteration. Three answers
//! matter and one mechanism gives all three: the volume is still the live one, a
//! successor took its id, or the registry dropped it. See
//! `smb::reconnect::spawn_watcher_death_reconnect`.

use super::smb_test_support::*;
use super::*;
use crate::file_system::volume::manager::get_volume_manager;
use cmdr_fs::volume::SelfHandle;
use std::sync::Arc;

/// Registers a volume under `volume_id` and hands back its share handle (the way
/// `spawn_watcher` captures one when it starts a watcher) plus a holder.
///
/// The holder is what makes these tests mean anything: in production a running
/// copy, an open viewer stream, or an indexer scan holds an `Arc` for its whole
/// duration, so a volume leaving the registry does NOT free its state. Dropping
/// the last reference would let a plain `Weak` answer "gone" for the wrong
/// reason, and the check under test would look right while proving nothing.
fn registered_share(volume_id: &str) -> (SelfHandle<SmbVolumeInner>, Arc<dyn Volume>) {
    let volume = make_test_volume_with_id(volume_id);
    let share = volume.inner.self_handle();
    let holder = Arc::new(volume) as Arc<dyn Volume>;
    get_volume_manager().register(volume_id, Arc::clone(&holder));
    (share, holder)
}

#[test]
fn a_registered_share_is_still_the_live_one() {
    let id = format!("smbretire-live-{}", std::process::id());
    let (share, _holder) = registered_share(&id);

    assert!(
        share.live().is_some(),
        "nothing has taken this share's id, so its watcher must keep working"
    );

    get_volume_manager().unregister(&id);
}

/// A successor took the id (the OS-mount-to-smb2 upgrade, a re-register after a
/// remount). The session stays up for whoever still holds this instance, but
/// everything scoped to the id belongs to the successor now: a watcher still
/// feeding this id would double-feed the index, and a reconnect loop still
/// driving it would mark a perfectly healthy volume disconnected.
#[test]
fn a_superseded_share_stands_down() {
    let id = format!("smbretire-superseded-{}", std::process::id());
    let (share, holder) = registered_share(&id);

    holder.on_superseded();

    assert!(
        share.live().is_none(),
        "the id belongs to the successor, so this share's background work must go quiet"
    );

    get_volume_manager().unregister(&id);
}

/// The gap this milestone closes. A volume can leave the registry without being
/// superseded and without being unmounted: an eject, an archive-cache eviction,
/// the last mount root of a share going away. Nothing on the volume recorded
/// that, and the state its watcher hangs off stays alive for as long as any
/// in-flight holder has it, so "am I still live?" was unanswerable from inside
/// and the honest-looking answer was "yes".
#[test]
fn a_share_the_registry_dropped_stands_down() {
    let id = format!("smbretire-removed-{}", std::process::id());
    let (share, _holder) = registered_share(&id);

    get_volume_manager().unregister(&id);

    assert!(
        share.live().is_none(),
        "the app has forgotten this share, so nothing may keep reconnecting to it"
    );
}

/// Dropping every holder is the third way out, and the `Weak` half of the handle
/// covers it: background work must not be what keeps a dead share allocated.
#[test]
fn a_share_nobody_holds_stands_down() {
    let volume = make_test_volume();
    let share = volume.inner.self_handle();

    drop(volume);

    assert!(share.live().is_none(), "nothing holds this share any more");
}
