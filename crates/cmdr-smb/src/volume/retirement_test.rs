//! What the share's background work does when it stops being the live volume.
//!
//! The watcher and the watcher-death reconnect loop both hold a
//! `SelfHandle<SmbVolumeInner>` and re-ask it every iteration. What they get is
//! only right if THREE flags are one flag: the one `Volume::retirement` hands
//! the registry, the one `on_superseded` writes, and the one the handle reads.
//! These pin that they are.
//!
//! `SelfHandle`'s own mechanics (retired stops answering, dropped stops
//! answering, retirement is one-way) belong to `cmdr-fs` and are tested there.
//! The registry's side — that leaving it retires a volume at all — belongs to
//! the app's `VolumeManager`.

use std::sync::Arc;

use super::test_support::*;
use super::*;

/// A volume nobody has retired is the live one, so its watcher keeps working.
#[test]
fn a_fresh_share_is_the_live_one() {
    let volume = make_test_volume();

    assert!(
        volume.inner.self_handle().live().is_some(),
        "nothing has taken this share's id, so its watcher must keep working"
    );
}

/// A successor took the id (the OS-mount-to-smb2 upgrade, a re-register after a
/// remount). The session stays up for whoever still holds this instance, but
/// everything scoped to the id belongs to the successor now: a watcher still
/// feeding this id would double-feed the index, and a reconnect loop still
/// driving it would mark a perfectly healthy volume disconnected.
#[test]
fn a_superseded_share_stands_down() {
    let volume = make_test_volume();
    let share = volume.inner.self_handle();

    volume.on_superseded();

    assert!(
        share.live().is_none(),
        "the id belongs to the successor, so this share's background work must go quiet"
    );
}

/// The wiring the registry depends on: what it writes through
/// [`Volume::retirement`] is the flag this share's background work reads.
///
/// A share splits its state in two (a per-mount-root instance over a shared
/// per-share session), so these could easily be different fields, and every
/// other test here would still pass while a removed share kept reconnecting.
#[test]
fn the_flag_the_registry_writes_is_the_one_the_watcher_reads() {
    let volume = make_test_volume();
    let share = volume.inner.self_handle();

    volume
        .retirement()
        .expect("an SMB share carries a retirement flag, or the registry can't tell it that it left")
        .retire();

    assert!(
        share.live().is_none(),
        "the app has forgotten this share, so nothing may keep reconnecting to it"
    );
}

/// A re-root is the case retiring must NOT cover: the promoted instance is the
/// same share on the same session, so the flag it publishes has to be the same
/// one, and retiring either would stand the live watcher down.
#[test]
fn a_rerooted_instance_publishes_the_same_flag() {
    let volume = make_test_volume();
    let promoted = volume
        .rerooted(Path::new("/Volumes/TestShare-1"))
        .expect("a direct SMB share re-roots");

    volume.retirement().expect("a flag").retire();

    assert!(
        promoted.retirement().expect("a flag").is_retired(),
        "both instances are one share, so they must publish one flag"
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

/// A holder keeps the state alive, which is what makes every assertion above
/// mean something: in production a running copy, an open viewer stream, or an
/// indexer scan holds an `Arc` for its whole duration, so a volume leaving the
/// registry does NOT free its state. Without this, a plain `Weak` would answer
/// "gone" for the wrong reason and the check under test would prove nothing.
#[test]
fn a_retired_share_keeps_serving_the_holder_that_has_it() {
    let volume = Arc::new(make_test_volume()) as Arc<dyn Volume>;
    let held = Arc::clone(&volume);

    volume.retirement().expect("a flag").retire();

    assert_eq!(held.name(), "TestShare", "a retired share still answers its holder");
}
