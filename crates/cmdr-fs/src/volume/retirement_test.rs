//! The liveness question a backend's background work asks every iteration.

use super::retirement::{Retirement, SelfHandle};
use std::sync::Arc;

/// Stands in for whatever share-scoped state a backend hangs its background
/// work off (for SMB, the session and its connection state).
struct Session {
    name: &'static str,
}

fn handle_over(session: &Arc<Session>) -> (SelfHandle<Session>, Arc<Retirement>) {
    let retirement = Arc::new(Retirement::new());
    (SelfHandle::new(Arc::downgrade(session), &retirement), retirement)
}

#[test]
fn a_registered_volume_answers_with_its_own_state() {
    let session = Arc::new(Session { name: "public" });
    let (handle, _retirement) = handle_over(&session);

    assert_eq!(
        handle.live().map(|s| s.name),
        Some("public"),
        "a live volume's background work must reach the state it was spawned for"
    );
}

/// The gap this type exists to close. A volume can be REMOVED from the registry
/// without being replaced or unmounted (an eject, an archive-cache eviction), and
/// the state it hangs off stays alive as long as any in-flight holder has it. So
/// "still allocated" is not "still registered", and only the registry knows.
#[test]
fn a_retired_volume_stops_answering_even_while_its_state_is_alive() {
    let session = Arc::new(Session { name: "public" });
    let (handle, retirement) = handle_over(&session);

    retirement.retire();

    assert!(
        handle.live().is_none(),
        "the registry retired this volume, so its background work must stand down"
    );
    assert!(
        Arc::strong_count(&session) > 0,
        "retirement must not tear the state down: whoever still holds it keeps working"
    );
}

#[test]
fn a_dropped_volume_stops_answering() {
    let session = Arc::new(Session { name: "public" });
    let (handle, _retirement) = handle_over(&session);

    drop(session);

    assert!(
        handle.live().is_none(),
        "nothing holds the state any more, so there is nothing left to act on"
    );
}

#[test]
fn retirement_is_one_way_and_idempotent() {
    let retirement = Retirement::new();
    assert!(!retirement.is_retired());

    retirement.retire();
    retirement.retire();

    assert!(retirement.is_retired(), "a second retire must not un-retire the volume");
}
