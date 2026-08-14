//! The retry window, over a real index DB with a clock the test moves itself.
//!
//! Every case here is about COST, which is why they read as arithmetic rather than
//! as behavior: a retry reopens a wedged mount's whole subtree, and the next walk
//! over that scope pays a stall timeout per directory in it. Getting the schedule
//! wrong doesn't break anything visibly, it just quietly re-pays the bill.

use super::*;
use crate::indexing::store::{ROOT_ID, register_platform_case_collation};

const MINUTE: u64 = 60;
const HOUR: u64 = 60 * MINUTE;

/// An open index DB with `n` directories under the root, all marked abandoned.
fn abandoned_index(marked: usize) -> (Connection, tempfile::TempDir) {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    let conn = IndexStore::open_write_connection(&db_path).expect("write connection");
    register_platform_case_collation(&conn).expect("collation");

    let ids: Vec<i64> = (0..marked)
        .map(|i| {
            IndexStore::insert_entry_v2(
                &conn,
                ROOT_ID,
                &format!("wedged{i}"),
                true,
                false,
                None,
                None,
                None,
                None,
            )
            .expect("insert")
        })
        .collect();
    IndexStore::mark_dirs_unreadable(&conn, &ids, Some(UnreadableCause::Abandoned)).expect("mark");
    (conn, dir)
}

/// The id of the one row carrying the abandoned cause.
fn only_marked_id(conn: &Connection) -> i64 {
    conn.query_row("SELECT id FROM entries WHERE unreadable_cause = 3", [], |r| r.get(0))
        .expect("exactly one marked row")
}

/// How many rows still carry the abandoned cause.
fn still_abandoned(conn: &Connection) -> i64 {
    conn.query_row("SELECT COUNT(*) FROM entries WHERE unreadable_cause = 3", [], |r| {
        r.get(0)
    })
    .expect("count")
}

/// A volume nothing gave up on does no work at all.
///
/// Load-bearing rather than tidy: `unreadable_cause` carries no index, so a
/// speculative clear is a full scan of every row on the volume — six million of
/// them on a boot disk — and the maintenance timer fires every 30 seconds.
#[test]
fn an_unarmed_volume_never_touches_entries() {
    let (conn, _dir) = abandoned_index(0);
    assert_eq!(
        clear_if_due(&conn, 10 * HOUR).expect("tick"),
        None,
        "nothing armed the window, so there is nothing to retry"
    );
}

/// The first retry waits five minutes, and not a moment less.
///
/// Short on purpose, and for a different reason than the rest of the schedule: a
/// one-off read failure puts a folder out of every search answer, and nothing a
/// user can do brings it back (the verifier bails on an unlisted directory, and the
/// frontier no longer offers it to a re-run search). Five minutes is how long that
/// injustice may last.
#[test]
fn the_first_window_is_five_minutes() {
    let (conn, _dir) = abandoned_index(3);
    arm(&conn, 0).expect("arm");

    assert_eq!(clear_if_due(&conn, 5 * MINUTE - 1).expect("tick"), None, "not due yet");
    assert_eq!(still_abandoned(&conn), 3, "and nothing was reopened early");

    assert_eq!(
        clear_if_due(&conn, 5 * MINUTE).expect("tick"),
        Some(3),
        "five minutes on, the ground goes back in the frontier"
    );
    assert_eq!(still_abandoned(&conn), 0);
}

/// A mount that stays wedged costs less each time: 5 min, 1 h, 4 h, 24 h, then 24 h
/// forever. The first step is the one-off-failure allowance; everything after it is
/// the wedged-ground curve, and without the growth a still-dead mount would be
/// re-walked every five minutes at full stall-timeout price — the bug the mark
/// exists to fix, slowed down.
#[test]
fn a_mount_that_stays_wedged_backs_off_and_then_holds() {
    let (conn, _dir) = abandoned_index(1);
    let wedged = only_marked_id(&conn);
    arm(&conn, 0).expect("arm");

    let mut now = 0;
    for window in [5 * MINUTE, HOUR, 4 * HOUR, 24 * HOUR, 24 * HOUR] {
        assert_eq!(
            clear_if_due(&conn, now + window - 1).expect("tick"),
            None,
            "a {window}s window must not fire a second early"
        );
        now += window;
        assert_eq!(
            clear_if_due(&conn, now).expect("tick"),
            Some(1),
            "and must fire once it elapses"
        );
        // The next walk finds the mount still wedged and condemns it again. Its
        // re-arm has to be a no-op, or the backoff never leaves its first step.
        IndexStore::mark_dirs_unreadable(&conn, &[wedged], Some(UnreadableCause::Abandoned)).expect("re-mark");
        arm(&conn, now).expect("arm");
    }
}

/// A mount that came back stops costing anything, and a NEW problem gets the fast
/// first window rather than inheriting the old one's patience.
#[test]
fn a_healed_volume_disarms_and_the_next_problem_starts_over() {
    let (conn, _dir) = abandoned_index(1);
    arm(&conn, 0).expect("arm");
    assert_eq!(clear_if_due(&conn, HOUR).expect("tick"), Some(1));

    // The walk that follows lists it successfully, which clears the cause itself.
    IndexStore::mark_dirs_listed(&conn, &[ROOT_ID + 1], 7).expect("listed");

    // Next window: nothing left to clear, so the volume goes quiet.
    assert_eq!(
        clear_if_due(&conn, HOUR + 4 * HOUR).expect("tick"),
        Some(0),
        "one attempt finds the ground already healed"
    );
    assert_eq!(clear_if_due(&conn, 100 * HOUR).expect("tick"), None, "and then stops");

    // Months later, a different mount goes wedged. It gets the five-minute
    // one-off-failure allowance back, not the old mount's 24-hour patience.
    let fresh = IndexStore::insert_entry_v2(&conn, ROOT_ID, "other", true, false, None, None, None, None).expect("row");
    IndexStore::mark_dirs_unreadable(&conn, &[fresh], Some(UnreadableCause::Abandoned)).expect("mark");
    let months_on = 1_000 * HOUR;
    arm(&conn, months_on).expect("arm");
    assert_eq!(clear_if_due(&conn, months_on + 5 * MINUTE - 1).expect("tick"), None);
    assert_eq!(clear_if_due(&conn, months_on + 5 * MINUTE).expect("tick"), Some(1));
}

/// A retry clears ONLY abandoned ground. A refusal is an answer the user has to
/// act on and a declined snapshot tree is a standing policy; reopening either would
/// re-pay a read that is going to fail the same way, forever.
#[test]
fn a_retry_leaves_refused_and_declined_ground_alone() {
    let (conn, _dir) = abandoned_index(1);
    let denied =
        IndexStore::insert_entry_v2(&conn, ROOT_ID, "denied", true, false, None, None, None, None).expect("row");
    let declined =
        IndexStore::insert_entry_v2(&conn, ROOT_ID, "@eaDir", true, false, None, None, None, None).expect("row");
    IndexStore::mark_dirs_unreadable(&conn, &[denied], Some(UnreadableCause::Denied)).expect("mark");
    IndexStore::mark_dirs_unreadable(&conn, &[declined], Some(UnreadableCause::Declined)).expect("mark");
    arm(&conn, 0).expect("arm");

    assert_eq!(clear_if_due(&conn, HOUR).expect("tick"), Some(1), "the abandoned one");
    let cause = |id: i64| {
        IndexStore::get_unreadable_cause_by_id(&conn, id)
            .expect("cause")
            .expect("row")
    };
    assert_eq!(cause(denied), Some(UnreadableCause::Denied));
    assert_eq!(cause(declined), Some(UnreadableCause::Declined));
}

/// A window that opened in the future (a clock that jumped backwards, or an index
/// carried over from a machine with a skewed clock) fires rather than wedging
/// retries shut until the calendar catches up.
#[test]
fn a_window_from_the_future_fails_open() {
    let (conn, _dir) = abandoned_index(1);
    arm(&conn, 10_000 * HOUR).expect("arm");
    assert_eq!(clear_if_due(&conn, HOUR).expect("tick"), Some(1));
}

/// The seam the whole heal hangs off: a walk's mark arms the window, and only an
/// ABANDONED one does.
///
/// Every other test here calls [`arm`] directly, so all of them would still pass
/// with nothing wired to it — and a window nothing ever arms means a wedged mount
/// is condemned once and never retried, which is a worse bug than the one this
/// mechanism fixes. So this one goes through the writer, as a walk does.
#[test]
fn a_walks_abandoned_mark_is_what_arms_the_window() {
    use crate::indexing::writer::{IndexWriter, WriteMessage, tests::setup_db};

    let (db_path, _dir) = setup_db();
    let writer = IndexWriter::spawn(&db_path, crate::NoopEventSink::shared()).expect("writer");
    let armed = |db_path: &std::path::Path| {
        let conn = IndexStore::open_read_connection(db_path).expect("read connection");
        IndexStore::get_meta(&conn, RETRY_AT_KEY).expect("meta").is_some()
    };

    // A refusal is the user's to fix, not Cmdr's to retry.
    writer
        .send(WriteMessage::MarkDirsUnreadable {
            ids: vec![ROOT_ID],
            cause: UnreadableCause::Denied,
        })
        .expect("send");
    writer.flush_blocking().expect("flush");
    assert!(!armed(&db_path), "❌ a refusal must not open a retry window");

    writer
        .send(WriteMessage::MarkDirsUnreadable {
            ids: vec![ROOT_ID],
            cause: UnreadableCause::Abandoned,
        })
        .expect("send");
    writer.flush_blocking().expect("flush");
    assert!(armed(&db_path), "abandoned ground is what Cmdr comes back to");
    writer.shutdown();
}

/// The window survives a relaunch, which is the whole reason it's in `meta` rather
/// than in memory: David restarts often, and a 24-hour window spans many restarts.
/// An in-memory one would reset to "due now" on every launch.
#[test]
fn the_window_survives_a_reopen() {
    let dir = tempfile::tempdir().expect("temp dir");
    let db_path = dir.path().join("index.db");
    let _store = IndexStore::open(&db_path).expect("open store");
    {
        let conn = IndexStore::open_write_connection(&db_path).expect("write connection");
        register_platform_case_collation(&conn).expect("collation");
        let id =
            IndexStore::insert_entry_v2(&conn, ROOT_ID, "wedged", true, false, None, None, None, None).expect("row");
        IndexStore::mark_dirs_unreadable(&conn, &[id], Some(UnreadableCause::Abandoned)).expect("mark");
        arm(&conn, 0).expect("arm");
    }

    let conn = IndexStore::open_write_connection(&db_path).expect("reopen");
    register_platform_case_collation(&conn).expect("collation");
    assert_eq!(
        clear_if_due(&conn, 5 * MINUTE - 1).expect("tick"),
        None,
        "the window a previous session opened still holds"
    );
    assert_eq!(clear_if_due(&conn, 5 * MINUTE).expect("tick"), Some(1));
}
