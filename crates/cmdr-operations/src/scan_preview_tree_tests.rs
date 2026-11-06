//! A transfer's scan preview builds the operation's live tree and hands it on
//! in the cached result, and only a transfer's does.

use crate::Operations;
use crate::testing::TestOperations;
use crate::transfer_tree::TreeOrder;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use super::event_sinks::{CollectorScanPreviewSink, ScanPreviewEventSink};
use super::scan_cache::{ScanPreviewState, ScanResult};
use super::scan_preview::{run_scan_preview, run_volume_scan_preview};
use super::scan_watchdog::{ScanTarget, ScanWatchdog};
use super::transfer_tree::test_support::outline;
use super::types::ScanPreviewPurpose;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::{InMemoryVolume, Volume};

fn fresh_state() -> Arc<ScanPreviewState> {
    Arc::new(ScanPreviewState {
        cancelled: AtomicBool::new(false),
        progress_interval: Duration::from_millis(50),
        mac_only: cmdr_fs::mac_only_files::MacOnlyFiles::Include,
    })
}

fn watchdog(engine: &TestOperations, preview_id: &str, sources: &[PathBuf]) -> Arc<ScanWatchdog> {
    ScanWatchdog::start(
        engine,
        preview_id.to_string(),
        ScanTarget::of(sources, "test"),
        Duration::from_secs(600),
        fresh_state(),
        Arc::new(CollectorScanPreviewSink::new()) as Arc<dyn ScanPreviewEventSink>,
    )
}

/// `src/photos/{a.jpg, 2024/b.jpg}` and `src/notes.txt`; returns the two sources.
fn seed(dir: &TestDir) -> Vec<PathBuf> {
    let src = dir.join("src");
    fs::create_dir_all(src.join("photos/2024")).unwrap();
    fs::write(src.join("photos/a.jpg"), b"aaa").unwrap();
    fs::write(src.join("photos/2024/b.jpg"), b"bbbbb").unwrap();
    fs::write(src.join("notes.txt"), b"nn").unwrap();
    vec![src.join("photos"), src.join("notes.txt")]
}

async fn local_preview(
    engine: &TestOperations,
    tag: &str,
    sources: &[PathBuf],
    purpose: ScanPreviewPurpose,
) -> ScanResult {
    let preview_id = format!("tree-{tag}-{}", std::process::id());
    let walk_id = preview_id.clone();
    let walk_sources = sources.to_vec();
    let watchdog = watchdog(engine, &preview_id, sources);
    let walker = Operations::clone(engine);
    tokio::task::spawn_blocking(move || {
        run_scan_preview(
            &walker,
            Arc::new(CollectorScanPreviewSink::new()),
            walk_id,
            walk_sources,
            fresh_state(),
            purpose,
            watchdog,
        );
    })
    .await
    .expect("the walk thread joins");
    engine
        .stores()
        .previews()
        .take_cached_scan_result(&preview_id, sources)
        .expect("the walk completed")
}

const EXPECTED: [&str; 5] = [
    "photos/ 2 files 8 B",
    "  2024/ 1 files 5 B",
    "    b.jpg 5 B",
    "  a.jpg 3 B",
    "notes.txt 2 B",
];

/// The files in the order the operation starts them: the tree's.
const TREE_ORDER: [&str; 3] = ["photos/2024/b.jpg", "photos/a.jpg", "notes.txt"];

fn file_order(result: &ScanResult, dir: &TestDir) -> Vec<String> {
    let src = dir.join("src");
    result
        .files
        .iter()
        .map(|file| file.path.strip_prefix(&src).unwrap_or(&file.path).display().to_string())
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_transfer_preview_of_a_local_selection_carries_its_tree_and_each_files_node() {
    let engine = TestOperations::new();
    let dir = TestDir::new("preview_tree_local");
    let sources = seed(&dir);

    let mut result = local_preview(&engine, "local", &sources, ScanPreviewPurpose::Transfer).await;
    assert!(result.tree.is_some(), "a transfer's preview builds the tree");

    let tree = result
        .order_by_transfer_tree(&sources, &TreeOrder::active(engine.host()))
        .expect("the tree builds");
    assert_eq!(outline(&tree), EXPECTED);
    assert_eq!(file_order(&result, &dir), TREE_ORDER);
    for file in &result.files {
        let node = file.tree_node.expect("every walked file knows its node");
        let name = file.path.file_name().and_then(|name| name.to_str()).unwrap_or_default();
        assert_eq!(tree.name(node), name);
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_delete_or_compress_preview_builds_no_tree() {
    let engine = TestOperations::new();
    let dir = TestDir::new("preview_tree_none");
    let sources = seed(&dir);

    for (tag, purpose) in [
        ("delete", ScanPreviewPurpose::Delete),
        ("compress", ScanPreviewPurpose::Compress),
    ] {
        let result = local_preview(&engine, tag, &sources, purpose).await;
        assert!(result.tree.is_none(), "{purpose:?}");
        assert!(result.files.iter().all(|file| file.tree_node.is_none()), "{purpose:?}");
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_operation_without_a_preview_tree_builds_one_from_the_walk() {
    let engine = TestOperations::new();
    let dir = TestDir::new("preview_tree_fallback");
    let sources = seed(&dir);
    let mut result = local_preview(&engine, "fallback", &sources, ScanPreviewPurpose::Delete).await;

    let tree = result
        .order_by_transfer_tree(&sources, &TreeOrder::active(engine.host()))
        .expect("the tree builds");

    assert_eq!(outline(&tree), EXPECTED);
    assert!(result.files.iter().all(|file| file.tree_node.is_some()));
    assert_eq!(file_order(&result, &dir), TREE_ORDER);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_transfer_preview_of_a_volume_records_its_tree() {
    let engine = TestOperations::new();
    let volume = InMemoryVolume::new("Source");
    volume.create_directory(Path::new("/src/photos/2024")).await.unwrap();
    volume
        .create_file(Path::new("/src/photos/a.jpg"), b"aaa")
        .await
        .unwrap();
    volume
        .create_file(Path::new("/src/photos/2024/b.jpg"), b"bbbbb")
        .await
        .unwrap();
    volume.create_file(Path::new("/src/notes.txt"), b"nn").await.unwrap();
    let sources = vec![PathBuf::from("/src/photos"), PathBuf::from("/src/notes.txt")];
    let preview_id = format!("tree-volume-{}", std::process::id());

    run_volume_scan_preview(
        engine.clone(),
        Arc::new(CollectorScanPreviewSink::new()),
        preview_id.clone(),
        sources.clone(),
        Arc::new(volume) as Arc<dyn Volume>,
        String::from("tree-volume"),
        fresh_state(),
        ScanPreviewPurpose::Transfer,
        watchdog(&engine, &preview_id, &sources),
    )
    .await;

    let result = engine
        .stores()
        .previews()
        .take_cached_scan_result(&preview_id, &sources)
        .expect("the walk completed");
    assert_eq!(
        outline(&result.tree.expect("a transfer's preview builds the tree")),
        EXPECTED
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tree_that_fails_to_build_leaves_the_preview_and_its_walk_order_as_they_were() {
    use super::transfer_tree::test_support::TREE_BUG_NAME;

    let engine = TestOperations::new();
    let dir = TestDir::new("preview_tree_bug");
    let src = dir.join("src");
    fs::create_dir_all(src.join("b")).unwrap();
    fs::write(src.join("z.txt"), b"1").unwrap();
    fs::write(src.join("b").join(TREE_BUG_NAME), b"2").unwrap();
    let sources = vec![src.join("z.txt"), src.join("b")];

    let mut result = local_preview(&engine, "bug", &sources, ScanPreviewPurpose::Transfer).await;
    assert!(result.tree.is_none(), "the preview completes, without a tree");
    let walk_order = file_order(&result, &dir);

    assert!(
        result
            .order_by_transfer_tree(&sources, &TreeOrder::active(engine.host()))
            .is_none()
    );
    assert_eq!(file_order(&result, &dir), walk_order, "the files keep the walk's order");
    assert!(result.files.iter().all(|file| file.tree_node.is_none()));
}
