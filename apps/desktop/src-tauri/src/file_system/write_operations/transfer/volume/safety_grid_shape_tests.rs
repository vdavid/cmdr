//! Tier B of the coverage grid: the SHAPE axis, against a per-path-less cache.
//!
//! `{copy, move}` × item kind `{file, dir-onto-fresh-dest, dir-onto-an-existing-file}`
//! × driver `{serial, concurrent}`, with the cache pinned to `hit-without-per-path`:
//! the other two states are already covered by the existing suites, and that one is
//! the shape that used to be a lie.
//!
//! Tier A (op × cache state × outcome, and what the grid deliberately leaves out) is
//! `safety_grid_tests.rs`, which also owns the fixtures both tiers share.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::safety_grid_tests::{CacheState, make_state, seed_cache};
use super::safety_oracle::try_read_all;
use super::{copy_volumes_with_progress, move_volumes_with_progress};
use crate::file_system::volume::{InMemoryVolume, Volume};
use crate::file_system::write_operations::types::{CollectorEventSink, ConflictResolution, VolumeCopyConfig};

/// The three source shapes Tier B drives, each paired with the destination
/// state it lands on.
#[derive(Clone, Copy, Debug)]
enum ItemKind {
    /// A plain file onto a fresh destination.
    File,
    /// A directory onto a destination that has no such name yet.
    DirOntoFreshDest,
    /// A directory onto a destination FILE of the same name: the cross-type
    /// clash, where a wrong answer about the source's type picks the branch
    /// that replaces the destination wholesale.
    DirOntoExistingFile,
}

impl ItemKind {
    fn label(self) -> &'static str {
        match self {
            ItemKind::File => "file",
            ItemKind::DirOntoFreshDest => "dir-onto-fresh-dest",
            ItemKind::DirOntoExistingFile => "dir-onto-existing-file",
        }
    }
}

const ITEM_KINDS: &[ItemKind] = &[
    ItemKind::File,
    ItemKind::DirOntoFreshDest,
    ItemKind::DirOntoExistingFile,
];

/// Seeds one Tier B shape and returns the sources to hand the driver.
///
/// `filler` extra top-level sources go in for the concurrent driver, which only
/// engages from three sources up; they're plain files so they add no shape.
async fn tier_b_fixture(
    kind: ItemKind,
    concurrent: bool,
) -> (
    Arc<dyn Volume>,
    Arc<dyn Volume>,
    Vec<PathBuf>,
    Vec<(&'static str, &'static [u8])>,
) {
    let source: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));

    let mut sources = Vec::new();
    let expected: Vec<(&'static str, &'static [u8])>;

    match kind {
        ItemKind::File => {
            source.create_file(Path::new("/solo.bin"), b"SRC-solo").await.unwrap();
            sources.push(PathBuf::from("/solo.bin"));
            expected = vec![("/solo.bin", b"SRC-solo")];
        }
        ItemKind::DirOntoFreshDest => {
            source.create_directory(Path::new("/album")).await.unwrap();
            source
                .create_file(Path::new("/album/one.bin"), b"SRC-one")
                .await
                .unwrap();
            source.create_directory(Path::new("/album/inner")).await.unwrap();
            source
                .create_file(Path::new("/album/inner/two.bin"), b"SRC-two")
                .await
                .unwrap();
            sources.push(PathBuf::from("/album"));
            expected = vec![("/album/one.bin", b"SRC-one"), ("/album/inner/two.bin", b"SRC-two")];
        }
        ItemKind::DirOntoExistingFile => {
            source.create_directory(Path::new("/album")).await.unwrap();
            source
                .create_file(Path::new("/album/one.bin"), b"SRC-one")
                .await
                .unwrap();
            dest.create_file(Path::new("/album"), b"DEST-was-a-file").await.unwrap();
            sources.push(PathBuf::from("/album"));
            expected = vec![("/album/one.bin", b"SRC-one")];
        }
    }

    if concurrent {
        for name in ["/filler_a.bin", "/filler_b.bin"] {
            source.create_file(Path::new(name), b"filler").await.unwrap();
            sources.push(PathBuf::from(name));
        }
    }

    (source, dest, sources, expected)
}

/// Drives one Tier B cell through the copy or move pipeline against a cache
/// entry that counted files but recorded no per-source result.
async fn run_tier_b(kind: ItemKind, concurrent: bool, is_move: bool) {
    let (source, dest, sources, expected) = tier_b_fixture(kind, concurrent).await;
    let driver = if concurrent { "concurrent" } else { "serial" };
    let op = if is_move { "move" } else { "copy" };
    let cell = format!("tier-b-{op}-{driver}-{}", kind.label());
    let label = cell.clone();

    let source_strs: Vec<String> = sources.iter().map(|p| p.to_string_lossy().into_owned()).collect();
    let source_refs: Vec<&str> = source_strs.iter().map(String::as_str).collect();
    let preview_id = seed_cache(CacheState::HitWithoutPerPath, &cell, &source_refs, sources.len(), 64);

    let state = make_state();
    let events = Arc::new(CollectorEventSink::new());
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        preview_id,
        ..VolumeCopyConfig::default()
    };

    let result = if is_move {
        move_volumes_with_progress(
            events,
            &cell,
            &state,
            Arc::clone(&source),
            &sources,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await
    } else {
        copy_volumes_with_progress(
            events,
            &cell,
            &state,
            Arc::clone(&source),
            &sources,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        )
        .await
    };
    assert!(result.is_ok(), "{label}: the transfer should complete, got {result:?}");

    // Every shape must arrive intact: a per-path-less cache is missing
    // information, ❌ never license to guess that a directory is a file and
    // stream it as one.
    for (path, content) in &expected {
        assert_eq!(
            try_read_all(&dest, path).await.as_deref(),
            Some(*content),
            "{label}: {path} didn't arrive intact"
        );
    }

    // A move's sources are gone; a copy's are still there. Either way no byte
    // is missing from both sides.
    if !is_move {
        // Source and destination roots are both `/` here, so a delivered path is
        // also the source path it came from.
        for (path, content) in &expected {
            assert_eq!(
                try_read_all(&source, path).await.as_deref(),
                Some(*content),
                "{label}: a COPY took {path} from the source"
            );
        }
    }
}

/// Tier B: every source shape survives a per-path-less cache on the SERIAL
/// copy driver.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_b_copy_serial_survives_a_cache_with_no_per_path() {
    for kind in ITEM_KINDS {
        run_tier_b(*kind, false, false).await;
    }
}

/// Tier B: the same three shapes on the CONCURRENT copy driver, which resolves
/// each source's type on its own task.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tier_b_copy_concurrent_survives_a_cache_with_no_per_path() {
    for kind in ITEM_KINDS {
        run_tier_b(*kind, true, false).await;
    }
}

/// Tier B: the same three shapes on the SERIAL move pipeline, whose source
/// sweep is what a wrong type answer turns destructive.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn tier_b_move_serial_survives_a_cache_with_no_per_path() {
    for kind in ITEM_KINDS {
        run_tier_b(*kind, false, true).await;
    }
}

/// Tier B: the same three shapes on the CONCURRENT move pipeline.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn tier_b_move_concurrent_survives_a_cache_with_no_per_path() {
    for kind in ITEM_KINDS {
        run_tier_b(*kind, true, true).await;
    }
}
