//! What the decorator writes back, and what it must leave alone.
//!
//! A file-backed DB rather than an in-memory one: the decorator owns its own connection
//! because the operation outlives the call that started it, and two in-memory connections are
//! two different databases. This is the production shape.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use rusqlite::Connection;

use super::decorator::ProposalReportingSink;
use crate::agent::store::open_write_connection;
use crate::agent::store::proposals::{
    GroupIntent, NewGroup, NewOp, NewSweep, ProposalOp, create_group, create_sweep, page_ops,
};
use crate::agent::types::{OpStatus, ProposalStatus};
use crate::file_system::write_operations::{
    CollectorEventSink, OperationEventSink, SourceItemOutcome, WriteOperationType, WriteSettledEvent,
    WriteSourceItemDoneEvent,
};
use crate::ignore_poison::IgnorePoison;
use crate::test_support::TestDir;

/// A trash group over two sources on a real file DB, plus a second connection for the
/// decorator to own.
fn fixture(dir: &TestDir) -> (Connection, Connection, i64, Vec<ProposalOp>) {
    let db_path = dir.join("main.db");
    let conn = open_write_connection(&db_path).expect("open");
    let set_id = create_sweep(&conn, &NewSweep::default(), 100).expect("sweep");
    let group_id = create_group(
        &conn,
        set_id,
        &NewGroup {
            intent: GroupIntent::Trash {
                sources: ["/Users/someone/Downloads/one.dmg", "/Users/someone/Downloads/two.dmg"]
                    .into_iter()
                    .map(|path| NewOp {
                        source_path: path.to_string(),
                        snapshot: None,
                    })
                    .collect(),
            },
            source_volume_id: "root".to_string(),
            display_name: "two installers".to_string(),
            rationale: None,
            selector: None,
        },
        100,
    )
    .expect("group");
    let ops = page_ops(&conn, group_id, 10, 0).expect("ops");
    let reporting = open_write_connection(&db_path).expect("second connection");
    (conn, reporting, group_id, ops)
}

fn sink_over(
    ops: &[ProposalOp],
    group_id: i64,
    reporting: Connection,
) -> (Arc<CollectorEventSink>, ProposalReportingSink) {
    let collector = Arc::new(CollectorEventSink::new());
    let op_ids: HashMap<PathBuf, i64> = ops.iter().map(|op| (PathBuf::from(&op.source_path), op.id)).collect();
    let sink = ProposalReportingSink::new(collector.clone(), group_id, op_ids, reporting, None);
    (collector, sink)
}

fn done(source_path: &str, outcome: SourceItemOutcome) -> WriteSourceItemDoneEvent {
    WriteSourceItemDoneEvent {
        operation_id: "op-1".to_string(),
        source_path: source_path.to_string(),
        source_removed: matches!(outcome, SourceItemOutcome::Done),
        outcome,
    }
}

fn status_of_op(conn: &Connection, group_id: i64, source_path: &str) -> OpStatus {
    page_ops(conn, group_id, 10, 0)
        .expect("ops")
        .into_iter()
        .find(|op| op.source_path == source_path)
        .expect("the op exists")
        .status
}

fn group_status(conn: &Connection, group_id: i64) -> ProposalStatus {
    crate::agent::store::proposals::get_group(conn, group_id)
        .expect("read")
        .expect("exists")
        .status
}

#[test]
fn each_outcome_the_engine_reports_lands_on_its_op() {
    for (reported, stored) in [
        (SourceItemOutcome::Done, OpStatus::Done),
        (SourceItemOutcome::Skipped, OpStatus::Skipped),
        (SourceItemOutcome::Failed, OpStatus::Failed),
    ] {
        let dir = TestDir::new("bridge_outcome");
        let (conn, reporting, group_id, ops) = fixture(&dir);
        let (collector, sink) = sink_over(&ops, group_id, reporting);

        sink.emit_source_item_done(done("/Users/someone/Downloads/one.dmg", reported));

        assert_eq!(
            status_of_op(&conn, group_id, "/Users/someone/Downloads/one.dmg"),
            stored
        );
        assert_eq!(
            status_of_op(&conn, group_id, "/Users/someone/Downloads/two.dmg"),
            OpStatus::Pending,
            "the source nothing was said about is untouched"
        );
        assert_eq!(
            collector.source_items_done.lock_ignore_poison().len(),
            1,
            "the decorator adds a write, it never swallows the event"
        );
    }
}

/// A cross-filesystem move speaks twice for one source, and staging succeeding says nothing
/// about where the item ended up. Recording the FIRST word would tell the review surface a
/// move happened that did not.
#[test]
fn the_last_word_about_a_source_is_the_one_recorded() {
    let dir = TestDir::new("bridge_last_word");
    let (conn, reporting, group_id, ops) = fixture(&dir);
    let (_collector, sink) = sink_over(&ops, group_id, reporting);

    sink.emit_source_item_done(done("/Users/someone/Downloads/one.dmg", SourceItemOutcome::Done));
    sink.emit_source_item_done(done("/Users/someone/Downloads/one.dmg", SourceItemOutcome::Skipped));

    assert_eq!(
        status_of_op(&conn, group_id, "/Users/someone/Downloads/one.dmg"),
        OpStatus::Skipped
    );
}

/// Nothing stops a caller running an approved group beside sources of its own, and the engine
/// reports every top-level source it was handed. An unknown path is not an error.
#[test]
fn a_path_the_group_never_proposed_is_passed_through_untouched() {
    let dir = TestDir::new("bridge_stranger");
    let (conn, reporting, group_id, ops) = fixture(&dir);
    let (collector, sink) = sink_over(&ops, group_id, reporting);

    sink.emit_source_item_done(done("/Users/someone/Elsewhere/other.dmg", SourceItemOutcome::Done));

    assert_eq!(
        status_of_op(&conn, group_id, "/Users/someone/Downloads/one.dmg"),
        OpStatus::Pending
    );
    assert_eq!(collector.source_items_done.lock_ignore_poison().len(), 1);
}

fn settled() -> WriteSettledEvent {
    WriteSettledEvent {
        operation_id: "op-1".to_string(),
        operation_type: WriteOperationType::Trash,
        volume_id: None,
    }
}

/// Settle is the hook because it fires on every ending, including a cancel. Marking only on
/// success would leave a cancelled group `approved`, and the next launch would call it
/// `interrupted`: a claim that the app died, about an operation the user stopped on purpose.
#[test]
fn settling_ends_the_group_whatever_the_operation_did() {
    let dir = TestDir::new("bridge_settle");
    let (conn, reporting, group_id, ops) = fixture(&dir);
    crate::agent::store::proposals::record_acceptance(&conn, group_id, &[], 200).expect("preflight");
    crate::agent::store::proposals::claim_group_for_execution(&conn, group_id, 200).expect("claim");
    let (collector, sink) = sink_over(&ops, group_id, reporting);

    sink.emit_settled(settled());

    assert_eq!(group_status(&conn, group_id), ProposalStatus::Completed);
    assert_eq!(
        collector.settled.lock_ignore_poison().len(),
        1,
        "the settle still reaches every surface waiting for it"
    );
}

/// The whole point, end to end through the decorator: a group that settled is not asked about
/// again on the next launch.
#[test]
fn a_group_the_decorator_settled_survives_the_next_launch_sweep() {
    let dir = TestDir::new("bridge_survives");
    let (conn, reporting, group_id, ops) = fixture(&dir);
    crate::agent::store::proposals::record_acceptance(&conn, group_id, &[], 200).expect("preflight");
    crate::agent::store::proposals::claim_group_for_execution(&conn, group_id, 200).expect("claim");
    let (_collector, sink) = sink_over(&ops, group_id, reporting);

    sink.emit_source_item_done(done("/Users/someone/Downloads/one.dmg", SourceItemOutcome::Done));
    sink.emit_source_item_done(done("/Users/someone/Downloads/two.dmg", SourceItemOutcome::Done));
    sink.emit_settled(settled());

    crate::agent::store::proposals::recover_interrupted_groups(&conn).expect("sweep");
    assert_eq!(group_status(&conn, group_id), ProposalStatus::Completed);
}

/// ⚠️ **An approval's real outcome is only known at SETTLE.** Recording it at the claim would
/// tell the agent the user got what they approved; here, one of the two files was skipped, and
/// the lesson has to say so or the agent learns from a claim rather than from what happened.
#[test]
fn what_the_agent_learns_from_an_approval_is_what_actually_ran() {
    let dir = TestDir::new("bridge_learns");
    let (_conn, reporting, group_id, ops) = fixture(&dir);
    let memory_root = dir.join("memory");
    std::fs::create_dir_all(&memory_root).expect("memory root");
    let memory = crate::agent::memory::MemoryStore::new(&memory_root);
    let collector = Arc::new(CollectorEventSink::new());
    let op_ids: HashMap<PathBuf, i64> = ops.iter().map(|op| (PathBuf::from(&op.source_path), op.id)).collect();
    let sink = ProposalReportingSink::new(collector, group_id, op_ids, reporting, Some(memory));

    sink.emit_source_item_done(done("/Users/someone/Downloads/one.dmg", SourceItemOutcome::Done));
    sink.emit_source_item_done(done("/Users/someone/Downloads/two.dmg", SourceItemOutcome::Skipped));
    sink.emit_settled(settled());

    let learned = std::fs::read_to_string(memory_root.join(crate::agent::memory::OUTCOMES_FILE)).expect("the ring");
    assert!(learned.contains("approved: trash"), "{learned:?}");
    assert!(
        learned.contains("1 done, 1 skipped, 0 failed"),
        "the lesson is what ran, not what was claimed: {learned:?}"
    );
}

/// The binding is a LIVE capture, and this is the difference that makes: it holds the file as
/// it was while the user was deciding, at nanosecond precision, so a rewrite in the window
/// between the review and the operation getting its lane is caught. The stored creation
/// snapshot could not do this — whole seconds, and no device.
#[tokio::test]
async fn the_binding_holds_what_preflight_saw_not_what_the_agent_saw() {
    let dir = TestDir::new("bridge_live_binding");
    let reviewed = dir.join("reviewed.dmg");
    std::fs::write(&reviewed, b"as reviewed").expect("seed");

    let volume = crate::file_system::volume::LocalPosixVolume::new("Root", "/");
    let sources = vec![reviewed.clone()];
    let expected = super::capture_expected_sources(&volume, &sources).await;

    // Somebody rewrites it between the review and the operation getting its turn.
    std::fs::write(&reviewed, b"edited while it waited in the queue").expect("rewrite");

    let sink = CollectorEventSink::new();
    let kept = crate::file_system::write_operations::retain_bound_sources(
        &sink,
        "op-1",
        WriteOperationType::Trash,
        Some(&expected),
        sources,
    );

    assert!(kept.is_none(), "the file the user approved is not the file on disk now");
}

/// The other half: an untouched source survives, so the binding is not simply refusing
/// everything.
#[tokio::test]
async fn an_untouched_source_survives_its_own_binding() {
    let dir = TestDir::new("bridge_live_binding_ok");
    let reviewed = dir.join("reviewed.dmg");
    std::fs::write(&reviewed, b"as reviewed").expect("seed");

    let volume = crate::file_system::volume::LocalPosixVolume::new("Root", "/");
    let sources = vec![reviewed.clone()];
    let expected = super::capture_expected_sources(&volume, &sources).await;

    let sink = CollectorEventSink::new();
    let kept = crate::file_system::write_operations::retain_bound_sources(
        &sink,
        "op-1",
        WriteOperationType::Trash,
        Some(&expected),
        sources.clone(),
    );

    assert_eq!(kept, Some(sources));
}

/// A source that vanished between the proposal and the review gets no fingerprint, and the
/// binding drops what it cannot name. Skipping it is the honest answer, and it is the same
/// answer every other unverifiable source gets.
#[tokio::test]
async fn a_source_that_vanished_before_preflight_is_left_out_of_the_binding() {
    let dir = TestDir::new("bridge_live_binding_gone");
    let gone = dir.join("gone.dmg");

    let volume = crate::file_system::volume::LocalPosixVolume::new("Root", "/");
    let expected = super::capture_expected_sources(&volume, std::slice::from_ref(&gone)).await;

    let sink = CollectorEventSink::new();
    let kept = crate::file_system::write_operations::retain_bound_sources(
        &sink,
        "op-1",
        WriteOperationType::Trash,
        Some(&expected),
        vec![gone],
    );

    assert!(kept.is_none());
}

/// Registers the local-FS "root" volume the bridge resolves a group's sources through.
fn ensure_root_volume() {
    use crate::file_system::volume::LocalPosixVolume;
    use crate::file_system::volume::manager::get_volume_manager;
    get_volume_manager().register_if_absent("root", Arc::new(LocalPosixVolume::new("Test root", "/")));
}

/// A rename group whose files are all gone by the time the user approves it: preflight reads
/// nothing, so the executor has no row to run.
fn vanished_rename_group(dir: &TestDir) -> (Connection, Connection, i64) {
    let db_path = dir.join("main.db");
    let conn = open_write_connection(&db_path).expect("open");
    let set_id = create_sweep(&conn, &NewSweep::default(), 100).expect("sweep");
    let parent = dir.join("shots").display().to_string();
    let group_id = create_group(
        &conn,
        set_id,
        &NewGroup {
            intent: GroupIntent::Rename {
                parent: parent.clone(),
                renames: vec![crate::agent::store::proposals::NewRename {
                    source_path: format!("{parent}/gone.png"),
                    new_name: "renamed.png".to_string(),
                    snapshot: None,
                }],
            },
            source_volume_id: "root".to_string(),
            display_name: "one screenshot".to_string(),
            rationale: None,
            selector: None,
        },
        100,
    )
    .expect("group");
    let reporting = open_write_connection(&db_path).expect("second connection");
    (conn, reporting, group_id)
}

/// ❗ **A refused approval gives the group back.** The claim moves it to `approved` before the
/// engine is asked, so an engine that then refuses used to leave it approved with nothing
/// running: it vanished from the review list without a word (#184). Nothing ran, so the user
/// is back where they were, and the group is theirs to approve or reject again.
#[tokio::test]
async fn an_approval_the_engine_refuses_returns_the_group_to_pending() {
    ensure_root_volume();
    let dir = TestDir::new("bridge_refused_gives_back");
    let (conn, reporting, group_id) = vanished_rename_group(&dir);

    let outcome = super::approve_and_execute(
        &conn,
        reporting,
        Arc::new(CollectorEventSink::new()),
        group_id,
        &[],
        200,
        None,
    )
    .await
    .expect("the store answers");

    assert!(
        matches!(outcome, super::ApprovalOutcome::Refused(_)),
        "nothing could start: {outcome:?}"
    );
    assert_eq!(
        group_status(&conn, group_id),
        ProposalStatus::Pending,
        "the refused group is back on the list"
    );
}

/// An approved rename group renames its file. A rename binds only its parent folder and stores
/// no destination volume, so a target check that wanted one refused every rename group the
/// review dialog approved.
#[tokio::test]
async fn an_approved_rename_group_renames_its_file() {
    ensure_root_volume();
    let dir = TestDir::new("bridge_rename_runs");
    let (conn, reporting, group_id) = vanished_rename_group(&dir);
    std::fs::create_dir_all(dir.join("shots")).expect("parent");
    std::fs::write(dir.join("shots").join("gone.png"), b"here after all").expect("seed");

    let outcome = approve(&conn, reporting, group_id).await;

    assert!(matches!(outcome, super::ApprovalOutcome::Started(_)), "{outcome:?}");
    let renamed = dir.join("shots").join("renamed.png");
    crate::test_support::wait_until_async(std::time::Duration::from_secs(5), "the rename lands", || {
        renamed.exists()
    })
    .await;
}

/// ❗ **An approval is counted when its operation STARTS, never at the claim.** A refused start
/// gives the group back, so counting the claim would count an approval nothing came of, and
/// count the same group again when the user re-approves it. The acceptance-rate metric then
/// reads higher than what users actually let run.
#[tokio::test]
async fn an_approval_is_counted_once_when_it_starts_and_not_when_the_engine_refuses_it() {
    ensure_root_volume();
    let dir = TestDir::new("bridge_counted_at_start");
    let (conn, reporting, group_id) = vanished_rename_group(&dir);
    super::super::analytics::take_captured();

    let refused = approve(&conn, reporting, group_id).await;
    assert!(matches!(refused, super::ApprovalOutcome::Refused(_)), "{refused:?}");
    assert_eq!(
        super::super::analytics::take_captured(),
        Vec::<String>::new(),
        "nothing ran, so nothing was approved"
    );

    // The file turns up, and the user approves the same group again.
    std::fs::create_dir_all(dir.join("shots")).expect("parent");
    std::fs::write(dir.join("shots").join("gone.png"), b"back").expect("seed");
    let reporting = open_write_connection(&dir.join("main.db")).expect("second connection");
    let started = approve(&conn, reporting, group_id).await;
    assert!(matches!(started, super::ApprovalOutcome::Started(_)), "{started:?}");

    assert_eq!(
        super::super::analytics::take_captured(),
        vec!["suggestion_group_approved".to_string()],
        "one group ran, so one approval"
    );
}

/// A trash group on `volume_id`, the shape the source-volume check sees before any claim.
fn trash_group_on(dir: &TestDir, volume_id: &str) -> (Connection, Connection, i64) {
    let db_path = dir.join("main.db");
    let conn = open_write_connection(&db_path).expect("open");
    let set_id = create_sweep(&conn, &NewSweep::default(), 100).expect("sweep");
    let group_id = create_group(
        &conn,
        set_id,
        &NewGroup {
            intent: GroupIntent::Trash {
                sources: vec![NewOp {
                    source_path: "/public/old.iso".to_string(),
                    snapshot: None,
                }],
            },
            source_volume_id: volume_id.to_string(),
            display_name: "one disk image".to_string(),
            rationale: None,
            selector: None,
        },
        100,
    )
    .expect("group");
    let reporting = open_write_connection(&db_path).expect("second connection");
    (conn, reporting, group_id)
}

async fn approve(conn: &Connection, reporting: Connection, group_id: i64) -> super::ApprovalOutcome {
    super::approve_and_execute(
        conn,
        reporting,
        Arc::new(CollectorEventSink::new()),
        group_id,
        &[],
        200,
        None,
    )
    .await
    .expect("the store answers")
}

/// A suggestion about a saved share nobody has mounted is refused as NOT CONNECTED, the
/// transfer dialogs' own variant, so the dialog says "open it from the volume switcher" rather
/// than "it's gone". It was refused before the claim, so the group never left `pending`.
#[tokio::test]
async fn approving_a_group_on_a_share_nobody_connected_is_refused_as_not_connected() {
    use crate::network::known_shares::{self, AuthOptions, ConnectionMode, KnownNetworkShare};
    let volume_id = "smb-198-51-100-62-11483-public-bridge-test";
    known_shares::remember_share(KnownNetworkShare {
        server_name: "198.51.100.62:11483".to_string(),
        share_name: "public".to_string(),
        protocol: "smb".to_string(),
        last_connected_at: "2026-10-06T00:00:00Z".to_string(),
        last_connection_mode: ConnectionMode::Guest,
        last_known_auth_options: AuthOptions::GuestOrCredentials,
        username: None,
        address: Some("198.51.100.62".to_string()),
        port: Some(11483),
        volume_id: Some(volume_id.to_string()),
        mount_path: Some("/Volumes/public-bridge-test".to_string()),
        pinned: true,
    });
    let dir = TestDir::new("bridge_not_connected");
    let (conn, reporting, group_id) = trash_group_on(&dir, volume_id);

    let outcome = approve(&conn, reporting, group_id).await;

    assert!(
        matches!(
            outcome,
            super::ApprovalOutcome::Refused(super::ApprovalRefusal::SourceUnavailable(
                crate::file_system::write_operations::WriteOperationError::SourceNotConnected { .. }
            ))
        ),
        "{outcome:?}"
    );
    assert_eq!(group_status(&conn, group_id), ProposalStatus::Pending);
}

/// An id nothing lists or saves is a drive that LEFT, worded as no longer connected.
#[tokio::test]
async fn approving_a_group_on_a_volume_that_left_is_refused_as_no_longer_connected() {
    let dir = TestDir::new("bridge_volume_left");
    let (conn, reporting, group_id) = trash_group_on(&dir, "mtp-nothing-lists-this-bridge-test");

    let outcome = approve(&conn, reporting, group_id).await;

    assert!(
        matches!(
            outcome,
            super::ApprovalOutcome::Refused(super::ApprovalRefusal::SourceUnavailable(
                crate::file_system::write_operations::WriteOperationError::SourceNoLongerConnected { .. }
            ))
        ),
        "{outcome:?}"
    );
    assert_eq!(group_status(&conn, group_id), ProposalStatus::Pending);
}
