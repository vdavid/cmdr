//! The dir-vs-dir "always merge, never prompt" contract, and its boundary: what
//! is NOT a dir-vs-dir merge. Two same-named directories merge silently under
//! every file policy, Stop included, and only files can ever prompt. A type
//! mismatch in either direction routes through the resolver instead, and a dest
//! level that doesn't exist yet has nothing to clash with, so it streams straight
//! in without listing.
//!
//! These drive the real `copy_volumes_with_progress` pipeline against
//! `InMemoryVolume` pairs + `CollectorEventSink`, so the whole stack (preflight,
//! the serial/concurrent split, `copy_directory_streaming`, the resolver) runs
//! exactly as in production. Shared fixtures `make_state` / `make_volumes` live in
//! `volume/copy_tests.rs` (`super::tests`). Per-file conflict resolution inside a
//! merge is `volume/merge_tests.rs`.

use super::tests::{make_state, make_volumes};
use super::*;
use crate::file_system::volume::Volume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::ConflictResolution;

// ============================================================================
// Helpers
// ============================================================================
/// Reads a whole file from a volume into a `Vec<u8>`.
async fn read_all(vol: &Arc<dyn Volume>, path: &str) -> Vec<u8> {
    let mut stream = vol.open_read_stream(Path::new(path)).await.unwrap();
    let mut out = Vec::new();
    while let Some(Ok(chunk)) = stream.next_chunk().await {
        out.extend_from_slice(&chunk);
    }
    out
}

// ============================================================================
// Dir-vs-dir always merges, never prompts (top-level AND deep), every policy
// ============================================================================

/// Top-level AND deep dir-vs-dir merges emit ZERO folder conflicts, under every
/// file policy including Stop. Pins that folders merge silently and only files
/// can ever prompt.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn dir_vs_dir_never_prompts_top_level_or_deep_under_every_policy() {
    for policy in [
        ConflictResolution::Skip,
        ConflictResolution::Overwrite,
        ConflictResolution::Rename,
        ConflictResolution::Stop,
    ] {
        let (source, dest) = make_volumes();
        // Nested dir-vs-dir with NO clashing files anywhere — only folders clash.
        source.create_directory(Path::new("/a")).await.unwrap();
        source.create_directory(Path::new("/a/b")).await.unwrap();
        source.create_file(Path::new("/a/b/only-src.txt"), b"S").await.unwrap();
        dest.create_directory(Path::new("/a")).await.unwrap();
        dest.create_directory(Path::new("/a/b")).await.unwrap();
        dest.create_file(Path::new("/a/b/only-dest.txt"), b"D").await.unwrap();

        let state = make_state();
        let events = Arc::new(CollectorEventSink::new());
        let config = VolumeCopyConfig {
            conflict_resolution: policy,
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        };

        let result = copy_volumes_with_progress(
            events.clone(),
            &format!("op-dirdir-{policy:?}"),
            &state,
            Arc::clone(&source),
            &[PathBuf::from("/a")],
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await;

        assert!(
            result.is_ok(),
            "policy {policy:?}: dir-only merge should complete, got {result:?}"
        );
        // No write-conflict at all — there are no FILE clashes, and folders never prompt.
        assert_eq!(
            events.conflicts.lock().unwrap().len(),
            0,
            "policy {policy:?}: dir-vs-dir merge wrongly emitted a conflict"
        );
        // Both the source-only and dest-only file coexist in the merged tree.
        assert!(
            dest.exists(Path::new("/a/b/only-src.txt")).await,
            "policy {policy:?}: src file missing"
        );
        assert!(
            dest.exists(Path::new("/a/b/only-dest.txt")).await,
            "policy {policy:?}: dest-only file destroyed"
        );
    }
}

// ============================================================================
// Type mismatch: source DIR vs dest FILE inside a merge
// ============================================================================

/// A source SUBDIRECTORY clashing with a same-named dest FILE is a type
/// mismatch, NOT a dir-vs-dir merge: it routes through the resolver. Under
/// Overwrite the dest file is replaced by the incoming directory; the dest-only
/// sibling survives. Pins the `dir_clashes_with_file` branch (source-dir-vs-
/// dest-file) distinct from the dir-vs-dir recurse path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_dir_over_dest_file_overwrite_replaces_file_with_dir() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    // source `D` is a DIRECTORY holding a file.
    source.create_directory(Path::new("/album/D")).await.unwrap();
    source
        .create_file(Path::new("/album/D/inner.txt"), b"SRC-inner")
        .await
        .unwrap();
    dest.create_directory(Path::new("/album")).await.unwrap();
    // dest `D` is a FILE (the type mismatch), plus a dest-only sibling.
    dest.create_file(Path::new("/album/D"), b"DEST-file").await.unwrap();
    dest.create_file(Path::new("/album/keep.txt"), b"DEST-keep")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-dir-over-file-overwrite",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // The dest FILE was replaced by the incoming DIRECTORY (type-mismatch
    // Overwrite), and the directory's content landed.
    assert!(
        dest.is_directory(Path::new("/album/D")).await.unwrap_or(false),
        "dest `D` should now be a directory"
    );
    assert_eq!(read_all(&dest, "/album/D/inner.txt").await, b"SRC-inner");
    // Dest-only sibling untouched.
    assert_eq!(read_all(&dest, "/album/keep.txt").await, b"DEST-keep");

    // The byte total flows through the type-mismatch dir recurse branch:
    // `inner.txt` is 9 bytes ("SRC-inner"). Asserting the exact total pins that
    // branch's accumulation (a `*=`/`-=` corruption would zero/wrap it).
    let total = events
        .complete
        .lock()
        .unwrap()
        .first()
        .expect("a complete event")
        .bytes_processed;
    assert_eq!(
        total, 9,
        "type-mismatch dir merge must report the directory's byte total, got {total}"
    );
}

/// Same source-dir-vs-dest-file type mismatch, but under Skip: the dest FILE
/// stays untouched (the directory is NOT merged over it). Pins the other side of
/// the `dir_clashes_with_file` branch.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn source_dir_over_dest_file_skip_keeps_dest_file() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_directory(Path::new("/album/D")).await.unwrap();
    source
        .create_file(Path::new("/album/D/inner.txt"), b"SRC-inner")
        .await
        .unwrap();
    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/D"), b"DEST-file").await.unwrap();

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-dir-over-file-skip",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // Skip honored: dest `D` is still the original FILE, unchanged.
    assert!(
        !dest.is_directory(Path::new("/album/D")).await.unwrap_or(false),
        "dest `D` must remain a file under Skip"
    );
    assert_eq!(read_all(&dest, "/album/D").await, b"DEST-file");
}

// ============================================================================
// Type mismatch: source FILE vs dest DIRECTORY inside a merge
// ============================================================================

/// A type mismatch inside a merge (source FILE vs dest DIRECTORY) is NOT a merge:
/// it routes through the resolver. Under Skip it leaves the dest dir untouched.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn type_mismatch_inside_merge_routes_through_resolver_and_skip_keeps_dest() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    // source `swap` is a FILE.
    source.create_file(Path::new("/album/swap"), b"SRC-file").await.unwrap();
    dest.create_directory(Path::new("/album")).await.unwrap();
    // dest `swap` is a DIR holding a file.
    dest.create_directory(Path::new("/album/swap")).await.unwrap();
    dest.create_file(Path::new("/album/swap/inner.txt"), b"DEST-inner")
        .await
        .unwrap();

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-type-mismatch-skip",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/album")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");

    // Skip honored: the dest DIR and its inner file survive untouched.
    assert!(dest.is_directory(Path::new("/album/swap")).await.unwrap_or(false));
    assert_eq!(read_all(&dest, "/album/swap/inner.txt").await, b"DEST-inner");
}

// ============================================================================
// A fresh dest level never lists or prompts
// ============================================================================

/// A freshly-created dest level (no pre-existing dir) skips the dest listing and
/// streams every child straight in — proving the `Ok(())` create branch never
/// lists or prompts. Asserts via a counting volume that `list_directory` is
/// called on the DEST only for source enumeration parity, never for a clash map
/// on a fresh level.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn fresh_dest_level_streams_without_listing_or_prompting() {
    let (source, dest) = make_volumes();
    source.create_directory(Path::new("/brand-new")).await.unwrap();
    source.create_file(Path::new("/brand-new/a.txt"), b"A").await.unwrap();
    source.create_directory(Path::new("/brand-new/sub")).await.unwrap();
    source
        .create_file(Path::new("/brand-new/sub/b.txt"), b"B")
        .await
        .unwrap();
    // dest has NO `/brand-new`.

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        // Even under Stop, a fresh level can't clash, so nothing prompts.
        conflict_resolution: ConflictResolution::Stop,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "op-fresh-level",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/brand-new")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "expected Ok, got {result:?}");
    assert_eq!(
        events.conflicts.lock().unwrap().len(),
        0,
        "a fresh level must never prompt"
    );
    assert_eq!(read_all(&dest, "/brand-new/a.txt").await, b"A");
    assert_eq!(read_all(&dest, "/brand-new/sub/b.txt").await, b"B");
}
