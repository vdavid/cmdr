//! Merge-safety tests for `conflict.rs` (the cross-volume conflict resolver):
//! that an Overwrite merges rather than replaces, that an unestablished type
//! never clears anything, and that the finalize swap keeps the original until
//! the temp is whole. The resolver's other three questions have their own
//! siblings: `conflict_conditional_tests.rs`, `conflict_same_item_tests.rs`,
//! and `conflict_cross_type_tests.rs`.
//!
//! A `#[path]` child, so `super::` here is `conflict` and `super::super::` is
//! `volume` — the same one-level-shallower rule every other `*_tests.rs` in
//! this directory follows.

use super::*;
use super::super::finalize::finalize_safe_replace;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use std::sync::Arc;

/// The recursive-delete double lives in `strategy_test_support.rs`: the cleanup
/// suite pins `prune_created_dir_if_empty` against the same lying backend.
use super::super::strategy::test_support::RecursiveDeleteVolume;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dir_overwrite_must_merge_not_replace_even_with_recursive_delete() {
    // Build a dest dir with two files: one will conflict with the source,
    // one is unique to dest (`keep-me.jpg`) and MUST survive merge.
    let inner = Arc::new(InMemoryVolume::new("dest"));
    inner.create_directory(Path::new("/photos")).await.unwrap();
    inner
        .create_file(Path::new("/photos/keep-me.jpg"), b"existing")
        .await
        .unwrap();
    inner
        .create_file(Path::new("/photos/will-conflict.jpg"), b"old")
        .await
        .unwrap();

    // Wrap so `delete` is recursive: the dangerous future-backend scenario.
    let dest_recursive: Arc<dyn Volume> = RecursiveDeleteVolume::wrapping(Arc::clone(&inner));

    // Resolve an Overwrite conflict for `/photos` (source is also a directory).
    let result = apply_volume_conflict_resolution(
        ConflictResolution::Overwrite,
        &dest_recursive,
        Path::new("/photos"),
        true,
        &ClaimedNames::default(),
    )
    .await
    .unwrap()
    .expect("dir→dir Overwrite must resolve to a merge target, not Skip");

    // The resolver should hand back the same path (caller will merge into it)
    // and must NOT request a safe-replace finalize (dirs merge, not replace).
    assert_eq!(result.write_path, PathBuf::from("/photos"));
    assert_eq!(result.replace_after_write, None);

    // CRITICAL: files unique to dest must still be there. If this fails, the
    // resolver wholesale-deleted the dest tree. Cmdr's "Overwrite means merge
    // for dirs" UX has silently flipped to "Overwrite means replace", and any
    // file in dest that isn't in source is gone.
    assert!(
        inner.exists(Path::new("/photos/keep-me.jpg")).await,
        "Overwrite resolution must NOT recursively delete the dest directory. \
         Cmdr's UX promise is merge-not-replace for dirs; if this fails, users \
         will lose files that exist in dest but not in source."
    );

    // Also check the dir itself is intact (not a `delete` retry surprise).
    assert!(
        inner.exists(Path::new("/photos")).await,
        "Dest directory itself must remain; the recursive copy needs it as a merge target."
    );
}

/// **The destructive one.** A source whose type can't be established must
/// never route a folder-onto-folder clash into the cross-type latch.
///
/// `is_file_to_folder` is `!source_is_directory && destination_is_directory`.
/// A `.unwrap_or(false)` on the source probe makes an unanswerable stat say
/// "file", which flips that latch on for a source that's really a folder,
/// and Overwrite's cross-type arm then runs a RECURSIVE delete over the
/// user's destination folder. The old comment above the probe said the
/// opposite of what the code did ("we'd rather over-prompt than route an
/// unknown clash into the destructive file→folder latch"): `false` is
/// exactly what routes it there.
///
/// Pre-fix this goes red by `/album/precious.txt` disappearing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_source_whose_type_cannot_be_established_never_clears_the_destination_folder() {
    let source = Arc::new(InMemoryVolume::new("source"));
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_file(Path::new("/album/new.jpg"), b"new").await.unwrap();
    source.set_stat_failing(Path::new("/album"));
    let source_dyn: Arc<dyn Volume> = source.clone();

    let dest = Arc::new(InMemoryVolume::new("dest"));
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/precious.txt"), b"precious user data")
        .await
        .unwrap();
    let dest_dyn: Arc<dyn Volume> = dest.clone();

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(std::time::Duration::from_millis(0)));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };
    let mut apply_to_all = ApplyToAll::default();

    let result = resolve_volume_conflict(
        &source_dyn,
        Path::new("/album"),
        &dest_dyn,
        Path::new("/album"),
        &config,
        &events,
        "op-stat-fail-source",
        &state,
        &mut apply_to_all,
        None,
        None,
        // No preflight hint: this is the branch that has to decide without one.
        None,
    )
    .await;

    assert!(
        dest.exists(Path::new("/album/precious.txt")).await,
        "an unanswerable source stat must never authorize clearing the destination folder"
    );
    assert!(
        dest.exists(Path::new("/album")).await,
        "the destination folder must survive"
    );
    let err = result.expect_err("the item must fail rather than resolve on a guess");
    assert!(
        matches!(&err, WriteOperationError::IoError { path, .. } if path == "/album"),
        "the failure must name the source whose stat failed; got {err:?}"
    );
}

/// A destination whose type can't be established is the same shape one
/// branch over: `apply_volume_conflict_resolution`'s `else if` arm reaches a
/// bare `dest_volume.delete(dest_path)` whenever the dest probe answered
/// `false`, so a guessed `false` on a real (empty) folder removes it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_whose_type_cannot_be_established_is_left_alone() {
    let source = Arc::new(InMemoryVolume::new("source"));
    source.create_file(Path::new("/notes.txt"), b"new").await.unwrap();
    let source_dyn: Arc<dyn Volume> = source.clone();

    let dest = Arc::new(InMemoryVolume::new("dest"));
    dest.create_directory(Path::new("/notes.txt")).await.unwrap();
    dest.set_stat_failing(Path::new("/notes.txt"));
    let dest_dyn: Arc<dyn Volume> = dest.clone();

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(std::time::Duration::from_millis(0)));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };
    let mut apply_to_all = ApplyToAll::default();

    let result = resolve_volume_conflict(
        &source_dyn,
        Path::new("/notes.txt"),
        &dest_dyn,
        Path::new("/notes.txt"),
        &config,
        &events,
        "op-stat-fail-dest",
        &state,
        &mut apply_to_all,
        None,
        None,
        Some(false),
    )
    .await;

    assert!(
        dest.exists(Path::new("/notes.txt")).await,
        "an unanswerable destination stat must never authorize deleting it"
    );
    let err = result.expect_err("the item must fail rather than resolve on a guess");
    assert!(
        matches!(&err, WriteOperationError::IoError { path, .. } if path == "/notes.txt"),
        "the failure must name the destination whose stat failed; got {err:?}"
    );
}

/// A destination that genuinely isn't there is an ANSWER, not a refusal to
/// answer, and must keep behaving like "not a directory". Without this the
/// propagation above would turn a raced-away destination into a failed item
/// where the write would simply have succeeded.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_that_raced_away_still_resolves_as_a_plain_write() {
    let source = Arc::new(InMemoryVolume::new("source"));
    source.create_file(Path::new("/notes.txt"), b"new").await.unwrap();
    let source_dyn: Arc<dyn Volume> = source.clone();

    // Empty destination: the conflict was detected, then the dest vanished.
    let dest_dyn: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("dest"));

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(std::time::Duration::from_millis(0)));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };
    let mut apply_to_all = ApplyToAll::default();

    let resolved = resolve_volume_conflict(
        &source_dyn,
        Path::new("/notes.txt"),
        &dest_dyn,
        Path::new("/notes.txt"),
        &config,
        &events,
        "op-dest-gone",
        &state,
        &mut apply_to_all,
        None,
        None,
        Some(false),
    )
    .await
    .expect("a missing destination is an answer, not a failure")
    .expect("Overwrite must resolve to a write path");

    // Resolves as file→file: safe-replace via a temp sibling, exactly as it
    // would if the destination were a plain file.
    assert_eq!(resolved.replace_after_write, Some(PathBuf::from("/notes.txt")));
    assert!(
        resolved.write_path.to_string_lossy().contains(".cmdr-tmp-"),
        "expected a temp sibling, got {:?}",
        resolved.write_path
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn file_overwrite_keeps_original_until_temp_is_written() {
    // For a file→file Overwrite, the resolver must NOT delete the existing
    // destination. Instead it hands back a temp sibling to write into plus
    // `replace_after_write: Some(orig)`, so the original survives the full
    // streaming write and is only swapped out at finalize time. This is the
    // safe-replace contract that protects data on a mid-stream failure.
    let dest = Arc::new(InMemoryVolume::new("dest"));
    dest.create_file(Path::new("/notes.txt"), b"old content").await.unwrap();
    let dest_dyn: Arc<dyn Volume> = dest.clone();

    let resolved = apply_volume_conflict_resolution(
        ConflictResolution::Overwrite,
        &dest_dyn,
        Path::new("/notes.txt"),
        false,
        &ClaimedNames::default(),
    )
    .await
    .unwrap()
    .expect("file→file Overwrite must resolve to a write path, not Skip");

    // (a) The original MUST still exist after resolution — current code
    // deletes it here, so this assertion is RED against the buggy version.
    assert!(
        dest.exists(Path::new("/notes.txt")).await,
        "Overwrite resolution must NOT delete the existing FILE before the \
         streaming write. The original must survive so a mid-stream failure \
         can't lose both the old and the new copy."
    );

    // (b) The caller is told to replace `/notes.txt` after the write lands.
    assert_eq!(
        resolved.replace_after_write,
        Some(PathBuf::from("/notes.txt")),
        "file→file Overwrite must request a post-write replace of the original"
    );

    // (c) The write lands in a temp sibling, not directly on the original.
    assert_ne!(resolved.write_path, PathBuf::from("/notes.txt"));
    assert_eq!(resolved.write_path.parent(), Path::new("/notes.txt").parent());
    assert!(
        resolved
            .write_path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.contains(".cmdr-tmp-"))
            .unwrap_or(false),
        "temp sibling should carry the recognizable .cmdr-tmp- marker, got {:?}",
        resolved.write_path
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn finalize_safe_replace_swaps_temp_over_original() {
    // After the streaming write lands the new bytes in the temp sibling,
    // `finalize_safe_replace` must delete the original and rename the temp
    // into its place — leaving exactly the new content and no temp behind.
    let dest = Arc::new(InMemoryVolume::new("dest"));
    dest.create_file(Path::new("/notes.txt"), b"OLD").await.unwrap();
    dest.create_file(Path::new("/notes.txt.cmdr-tmp-abc"), b"NEW")
        .await
        .unwrap();
    let dest_dyn: Arc<dyn Volume> = dest.clone();

    finalize_safe_replace(&dest_dyn, Path::new("/notes.txt.cmdr-tmp-abc"), Path::new("/notes.txt"))
        .await
        .unwrap();

    assert!(!dest.exists(Path::new("/notes.txt.cmdr-tmp-abc")).await);
    let mut stream = dest.open_read_stream(Path::new("/notes.txt")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"NEW");
}
