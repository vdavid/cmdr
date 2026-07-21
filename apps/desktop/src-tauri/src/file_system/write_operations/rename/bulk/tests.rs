use super::*;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicU8;
use uuid::Uuid;

use super::super::super::operation_intent::OperationIntent;

fn create_test_dir(name: &str) -> PathBuf {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .nth(3)
        .expect("src-tauri manifest has the repository root as its third ancestor");
    let dir = repo_root
        .join("_ignored")
        .join(format!("cmdr_bulk_rename_test_{name}_{}", Uuid::new_v4()));
    fs::create_dir_all(&dir).expect("create test directory");
    dir
}

fn local_row(row_id: &str, source: PathBuf, destination: PathBuf) -> BulkRenameRow {
    let expected_fingerprint = local_fingerprint(&source).expect("fingerprint fixture source");
    BulkRenameRow {
        row_id: row_id.to_string(),
        source,
        destination,
        expected_fingerprint,
    }
}

fn assert_no_staging_paths(dir: &Path) {
    let staging_paths: Vec<_> = fs::read_dir(dir)
        .expect("read fixture directory")
        .flatten()
        .filter(|entry| entry.file_name().to_string_lossy().starts_with(".cmdr-bulk-rename-"))
        .collect();
    assert!(staging_paths.is_empty(), "unexpected staging paths: {staging_paths:?}");
}

#[test]
fn bulk_local_rename_preserves_chains_and_cycles() {
    let tmp = create_test_dir("chain_cycle");
    let chain_a = tmp.join("chain-a.txt");
    let chain_b = tmp.join("chain-b.txt");
    let chain_c = tmp.join("chain-c.txt");
    let chain_d = tmp.join("chain-d.txt");
    let cycle_a = tmp.join("cycle-a.txt");
    let cycle_b = tmp.join("cycle-b.txt");
    let cycle_c = tmp.join("cycle-c.txt");
    for (path, contents) in [
        (&chain_a, "chain a"),
        (&chain_b, "chain b"),
        (&chain_c, "chain c"),
        (&cycle_a, "cycle a"),
        (&cycle_b, "cycle b"),
        (&cycle_c, "cycle c"),
    ] {
        fs::write(path, contents).expect("write fixture");
    }

    let rows = vec![
        local_row("chain-a", chain_a.clone(), chain_b.clone()),
        local_row("chain-b", chain_b.clone(), chain_c.clone()),
        local_row("chain-c", chain_c.clone(), chain_d.clone()),
        local_row("cycle-a", cycle_a.clone(), cycle_b.clone()),
        local_row("cycle-b", cycle_b.clone(), cycle_c.clone()),
        local_row("cycle-c", cycle_c.clone(), cycle_a.clone()),
    ];

    let run = bulk_rename_local(&rows, &AtomicU8::new(OperationIntent::Running as u8));

    assert_eq!(fs::read_to_string(&chain_b).expect("read chain b"), "chain a");
    assert_eq!(fs::read_to_string(&chain_c).expect("read chain c"), "chain b");
    assert_eq!(fs::read_to_string(&chain_d).expect("read chain d"), "chain c");
    assert_eq!(fs::read_to_string(&cycle_a).expect("read cycle a"), "cycle c");
    assert_eq!(fs::read_to_string(&cycle_b).expect("read cycle b"), "cycle a");
    assert_eq!(fs::read_to_string(&cycle_c).expect("read cycle c"), "cycle b");
    assert!(
        run.outcomes.iter().all(|outcome| outcome.is_done()),
        "unexpected outcomes: {:?}",
        run.outcomes
    );
    assert_no_staging_paths(&tmp);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn bulk_local_rename_preserves_swaps_and_case_only_names() {
    let tmp = create_test_dir("swap_case_only");
    let first = tmp.join("first.txt");
    let second = tmp.join("second.txt");
    let case_source = tmp.join("screenshot.png");
    fs::write(&first, "first").expect("write first fixture");
    fs::write(&second, "second").expect("write second fixture");
    fs::write(&case_source, "image").expect("write case fixture");

    let rows = vec![
        local_row("first", first.clone(), second.clone()),
        local_row("second", second.clone(), first.clone()),
        local_row("case", case_source.clone(), tmp.join("Screenshot.png")),
    ];

    let run = bulk_rename_local(&rows, &AtomicU8::new(OperationIntent::Running as u8));

    assert_eq!(fs::read_to_string(&first).expect("read swapped first"), "second");
    assert_eq!(fs::read_to_string(&second).expect("read swapped second"), "first");
    assert_eq!(
        fs::read_to_string(tmp.join("Screenshot.png")).expect("read case-only rename"),
        "image"
    );
    assert!(
        run.outcomes.iter().all(|outcome| outcome.is_done()),
        "unexpected outcomes: {:?}",
        run.outcomes
    );
    assert_no_staging_paths(&tmp);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn bulk_local_rename_skips_a_source_that_changed_after_preflight() {
    let tmp = create_test_dir("changed_source");
    let source = tmp.join("before.txt");
    let destination = tmp.join("after.txt");
    fs::write(&source, "reviewed").expect("write fixture");
    let row = local_row("changed", source.clone(), destination.clone());
    fs::write(&source, "changed after review").expect("change fixture after fingerprint");

    let run = bulk_rename_local(&[row], &AtomicU8::new(OperationIntent::Running as u8));

    assert_eq!(run.outcomes, vec![BulkRenameOutcome::Skipped]);
    assert_eq!(
        fs::read_to_string(&source).expect("read changed source"),
        "changed after review"
    );
    assert!(!destination.exists(), "a changed source must not be renamed");
    assert_no_staging_paths(&tmp);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn bulk_local_rename_honours_cancel_before_staging() {
    let tmp = create_test_dir("cancel_before_staging");
    let source = tmp.join("before.txt");
    let destination = tmp.join("after.txt");
    fs::write(&source, "reviewed").expect("write fixture");
    let row = local_row("cancelled", source.clone(), destination.clone());

    let run = bulk_rename_local(&[row], &AtomicU8::new(OperationIntent::Stopped as u8));

    assert!(run.cancelled, "cancel must stop the batch driver");
    assert_eq!(run.outcomes, vec![BulkRenameOutcome::Skipped]);
    assert_eq!(fs::read_to_string(&source).expect("read preserved source"), "reviewed");
    assert!(!destination.exists(), "cancel must not apply a final rename");
    assert_no_staging_paths(&tmp);
    let _ = fs::remove_dir_all(&tmp);
}

#[test]
fn restoring_cancelled_local_staging_path_recovers_the_source_name() {
    let tmp = create_test_dir("cancel_restore");
    let source = tmp.join("before.txt");
    let destination = tmp.join("after.txt");
    fs::write(&source, "reviewed").expect("write fixture");
    let row = local_row("restore", source.clone(), destination);
    let temporary = unique_temporary_path(&source, &row.row_id).expect("temporary path");
    fs::rename(&source, &temporary).expect("stage fixture source");

    restore_local_temporaries(&[row], &[Some(temporary)], &[BulkRenameOutcome::Skipped]);

    assert_eq!(fs::read_to_string(&source).expect("read restored source"), "reviewed");
    assert_no_staging_paths(&tmp);
    let _ = fs::remove_dir_all(&tmp);
}
