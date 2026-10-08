//! What the ledger hands a sweep, and ❗ what it never does.
//!
//! The registry of in-flight uploads is process-wide, so every cell names an
//! account of its own and can't see another cell's records.

use cmdr_fs::testing::TestDir;

use super::{UnfinishedUpload, UploadLedger};

fn upload(account: &str, key: &str, id: &str) -> UnfinishedUpload {
    UnfinishedUpload {
        account: account.to_string(),
        bucket: "cmdr-test".to_string(),
        key: key.to_string(),
        upload_id: id.to_string(),
    }
}

/// The crash case: a previous launch recorded an upload and never got to
/// finish or abort it. Its line is all that's left, and it's what the sweep
/// acts on.
#[test]
fn an_upload_a_previous_launch_left_behind_is_a_leftover() {
    let dir = TestDir::new("ledger_crash");
    let crashed = upload("acct-crash", "photos/big.mov", "id-1");
    UploadLedger::at(Some(dir.to_path_buf())).started(&crashed);
    // A crash takes the in-flight mark with it; a later launch only has the file.
    UploadLedger::forget_in_flight(&crashed);

    assert_eq!(
        UploadLedger::at(Some(dir.to_path_buf())).leftovers("acct-crash"),
        vec![crashed]
    );
}

/// ❗ An upload still running in this process is never a leftover, even to a
/// second place of the same account connecting mid-upload: the sweep would
/// abort a live upload.
#[test]
fn an_upload_in_flight_is_never_a_leftover() {
    let dir = TestDir::new("ledger_in_flight");
    let live = upload("acct-live", "a.bin", "id-live");
    UploadLedger::at(Some(dir.to_path_buf())).started(&live);

    assert!(
        UploadLedger::at(Some(dir.to_path_buf()))
            .leftovers("acct-live")
            .is_empty()
    );
    UploadLedger::at(Some(dir.to_path_buf())).finished(&live);
}

#[test]
fn a_finished_upload_is_forgotten() {
    let dir = TestDir::new("ledger_finished");
    let ledger = UploadLedger::at(Some(dir.to_path_buf()));
    let done = upload("acct-done", "a.bin", "id-done");
    ledger.started(&done);
    ledger.finished(&done);
    UploadLedger::forget_in_flight(&done);

    assert!(ledger.leftovers("acct-done").is_empty());
}

/// An abort that failed (the server was gone) leaves the record for the next
/// sweep, and stops counting the upload as in flight.
#[test]
fn an_abandoned_upload_waits_for_the_next_sweep() {
    let dir = TestDir::new("ledger_abandoned");
    let ledger = UploadLedger::at(Some(dir.to_path_buf()));
    let stuck = upload("acct-stuck", "a.bin", "id-stuck");
    ledger.started(&stuck);
    ledger.abandoned(&stuck);

    assert_eq!(ledger.leftovers("acct-stuck"), vec![stuck]);
}

#[test]
fn a_sweep_sees_only_its_own_accounts_records() {
    let dir = TestDir::new("ledger_accounts");
    let ledger = UploadLedger::at(Some(dir.to_path_buf()));
    let mine = upload("acct-mine", "a.bin", "id-a");
    let theirs = upload("acct-theirs", "a.bin", "id-b");
    for record in [&mine, &theirs] {
        ledger.started(record);
        ledger.abandoned(record);
    }

    assert_eq!(ledger.leftovers("acct-mine"), vec![mine]);
}

/// Keys hold anything, spaces and line breaks included, and a record has to
/// name the same object when it's read back.
#[test]
fn a_key_with_spaces_and_line_breaks_round_trips() {
    let dir = TestDir::new("ledger_keys");
    let odd = upload("acct-odd key", "a b/c\nd%25 +e.txt", "id/with+odd=chars");
    UploadLedger::at(Some(dir.to_path_buf())).started(&odd);
    UploadLedger::forget_in_flight(&odd);

    assert_eq!(
        UploadLedger::at(Some(dir.to_path_buf())).leftovers("acct-odd key"),
        vec![odd]
    );
}

/// The log is rewritten to what's still open, so it doesn't grow with every
/// upload ever made, and the rewrite keeps every open record.
#[test]
fn reading_leftovers_compacts_the_log_to_what_is_still_open() {
    let dir = TestDir::new("ledger_compact");
    let ledger = UploadLedger::at(Some(dir.to_path_buf()));
    let open = upload("acct-compact", "open.bin", "id-open");
    ledger.started(&open);
    ledger.abandoned(&open);
    for n in 0..20 {
        let done = upload("acct-compact", "done.bin", &format!("id-{n}"));
        ledger.started(&done);
        ledger.finished(&done);
    }

    assert_eq!(ledger.leftovers("acct-compact"), vec![open.clone()]);
    let log = std::fs::read_to_string(dir.join("unfinished-uploads")).expect("the log is there");
    assert_eq!(log.lines().count(), 1, "only the open record is left: {log:?}");
    assert_eq!(ledger.leftovers("acct-compact"), vec![open]);
}

/// A line this build can't read is skipped, never fatal, and never read as a
/// record of something else.
#[test]
fn an_unreadable_line_is_skipped() {
    let dir = TestDir::new("ledger_garbage");
    std::fs::write(dir.join("unfinished-uploads"), "garbage\n+ only two\n").expect("seeding the log");
    let fine = upload("acct-garbage", "a.bin", "id-fine");
    let ledger = UploadLedger::at(Some(dir.to_path_buf()));
    ledger.started(&fine);
    ledger.abandoned(&fine);

    assert_eq!(ledger.leftovers("acct-garbage"), vec![fine]);
}

/// With no state directory (a test host, a tool), the ledger still serves the
/// session: an abort that failed is swept at the next connect in this process.
#[test]
fn a_ledger_without_a_directory_still_serves_the_session() {
    let ledger = UploadLedger::at(None);
    let stuck = upload("acct-memory", "a.bin", "id-memory");
    ledger.started(&stuck);
    assert!(ledger.leftovers("acct-memory").is_empty(), "in flight");
    ledger.abandoned(&stuck);
    assert_eq!(ledger.leftovers("acct-memory"), vec![stuck.clone()]);
    ledger.finished(&stuck);
    assert!(ledger.leftovers("acct-memory").is_empty());
}

/// ❗ A fixture cell asks whether ITS writes left a record, while another cell
/// of the same account (the registry is process-wide) may hold one open right
/// then, the sweep cell's planted record for one: the question is scoped to
/// the cell's own key prefix.
#[test]
fn a_cells_question_sees_only_the_records_under_its_own_prefix() {
    let ledger = UploadLedger::at(None);
    let neighbours = upload("acct-shared", "cell-a/stuck.bin", "id-a");
    ledger.started(&neighbours);
    ledger.abandoned(&neighbours);

    assert!(
        ledger.open_under("acct-shared", "cell-b/").is_empty(),
        "cell b sees cell a's record"
    );
    assert_eq!(ledger.open_under("acct-shared", "cell-a/"), vec![neighbours.clone()]);
    ledger.finished(&neighbours);
    assert!(ledger.open_under("acct-shared", "cell-a/").is_empty());
}
