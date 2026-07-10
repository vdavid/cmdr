//! Retention tests: age + size pruning of whole operations, the interned-dir GC,
//! the rolled-back-pair and rolling-back invariants, and the tiered vacuum policy.
//! Split out of `writer/tests.rs` (which keeps the open/record/finalize round-trip
//! and the M3 rollback reads) to keep each test file under the length budget.

use super::tests::fresh;
use super::*;
use crate::operation_log::store::{operation_log_db_path, read_operation, read_operation_items};

/// Journal a whole op of `item_count` file rows under a per-op directory, so each
/// op both adds bulk and interns a unique dir. `ended_at = op_index` orders ops by
/// age (lower = older).
fn journal_bulk_op(writer: &OperationLogWriter, op_index: i64, item_count: i64) {
    let op_id = format!("op-{op_index}");
    let dir = format!("/vol/tree-{op_index}");
    writer
        .open_operation(OpenOperation {
            op_id: op_id.clone(),
            kind: OpKind::Delete,
            initiator: Initiator::User,
            source_volume_id: Some("vol-1".to_string()),
            dest_volume_id: None,
            item_count: item_count as u64,
            started_at: op_index,
            rolls_back_op_id: None,
            execution_status: ExecutionStatus::Running,
        })
        .expect("open");
    let items: Vec<JournalItem> = (0..item_count)
        .map(|seq| JournalItem {
            seq,
            entry_type: EntryType::File,
            row_role: RowRole::RollbackUnit,
            source_volume_id: "vol-1".to_string(),
            source_dir: dir.clone(),
            // A longish, unique name to give the DB measurable bulk.
            source_name: format!("file-{op_index:04}-{seq:06}-payload-padding.dat"),
            dest_volume_id: None,
            dest_dir: None,
            dest_name: None,
            size: Some(seq),
            mtime: Some(1_700_000_000 + seq),
            outcome: ItemOutcome::Done,
            overwrote: false,
        })
        .collect();
    writer.record_items(&op_id, items).expect("record");
    writer
        .finalize_operation(FinalizeOperation {
            op_id,
            execution_status: ExecutionStatus::Done,
            rollback_state: RollbackState::NotRollbackable,
            not_rollbackable_reason: Some(NotRollbackableReason::PermanentDelete),
            archive_subkind: None,
            search_coverage: SearchCoverage::Full,
            search_coverage_reason: None,
            ended_at: op_index,
            item_count: None,
            items_done: item_count as u64,
            bytes_total: 0,
            dev_summary: None,
        })
        .expect("finalize");
}

/// Retention prunes whole operations older than the age cutoff, GCs the interned
/// dirs that only the pruned op referenced, and keeps a recent op (and its dirs)
/// intact. The mechanism M4 will wire to a periodic timer + size budget.
#[test]
fn prune_removes_old_operation_and_gcs_its_dirs() {
    let (store, writer, _dir) = fresh();

    // An old op (ended long ago) touching /shared/oldtree, and a recent op
    // touching /shared/recenttree. The GC must drop /shared/oldtree but keep
    // /shared — it's the ancestor chain of the still-referenced recent dir, and
    // path reconstruction walks it.
    for (op_id, dir, ended) in [
        ("old", "/shared/oldtree", 1_000i64),
        ("recent", "/shared/recenttree", 9_000i64),
    ] {
        writer
            .open_operation(OpenOperation {
                op_id: op_id.to_string(),
                kind: OpKind::Delete,
                initiator: Initiator::User,
                source_volume_id: Some("vol-1".to_string()),
                dest_volume_id: None,
                item_count: 1,
                started_at: ended - 1,
                rolls_back_op_id: None,
                execution_status: ExecutionStatus::Running,
            })
            .expect("open");
        writer
            .record_items(
                op_id,
                vec![JournalItem {
                    seq: 0,
                    entry_type: EntryType::File,
                    row_role: RowRole::RollbackUnit,
                    source_volume_id: "vol-1".to_string(),
                    source_dir: dir.to_string(),
                    source_name: "leaf".to_string(),
                    dest_volume_id: None,
                    dest_dir: None,
                    dest_name: None,
                    size: Some(1),
                    mtime: None,
                    outcome: ItemOutcome::Done,
                    overwrote: false,
                }],
            )
            .expect("record");
        writer
            .finalize_operation(FinalizeOperation {
                op_id: op_id.to_string(),
                execution_status: ExecutionStatus::Done,
                rollback_state: RollbackState::NotRollbackable,
                not_rollbackable_reason: Some(NotRollbackableReason::PermanentDelete),
                archive_subkind: None,
                search_coverage: SearchCoverage::Full,
                search_coverage_reason: None,
                ended_at: ended,
                item_count: None,
                items_done: 1,
                bytes_total: 1,
                dev_summary: None,
            })
            .expect("finalize");
    }
    writer.flush_blocking().expect("flush");

    let dir_count = |folded: &str| -> i64 {
        store
            .conn()
            .query_row(
                "SELECT COUNT(*) FROM dirs WHERE name_folded = ?1",
                rusqlite::params![folded],
                |row| row.get(0),
            )
            .expect("count")
    };
    assert_eq!(dir_count("oldtree"), 1, "oldtree exists before prune");
    assert_eq!(dir_count("recenttree"), 1);
    assert_eq!(dir_count("shared"), 1, "the shared ancestor exists before prune");

    // Prune everything that ended before now(10_000) - age(5_000) = 5_000: the
    // old op (ended 1_000) goes, the recent op (ended 9_000) stays.
    writer
        .prune(PruneRequest {
            max_age_secs: Some(5_000),
            max_size_bytes: None,
            now_secs: 10_000,
            vacuum: true,
        })
        .expect("prune");
    writer.flush_blocking().expect("flush");

    assert!(
        read_operation(store.conn(), "old").expect("read").is_none(),
        "old op pruned"
    );
    assert!(
        read_operation(store.conn(), "recent").expect("read").is_some(),
        "recent op kept"
    );
    assert_eq!(dir_count("oldtree"), 0, "the old op's unique dir is GC'd");
    assert_eq!(dir_count("recenttree"), 1, "the recent op's dir survives");
    assert_eq!(
        dir_count("shared"),
        1,
        "the shared ancestor survives — it's still on a referenced dir's parent chain"
    );

    writer.shutdown();
}

/// Pruning an op that a SURVIVING op references via `rolls_back_op_id` nulls the
/// survivor's link instead of tripping the self-FK on delete. Guards the
/// ordering: null-before-delete. (M4 expands retention tests; this one defends
/// the FK trap the M3 rollback linkage will exercise.)
#[test]
fn prune_nulls_a_survivors_rollback_link_to_a_pruned_op() {
    let (store, writer, _dir) = fresh();

    // An old original op, and a recent inverse op that rolled it back.
    let open = |op_id: &str, ended: i64, rolls_back: Option<String>| OpenOperation {
        op_id: op_id.to_string(),
        kind: OpKind::Copy,
        initiator: Initiator::User,
        source_volume_id: Some("vol-1".to_string()),
        dest_volume_id: Some("vol-1".to_string()),
        item_count: 0,
        started_at: ended - 1,
        rolls_back_op_id: rolls_back,
        execution_status: ExecutionStatus::Running,
    };
    let finalize = |op_id: &str, ended: i64| FinalizeOperation {
        op_id: op_id.to_string(),
        execution_status: ExecutionStatus::Done,
        rollback_state: RollbackState::RolledBack,
        not_rollbackable_reason: None,
        archive_subkind: None,
        search_coverage: SearchCoverage::Full,
        search_coverage_reason: None,
        ended_at: ended,
        item_count: None,
        items_done: 0,
        bytes_total: 0,
        dev_summary: None,
    };
    writer.open_operation(open("original", 1_000, None)).expect("open orig");
    writer
        .finalize_operation(finalize("original", 1_000))
        .expect("finalize orig");
    writer
        .open_operation(open("inverse", 9_000, Some("original".to_string())))
        .expect("open inverse");
    writer
        .finalize_operation(finalize("inverse", 9_000))
        .expect("finalize inverse");
    writer.flush_blocking().expect("flush");

    writer
        .prune(PruneRequest {
            max_age_secs: Some(5_000),
            max_size_bytes: None,
            now_secs: 10_000,
            vacuum: false,
        })
        .expect("prune");
    writer.flush_blocking().expect("flush");

    assert!(
        read_operation(store.conn(), "original").expect("read").is_none(),
        "old original pruned"
    );
    let inverse = read_operation(store.conn(), "inverse")
        .expect("read")
        .expect("inverse kept");
    assert_eq!(
        inverse.rolls_back_op_id, None,
        "the survivor's link to the pruned op is nulled, not left dangling (no FK violation)"
    );

    writer.shutdown();
}

/// The tiered vacuum policy: skip below MIN, steady cap for a modest freelist,
/// ramp for a real backlog. Regressing this would thrash the writer lock or let
/// the freelist grow unbounded.
#[test]
fn vacuum_cap_tiers() {
    assert_eq!(pick_vacuum_cap(0), None);
    assert_eq!(pick_vacuum_cap(VACUUM_MIN_FREELIST - 1), None);
    assert_eq!(pick_vacuum_cap(VACUUM_MIN_FREELIST), Some(VACUUM_STEADY_CAP));
    assert_eq!(pick_vacuum_cap(VACUUM_BACKLOG_THRESHOLD - 1), Some(VACUUM_STEADY_CAP));
    assert_eq!(pick_vacuum_cap(VACUUM_BACKLOG_THRESHOLD), Some(VACUUM_BACKLOG_CAP));
    assert_eq!(pick_vacuum_cap(1_000_000), Some(VACUUM_BACKLOG_CAP));
}

/// The size-budget prune brings the DB's live size under budget by pruning the
/// oldest whole operations, AND the `incremental_vacuum` + truncate actually
/// shrinks the file on disk (not just the logical page count).
#[test]
fn size_prune_brings_db_under_budget_and_shrinks_the_file() {
    let (store, writer, dir) = fresh();
    let db_path = operation_log_db_path(dir.path());

    // Populate a few MB of journal across many ops.
    for op_index in 0..40 {
        journal_bulk_op(&writer, op_index, 300);
    }
    writer.flush_blocking().expect("flush");
    // Force a full checkpoint so the baseline reflects everything on the main DB
    // file (not sitting in the WAL), a fair before/after comparison.
    store
        .conn()
        .execute_batch("PRAGMA wal_checkpoint(TRUNCATE);")
        .expect("baseline checkpoint");

    let live_before = live_size_bytes(store.conn()).expect("live before");
    let file_before = std::fs::metadata(&db_path).expect("stat before").len();
    let budget = live_before / 4;
    assert!(live_before > budget, "precondition: over budget before prune");

    writer
        .prune(PruneRequest {
            max_age_secs: None,
            max_size_bytes: Some(budget),
            now_secs: 1_000_000,
            vacuum: false,
        })
        .expect("prune");
    writer.flush_blocking().expect("flush");

    let live_after = live_size_bytes(store.conn()).expect("live after");
    let file_after = std::fs::metadata(&db_path).expect("stat after").len();
    assert!(
        live_after <= budget,
        "live size {live_after} should be within budget {budget}"
    );
    assert!(
        file_after < file_before,
        "the file shrinks on disk after vacuum+truncate: {file_after} !< {file_before}"
    );

    // The newest ops survive; some old ones are gone.
    assert!(
        read_operation(store.conn(), "op-39").expect("read").is_some(),
        "the newest op is retained"
    );
    assert!(
        read_operation(store.conn(), "op-0").expect("read").is_none(),
        "the oldest op was pruned first"
    );
    writer.shutdown();
}

/// A size prune never touches an op in `rolling_back` (nor its rows), even under
/// budget pressure — a live rollback streams its source, so pruning it would
/// under-restore (Finding 6/7).
#[test]
fn size_prune_skips_a_rolling_back_op() {
    let (store, writer, _dir) = fresh();
    // The oldest op is mid-rollback; two newer ops add bulk.
    journal_bulk_op(&writer, 0, 200);
    writer
        .set_rollback_state("op-0", RollbackState::RollingBack, None)
        .expect("set rolling_back");
    journal_bulk_op(&writer, 1, 200);
    journal_bulk_op(&writer, 2, 200);
    writer.flush_blocking().expect("flush");

    // A budget so tight it would prune everything if allowed.
    writer
        .prune(PruneRequest {
            max_age_secs: None,
            max_size_bytes: Some(1),
            now_secs: 1_000_000,
            vacuum: false,
        })
        .expect("prune");
    writer.flush_blocking().expect("flush");

    assert!(
        read_operation(store.conn(), "op-0").expect("read").is_some(),
        "the rolling_back op is never pruned, even under extreme budget pressure"
    );
    assert_eq!(
        read_operation_items(store.conn(), "op-0", 1000).expect("items").len(),
        200,
        "its streamed source rows are intact"
    );
    // The unprotected newer ops were pruned to chase the (unreachable) budget.
    assert!(read_operation(store.conn(), "op-1").expect("read").is_none());
    assert!(read_operation(store.conn(), "op-2").expect("read").is_none());
    writer.shutdown();
}

/// A rolled-back pair (original + its inverse) prunes together under size
/// pressure: pruning the older original also removes the inverse that references
/// it, leaving no dangling `rolls_back_op_id`.
#[test]
fn size_prune_removes_a_rolled_back_pair_together() {
    let (store, writer, _dir) = fresh();
    // Original (oldest), then its inverse (references the original), then a newer
    // unrelated op.
    let open = |op_id: &str, started: i64, rolls_back: Option<String>| OpenOperation {
        op_id: op_id.to_string(),
        kind: OpKind::Copy,
        initiator: Initiator::User,
        source_volume_id: Some("vol-1".to_string()),
        dest_volume_id: Some("vol-1".to_string()),
        item_count: 0,
        started_at: started,
        rolls_back_op_id: rolls_back,
        execution_status: ExecutionStatus::Running,
    };
    let finalize = |op_id: &str, ended: i64, state: RollbackState| FinalizeOperation {
        op_id: op_id.to_string(),
        execution_status: ExecutionStatus::Done,
        rollback_state: state,
        not_rollbackable_reason: None,
        archive_subkind: None,
        search_coverage: SearchCoverage::Full,
        search_coverage_reason: None,
        ended_at: ended,
        item_count: None,
        items_done: 0,
        bytes_total: 0,
        dev_summary: None,
    };
    // The original carries the bulk (so pruning the pair reclaims real space); the
    // inverse is a small metadata op; a tiny newer op survives within budget.
    writer.open_operation(open("original", 0, None)).expect("open orig");
    let bulk: Vec<JournalItem> = (0..400)
        .map(|seq| JournalItem {
            seq,
            entry_type: EntryType::File,
            row_role: RowRole::RollbackUnit,
            source_volume_id: "vol-1".to_string(),
            source_dir: "/vol/orig".to_string(),
            source_name: format!("file-{seq:06}-payload-padding.dat"),
            dest_volume_id: Some("vol-1".to_string()),
            dest_dir: Some("/vol/copy".to_string()),
            dest_name: Some(format!("file-{seq:06}-payload-padding.dat")),
            size: Some(seq),
            mtime: Some(1_700_000_000 + seq),
            outcome: ItemOutcome::Done,
            overwrote: false,
        })
        .collect();
    writer.record_items("original", bulk).expect("record orig");
    writer
        .finalize_operation(finalize("original", 0, RollbackState::RolledBack))
        .expect("fin orig");
    writer
        .open_operation(open("inverse", 1, Some("original".to_string())))
        .expect("open inv");
    writer
        .finalize_operation(finalize("inverse", 1, RollbackState::NotRollbackable))
        .expect("fin inv");
    // A tiny newer op that must survive.
    journal_bulk_op(&writer, 100, 5);
    writer.flush_blocking().expect("flush");

    let live = live_size_bytes(store.conn()).expect("live");
    // A budget the bulky original overshoots on its own, so pruning the old pair
    // brings us under it while the tiny newest op fits.
    let budget = live / 2;
    writer
        .prune(PruneRequest {
            max_age_secs: None,
            max_size_bytes: Some(budget),
            now_secs: 1_000_000,
            vacuum: false,
        })
        .expect("prune");
    writer.flush_blocking().expect("flush");

    assert!(
        read_operation(store.conn(), "original").expect("read").is_none(),
        "the original was pruned"
    );
    assert!(
        read_operation(store.conn(), "inverse").expect("read").is_none(),
        "its inverse pruned together — no dangling link left behind"
    );
    assert!(
        read_operation(store.conn(), "op-100").expect("read").is_some(),
        "the newest op survives"
    );
    writer.shutdown();
}

/// A combined age + size retention pass over a populated DB with mixed ages and
/// sizes: age drops the ancient ops, then the size budget trims the rest oldest-
/// first until within budget, never orphaning an item or leaving a dangling link.
#[test]
fn retention_over_mixed_ages_and_sizes() {
    let (store, writer, _dir) = fresh();
    // Ages 0..30 (op-N ended at N). Some big, some small.
    for op_index in 0..30 {
        let items = if op_index % 2 == 0 { 200 } else { 20 };
        journal_bulk_op(&writer, op_index, items);
    }
    writer.flush_blocking().expect("flush");

    // Age prune removes anything ended before now(1000) - age(985) = 15, then the
    // size budget trims the survivors (ages 15..29) further.
    let live = live_size_bytes(store.conn()).expect("live");
    writer
        .prune(PruneRequest {
            max_age_secs: Some(985),
            max_size_bytes: Some(live / 3),
            now_secs: 1_000,
            vacuum: false,
        })
        .expect("prune");
    writer.flush_blocking().expect("flush");

    // Everything older than the age cutoff is gone.
    for op_index in 0..15 {
        assert!(
            read_operation(store.conn(), &format!("op-{op_index}"))
                .expect("read")
                .is_none(),
            "op-{op_index} is past the age cutoff"
        );
    }
    // Within budget, and no orphaned items (every surviving item belongs to a
    // surviving op).
    assert!(live_size_bytes(store.conn()).expect("live") <= live / 3);
    let orphans: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM operation_items WHERE op_id NOT IN (SELECT op_id FROM operations)",
            [],
            |row| row.get(0),
        )
        .expect("orphan count");
    assert_eq!(orphans, 0, "no item outlives its operation");
    // No dangling rollback links.
    let dangling: i64 = store
        .conn()
        .query_row(
            "SELECT COUNT(*) FROM operations WHERE rolls_back_op_id IS NOT NULL \
             AND rolls_back_op_id NOT IN (SELECT op_id FROM operations)",
            [],
            |row| row.get(0),
        )
        .expect("dangling count");
    assert_eq!(dangling, 0, "no surviving op points at a pruned op");
    writer.shutdown();
}
