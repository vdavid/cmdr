//! Unit tests for `conflict.rs` (the cross-volume conflict resolver), split out
//! as a `#[path]` child so the module itself stays readable. `super::` here is
//! `conflict` and `super::super::` is `volume`, exactly as when these lived
//! inline, and the same one-level-shallower rule every other `*_tests.rs` in
//! this directory follows.

use super::*;
use crate::file_system::listing::FileEntry;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::types::CollectorEventSink;
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

    let resolved =
        apply_volume_conflict_resolution(ConflictResolution::Overwrite, &dest_dyn, Path::new("/notes.txt"), false)
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

// ======================================================================
// Conditional resolution (OverwriteSmaller / OverwriteOlder)
// ======================================================================
//
// Same data-safety contract as the local-FS path: a destination is
// overwritten ONLY when strictly smaller / strictly older than the source.
// The volume side has two extra wrinkles the local side doesn't:
//   1. Size hints from the caller (preview scan) can short-circuit the `get_metadata` round-trip;
//      tests cover both hint-provided and hint-absent paths.
//   2. Volume backends may not surface `modified_at` (SMB servers vary). OverwriteOlder must Skip
//      rather than overwrite when mtime is unknown on either side.

/// Build an InMemoryVolume holding a single file at `path` with the given
/// `size` and `modified_at`. The volume's `get_metadata` will return
/// exactly these values, letting tests pin the comparison behavior
/// independent of clock drift.
fn volume_with_file(name: &str, path: &str, size: u64, modified_at: Option<u64>) -> Arc<InMemoryVolume> {
    let entry = FileEntry {
        size: Some(size),
        modified_at,
        created_at: modified_at,
        permissions: 0o644,
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(
            path.rsplit('/').next().unwrap_or(path).to_string(),
            path.to_string(),
            false,
            false,
        )
    };
    Arc::new(InMemoryVolume::with_entries(name, vec![entry]))
}

// ----- OverwriteSmaller -----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_smaller_overwrites_when_dest_strictly_smaller_via_hints() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 1000, Some(100));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 500, Some(100));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteSmaller,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        Some(1000),
        Some(500),
    )
    .await;

    assert_eq!(resolved, ConflictResolution::Overwrite);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_smaller_skips_when_dest_equal_size() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 500, Some(100));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 500, Some(100));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteSmaller,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        Some(500),
        Some(500),
    )
    .await;

    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Equal-size dst must NOT be overwritten on a volume any more than on local FS"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_smaller_skips_when_dest_larger() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(100));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 9999, Some(100));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteSmaller,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        Some(100),
        Some(9999),
    )
    .await;

    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Larger dst must NOT be overwritten — would clobber the user's keeper file"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_smaller_falls_back_to_get_metadata_when_hints_missing() {
    // Critical: when the caller (move path, no scan phase) passes no hints,
    // the reducer must `get_metadata` from each volume rather than
    // defaulting to Skip on absent hints. Otherwise OverwriteSmaller would
    // never actually overwrite on moves.
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 1000, Some(100));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 500, Some(100));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteSmaller,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(
        resolved,
        ConflictResolution::Overwrite,
        "With no hints, the reducer should still get_metadata and compare correctly"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_smaller_skips_when_dest_metadata_unavailable() {
    // Source is fine but dest get_metadata fails (path missing). Reducer
    // must Skip — we can't prove the destination is smaller, so we never
    // touch it.
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 1000, Some(100));
    let dst: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("dst")); // empty

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteSmaller,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(resolved, ConflictResolution::Skip);
}

// ----- OverwriteOlder -----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_older_overwrites_when_dest_strictly_older() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_700_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_600_000_000));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteOlder,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(resolved, ConflictResolution::Overwrite);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_older_skips_when_dest_equal_mtime() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_600_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_600_000_000));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteOlder,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(resolved, ConflictResolution::Skip);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_older_skips_when_dest_strictly_newer() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_600_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_700_000_000));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteOlder,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Newer dst must NOT be overwritten — would clobber the user's fresher file"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_older_skips_when_source_mtime_unknown() {
    // Many SMB servers don't surface modified_at reliably. The reducer
    // must fail closed to Skip rather than defaulting to overwrite.
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, None);
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_600_000_000));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteOlder,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Unknown source mtime must fail closed; we cannot prove dst is older"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_older_skips_when_dest_mtime_unknown() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_700_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, None);

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteOlder,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(
        resolved,
        ConflictResolution::Skip,
        "Unknown dest mtime must fail closed; we cannot prove it's older"
    );
}

// ----- Pass-through -----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_non_conditional_variants_pass_through_unchanged() {
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_600_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_600_000_000));

    for v in [
        ConflictResolution::Stop,
        ConflictResolution::Skip,
        ConflictResolution::Overwrite,
        ConflictResolution::Rename,
    ] {
        let resolved = reduce_volume_conditional_resolution(
            v,
            &src,
            Path::new("/f.bin"),
            &dst,
            Path::new("/f.bin"),
            Some(100),
            Some(100),
        )
        .await;
        assert_eq!(resolved, v, "Variant {v:?} must pass through unchanged");
    }
}

// ======================================================================
// find_unique_volume_name — TOCTOU reservation on local-FS dest volumes
// ======================================================================
//
// Volume-side sibling of `conflict::find_unique_name`. For a Rename
// resolution the chosen `name (N)` must be atomically RESERVED with an
// `O_CREAT|O_EXCL` placeholder when the destination volume is backed by a
// local filesystem (`local_path().is_some()`), so a concurrent writer
// (second Cmdr op, cloud-sync agent, backup tool) can't land a file at the
// same name between our pick and the streaming write. Pre-fix the function
// only probed `dest_volume.exists()` (non-atomic) and returned the path,
// leaving a TOCTOU window. Mirrors `conflict.rs::find_unique_name_tests`.

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_fs_rename_reserves_the_chosen_name_on_disk() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    let temp = tempfile::TempDir::new().unwrap();
    let target = temp.path().join("notes.txt");
    std::fs::write(&target, b"original").unwrap();

    let vol: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("dst", temp.path().to_path_buf()));

    let unique = find_unique_volume_name(&vol, &target).await;

    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "notes (1).txt");
    // The O_EXCL placeholder must already exist on disk after the call.
    assert!(
        unique.exists(),
        "reservation must create the placeholder on a local-FS dest"
    );
    // A second call escalates to (2), proving the first reservation persisted.
    let next = find_unique_volume_name(&vol, &target).await;
    assert_eq!(next.file_name().unwrap().to_string_lossy(), "notes (2).txt");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn local_fs_rename_keeps_extension_in_the_right_place() {
    use crate::file_system::volume::backends::LocalPosixVolume;
    let temp = tempfile::TempDir::new().unwrap();
    let target = temp.path().join("report.pdf");
    std::fs::write(&target, b"x").unwrap();

    let vol: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("dst", temp.path().to_path_buf()));
    let unique = find_unique_volume_name(&vol, &target).await;
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "report (1).pdf");
    assert!(unique.exists(), "reservation must create the placeholder");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn non_local_dest_does_not_reserve_a_placeholder() {
    // MTP / SMB / InMemory have no exclusive-create semantics here
    // (`local_path()` is `None`), so the function must NOT try to touch the
    // real local FS. It returns the next free name based on `exists()`,
    // accepting the documented narrow residual window.
    let dst = Arc::new(InMemoryVolume::new("dst"));
    dst.create_file(Path::new("/notes.txt"), b"old").await.unwrap();
    let dst_dyn: Arc<dyn Volume> = dst.clone();

    let unique = find_unique_volume_name(&dst_dyn, Path::new("/notes.txt")).await;
    assert_eq!(unique.file_name().unwrap().to_string_lossy(), "notes (1).txt");
    // No placeholder was created on the in-memory volume.
    assert!(
        !dst.exists(&unique).await,
        "non-local dest must not pre-create the renamed name"
    );
}

// ----- Axis independence -----

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_smaller_ignores_mtime() {
    // Smaller AND newer dst: still overwrite under OverwriteSmaller.
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 1000, Some(1_600_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 100, Some(1_700_000_000));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteSmaller,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        Some(1000),
        Some(100),
    )
    .await;

    assert_eq!(resolved, ConflictResolution::Overwrite);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn volume_older_ignores_size() {
    // Older AND larger dst: still overwrite under OverwriteOlder.
    let src: Arc<dyn Volume> = volume_with_file("src", "/f.bin", 100, Some(1_700_000_000));
    let dst: Arc<dyn Volume> = volume_with_file("dst", "/f.bin", 9999, Some(1_600_000_000));

    let resolved = reduce_volume_conditional_resolution(
        ConflictResolution::OverwriteOlder,
        &src,
        Path::new("/f.bin"),
        &dst,
        Path::new("/f.bin"),
        None,
        None,
    )
    .await;

    assert_eq!(resolved, ConflictResolution::Overwrite);
}
