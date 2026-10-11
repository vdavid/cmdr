//! The attempt table's own promises, with no backend behind it.
//!
//! ❗ These used to be reachable only through a real dial, which is why the
//! repeated-id race went untested in both backends that carried a copy.

use super::*;

static TABLE: AttemptTable = AttemptTable::new("a test");
static OTHER: AttemptTable = AttemptTable::new("another test");

#[test]
fn cancelling_an_id_nobody_is_running_is_a_plain_no() {
    assert!(!TABLE.cancel("nothing-is-filed-under-this"));
}

#[test]
fn a_registered_attempt_is_cancelable_and_its_token_says_so() {
    let (cancel, _guard) = TABLE.register("cancelable");

    assert!(!cancel.is_cancelled());
    assert!(TABLE.cancel("cancelable"));
    assert!(cancel.is_cancelled(), "the dial's own token is the one that moved");
}

#[test]
fn the_guard_takes_the_entry_out_however_the_connect_ended() {
    {
        let (_cancel, _guard) = TABLE.register("ends-on-its-own");
    }

    assert!(
        !TABLE.cancel("ends-on-its-own"),
        "a token nobody collects is an id that can never be reused"
    );
}

#[test]
fn a_second_attempt_under_one_id_stays_cancelable_after_the_first_ends() {
    // ❗ The race the serial exists for. Without it the first attempt's guard
    // removes the SECOND one's entry on its way out, and the live dial can no
    // longer be called off.
    let (_first_token, first_guard) = TABLE.register("reused-id");
    let (second_token, _second_guard) = TABLE.register("reused-id");

    drop(first_guard);

    assert!(TABLE.cancel("reused-id"), "the live attempt is still filed");
    assert!(second_token.is_cancelled());
}

#[tokio::test]
async fn two_dials_of_one_saved_place_leave_one_volume_serving_its_id() {
    // ❗ Two panes can dial one saved place at once, and `connect_saved_place`
    // checks the registry with a `get`, not a claim, so both dials pass it. That
    // still can't register the place twice: both mint the same id from
    // `(host, port, username)`, the registry is keyed by it, and the second
    // install retires the first and takes the id over.
    use crate::file_system::volume::InMemoryVolume;

    let volume_id = cmdr_fs::volume::sftp_volume_id("two-dials.example", 22, "ada");
    let first: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("first dial").with_root("/srv/data"));
    let second: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("second dial").with_root("/srv/data"));

    tokio::join!(
        install_retiring_incumbent(&volume_id, Arc::clone(&first)),
        install_retiring_incumbent(&volume_id, Arc::clone(&second)),
    );

    let serving = crate::file_system::volume::manager::get_volume_manager()
        .get(&volume_id)
        .expect("the place is registered");
    assert!(Arc::ptr_eq(&serving, &second), "the later install holds the one id");
    assert!(!Arc::ptr_eq(&serving, &first));
}

#[test]
fn one_backends_cancel_never_reaches_another_backends_dial() {
    // ❗ Why each backend holds its own table rather than sharing one: the
    // frontend mints ids per backend, and a collision would otherwise let a
    // stray cancel end someone else's connect.
    let (mine, _guard) = TABLE.register("same-id-in-both");
    let (theirs, _other_guard) = OTHER.register("same-id-in-both");

    assert!(TABLE.cancel("same-id-in-both"));

    assert!(mine.is_cancelled());
    assert!(!theirs.is_cancelled());
}

// -- a place that moved while a dial to it was out --

#[tokio::test]
async fn a_dial_whose_place_moved_while_it_was_out_cannot_land() {
    // ❗ The race the landing exists for: a dial to the OLD address that set out
    // before Save and succeeds after it would remember the old entry again, a
    // second saved server beside the moved one.
    let (_cancel, attempt) = TABLE.register_dialing(
        "dial-out-during-move",
        vec!["moved-while-out".to_string()],
        DialTicket::now(),
    );

    let mut moving = start_move().await;
    moving.moved_away("moved-while-out");
    drop(moving);

    assert!(attempt.land().await.is_none());
}

#[tokio::test]
async fn a_dial_that_read_its_target_before_a_move_cannot_land_however_late_it_filed() {
    // ❗ A saved place's dial reads its address from the store, THEN files its
    // attempt. A move landing between the two would otherwise look older than the
    // dial, and a host-key approval answered after a move would save the old
    // address again. The ticket is taken before the read.
    let set_out = DialTicket::now();
    let mut moving = start_move().await;
    moving.moved_away("read-before-move");
    drop(moving);

    let (_cancel, attempt) =
        TABLE.register_dialing("dial-read-before-move", vec!["read-before-move".to_string()], set_out);

    assert!(attempt.land().await.is_none());
}

#[tokio::test]
async fn a_dial_that_set_out_after_the_move_lands() {
    let mut moving = start_move().await;
    moving.moved_away("moved-before-dial");
    drop(moving);

    let (_cancel, attempt) = TABLE.register_dialing(
        "dial-after-move",
        vec!["moved-before-dial".to_string()],
        DialTicket::now(),
    );

    assert!(
        attempt.land().await.is_some(),
        "a later Add of the old address is a new server"
    );
}

#[tokio::test]
async fn a_move_waits_for_a_dial_that_is_landing() {
    use futures_util::FutureExt;

    let (_cancel, attempt) =
        TABLE.register_dialing("landing-now", vec!["landing-place".to_string()], DialTicket::now());
    let landing = attempt.land().await.expect("nothing moved");

    assert!(
        start_move().now_or_never().is_none(),
        "a move between a landing's install and its remember would leave the old entry behind it"
    );
    drop(landing);
    drop(start_move().await);
}

#[test]
fn a_move_calls_off_the_dials_to_its_place_and_only_those() {
    let (to_moved, _guard) =
        TABLE.register_dialing("dial-to-moved", vec!["calls-off-this".to_string()], DialTicket::now());
    let (elsewhere, _other) =
        TABLE.register_dialing("dial-elsewhere", vec!["leaves-this".to_string()], DialTicket::now());

    assert_eq!(TABLE.cancel_dials_to("calls-off-this"), 1);

    assert!(to_moved.is_cancelled());
    assert!(!elsewhere.is_cancelled());
}
