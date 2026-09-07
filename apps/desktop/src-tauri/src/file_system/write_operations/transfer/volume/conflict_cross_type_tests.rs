//! Cross-type tests for `conflict.rs`: a blanket policy never replaces one KIND
//! of entry with another. A `#[path]` child of `conflict.rs`, so `super::` is
//! `conflict` and `super::super::` is `volume` — the same one-level-shallower
//! rule every `*_tests.rs` in this directory follows.

use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use std::sync::Arc;

// ============================================================================
// A blanket policy never replaces one KIND of entry with another
// ============================================================================

/// Drives `resolve_volume_conflict` for a source file landing on a destination
/// FOLDER under the given blanket policy, and answers what it did to the
/// destination. The folder holds `precious.txt`, so its survival is the whole
/// assertion.
async fn resolve_file_over_folder(policy: ConflictResolution) -> (Arc<InMemoryVolume>, Option<ResolvedConflict>) {
    let source = Arc::new(InMemoryVolume::new("source"));
    source
        .create_file(Path::new("/notes"), b"incoming bytes")
        .await
        .unwrap();
    let source_dyn: Arc<dyn Volume> = source.clone();

    let dest = Arc::new(InMemoryVolume::new("dest"));
    dest.create_directory(Path::new("/notes")).await.unwrap();
    dest.create_file(Path::new("/notes/precious.txt"), b"precious user data")
        .await
        .unwrap();
    let dest_dyn: Arc<dyn Volume> = dest.clone();

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(std::time::Duration::from_millis(0)));
    let config = VolumeCopyConfig {
        conflict_resolution: policy,
        ..VolumeCopyConfig::default()
    };
    let mut apply_to_all = ApplyToAll::default();

    let resolved = resolve_volume_conflict(
        &source_dyn,
        Path::new("/notes"),
        &dest_dyn,
        Path::new("/notes"),
        &config,
        &events,
        "op-blanket-cross-type",
        &state,
        &mut apply_to_all,
        // A source far bigger and newer than any directory entry, so every
        // conditional comparison that treats the folder as a file says
        // "overwrite".
        Some(100_000),
        None,
        Some(false),
    )
    .await
    .expect("a refused cross-type clash is a Skip, never a failure");
    (dest, resolved)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blanket_overwrite_never_clears_a_folder_a_file_landed_on() {
    for policy in [
        ConflictResolution::Overwrite,
        ConflictResolution::OverwriteSmaller,
        ConflictResolution::OverwriteOlder,
    ] {
        let (dest, resolved) = resolve_file_over_folder(policy).await;
        assert!(
            resolved.is_none(),
            "{policy:?} across types must resolve to Skip, got {resolved:?}"
        );
        assert!(
            dest.exists(Path::new("/notes/precious.txt")).await,
            "{policy:?} must not recursively delete the destination folder"
        );
        assert!(
            dest.exists(Path::new("/notes")).await,
            "{policy:?} must leave the destination folder standing"
        );
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_blanket_overwrite_never_deletes_a_file_a_folder_landed_on() {
    for policy in [
        ConflictResolution::Overwrite,
        ConflictResolution::OverwriteSmaller,
        ConflictResolution::OverwriteOlder,
    ] {
        let source = Arc::new(InMemoryVolume::new("source"));
        source.create_directory(Path::new("/thing")).await.unwrap();
        source
            .create_file(Path::new("/thing/inside.txt"), b"incoming")
            .await
            .unwrap();
        let source_dyn: Arc<dyn Volume> = source.clone();

        let dest = Arc::new(InMemoryVolume::new("dest"));
        dest.create_file(Path::new("/thing"), b"precious user bytes")
            .await
            .unwrap();
        let dest_dyn: Arc<dyn Volume> = dest.clone();

        let events = CollectorEventSink::new();
        let state = Arc::new(WriteOperationState::new(std::time::Duration::from_millis(0)));
        let config = VolumeCopyConfig {
            conflict_resolution: policy,
            ..VolumeCopyConfig::default()
        };
        let mut apply_to_all = ApplyToAll::default();

        let resolved = resolve_volume_conflict(
            &source_dyn,
            Path::new("/thing"),
            &dest_dyn,
            Path::new("/thing"),
            &config,
            &events,
            "op-blanket-folder-over-file",
            &state,
            &mut apply_to_all,
            None,
            None,
            Some(true),
        )
        .await
        .expect("a refused cross-type clash is a Skip, never a failure");

        assert!(
            resolved.is_none(),
            "{policy:?} across types must resolve to Skip, got {resolved:?}"
        );
        assert!(
            dest.exists(Path::new("/thing")).await,
            "{policy:?} must not delete the destination file"
        );
    }
}

/// Builds the file→folder clash both cells below resolve: an incoming FILE
/// `/notes` against a destination FOLDER `/notes/` holding one child.
fn file_over_folder_volumes() -> (Arc<InMemoryVolume>, Arc<InMemoryVolume>) {
    (
        Arc::new(InMemoryVolume::new("source")),
        Arc::new(InMemoryVolume::new("dest")),
    )
}

/// Resolves that clash under `apply_to_all` and `configured`.
///
/// ⚠️ `configured` must never be `Stop` unless the latch is guaranteed to
/// answer: nothing here responds to a prompt, so the resolve would park on a
/// oneshot forever and hang the test binary.
async fn resolve_file_over_folder_with_latch(
    op: &str,
    mut apply_to_all: ApplyToAll,
    configured: ConflictResolution,
) -> (Option<ResolvedConflict>, Arc<InMemoryVolume>) {
    let (source, dest) = file_over_folder_volumes();
    source.create_file(Path::new("/notes"), b"incoming").await.unwrap();
    dest.create_directory(Path::new("/notes")).await.unwrap();
    dest.create_file(Path::new("/notes/precious.txt"), b"precious user data")
        .await
        .unwrap();
    let source_dyn: Arc<dyn Volume> = source.clone();
    let dest_dyn: Arc<dyn Volume> = dest.clone();

    let events = CollectorEventSink::new();
    let state = Arc::new(WriteOperationState::new(std::time::Duration::from_millis(0)));
    let config = VolumeCopyConfig {
        conflict_resolution: configured,
        ..VolumeCopyConfig::default()
    };

    let resolved = resolve_volume_conflict(
        &source_dyn,
        Path::new("/notes"),
        &dest_dyn,
        Path::new("/notes"),
        &config,
        &events,
        op,
        &state,
        &mut apply_to_all,
        None,
        None,
        Some(false),
    )
    .await
    .expect("resolving a cross-type clash is never a failure");
    (resolved, dest)
}

/// A carry latched on a file→folder prompt REPLACES, here as on the local
/// engine. The button that fills this bucket says "Overwrite folders with
/// files"; refusing its carry would overwrite the first folder and silently
/// skip the rest.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_latched_cross_type_overwrite_all_replaces_the_folder_it_was_answered_for() {
    let mut apply_to_all = ApplyToAll::default();
    apply_to_all_record(
        &mut apply_to_all,
        ClashKind::FileOverFolder,
        ConflictResolution::Overwrite,
        true,
    );

    // `Stop` is safe here precisely because the latch answers; if the carry
    // were refused this would park on a prompt nothing responds to.
    let (resolved, dest) =
        resolve_file_over_folder_with_latch("op-latched-cross-type", apply_to_all, ConflictResolution::Stop).await;

    assert!(
        resolved.is_some(),
        "an answered 'Overwrite folders with files' must carry, not Skip"
    );
    assert!(
        !dest.exists(Path::new("/notes/precious.txt")).await,
        "the folder the person consented to replace must be gone"
    );
}

/// The counterweight: a carry latched on a SAME-KIND prompt is a blanket policy
/// as far as this clash is concerned. Nobody was shown a folder, so it never
/// reaches across — the configured policy decides instead, and `Skip` here
/// proves the Overwrite in the same-kind bucket did not leak over.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_latched_same_kind_overwrite_all_never_clears_a_folder_a_file_landed_on() {
    let mut apply_to_all = ApplyToAll::default();
    // A non-latching answer first, so the first-clash spread can't put this
    // Overwrite into the cross-type bucket by the back door.
    apply_to_all_record(
        &mut apply_to_all,
        ClashKind::SameKind,
        ConflictResolution::Skip,
        /* apply_to_all */ false,
    );
    apply_to_all_record(
        &mut apply_to_all,
        ClashKind::SameKind,
        ConflictResolution::Overwrite,
        true,
    );

    let (resolved, dest) =
        resolve_file_over_folder_with_latch("op-latched-same-kind", apply_to_all, ConflictResolution::Skip).await;

    assert!(
        resolved.is_none(),
        "the same-kind Overwrite must not reach this clash; the configured Skip decides it"
    );
    assert!(
        dest.exists(Path::new("/notes/precious.txt")).await,
        "a blanket Overwrite must not recursively delete the destination folder"
    );
}
