//! Tests for the `cmdr://state` `operations:` section builder (the two-source
//! join of registry membership/status and the live progress cache).

use crate::mcp::resources::operations::{OperationRow, build_operations_yaml};
use cmdr_operations::testing::{SourcesGoneNext, TransferWaitReason};
use cmdr_operations::{
    ConflictId, KnownSize, LifecycleStatus, OperationSnapshot, OperationStatus, TransferActivity, WriteConflictEvent,
    WriteLockedSourcesEvent, WriteOperationError, WriteOperationPhase, WriteOperationType, WriteSourcesGoneEvent,
};

fn clash(conflict_id: u64, destination_size: Option<KnownSize>) -> WriteConflictEvent {
    WriteConflictEvent {
        operation_id: "op-1".to_string(),
        conflict_id: ConflictId(conflict_id),
        source_path: "/src/photos/dsc-1.raw".to_string(),
        destination_path: "/dst/photos/dsc-1.raw".to_string(),
        source_size: Some(KnownSize::Exact { bytes: 2_048 }),
        destination_size,
        source_modified: Some(1_700_000_000),
        destination_modified: Some(1_700_000_500),
        destination_is_newer: true,
        size_difference: None,
        source_is_directory: false,
        destination_is_directory: false,
        destination_is_look_alike: false,
    }
}

fn snapshot(id: &str, status: LifecycleStatus) -> OperationSnapshot {
    OperationSnapshot {
        operation_id: id.to_string(),
        operation_type: WriteOperationType::Copy,
        status,
        source: Some("/src/photos".to_string()),
        destination: Some("/dst/photos".to_string()),
        rollback_support: cmdr_operations::testing::RollbackSupport::Supported,
        reverses: None,
        error: None,
        can_retry: false,
        items: None,
    }
}

/// The status-cache half of a row. `status` must match the [`snapshot`] it is
/// paired with: both describe the same operation, and a fixture where they
/// disagree would assert against a state the backend can't produce.
fn progress(
    id: &str,
    status: LifecycleStatus,
    bytes_done: u64,
    bytes_total: u64,
    files_done: usize,
    files_total: usize,
) -> OperationStatus {
    OperationStatus {
        operation_id: id.to_string(),
        operation_type: WriteOperationType::Copy,
        phase: WriteOperationPhase::Copying,
        lifecycle: Some(status),
        current_file: Some("photo.jpg".to_string()),
        files_done,
        files_total,
        bytes_done,
        bytes_total,
        started_at: 10_000,
        activity: None,
    }
}

/// The same snapshot, carrying the live wait the backend classified.
fn waiting(
    mut op: OperationStatus,
    waiting_on: TransferWaitReason,
    still_for_seconds: u32,
    in_flight: u32,
) -> OperationStatus {
    op.activity = Some(TransferActivity {
        in_flight,
        still_for_seconds,
        waiting_on,
        opening_source: false,
        source_inbound_bytes_per_second: None,
        reconnecting_volume: None,
    });
    op
}

#[test]
fn empty_operations_render_as_empty_list() {
    assert_eq!(build_operations_yaml(&[], 20_000), "operations: []\n");
}

#[test]
fn running_op_shows_status_progress_speed_and_eta() {
    // 4 s elapsed, 200 MB of 1 GB done → 50 MB/s, ETA (824 MB at 50 MB/s) = 16 s
    let mb = 1_024 * 1_024;
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: Some(progress("op-1", LifecycleStatus::Running, 200 * mb, 1_024 * mb, 3, 10)),
        pending_conflict: None,
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }];
    let yaml = build_operations_yaml(&rows, 14_000);
    assert!(yaml.contains("- operationId: op-1"), "yaml: {yaml}");
    assert!(yaml.contains("type: copy"), "yaml: {yaml}");
    assert!(yaml.contains("status: running"), "yaml: {yaml}");
    assert!(
        yaml.contains("progress: 200 MiB / 1 GiB (19%), 3/10 files"),
        "yaml: {yaml}"
    );
    assert!(yaml.contains("currentFile: \"photo.jpg\""), "yaml: {yaml}");
    assert!(yaml.contains("speed: 50 MiB/s"), "yaml: {yaml}");
    assert!(yaml.contains("etaSeconds: 16"), "yaml: {yaml}");
    assert!(yaml.contains("elapsedSeconds: 4"), "yaml: {yaml}");
}

#[test]
fn operation_summaries_and_current_file_render_verbatim() {
    // An agent answering "what is Cmdr copying right now?" has to be able to name
    // it: `redact_line` would turn these into `<file>.pdf` and `<dir>`.
    let mut operation = snapshot("op-remote", LifecycleStatus::Running);
    operation.source = Some("Ada Lovelace taxes 2025.pdf (3 items)".to_string());
    operation.destination = Some("ada-nas.local".to_string());
    let mut live = progress("op-remote", LifecycleStatus::Running, 0, 0, 0, 0);
    live.current_file = Some("/Users/ada/Documents/Ada Lovelace taxes 2025.pdf".to_string());
    let yaml = build_operations_yaml(
        &[OperationRow {
            snapshot: operation,
            progress: Some(live),
            pending_conflict: None,
            tree: None,
            pending_sources_gone: None,
            pending_locked_sources: None,
        }],
        10_500,
    );

    assert_eq!(
        yaml,
        concat!(
            "operations:\n",
            "  - operationId: op-remote\n",
            "    type: copy\n",
            "    status: running\n",
            "    source: \"Ada Lovelace taxes 2025.pdf (3 items)\"\n",
            "    destination: \"ada-nas.local\"\n",
            "    progress: scanning\n",
            "    currentFile: \"/Users/ada/Documents/Ada Lovelace taxes 2025.pdf\"\n",
            "    elapsedSeconds: 0\n",
        )
    );
}

#[test]
fn paused_op_keeps_its_progress_but_reports_paused() {
    let mb = 1_024 * 1_024;
    let rows = vec![OperationRow {
        snapshot: snapshot("op-2", LifecycleStatus::Paused),
        progress: Some(progress("op-2", LifecycleStatus::Paused, 100 * mb, 1_024 * mb, 1, 10)),
        pending_conflict: None,
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("- operationId: op-2"), "yaml: {yaml}");
    assert!(yaml.contains("status: paused"), "yaml: {yaml}");
    assert!(yaml.contains("progress: 100 MiB / 1 GiB"), "yaml: {yaml}");
}

#[test]
fn queued_op_has_status_but_no_progress_fields() {
    // A queued op has no status-cache entry, so progress is None: status only.
    let rows = vec![OperationRow {
        snapshot: snapshot("op-3", LifecycleStatus::Queued),
        progress: None,
        pending_conflict: None,
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("- operationId: op-3"), "yaml: {yaml}");
    assert!(yaml.contains("status: queued"), "yaml: {yaml}");
    assert!(
        !yaml.contains("progress:"),
        "queued op must have no progress line: {yaml}"
    );
    assert!(!yaml.contains("etaSeconds"), "yaml: {yaml}");
}

#[test]
fn a_running_paused_queued_mix_renders_every_row() {
    let mb = 1_024 * 1_024;
    let rows = vec![
        OperationRow {
            snapshot: snapshot("op-run", LifecycleStatus::Running),
            progress: Some(progress("op-run", LifecycleStatus::Running, 50 * mb, 100 * mb, 2, 4)),
            pending_conflict: None,
            tree: None,
            pending_sources_gone: None,
            pending_locked_sources: None,
        },
        OperationRow {
            snapshot: snapshot("op-pause", LifecycleStatus::Paused),
            progress: Some(progress("op-pause", LifecycleStatus::Paused, 10 * mb, 100 * mb, 1, 4)),
            pending_conflict: None,
            tree: None,
            pending_sources_gone: None,
            pending_locked_sources: None,
        },
        OperationRow {
            snapshot: snapshot("op-queue", LifecycleStatus::Queued),
            progress: None,
            pending_conflict: None,
            tree: None,
            pending_sources_gone: None,
            pending_locked_sources: None,
        },
    ];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("operationId: op-run"), "yaml: {yaml}");
    assert!(yaml.contains("operationId: op-pause"), "yaml: {yaml}");
    assert!(yaml.contains("operationId: op-queue"), "yaml: {yaml}");
    assert!(yaml.contains("status: running"), "yaml: {yaml}");
    assert!(yaml.contains("status: paused"), "yaml: {yaml}");
    assert!(yaml.contains("status: queued"), "yaml: {yaml}");
}

#[test]
fn an_operation_parked_on_a_clash_says_which_clash_and_how_to_answer_it() {
    // The reason this block exists: the row still says `running` and its
    // counters have stopped moving. Without the clash on the row an agent can
    // see a stuck operation and has no way to learn what it's waiting for, or
    // which id an answer has to name.
    let mb = 1_024 * 1_024;
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: Some(progress("op-1", LifecycleStatus::Running, 50 * mb, 100 * mb, 2, 4)),
        pending_conflict: Some(clash(3, Some(KnownSize::Exact { bytes: 4_096 }))),
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("pendingConflict:"), "yaml: {yaml}");
    assert!(yaml.contains("conflictId: 3"), "yaml: {yaml}");
    // Verbatim: an agent choosing between skip and overwrite has to know which
    // file it is deciding about, and `redact_line` would leave it `<file>.raw`.
    assert!(yaml.contains("source: \"/src/photos/dsc-1.raw\""), "yaml: {yaml}");
    assert!(yaml.contains("destination: \"/dst/photos/dsc-1.raw\""), "yaml: {yaml}");
    assert!(yaml.contains("destinationIsNewer: true"), "yaml: {yaml}");
    assert!(yaml.contains("answerWith: resolve_conflict"), "yaml: {yaml}");
    assert!(yaml.contains("destinationIsLookAlike: false"), "yaml: {yaml}");
}

#[test]
fn a_look_alike_clash_says_the_names_only_look_the_same() {
    // The two paths print identically, so without the flag an agent can't tell
    // that an overwrite lands on an entry spelled another way.
    let mut conflict = clash(4, Some(KnownSize::Exact { bytes: 4_096 }));
    conflict.source_path = "/src/cafe\u{301}.txt".to_string();
    conflict.destination_path = "/dst/caf\u{e9}.txt".to_string();
    conflict.destination_is_look_alike = true;
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: None,
        pending_conflict: Some(conflict),
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("destinationIsLookAlike: true"), "yaml: {yaml}");
}

#[test]
fn a_size_the_backend_could_not_read_says_unknown_rather_than_zero() {
    // A folder outside the index has no destination size. Rendering `0 B` would
    // read as "the destination is empty, overwriting costs nothing".
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: None,
        pending_conflict: Some(clash(1, None)),
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("destinationSize: unknown"), "yaml: {yaml}");
    assert!(!yaml.contains("destinationSize: 0 B"), "yaml: {yaml}");
}

#[test]
fn a_partly_counted_folder_size_reads_as_a_floor() {
    // An agent deciding whether to overwrite a folder must not read a partial
    // count as its size: 12 KB counted of a 40 GB tree (#528).
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: None,
        pending_conflict: Some(clash(1, Some(KnownSize::AtLeast { bytes: 12_288 }))),
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("destinationSize: ≥"), "yaml: {yaml}");
}

#[test]
fn a_queued_operation_asking_about_sources_that_left_says_how_many_and_how_to_answer() {
    // The row says `running` and nothing moves: without this block an agent sees
    // a stuck move and can't learn that it waits on a question, let alone answer
    // it.
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: None,
        pending_conflict: None,
        tree: None,
        pending_sources_gone: Some(WriteSourcesGoneEvent {
            operation_id: "op-1".to_string(),
            operation_type: WriteOperationType::Move,
            total_count: 22,
            gone_count: 4,
            gone_sample: vec!["/src/cd1/01.mp3".to_string()],
            next: SourcesGoneNext::AwaitingAnswer,
        }),
        pending_locked_sources: None,
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("pendingSourcesGone:"), "yaml: {yaml}");
    assert!(yaml.contains("goneCount: 4"), "yaml: {yaml}");
    assert!(yaml.contains("totalCount: 22"), "yaml: {yaml}");
    assert!(yaml.contains("\"/src/cd1/01.mp3\""), "yaml: {yaml}");
    assert!(yaml.contains("answerWith: queue"), "yaml: {yaml}");
    assert!(yaml.contains("continue_without_gone"), "yaml: {yaml}");
}

#[test]
fn a_move_asking_about_locked_items_says_how_many_and_how_to_answer() {
    // Parked before any byte moves, so the row reads `running` with nothing
    // moving: the block is how an agent learns it waits on a question.
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: None,
        pending_conflict: None,
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: Some(WriteLockedSourcesEvent {
            operation_id: "op-1".to_string(),
            operation_type: WriteOperationType::Move,
            locked_count: 3,
            locked_sample: vec!["/src/contract.pdf".to_string()],
        }),
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("pendingLockedSources:"), "yaml: {yaml}");
    assert!(yaml.contains("lockedCount: 3"), "yaml: {yaml}");
    assert!(yaml.contains("\"/src/contract.pdf\""), "yaml: {yaml}");
    assert!(
        yaml.contains("move_locked") && yaml.contains("skip_locked"),
        "yaml: {yaml}"
    );
}

#[test]
fn a_retained_failure_says_why_it_stopped() {
    // A `failed` row with no reason leaves an agent guessing between a vanished
    // source, a full disk, and a permission refusal — three completely different
    // next moves. The typed variant name, ❌ never a sentence.
    let mut failed = snapshot("op-1", LifecycleStatus::Failed);
    failed.error = Some(WriteOperationError::SourceNotFound {
        path: "/src/photos/gone.raw".to_string(),
    });
    let rows = vec![OperationRow {
        snapshot: failed,
        progress: None,
        pending_conflict: None,
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }];
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("status: failed"), "yaml: {yaml}");
    assert!(yaml.contains("error: source_not_found"), "yaml: {yaml}");
}

#[test]
fn a_live_operation_carries_no_error_line() {
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: None,
        pending_conflict: None,
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }];
    assert!(!build_operations_yaml(&rows, 12_000).contains("error:"));
}

#[test]
fn an_operation_that_is_not_asking_anything_carries_no_conflict_block() {
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: None,
        pending_conflict: None,
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }];
    assert!(!build_operations_yaml(&rows, 12_000).contains("pendingConflict"));
}

// ---- the wait, on the row (why isn't this moving?) ----

/// A `running` row with frozen counters, for the wait tests below.
fn stalled_row(waiting_on: TransferWaitReason, still_for_seconds: u32, in_flight: u32) -> Vec<OperationRow> {
    let mb = 1_024 * 1_024;
    vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: Some(waiting(
            progress("op-1", LifecycleStatus::Running, 50 * mb, 100 * mb, 2, 4),
            waiting_on,
            still_for_seconds,
            in_flight,
        )),
        pending_conflict: None,
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }]
}

#[test]
fn a_transfer_parked_on_the_destination_says_so_and_says_for_how_long() {
    // Without the wait an agent can't tell a busy share from a slow copy, and
    // both look exactly like a dead mount.
    let yaml = build_operations_yaml(&stalled_row(TransferWaitReason::Destination, 47, 4), 12_000);
    assert!(yaml.contains("waitingOn: destination"), "yaml: {yaml}");
    assert!(yaml.contains("stillForSeconds: 47"), "yaml: {yaml}");
    assert!(yaml.contains("inFlight: 4"), "yaml: {yaml}");
}

#[test]
fn a_wedge_reads_as_unknown_rather_than_as_something_explained() {
    // The 2026-07-31 shape: nothing is moving and no task explains why. It has to
    // stay distinguishable from every wait that DOES have a reason, because it's
    // the only one an agent should escalate on.
    let yaml = build_operations_yaml(&stalled_row(TransferWaitReason::Unknown, 310, 2), 12_000);
    assert!(yaml.contains("waitingOn: unknown"), "yaml: {yaml}");
    assert!(yaml.contains("stillForSeconds: 310"), "yaml: {yaml}");
}

#[test]
fn a_slow_open_says_the_source_is_still_sending_rather_than_looking_wedged() {
    // ERR-CNK7M from an agent's side: `waitingOn: source` for 18 s reads like a
    // wedge until it also says nothing has landed yet and bytes are arriving.
    let mut rows = stalled_row(TransferWaitReason::Source, 18, 1);
    let activity = rows[0]
        .progress
        .as_mut()
        .and_then(|p| p.activity.as_mut())
        .expect("the row carries an activity");
    activity.opening_source = true;
    activity.source_inbound_bytes_per_second = Some(19_000);

    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("openingSource: true"), "yaml: {yaml}");
    assert!(yaml.contains("sourceInboundBytesPerSecond: 19000"), "yaml: {yaml}");
}

#[test]
fn a_moving_transfer_says_it_is_moving_and_claims_no_stillness() {
    let yaml = build_operations_yaml(&stalled_row(TransferWaitReason::Moving, 0, 6), 12_000);
    assert!(yaml.contains("waitingOn: moving"), "yaml: {yaml}");
    assert!(
        !yaml.contains("stillForSeconds"),
        "zero stillness is noise, not a finding: {yaml}"
    );
    assert!(!yaml.contains("openingSource"), "yaml: {yaml}");
    assert!(!yaml.contains("sourceInbound"), "yaml: {yaml}");
}

#[test]
fn an_operation_parked_on_a_clash_names_the_wait_and_the_clash_together() {
    // `waitingOn: conflict` says there IS a question; `pendingConflict` says
    // which one, and carries the `conflictId` an answer has to name. One read,
    // both halves.
    let mut rows = stalled_row(TransferWaitReason::Conflict, 0, 0);
    rows[0].pending_conflict = Some(clash(7, Some(KnownSize::Exact { bytes: 4_096 })));
    let yaml = build_operations_yaml(&rows, 12_000);
    assert!(yaml.contains("waitingOn: conflict"), "yaml: {yaml}");
    assert!(yaml.contains("conflictId: 7"), "yaml: {yaml}");
    assert!(yaml.contains("answerWith: resolve_conflict"), "yaml: {yaml}");
}

#[test]
fn a_backend_that_cannot_classify_its_wait_says_nothing_rather_than_moving() {
    // A local copy keeps no in-flight table. Rendering `waitingOn: moving` for it
    // would be an invention an agent would act on.
    let mb = 1_024 * 1_024;
    let rows = vec![OperationRow {
        snapshot: snapshot("op-1", LifecycleStatus::Running),
        progress: Some(progress("op-1", LifecycleStatus::Running, 50 * mb, 100 * mb, 2, 4)),
        pending_conflict: None,
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }];
    assert!(!build_operations_yaml(&rows, 12_000).contains("waitingOn"));
}

#[test]
fn scanning_op_has_no_bogus_numbers() {
    // Totals unknown during scanning: no percent, no ETA, no "0 B / 0 B".
    let mut scanning = progress("op-scan", LifecycleStatus::Running, 0, 0, 0, 0);
    scanning.phase = WriteOperationPhase::Scanning;
    scanning.current_file = None;
    let rows = vec![OperationRow {
        snapshot: snapshot("op-scan", LifecycleStatus::Running),
        progress: Some(scanning),
        pending_conflict: None,
        tree: None,
        pending_sources_gone: None,
        pending_locked_sources: None,
    }];
    let yaml = build_operations_yaml(&rows, 10_500);
    assert!(yaml.contains("progress: scanning"), "yaml: {yaml}");
    assert!(!yaml.contains("etaSeconds"), "yaml: {yaml}");
    assert!(!yaml.contains("speed"), "yaml: {yaml}");
    assert!(!yaml.contains("currentFile"), "yaml: {yaml}");
}
