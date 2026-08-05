//! What a cover walk reports about itself while it runs, and what it admits
//! afterwards.
//!
//! Two facts a batch can't carry, and both are silent-failure cases:
//!
//! - progress that follows the WALK, so a run receiving no batches still looks
//!   alive rather than frozen at "0 folders scanned";
//! - ground the walk gave up on, so a short list is labelled short instead of
//!   reading as exhaustive (Accepted difference 9).

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio_util::sync::CancellationToken;

use super::test_fixtures::setup_writer;
use super::walker::{RawDirEntry, RawFileType, ReadDirFn, ReadProgress};
use super::{ScanRoot, WalkHeartbeat, WalkPolicy, run_scan};
use crate::indexing::IndexPathSpace;
use crate::indexing::scanner::ScanProgress;

/// A reader over a fixed tree, optionally hanging on one directory.
fn mock_reader(dirs: HashMap<PathBuf, Vec<(&'static str, RawFileType)>>, hang_on: Option<PathBuf>) -> ReadDirFn {
    let dirs = Arc::new(dirs);
    Arc::new(move |p: &Path, progress: &ReadProgress| {
        if hang_on.as_deref() == Some(p) {
            // allowed-test-sleep: this stub fakes a directory that stops
            // responding, which is the only way to reach the abandon path.
            std::thread::sleep(Duration::from_secs(2));
        }
        match dirs.get(p) {
            Some(children) => Ok(children
                .iter()
                .map(|(name, kind)| {
                    progress.record_entries(1);
                    RawDirEntry {
                        path: p.join(name),
                        file_type: *kind,
                        stat: None,
                    }
                })
                .collect()),
            None => Err(std::io::Error::new(std::io::ErrorKind::NotFound, "no mock dir")),
        }
    })
}

/// Run one cover-shaped walk over `dirs` and hand back its heartbeat.
fn walk_with_heartbeat(
    root: &Path,
    dirs: HashMap<PathBuf, Vec<(&'static str, RawFileType)>>,
    hang_on: Option<PathBuf>,
) -> WalkHeartbeat {
    let (writer, _db_path, _db_dir) = setup_writer();
    let heartbeat = WalkHeartbeat::new();
    let progress = Arc::new(ScanProgress::new());
    run_scan(
        root,
        &CancellationToken::new(),
        &progress,
        &writer,
        100,
        2,
        WalkPolicy::for_walk(ScanRoot::Volume, &IndexPathSpace::root(), root),
        &IndexPathSpace::root(),
        mock_reader(dirs, hang_on),
        // Short, so an unresponsive directory is abandoned inside the test rather
        // than after the production 15 s.
        Duration::from_millis(50),
        None,
        Some(&heartbeat),
    )
    .expect("the walk runs");
    writer.shutdown();
    heartbeat
}

#[test]
fn the_heartbeat_counts_directories_the_walk_read_with_nobody_taking_batches() {
    let root = PathBuf::from("/root");
    let mut dirs: HashMap<PathBuf, Vec<(&'static str, RawFileType)>> = HashMap::new();
    dirs.insert(root.clone(), vec![("a", RawFileType::Dir), ("b", RawFileType::Dir)]);
    dirs.insert(root.join("a"), vec![("deep", RawFileType::Dir)]);
    dirs.insert(root.join("a").join("deep"), vec![("leaf.txt", RawFileType::File)]);
    dirs.insert(root.join("b"), vec![("leaf.txt", RawFileType::File)]);

    // `emit: None` above, so not one batch leaves this walk. Progress derived
    // from batches would be zero here; progress derived from the walk is four.
    let heartbeat = walk_with_heartbeat(&root, dirs, None);

    assert_eq!(
        heartbeat
            .dirs_scanned_counter()
            .load(std::sync::atomic::Ordering::Relaxed),
        4,
        "every directory the walk read should be counted"
    );
    let current = heartbeat.current_dir_slot().lock().expect("slot").clone();
    let current = current.expect("the walk should have named a directory it was in");
    assert!(
        current.starts_with("/root"),
        "the named directory should be inside the walked tree, got {current}"
    );
}

#[test]
fn a_walk_that_gave_up_on_a_directory_says_so() {
    let root = PathBuf::from("/root");
    let slow = root.join("slow");
    let mut dirs: HashMap<PathBuf, Vec<(&'static str, RawFileType)>> = HashMap::new();
    dirs.insert(root.clone(), vec![("slow", RawFileType::Dir), ("ok", RawFileType::Dir)]);
    dirs.insert(slow.clone(), vec![("hidden.txt", RawFileType::File)]);
    dirs.insert(root.join("ok"), vec![("seen.txt", RawFileType::File)]);

    let heartbeat = walk_with_heartbeat(&root, dirs, Some(slow));

    // The walk finished (`run_scan` returned `Ok`) having read less than the tree
    // holds. Without this signal, that answer would present itself as exhaustive.
    assert!(
        heartbeat.abandoned_count() > 0,
        "abandoning an unresponsive directory has to be recorded, or a short walk reads as a complete one"
    );
}

#[test]
fn a_walk_that_read_everything_admits_nothing() {
    let root = PathBuf::from("/root");
    let mut dirs: HashMap<PathBuf, Vec<(&'static str, RawFileType)>> = HashMap::new();
    dirs.insert(root.clone(), vec![("a", RawFileType::Dir)]);
    dirs.insert(root.join("a"), vec![("leaf.txt", RawFileType::File)]);

    let heartbeat = walk_with_heartbeat(&root, dirs, None);

    assert_eq!(
        heartbeat.abandoned_count(),
        0,
        "a healthy walk must not claim it gave up on anything"
    );
}
