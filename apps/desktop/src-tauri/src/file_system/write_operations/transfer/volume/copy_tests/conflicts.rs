//! Per-file conflict resolution during a copy: Skip, Overwrite (including
//! file-over-folder and folder-over-file), and skipped files counting toward
//! progress.

use super::*;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_file_copy_skip_conflict() {
    let (source, dest) = make_volumes();

    source.create_file(Path::new("/new.txt"), b"new content").await.unwrap();
    source
        .create_file(Path::new("/conflict.txt"), b"source version")
        .await
        .unwrap();
    // Pre-existing file at destination
    dest.create_file(Path::new("/conflict.txt"), b"dest version")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-skip",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/new.txt"), PathBuf::from("/conflict.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok());

    // New file should be copied
    let mut stream = dest.open_read_stream(Path::new("/new.txt")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"new content");

    // Conflicting file should keep destination version (skip)
    let mut stream = dest.open_read_stream(Path::new("/conflict.txt")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"dest version");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_multi_file_copy_overwrite_conflict() {
    let (source, dest) = make_volumes();

    source
        .create_file(Path::new("/file.txt"), b"new version")
        .await
        .unwrap();
    dest.create_file(Path::new("/file.txt"), b"old version").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-overwrite",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/file.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok());

    // File should have source content (overwritten)
    let mut stream = dest.open_read_stream(Path::new("/file.txt")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"new version");
}

/// File→folder overwrite (volume copy): source is a file, dest holds a folder
/// at the same path. Picking Overwrite must delete the dest folder (recursively)
/// before the streaming writer lands the source file, otherwise the writer
/// fails or no-ops because the path isn't writable as a file.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_volume_overwrite_file_over_existing_folder() {
    let (source, dest) = make_volumes();

    source
        .create_file(Path::new("/clash"), b"I am the new file")
        .await
        .unwrap();
    // Dest is a folder with children at the same path
    dest.create_directory(Path::new("/clash")).await.unwrap();
    dest.create_file(Path::new("/clash/inner.txt"), b"inner").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-file-over-folder",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/clash")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "copy should succeed: {:?}", result);
    // Old folder + its child gone, replaced by the source's file bytes
    assert!(
        !dest.is_directory(Path::new("/clash")).await.unwrap_or(false),
        "dest should no longer be a directory"
    );
    let mut stream = dest.open_read_stream(Path::new("/clash")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"I am the new file");
    assert!(!dest.exists(Path::new("/clash/inner.txt")).await);
}

/// Folder→file overwrite (volume copy): source is a folder, dest is a file at
/// the same path. Overwrite must delete the dest file before the recursive
/// copy creates the directory tree.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_volume_overwrite_folder_over_existing_file() {
    let (source, dest) = make_volumes();

    source.create_directory(Path::new("/clash")).await.unwrap();
    source
        .create_file(Path::new("/clash/inside.txt"), b"inside content")
        .await
        .unwrap();
    // Dest is a file at the same top-level path
    dest.create_file(Path::new("/clash"), b"i am the old file")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-folder-over-file",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/clash")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "copy should succeed: {:?}", result);
    // Dest is now a directory containing the source's file
    assert!(
        dest.is_directory(Path::new("/clash")).await.unwrap_or(false),
        "dest should now be a directory"
    );
    let mut stream = dest.open_read_stream(Path::new("/clash/inside.txt")).await.unwrap();
    assert_eq!(stream.next_chunk().await.unwrap().unwrap(), b"inside content");
}

/// Skipped files must count toward `files_processed` and bump `bytes_done` by the
/// source's size, so the progress bar reflects them. Before this fix, "Skip all"
/// silently ran through dozens of conflicts with the bar pinned at 0%, even though
/// the operation was making progress through every source.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn test_skipped_files_count_toward_progress() {
    let (source, dest) = make_volumes();

    // 3 conflicting sources (all skipped) + 1 fresh source (copied).
    source.create_file(Path::new("/a.txt"), b"AA").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"BBBB").await.unwrap();
    source.create_file(Path::new("/c.txt"), b"CCCCCC").await.unwrap();
    source.create_file(Path::new("/d.txt"), b"D").await.unwrap();

    // Pre-existing at dest → triggers conflict for a, b, c.
    dest.create_file(Path::new("/a.txt"), b"old").await.unwrap();
    dest.create_file(Path::new("/b.txt"), b"old").await.unwrap();
    dest.create_file(Path::new("/c.txt"), b"old").await.unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    // Skip applies to all 3 conflicts; d.txt copies through. `progress_interval_ms: 0`
    // forces every skip + copy to emit a progress event.
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };

    let result = copy_volumes_with_progress(
        events.clone(),
        "test-op-skip-progress",
        &state,
        Arc::clone(&source),
        &[
            PathBuf::from("/a.txt"),
            PathBuf::from("/b.txt"),
            PathBuf::from("/c.txt"),
            PathBuf::from("/d.txt"),
        ],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await;

    assert!(result.is_ok(), "copy should succeed: {:?}", result);

    // Completion event must report all 4 sources as processed (3 skipped + 1 copied).
    let complete = events.complete.lock().unwrap();
    assert_eq!(complete.len(), 1);
    assert_eq!(
        complete[0].files_processed, 4,
        "skipped files must count toward files_processed",
    );
    // Bytes reflect the actually copied file's 1 byte plus the three skipped sources'
    // 2 + 4 + 6 = 12 bytes (their sizes contribute to the size bar via the hint).
    assert_eq!(
        complete[0].bytes_processed, 13,
        "bytes_processed must include skipped source sizes so the size bar advances",
    );

    // Progress events: the 3 skips must each emit with `files_done` advancing.
    // (The final copy's progress events fire *during* its streaming, when
    // `files_done` is still 3; `on_file_complete` bumps it to 4 only at the
    // very end of `copy_single_path` without an extra emit, so the highest
    // value seen in the per-file-progress event stream is 3. The completion
    // event's `files_processed=4` assertion above covers the final state.)
    let progress = events.progress.lock().unwrap();
    let max_files_done = progress.iter().map(|p| p.files_done).max().unwrap_or(0);
    assert!(
        max_files_done >= 3,
        "progress events should advance through the skips; saw max files_done = {max_files_done}",
    );
    // And each skip should have produced its own event with monotonic counter.
    let skip_milestones: Vec<usize> = progress
        .iter()
        .map(|p| p.files_done)
        .filter(|&n| (1..=3).contains(&n))
        .collect();
    assert!(
        skip_milestones.windows(2).all(|w| w[0] <= w[1]),
        "files_done across skip events should be monotonic; saw {skip_milestones:?}",
    );
    assert!(
        skip_milestones.contains(&1) && skip_milestones.contains(&2) && skip_milestones.contains(&3),
        "expected progress events for each of the 3 skipped files; saw {skip_milestones:?}",
    );
}
