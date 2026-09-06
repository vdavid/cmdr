//! Integration tests for what a BLANKET conflict policy is allowed to do when
//! the two sides of a clash are different kinds of entry.
//!
//! **Data-safety contract pinned here.** `Overwrite`, `Overwrite all smaller`,
//! and `Overwrite all older` are answers about two FILES: "the one at the
//! destination is stale, put the source there". Nobody picking one from the
//! transfer dialog was shown a folder, so none of them may replace a folder
//! with a file or a file with a folder. Such an item reduces to `Skip` and both
//! sides stay exactly as they were.
//!
//! The conditional variants can't even ask their own question across types: a
//! directory's `len()` is its inode's own size (a few hundred bytes), so every
//! ordinary file looks "bigger" and "Overwrite all smaller" deleted whole
//! destination folders under a policy documented as conservative.
//!
//! An explicit per-item Overwrite on a Stop prompt is a different thing and
//! still replaces: the dialog names both types and the click is the consent a
//! blanket policy lacks. Pinned at the bottom of this file.

use std::fs;
use std::sync::Arc;
use std::time::Duration;

use super::super::event_sinks::CollectorEventSink;
use super::super::state::WriteOperationState;
use super::super::types::{ConflictResolution, WriteOperationConfig};
use super::conflict_responder_test_support::ConflictResponderSink;
use super::copy::copy_files_with_progress_inner;
use super::move_op::move_files_with_progress_inner;
use crate::ignore_poison::IgnorePoison;
use crate::test_support::TestDir;

fn temp(name: &str) -> TestDir {
    TestDir::new(&format!("cross_type_policy_{}_{}", name, uuid::Uuid::new_v4()))
}

fn policy(conflict_resolution: ConflictResolution) -> WriteOperationConfig {
    WriteOperationConfig {
        conflict_resolution,
        ..Default::default()
    }
}

fn state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

/// Source: a plain file `notes` with real bytes. Destination: a folder `notes/`
/// holding `precious.txt`. Returns `(src_root, dst_root)`.
fn file_over_folder_fixture(dir: &TestDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let src_root = dir.join("src");
    let dst_root = dir.join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(dst_root.join("notes")).unwrap();
    // Far bigger than the destination directory's own inode size, so every
    // conditional comparison that treats the folder as a file says "overwrite".
    fs::write(src_root.join("notes"), "x".repeat(100_000)).unwrap();
    fs::write(dst_root.join("notes/precious.txt"), "precious user data").unwrap();
    (src_root, dst_root)
}

/// Source: a folder `thing/` with one child. Destination: a file `thing`.
fn folder_over_file_fixture(dir: &TestDir) -> (std::path::PathBuf, std::path::PathBuf) {
    let src_root = dir.join("src");
    let dst_root = dir.join("dst");
    fs::create_dir_all(src_root.join("thing")).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    fs::write(src_root.join("thing/sentinel.txt"), "source-sentinel").unwrap();
    fs::write(dst_root.join("thing"), "precious user bytes").unwrap();
    (src_root, dst_root)
}

fn assert_folder_survived(dst_root: &std::path::Path) {
    let folder = dst_root.join("notes");
    assert!(
        fs::symlink_metadata(&folder).unwrap().is_dir(),
        "a blanket Overwrite policy must leave the destination FOLDER a folder"
    );
    assert_eq!(
        fs::read_to_string(folder.join("precious.txt")).unwrap(),
        "precious user data",
        "the destination folder's contents must be untouched"
    );
}

fn assert_file_survived(dst_root: &std::path::Path) {
    let file = dst_root.join("thing");
    assert!(
        fs::symlink_metadata(&file).unwrap().is_file(),
        "a blanket Overwrite policy must leave the destination FILE a file"
    );
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "precious user bytes",
        "the destination file's bytes must be untouched"
    );
}

// ============================================================================
// file → folder, under each blanket policy
// ============================================================================

#[test]
fn overwrite_smaller_never_deletes_a_folder_a_file_landed_on() {
    let dir = temp("smaller_file_over_folder");
    let (src_root, dst_root) = file_over_folder_fixture(&dir);

    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op-smaller-file-over-folder",
        &state(),
        &[src_root.join("notes")],
        &dst_root,
        &policy(ConflictResolution::OverwriteSmaller),
    )
    .expect("the copy should finish, having skipped the cross-type clash");

    assert_folder_survived(&dst_root);
}

#[test]
fn overwrite_older_never_deletes_a_folder_a_file_landed_on() {
    let dir = temp("older_file_over_folder");
    let (src_root, dst_root) = file_over_folder_fixture(&dir);
    // The source is newer than the destination folder, so the mtime comparison
    // alone would say "overwrite".
    let old = filetime::FileTime::from_unix_time(1_600_000_000, 0);
    filetime::set_file_mtime(dst_root.join("notes"), old).unwrap();

    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op-older-file-over-folder",
        &state(),
        &[src_root.join("notes")],
        &dst_root,
        &policy(ConflictResolution::OverwriteOlder),
    )
    .expect("the copy should finish, having skipped the cross-type clash");

    assert_folder_survived(&dst_root);
}

#[test]
fn overwrite_all_never_deletes_a_folder_a_file_landed_on() {
    let dir = temp("overwrite_file_over_folder");
    let (src_root, dst_root) = file_over_folder_fixture(&dir);

    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op-overwrite-file-over-folder",
        &state(),
        &[src_root.join("notes")],
        &dst_root,
        &policy(ConflictResolution::Overwrite),
    )
    .expect("the copy should finish, having skipped the cross-type clash");

    assert_folder_survived(&dst_root);
}

// ============================================================================
// folder → file, under each blanket policy
// ============================================================================

#[test]
fn overwrite_all_never_deletes_a_file_a_folder_landed_on() {
    let dir = temp("overwrite_folder_over_file");
    let (src_root, dst_root) = folder_over_file_fixture(&dir);

    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op-overwrite-folder-over-file",
        &state(),
        &[src_root.join("thing")],
        &dst_root,
        &policy(ConflictResolution::Overwrite),
    )
    .expect("the copy should finish, having skipped the cross-type clash");

    assert_file_survived(&dst_root);
}

#[test]
fn overwrite_smaller_never_deletes_a_file_a_folder_landed_on() {
    let dir = temp("smaller_folder_over_file");
    let (src_root, dst_root) = folder_over_file_fixture(&dir);

    copy_files_with_progress_inner(
        &CollectorEventSink::new(),
        "op-smaller-folder-over-file",
        &state(),
        &[src_root.join("thing")],
        &dst_root,
        &policy(ConflictResolution::OverwriteSmaller),
    )
    .expect("the copy should finish, having skipped the cross-type clash");

    assert_file_survived(&dst_root);
}

// ============================================================================
// The same two shapes on the same-FS move engine
// ============================================================================

#[test]
fn a_move_under_overwrite_all_never_deletes_a_folder_a_file_landed_on() {
    let dir = temp("move_overwrite_file_over_folder");
    let (src_root, dst_root) = file_over_folder_fixture(&dir);

    move_files_with_progress_inner(
        &CollectorEventSink::new(),
        "mv-overwrite-file-over-folder",
        &state(),
        &[src_root.join("notes")],
        &dst_root,
        &policy(ConflictResolution::Overwrite),
    )
    .expect("the move should finish, having skipped the cross-type clash");

    assert_folder_survived(&dst_root);
    assert!(
        src_root.join("notes").exists(),
        "a skipped item's source must stay where it is"
    );
}

#[test]
fn a_move_under_overwrite_all_never_deletes_a_file_a_folder_landed_on() {
    let dir = temp("move_overwrite_folder_over_file");
    let (src_root, dst_root) = folder_over_file_fixture(&dir);

    move_files_with_progress_inner(
        &CollectorEventSink::new(),
        "mv-overwrite-folder-over-file",
        &state(),
        &[src_root.join("thing")],
        &dst_root,
        &policy(ConflictResolution::Overwrite),
    )
    .expect("the move should finish, having skipped the cross-type clash");

    assert_file_survived(&dst_root);
    assert!(
        src_root.join("thing/sentinel.txt").exists(),
        "a skipped item's source tree must stay where it is"
    );
}

// ============================================================================
// The explicit per-item answer still replaces
// ============================================================================

/// The counterweight to everything above: a person who saw the prompt, saw both
/// types named in it, and clicked Overwrite for THIS item gets what they asked
/// for. Refusing here would make the dialog's own button a lie.
#[test]
fn an_explicitly_answered_overwrite_still_replaces_a_folder_with_a_file() {
    let dir = temp("explicit_file_over_folder");
    let (src_root, dst_root) = file_over_folder_fixture(&dir);
    let state = state();
    // `apply_to_all: false` — this is the ONE item's answer, which is exactly
    // what the refusal above spares.
    let events = ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false);

    copy_files_with_progress_inner(
        &events,
        "op-explicit-file-over-folder",
        &state,
        &[src_root.join("notes")],
        &dst_root,
        // Stop is what puts the question to a person in the first place.
        &policy(ConflictResolution::Stop),
    )
    .expect("the copy should succeed");

    let landed = dst_root.join("notes");
    assert!(
        fs::symlink_metadata(&landed).unwrap().is_file(),
        "an explicit Overwrite must replace the folder with the incoming file"
    );
    assert_eq!(fs::metadata(&landed).unwrap().len(), 100_000);
}

// ============================================================================
// The explicit answer's "apply to all" carries across the same cross-type shape
// ============================================================================
//
// The bucket a cross-type "* all" latches into can only ever be filled by a
// click on a prompt that NAMED both types — the dialog relabels its
// apply-to-all button "Overwrite folders with files" for exactly this case. So
// the carry is consented, and the blanket refusal above must not touch it.
// Without these cells the checkbox is a trap: the first item replaces and every
// one after it is skipped, silently.
//
// Each test answers ONE prompt and asserts the prompt COUNT, which is what
// separates a carry from a re-prompt: the responder sink answers whatever it
// sees, so "both replaced" alone would pass even if nothing latched.

/// Two file→folder clashes, one answer. `ConflictResponderSink` answers with
/// `apply_to_all: true`, so the second clash must never reach a prompt AND must
/// still be replaced.
#[test]
fn an_answered_overwrite_all_carries_to_the_next_folder_a_file_lands_on() {
    let dir = temp("carry_file_over_folder");
    let src_root = dir.join("src");
    let dst_root = dir.join("dst");
    fs::create_dir_all(&src_root).unwrap();
    for name in ["1-notes", "2-notes"] {
        fs::create_dir_all(dst_root.join(name)).unwrap();
        fs::write(src_root.join(name), format!("source-{name}")).unwrap();
        fs::write(dst_root.join(name).join("precious.txt"), "precious user data").unwrap();
    }
    let state = state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Overwrite, true);

    copy_files_with_progress_inner(
        &events,
        "op-carry-file-over-folder",
        &state,
        &[src_root.join("1-notes"), src_root.join("2-notes")],
        &dst_root,
        &policy(ConflictResolution::Stop),
    )
    .expect("the copy should succeed");

    assert_eq!(
        events.inner.conflicts.lock_ignore_poison().len(),
        1,
        "the second clash must be decided by the latch, not by a second prompt"
    );
    for name in ["1-notes", "2-notes"] {
        let landed = dst_root.join(name);
        assert!(
            fs::symlink_metadata(&landed).unwrap().is_file(),
            "'Overwrite folders with files' must replace {name}, not silently skip it"
        );
        assert_eq!(fs::read_to_string(&landed).unwrap(), format!("source-{name}"));
    }
}

/// The mirror shape: two folder→file clashes, one answer, both replaced.
#[test]
fn an_answered_overwrite_all_carries_to_the_next_file_a_folder_lands_on() {
    let dir = temp("carry_folder_over_file");
    let src_root = dir.join("src");
    let dst_root = dir.join("dst");
    fs::create_dir_all(&dst_root).unwrap();
    for name in ["1-thing", "2-thing"] {
        fs::create_dir_all(src_root.join(name)).unwrap();
        fs::write(src_root.join(name).join("sentinel.txt"), format!("source-{name}")).unwrap();
        fs::write(dst_root.join(name), "precious user bytes").unwrap();
    }
    let state = state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Overwrite, true);

    copy_files_with_progress_inner(
        &events,
        "op-carry-folder-over-file",
        &state,
        &[src_root.join("1-thing"), src_root.join("2-thing")],
        &dst_root,
        &policy(ConflictResolution::Stop),
    )
    .expect("the copy should succeed");

    assert_eq!(
        events.inner.conflicts.lock_ignore_poison().len(),
        1,
        "the second clash must be decided by the latch, not by a second prompt"
    );
    for name in ["1-thing", "2-thing"] {
        let landed = dst_root.join(name);
        assert!(
            fs::symlink_metadata(&landed).unwrap().is_dir(),
            "an answered folder→file Overwrite all must replace {name}, not silently skip it"
        );
        assert_eq!(
            fs::read_to_string(landed.join("sentinel.txt")).unwrap(),
            format!("source-{name}")
        );
    }
}

/// The other half of the conjunction: consent is per SHAPE. An "Overwrite all"
/// answered on a plain file→file prompt says nothing about throwing a folder
/// away, so the cross-type clash after it must still ask. Two prompts, not one.
#[test]
fn an_overwrite_all_answered_on_a_plain_clash_never_carries_across_types() {
    let dir = temp("no_carry_from_plain");
    let src_root = dir.join("src");
    let dst_root = dir.join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();
    // `1-norm.txt` (file→file) is processed before `2-notes` (file→folder).
    fs::write(src_root.join("1-norm.txt"), "source-norm").unwrap();
    fs::write(dst_root.join("1-norm.txt"), "dest-norm").unwrap();
    fs::write(src_root.join("2-notes"), "source-notes").unwrap();
    fs::create_dir_all(dst_root.join("2-notes")).unwrap();
    fs::write(dst_root.join("2-notes/precious.txt"), "precious user data").unwrap();
    let state = state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::Overwrite, true);

    copy_files_with_progress_inner(
        &events,
        "op-no-carry-from-plain",
        &state,
        &[src_root.join("1-norm.txt"), src_root.join("2-notes")],
        &dst_root,
        &policy(ConflictResolution::Stop),
    )
    .expect("the copy should succeed");

    assert_eq!(
        events.inner.conflicts.lock_ignore_poison().len(),
        2,
        "a same-kind Overwrite all must not decide a cross-type clash on its own"
    );
}

/// Consent is to the ACT the prompt named, and the conditional variants never
/// name it. The dialog offers "Overwrite all smaller" / "Overwrite all older"
/// on a cross-type prompt too (their labels say nothing about types, and the
/// smaller one is only disabled when the destination size is unknown), but the
/// comparison behind them is meaningless here: a directory's `len()` is its own
/// inode's, a few hundred bytes, so EVERY ordinary file looks "bigger" and the
/// folder goes. Clicking one on a cross-type prompt must skip, not delete.
#[test]
fn an_answered_overwrite_smaller_never_deletes_the_folder_it_cannot_compare() {
    let dir = temp("answered_smaller_file_over_folder");
    let (src_root, dst_root) = file_over_folder_fixture(&dir);
    let state = state();
    // The dialog's smaller/older buttons only ever send `apply_to_all: true`.
    let events = ConflictResponderSink::new(&state, ConflictResolution::OverwriteSmaller, true);

    copy_files_with_progress_inner(
        &events,
        "op-answered-smaller-file-over-folder",
        &state,
        &[src_root.join("notes")],
        &dst_root,
        &policy(ConflictResolution::Stop),
    )
    .expect("the copy should finish, having skipped the cross-type clash");

    assert_folder_survived(&dst_root);
}

/// The mirror: a folder landing on a file, answered "Overwrite all older".
#[test]
fn an_answered_overwrite_older_never_deletes_the_file_it_cannot_compare() {
    let dir = temp("answered_older_folder_over_file");
    let (src_root, dst_root) = folder_over_file_fixture(&dir);
    let state = state();
    let events = ConflictResponderSink::new(&state, ConflictResolution::OverwriteOlder, true);

    copy_files_with_progress_inner(
        &events,
        "op-answered-older-folder-over-file",
        &state,
        &[src_root.join("thing")],
        &dst_root,
        &policy(ConflictResolution::Stop),
    )
    .expect("the copy should finish, having skipped the cross-type clash");

    assert_file_survived(&dst_root);
}
