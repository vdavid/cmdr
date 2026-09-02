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
