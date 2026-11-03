//! What a volume's status read says about unreadable ground while a walk holds
//! ground on it.
//!
//! The count comes from a coverage descent of the whole volume, which stops at
//! the first covered subtree. A walk in progress leaves everything it has listed
//! so far listed but not yet covered, so a descent mid-walk reads all of it, on
//! every status read (`cmdr://state` blocked for over 90 s on one, #870).

use super::test_support::{Share, drain};

#[test]
fn a_status_read_mid_walk_answers_from_the_last_settled_count() {
    let (share, volume) = Share::instrumented(
        "cover-status-mid-walk-test",
        |t| {
            vec![
                t.dir("first"),
                t.file("first/a.txt", 1),
                t.dir("second"),
                t.dir("second/inner"),
                t.file("second/inner/b.txt", 1),
            ]
        },
        Some("second"),
    );
    share.cover(&share.path("first"));
    share.mark_completed();

    let before = share.status();
    assert!(before.unreadable_settled, "nothing walks yet, so the count is current");

    let (walk, _cancel) = share.walk(&share.path("second"));
    volume.wait_for_the_gate();
    let mid_walk = share.status();
    volume.release_the_gate();
    drain(walk);

    assert!(
        !mid_walk.unreadable_settled,
        "a walk holds ground, so the read says the count is still moving instead of descending for it"
    );
    assert_eq!(
        (mid_walk.unreadable_locations, mid_walk.unreadable_retried),
        (before.unreadable_locations, before.unreadable_retried),
        "and answers with the last settled count"
    );

    cmdr_fs::testing::wait_until(
        std::time::Duration::from_secs(10),
        "the status to settle once the walk let go",
        || share.status().unreadable_settled,
    );
}
