//! Folder merge on the MOVE path, where the move invariant lives: **no byte is
//! ever lost**.
//!
//! A move is copy-then-delete-source, so every source file must end up readable
//! from the destination (it moved) or the source (it didn't). The hazard is a
//! merge: the deep walker resolves individual children to Skip, and a skipped
//! child never reached the destination, so its source copy is the only one that
//! exists. The matrix here drives every file policy (including the Stop-mode
//! answers) over both implementations: the cross-volume copy+delete
//! (`volume/move.rs`) and the same-volume rename-merge
//! (`volume/rename_merge.rs`).
//!
//! Shared fixtures live in `volume/move_test_support.rs`
//! (`super::test_support`); the merge fixture trees are local to this file, and
//! the assertions run through `volume/safety_oracle.rs`.

use super::super::super::conflict_responder_test_support::{ConflictResponderSink, folder_conflict_count_both_dirs};
use super::super::move_same::move_within_same_volume_with_progress;
use super::super::safety_oracle::{SafetySpec, assert_operation_was_safe};
use super::test_support::{make_state_with_interval_ms, make_volumes};
use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::ConflictResolution;

/// A folder move that MERGES must keep the source of every child it skipped.
///
/// A skipped child never landed at the destination, so its only copy is the
/// source one. Sweeping the source folder recursively because "the copy phase
/// returned Ok" destroys data the user explicitly chose not to move — the
/// move-path counterpart of the top-level rule that a skipped conflict
/// preserves its source
/// (`cross_volume_move_conflict_skip_preserves_source_and_dest`).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cross_volume_move_folder_merge_keeps_the_source_of_a_skipped_deep_child() {
    let (source, dest) = make_volumes();

    source.create_directory(Path::new("/album")).await.unwrap();
    source
        .create_file(Path::new("/album/clash.txt"), b"SRC-clash")
        .await
        .unwrap();
    source
        .create_file(Path::new("/album/fresh.txt"), b"SRC-fresh")
        .await
        .unwrap();
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/clash.txt"), b"DEST-clash")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state_with_interval_ms(0);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = move_volumes_with_progress(
        events.clone(),
        "op-move-merge-deep-skip",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // The dest keeps its own version of the clashing child (Skip honored).
    let mut kept = dest.open_read_stream(Path::new("/album/clash.txt")).await.unwrap();
    assert_eq!(kept.next_chunk().await.unwrap().unwrap(), b"DEST-clash");

    // The non-clashing child moved through: at the dest, gone from the source.
    assert!(dest.exists(Path::new("/album/fresh.txt")).await);
    assert!(!source.exists(Path::new("/album/fresh.txt")).await);

    // ❗ THE INVARIANT: the skipped child never landed anywhere, so its source
    // must survive. Deleting it loses the only copy of the user's data.
    assert!(
        source.exists(Path::new("/album/clash.txt")).await,
        "a deep child skipped by the conflict policy must keep its source — it never landed at the dest"
    );
}

/// A folder-merge fixture for the move matrix: a source tree and a same-named
/// dest tree overlapping at two depths, with a dest-only file per level and a
/// clashing file per level whose DEST copy is deliberately larger and newer.
/// That makes the conditional policies (`OverwriteSmaller` / `OverwriteOlder`)
/// resolve to Skip, which is exactly the case where a move must not sweep the
/// source.
/// Builds the SOURCE half of the merge fixture under `root`.
async fn build_merge_source_tree(vol: &Arc<dyn Volume>, root: &str) {
    let p = |rest: &str| PathBuf::from(format!("{root}{rest}"));
    vol.create_directory(&p("")).await.unwrap();
    vol.create_file(&p("/fresh.txt"), b"SRC-fresh").await.unwrap();
    vol.create_file(&p("/clash.txt"), b"SRC-c").await.unwrap();
    vol.create_directory(&p("/sub")).await.unwrap();
    vol.create_file(&p("/sub/fresh2.txt"), b"SRC-fresh2").await.unwrap();
    vol.create_file(&p("/sub/clash2.txt"), b"SRC-c2").await.unwrap();
    // Cross-type clash A: source FILE onto a dest DIRECTORY.
    vol.create_file(&p("/swap"), b"SRC-swap-file").await.unwrap();
    // Cross-type clash B: source DIRECTORY onto a dest FILE.
    vol.create_directory(&p("/swap2")).await.unwrap();
    vol.create_file(&p("/swap2/inner.txt"), b"SRC-swap2-inner")
        .await
        .unwrap();
}

/// Builds the DESTINATION half of the merge fixture under `root`: a dest-only
/// file per level, plus a clashing counterpart for each source item whose copy
/// is deliberately LARGER (so `OverwriteSmaller` reduces to Skip) and newer.
async fn build_merge_dest_tree(vol: &Arc<dyn Volume>, root: &str) {
    let p = |rest: &str| PathBuf::from(format!("{root}{rest}"));
    vol.create_directory(&p("")).await.unwrap();
    vol.create_file(&p("/keep.txt"), b"DEST-keep").await.unwrap();
    vol.create_file(&p("/clash.txt"), b"DEST-clash-is-bigger")
        .await
        .unwrap();
    vol.create_directory(&p("/sub")).await.unwrap();
    vol.create_file(&p("/sub/keep2.txt"), b"DEST-keep2").await.unwrap();
    vol.create_file(&p("/sub/clash2.txt"), b"DEST-clash2-is-bigger")
        .await
        .unwrap();
    // The other half of the two cross-type clashes.
    vol.create_directory(&p("/swap")).await.unwrap();
    vol.create_file(&p("/swap/inner.txt"), b"DEST-swap-inner")
        .await
        .unwrap();
    vol.create_file(&p("/swap2"), b"DEST-swap2-file").await.unwrap();
}

/// A folder-merge fixture for the move matrix: a source tree and a same-named
/// dest tree overlapping at two depths, with a dest-only file per level, a
/// clashing file per level whose DEST copy is deliberately larger and newer
/// (so `OverwriteSmaller` / `OverwriteOlder` resolve to Skip — exactly the case
/// where a move must not sweep the source), and both cross-type clashes.
async fn make_move_merge_fixture() -> (Arc<dyn Volume>, Arc<dyn Volume>) {
    let (source, dest) = make_volumes();
    build_merge_source_tree(&source, "/album").await;
    build_merge_dest_tree(&dest, "/album").await;
    (source, dest)
}

/// Every file policy, paired with the answer a Stop-mode prompt gets scripted.
const MOVE_MERGE_POLICIES: &[(ConflictResolution, Option<ConflictResolution>)] = &[
    (ConflictResolution::Skip, None),
    (ConflictResolution::Overwrite, None),
    (ConflictResolution::Rename, None),
    (ConflictResolution::OverwriteSmaller, None),
    (ConflictResolution::OverwriteOlder, None),
    (ConflictResolution::Stop, Some(ConflictResolution::Skip)),
    (ConflictResolution::Stop, Some(ConflictResolution::Overwrite)),
    (ConflictResolution::Stop, Some(ConflictResolution::Rename)),
    (ConflictResolution::Stop, Some(ConflictResolution::OverwriteSmaller)),
    (ConflictResolution::Stop, Some(ConflictResolution::OverwriteOlder)),
];

/// Every source file the merge fixture creates, relative to `/album`, with its
/// content. The last two are the cross-type clashes (source FILE onto a dest
/// DIRECTORY, and a file inside a source DIRECTORY landing on a dest FILE) — a
/// type swap replaces the destination wholesale by design, but the SOURCE side
/// still has to survive somewhere.
const MOVE_MERGE_SOURCE_FILES: &[(&str, &[u8])] = &[
    ("/fresh.txt", b"SRC-fresh"),
    ("/clash.txt", b"SRC-c"),
    ("/sub/fresh2.txt", b"SRC-fresh2"),
    ("/sub/clash2.txt", b"SRC-c2"),
    ("/swap", b"SRC-swap-file"),
    ("/swap2/inner.txt", b"SRC-swap2-inner"),
];

/// The source-only files, which land at the destination under every policy:
/// nothing shadows them, so no policy has a say. A clashing file's landing spot
/// IS a policy question, so it stays with oracle clause 1.
const MOVE_MERGE_DELIVERED: &[(&str, &[u8])] = &[("/fresh.txt", b"SRC-fresh"), ("/sub/fresh2.txt", b"SRC-fresh2")];

/// The dest-only files, which no policy may touch. Only the same-type merge
/// levels: a cross-type swap replaces the dest wholesale by design, so
/// `/album/swap`'s inner file is deliberately out of scope.
const MOVE_MERGE_UNTOUCHED_DEST: &[(&str, &[u8])] = &[("/keep.txt", b"DEST-keep"), ("/sub/keep2.txt", b"DEST-keep2")];

/// The oracle spec for a finished move over this fixture.
///
/// `dest_prefix` is `""` cross-volume and `"/dest"` for the same-volume move,
/// where both trees live on one volume.
fn move_merge_spec<'a>(dest_root: &'a str, label: &'a str) -> SafetySpec<'a> {
    SafetySpec {
        label,
        source_root: "/album",
        dest_root,
        source_files: MOVE_MERGE_SOURCE_FILES,
        delivered: MOVE_MERGE_DELIVERED,
        untouched_dest: MOVE_MERGE_UNTOUCHED_DEST,
    }
}

/// THE MOVE INVARIANT, over every file policy: **no byte is ever lost**.
///
/// A move is copy-then-delete-source, so every source file must end up readable
/// from EITHER the destination (it moved) OR the source (it didn't). A file that
/// is gone from both is destroyed data. The merge invariant rides along: every
/// dest-only file must survive untouched.
///
/// The copy pipeline has this matrix
/// (`volume/merge_tests.rs::merge_never_deletes_unshadowed_dest_files_under_every_policy`);
/// the move pipeline had no folder-merge coverage at all, which is how the
/// source-sweep hole survived.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn move_folder_merge_never_loses_a_byte_under_every_policy() {
    for (policy, scripted) in MOVE_MERGE_POLICIES {
        let (source, dest) = make_move_merge_fixture().await;
        let state = make_state_with_interval_ms(0);
        let events = Arc::new(ConflictResponderSink::new(
            &state,
            scripted.unwrap_or(ConflictResolution::Skip),
            true,
        ));
        let config = VolumeCopyConfig {
            conflict_resolution: *policy,
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        };

        let result = move_volumes_with_progress(
            events.clone(),
            &format!("op-move-merge-{policy:?}-{scripted:?}"),
            &state,
            Arc::clone(&source),
            &[PathBuf::from("/album")],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;
        assert!(
            result.is_ok(),
            "policy {policy:?}/{scripted:?} should complete, got {result:?}"
        );

        // ❗ NO BYTE LOST, everything unshadowed arrived, and the dest-only
        // files survive untouched.
        let label = format!("policy {policy:?}/{scripted:?}");
        assert_operation_was_safe(&source, &dest, &move_merge_spec("/album", &label)).await;

        // A dir-vs-dir clash never prompts, on the move path too.
        assert_eq!(
            folder_conflict_count_both_dirs(&events.inner),
            0,
            "policy {policy:?}/{scripted:?}: a dir-vs-dir merge wrongly emitted a folder conflict"
        );
    }
}

/// The same no-byte-lost matrix for the SAME-volume move, which is a recursive
/// rename-merge rather than copy+delete — a completely separate implementation
/// (`volume/rename_merge.rs`) with the same promises to keep.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn same_volume_move_folder_merge_never_loses_a_byte_under_every_policy() {
    for (policy, scripted) in MOVE_MERGE_POLICIES {
        // One volume holding both trees: `/album` merges onto `/dest/album`.
        let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("One").with_space_info(10_000_000, 10_000_000));
        build_merge_source_tree(&volume, "/album").await;
        volume.create_directory(Path::new("/dest")).await.unwrap();
        build_merge_dest_tree(&volume, "/dest/album").await;

        let state = make_state_with_interval_ms(0);
        let events = Arc::new(ConflictResponderSink::new(
            &state,
            scripted.unwrap_or(ConflictResolution::Skip),
            true,
        ));
        let config = VolumeCopyConfig {
            conflict_resolution: *policy,
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        };

        let result = move_within_same_volume_with_progress(
            events.clone(),
            &format!("op-same-merge-{policy:?}-{scripted:?}"),
            &state,
            Arc::clone(&volume),
            &[PathBuf::from("/album")],
            Path::new("/dest"),
            &config,
        )
        .await;
        assert!(
            result.is_ok(),
            "policy {policy:?}/{scripted:?} should complete, got {result:?}"
        );

        // Same invariants, with `/dest` prefixed onto the destination side.
        let label = format!("same-volume policy {policy:?}/{scripted:?}");
        assert_operation_was_safe(&volume, &volume, &move_merge_spec("/dest/album", &label)).await;
        assert_eq!(
            folder_conflict_count_both_dirs(&events.inner),
            0,
            "{label}: a dir-vs-dir merge wrongly emitted a folder conflict"
        );
    }
}
