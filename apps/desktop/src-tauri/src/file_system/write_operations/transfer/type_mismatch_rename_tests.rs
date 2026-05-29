//! Integration tests for Rename conflict resolution on **type-mismatch**
//! clashes (folder source over a file dest, and file source over a folder
//! dest).
//!
//! **Data-safety contract pinned here.** cmdr's Rename semantics: keep the
//! EXISTING destination item untouched at its name, and land the INCOMING
//! item under a fresh `name (1)` name, with its full content. This is the
//! same rule file→file Rename follows (pinned by `conflict-copy.spec.ts`);
//! these tests pin it for the two type-mismatch directions, which had two
//! distinct bugs before:
//!
//! - **folder→file Rename was backwards**: the parent-creation branch in
//!   `copy_single_item` renamed the EXISTING file aside and created the
//!   incoming folder at the original name. Expected: existing file stays put,
//!   incoming folder lands at `name (1)`.
//! - **file→folder Rename lost the incoming bytes**: `find_unique_name`
//!   reserves the chosen name by creating a 0-byte placeholder (TOCTOU guard,
//!   commit `cd48abb8`). The regular-file copy then ran with
//!   `needs_safe_overwrite = false`, which on the same-APFS-volume path goes
//!   through `copyfile(3)` with `COPYFILE_EXCL` — the placeholder is in the
//!   way, so the source bytes never land and `name (1)` stays 0 bytes.
//!
//! Both are driven end-to-end through `copy_files_with_progress_inner` with
//! `config.conflict_resolution = Rename`, which exercises the real
//! `resolve_conflict` → `apply_resolution` → copy path without needing the
//! Stop-mode oneshot channel.

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use super::super::state::WriteOperationState;
use super::super::types::{CollectorEventSink, ConflictResolution, WriteOperationConfig};
use super::copy::copy_files_with_progress_inner;
use super::move_op::move_files_with_progress_inner;

fn create_temp_dir(name: &str) -> PathBuf {
    let temp_dir = std::env::temp_dir().join(format!("cmdr_type_mismatch_rename_{}_{}", name, uuid::Uuid::new_v4()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Failed to create temp directory");
    temp_dir
}

fn cleanup_temp_dir(path: &PathBuf) {
    let _ = fs::remove_dir_all(path);
}

fn rename_config() -> WriteOperationConfig {
    WriteOperationConfig {
        conflict_resolution: ConflictResolution::Rename,
        ..Default::default()
    }
}

/// Lists the child names of a directory, sorted. Fails the test if the path
/// isn't a readable directory.
fn dir_children(dir: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir)
        .expect("dir should be readable")
        .map(|e| e.unwrap().file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// Asserts no `.cmdr-temp-` aside or other stray artifact survived in `dir`
/// beyond the explicitly-expected names.
fn assert_no_temp_artifacts(dir: &std::path::Path) {
    for name in dir_children(dir) {
        assert!(
            !name.contains(".cmdr-temp-") && !name.contains(".cmdr-tmp-"),
            "stray temp artifact left behind: {name}"
        );
    }
}

// ============================================================================
// folder → file Rename
// ============================================================================

/// folder→file Rename: source is a folder, dest holds a file at the same name.
/// The existing file must stay put (still a file, original bytes); the incoming
/// folder lands at `name (1)` with its child.
///
/// Pre-fix this was BACKWARDS: the existing file got renamed to `thing (1)`
/// and the incoming folder landed at `thing`, clobbering the type the user
/// expected to keep.
#[test]
fn folder_over_file_rename_keeps_existing_file_lands_folder_renamed() {
    let temp_dir = create_temp_dir("folder_over_file");
    let src_root = temp_dir.join("src");
    let dst_root = temp_dir.join("dst");
    fs::create_dir_all(&dst_root).unwrap();

    // Source: a folder `thing/` with one child.
    fs::create_dir_all(src_root.join("thing")).unwrap();
    fs::write(src_root.join("thing/sentinel.txt"), "source-sentinel").unwrap();
    // Dest: a file `thing`.
    fs::write(dst_root.join("thing"), "dest-thing-bytes").unwrap();

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let sources = vec![src_root.join("thing")];
    let config = rename_config();

    copy_files_with_progress_inner(&events, "op-folder-over-file", &state, &sources, &dst_root, &config)
        .expect("copy should succeed");

    // Existing dest file untouched at its name.
    let dest_file = dst_root.join("thing");
    assert!(
        fs::symlink_metadata(&dest_file).unwrap().is_file(),
        "existing dest `thing` must stay a file"
    );
    assert_eq!(
        fs::read_to_string(&dest_file).unwrap(),
        "dest-thing-bytes",
        "existing dest file bytes must be untouched"
    );

    // Incoming folder landed renamed, with its child intact.
    let renamed_dir = dst_root.join("thing (1)");
    assert!(
        fs::symlink_metadata(&renamed_dir).unwrap().is_dir(),
        "incoming folder must land at `thing (1)` as a directory"
    );
    assert_eq!(
        fs::read_to_string(renamed_dir.join("sentinel.txt")).unwrap(),
        "source-sentinel",
        "incoming folder's child must be copied with its content"
    );

    assert_eq!(
        dir_children(&dst_root),
        vec!["thing".to_string(), "thing (1)".to_string()]
    );
    assert_no_temp_artifacts(&dst_root);

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// file → folder Rename
// ============================================================================

/// file→folder Rename: source is a file, dest holds a folder at the same name.
/// The existing folder must stay put (still a dir, original child intact); the
/// incoming file lands at `name (1)` with its REAL bytes (not 0).
///
/// Pre-fix the dest folder correctly survived, but `name (1)` was the 0-byte
/// `find_unique_name` placeholder — the source bytes were lost.
#[test]
fn file_over_folder_rename_keeps_existing_folder_lands_file_with_bytes() {
    let temp_dir = create_temp_dir("file_over_folder");
    let src_root = temp_dir.join("src");
    let dst_root = temp_dir.join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();

    // Source: a file `item` with real bytes.
    let source_bytes = "source-item-real-bytes-not-empty";
    fs::write(src_root.join("item"), source_bytes).unwrap();
    // Dest: a folder `item/` with one child.
    fs::create_dir_all(dst_root.join("item")).unwrap();
    fs::write(dst_root.join("item/inside.txt"), "dest-inside").unwrap();

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let sources = vec![src_root.join("item")];
    let config = rename_config();

    copy_files_with_progress_inner(&events, "op-file-over-folder", &state, &sources, &dst_root, &config)
        .expect("copy should succeed");

    // Existing dest folder untouched.
    let dest_folder = dst_root.join("item");
    assert!(
        fs::symlink_metadata(&dest_folder).unwrap().is_dir(),
        "existing dest `item` must stay a directory"
    );
    assert_eq!(
        fs::read_to_string(dest_folder.join("inside.txt")).unwrap(),
        "dest-inside",
        "existing dest folder's child must be untouched"
    );

    // Incoming file landed renamed, with its REAL bytes (the lost-bytes bug).
    let renamed_file = dst_root.join("item (1)");
    assert!(
        fs::symlink_metadata(&renamed_file).unwrap().is_file(),
        "incoming file must land at `item (1)` as a file"
    );
    assert_eq!(
        fs::read_to_string(&renamed_file).unwrap(),
        source_bytes,
        "incoming file at `item (1)` must hold the source bytes, not be a 0-byte placeholder"
    );

    assert_eq!(
        dir_children(&dst_root),
        vec!["item".to_string(), "item (1)".to_string()]
    );
    assert_no_temp_artifacts(&dst_root);

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Uniqueness escalation (TOCTOU-guard regression check)
// ============================================================================

/// A pre-existing `name (1)` must force the incoming item to `name (2)`. This
/// proves the rename path still consults `find_unique_name` (which reserves
/// the name atomically), so the TOCTOU fix from commit `cd48abb8` isn't
/// regressed by making the copy land on the reserved placeholder.
#[test]
fn file_over_folder_rename_escalates_past_existing_numbered_name() {
    let temp_dir = create_temp_dir("file_over_folder_escalate");
    let src_root = temp_dir.join("src");
    let dst_root = temp_dir.join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();

    let source_bytes = "source-escalate-bytes";
    fs::write(src_root.join("item"), source_bytes).unwrap();
    // Dest folder at `item` plus an occupied `item (1)`.
    fs::create_dir_all(dst_root.join("item")).unwrap();
    fs::write(dst_root.join("item/inside.txt"), "dest-inside").unwrap();
    fs::write(dst_root.join("item (1)"), "pre-existing-one").unwrap();

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let sources = vec![src_root.join("item")];
    let config = rename_config();

    copy_files_with_progress_inner(&events, "op-escalate", &state, &sources, &dst_root, &config)
        .expect("copy should succeed");

    // The occupied `item (1)` is untouched.
    assert_eq!(
        fs::read_to_string(dst_root.join("item (1)")).unwrap(),
        "pre-existing-one"
    );
    // The dest folder survives.
    assert!(fs::symlink_metadata(dst_root.join("item")).unwrap().is_dir());
    // The incoming file escalated to `item (2)` with its real bytes.
    let escalated = dst_root.join("item (2)");
    assert!(fs::symlink_metadata(&escalated).unwrap().is_file());
    assert_eq!(fs::read_to_string(&escalated).unwrap(), source_bytes);

    assert_no_temp_artifacts(&dst_root);

    cleanup_temp_dir(&temp_dir);
}

/// folder→file Rename with a pre-existing numbered name escalates too, and the
/// incoming folder's child still lands.
#[test]
fn folder_over_file_rename_escalates_past_existing_numbered_name() {
    let temp_dir = create_temp_dir("folder_over_file_escalate");
    let src_root = temp_dir.join("src");
    let dst_root = temp_dir.join("dst");
    fs::create_dir_all(&dst_root).unwrap();

    fs::create_dir_all(src_root.join("thing")).unwrap();
    fs::write(src_root.join("thing/sentinel.txt"), "source-sentinel").unwrap();
    fs::write(dst_root.join("thing"), "dest-thing").unwrap();
    fs::write(dst_root.join("thing (1)"), "pre-existing-one").unwrap();

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let sources = vec![src_root.join("thing")];
    let config = rename_config();

    copy_files_with_progress_inner(&events, "op-folder-escalate", &state, &sources, &dst_root, &config)
        .expect("copy should succeed");

    assert_eq!(fs::read_to_string(dst_root.join("thing")).unwrap(), "dest-thing");
    assert_eq!(
        fs::read_to_string(dst_root.join("thing (1)")).unwrap(),
        "pre-existing-one"
    );
    let escalated = dst_root.join("thing (2)");
    assert!(fs::symlink_metadata(&escalated).unwrap().is_dir());
    assert_eq!(
        fs::read_to_string(escalated.join("sentinel.txt")).unwrap(),
        "source-sentinel"
    );

    assert_no_temp_artifacts(&dst_root);

    cleanup_temp_dir(&temp_dir);
}

// ============================================================================
// Move path (same-FS rename) — mirrors the copy cases above
// ============================================================================

/// folder→file Rename on the same-FS move path: existing file kept, incoming
/// folder lands at `name (1)`. The move path had the same backwards/overwrite
/// hazard as copy once `apply_resolution` started flagging Rename as
/// `needs_safe_overwrite`; this pins the corrected `move_resolved_into_place`.
#[test]
fn move_folder_over_file_rename_keeps_existing_file_lands_folder_renamed() {
    let temp_dir = create_temp_dir("move_folder_over_file");
    let src_root = temp_dir.join("src");
    let dst_root = temp_dir.join("dst");
    fs::create_dir_all(&dst_root).unwrap();

    fs::create_dir_all(src_root.join("thing")).unwrap();
    fs::write(src_root.join("thing/sentinel.txt"), "source-sentinel").unwrap();
    fs::write(dst_root.join("thing"), "dest-thing-bytes").unwrap();

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let sources = vec![src_root.join("thing")];
    let config = rename_config();

    move_files_with_progress_inner(&events, "mv-folder-over-file", &state, &sources, &dst_root, &config)
        .expect("move should succeed");

    let dest_file = dst_root.join("thing");
    assert!(fs::symlink_metadata(&dest_file).unwrap().is_file(), "dest file kept");
    assert_eq!(fs::read_to_string(&dest_file).unwrap(), "dest-thing-bytes");

    let renamed_dir = dst_root.join("thing (1)");
    assert!(
        fs::symlink_metadata(&renamed_dir).unwrap().is_dir(),
        "folder landed renamed"
    );
    assert_eq!(
        fs::read_to_string(renamed_dir.join("sentinel.txt")).unwrap(),
        "source-sentinel"
    );
    // Move consumed the source.
    assert!(!src_root.join("thing").exists(), "source folder moved away");
    assert_no_temp_artifacts(&dst_root);

    cleanup_temp_dir(&temp_dir);
}

/// file→folder Rename on the same-FS move path: existing folder kept, incoming
/// file lands at `name (1)` with its real bytes (placeholder consumed).
#[test]
fn move_file_over_folder_rename_keeps_existing_folder_lands_file() {
    let temp_dir = create_temp_dir("move_file_over_folder");
    let src_root = temp_dir.join("src");
    let dst_root = temp_dir.join("dst");
    fs::create_dir_all(&src_root).unwrap();
    fs::create_dir_all(&dst_root).unwrap();

    let source_bytes = "source-item-real-bytes";
    fs::write(src_root.join("item"), source_bytes).unwrap();
    fs::create_dir_all(dst_root.join("item")).unwrap();
    fs::write(dst_root.join("item/inside.txt"), "dest-inside").unwrap();

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let sources = vec![src_root.join("item")];
    let config = rename_config();

    move_files_with_progress_inner(&events, "mv-file-over-folder", &state, &sources, &dst_root, &config)
        .expect("move should succeed");

    let dest_folder = dst_root.join("item");
    assert!(fs::symlink_metadata(&dest_folder).unwrap().is_dir(), "dest folder kept");
    assert_eq!(
        fs::read_to_string(dest_folder.join("inside.txt")).unwrap(),
        "dest-inside"
    );

    let renamed_file = dst_root.join("item (1)");
    assert!(
        fs::symlink_metadata(&renamed_file).unwrap().is_file(),
        "file landed renamed"
    );
    assert_eq!(
        fs::read_to_string(&renamed_file).unwrap(),
        source_bytes,
        "moved file must hold its real bytes"
    );
    assert!(!src_root.join("item").exists(), "source file moved away");
    assert_no_temp_artifacts(&dst_root);

    cleanup_temp_dir(&temp_dir);
}
