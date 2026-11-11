//! A copy lands a symlink as a symlink where the destination holds one, and
//! skips it, counted, where neither the destination nor the source can do better.
//! `DETAILS.md` § "A copy carries a link as the link".

use cmdr_fs::testing::symlink;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::super::cleanup::volume_rollback_with_progress;
use super::super::copy::copy_volumes_with_progress;
use super::super::faulty_volume::{FaultyOp, FaultyVolume};
use super::super::move_cross::move_volumes_with_progress;
use super::super::transfer_error::WriteFailure;
use crate::event_sinks::CollectorEventSink;
use crate::state::WindDown;
use crate::testing::TestOperations;
use crate::transfer::conflict_responder_test_support::ConflictResponderSink;
use crate::types::{ConflictResolution, VolumeCopyConfig, WriteOperationPhase};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::TestDir;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};
use cmdr_local::LocalPosixVolume;

/// A remote that can hold a link (an SFTP server, to the engine).
fn holds_links() -> Arc<InMemoryVolume> {
    Arc::new(
        InMemoryVolume::new("Holds links")
            .with_space_info(10_000_000, 10_000_000)
            .with_links(),
    )
}

/// A remote with no links at all (SMB, WebDAV, a phone).
fn no_links() -> Arc<InMemoryVolume> {
    Arc::new(InMemoryVolume::new("No links").with_space_info(10_000_000, 10_000_000))
}

/// `/tree/keep.txt` plus `/tree/link`, whose target escapes any destination.
const ESCAPING: &str = "../../../etc/passwd";
async fn source_with_link() -> Arc<InMemoryVolume> {
    let source = no_links();
    source.create_directory(Path::new("/tree")).await.unwrap();
    source.create_file(Path::new("/tree/keep.txt"), b"kept").await.unwrap();
    source.plant_symlink(Path::new("/tree/link"), Path::new(ESCAPING));
    source
}

fn local_folder(dir: &Path) -> Arc<dyn Volume> {
    Arc::new(LocalPosixVolume::local_folder(
        "Local",
        dir.to_path_buf(),
        crate::host::testing::pane_order_host(),
    ))
}

async fn copy(
    engine: &TestOperations,
    source: Arc<dyn Volume>,
    sources: &[PathBuf],
    dest: Arc<dyn Volume>,
) -> (Result<(), WriteFailure>, Arc<CollectorEventSink>) {
    let events = Arc::new(CollectorEventSink::new());
    let result = copy_volumes_with_progress(
        events.clone(),
        "op-link-carry",
        &Arc::new(engine.new_operation_state(Duration::from_millis(50))),
        source,
        sources,
        dest,
        Path::new("/"),
        &VolumeCopyConfig {
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        },
    )
    .await;
    (result, events)
}

async fn target_at(volume: &InMemoryVolume, path: &str) -> Option<PathBuf> {
    volume.read_link_target(Path::new(path)).await.unwrap()
}

fn skipped(events: &CollectorEventSink) -> usize {
    events.complete.lock_ignore_poison()[0].files_skipped
}

fn no_temps(names: &[String]) {
    assert!(
        !names.iter().any(|name| name.starts_with(".cmdr-tmp-")),
        "no staging left behind: {names:?}"
    );
}

async fn names_at(volume: &dyn Volume, dir: &str) -> Vec<String> {
    let listed = volume.list_directory(Path::new(dir), None).await.unwrap();
    listed.into_iter().map(|entry| entry.name).collect()
}

/// The restore case, an escaping target included: the link lands holding exactly
/// its bytes, and nothing is created where it points.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_copy_lands_a_link_verbatim_wherever_it_points() {
    let engine = TestOperations::new();
    let dest = holds_links();
    let (result, events) = copy(
        &engine,
        source_with_link().await,
        &[PathBuf::from("/tree")],
        dest.clone(),
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        target_at(&dest, "/tree/link").await.as_deref(),
        Some(Path::new(ESCAPING))
    );
    assert!(dest.exists(Path::new("/tree/keep.txt")).await);
    assert_eq!(skipped(&events), 0);
    no_temps(&names_at(&*dest, "/tree").await);
    no_temps(&names_at(&*dest, "/").await);
}

/// Three sources take the concurrent driver, whose ledger is its own.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_concurrent_driver_lands_selected_and_nested_links_too() {
    let engine = TestOperations::new();
    let source = source_with_link().await;
    source.plant_symlink(Path::new("/alone-a"), Path::new("/abs/a"));
    source.plant_symlink(Path::new("/alone-b"), Path::new("b"));
    let dest = holds_links();
    let sources = [
        PathBuf::from("/tree"),
        PathBuf::from("/alone-a"),
        PathBuf::from("/alone-b"),
    ];
    let (result, events) = copy(&engine, source, &sources, dest.clone()).await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        target_at(&dest, "/tree/link").await.as_deref(),
        Some(Path::new(ESCAPING))
    );
    assert_eq!(target_at(&dest, "/alone-a").await.as_deref(), Some(Path::new("/abs/a")));
    assert_eq!(target_at(&dest, "/alone-b").await.as_deref(), Some(Path::new("b")));
    assert_eq!(skipped(&events), 0);
}

/// A link selected on its own used to fail as an I/O error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_link_lands_as_the_link() {
    let engine = TestOperations::new();
    let source = no_links();
    source.plant_symlink(Path::new("/alone"), Path::new("somewhere"));
    let dest = holds_links();
    let (result, events) = copy(&engine, source, &[PathBuf::from("/alone")], dest.clone()).await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        target_at(&dest, "/alone").await.as_deref(),
        Some(Path::new("somewhere"))
    );
    let last = events.progress.lock_ignore_poison().last().cloned().expect("progress");
    assert_eq!((last.files_done, last.files_total), (1, 1), "one leaf, done");
}

/// A source that holds a link it has no bytes for, on a destination with no links.
fn answering_no_content(source: Arc<InMemoryVolume>, link: &str) -> Arc<FaultyVolume<InMemoryVolume>> {
    FaultyVolume::wrapping(source)
        .failing_call_at(
            FaultyOp::OpenReadStream,
            link,
            1,
            VolumeError::NoContent(link.to_string()),
        )
        .arc()
}

/// The issue's second half: alone, the link is a counted skip, ❌ not an I/O error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_link_a_destination_cant_hold_is_a_counted_skip_not_a_failure() {
    let engine = TestOperations::new();
    let inner = no_links();
    inner.plant_symlink(Path::new("/alone"), Path::new("somewhere"));
    let source = answering_no_content(inner, "/alone");
    let dest = no_links();
    let (result, events) = copy(&engine, source.clone(), &[PathBuf::from("/alone")], dest.clone()).await;

    assert!(source.fault_fired(FaultyOp::OpenReadStream));
    assert!(result.is_ok(), "{result:?}");
    assert!(!dest.exists(Path::new("/alone")).await);
    assert_eq!(skipped(&events), 1);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn the_concurrent_driver_counts_a_selected_link_it_cant_land() {
    let engine = TestOperations::new();
    let inner = no_links();
    inner.plant_symlink(Path::new("/alone"), Path::new("somewhere"));
    inner.create_file(Path::new("/a.txt"), b"a").await.unwrap();
    inner.create_file(Path::new("/b.txt"), b"b").await.unwrap();
    let source = answering_no_content(inner, "/alone");
    let dest = no_links();
    let sources = [
        PathBuf::from("/a.txt"),
        PathBuf::from("/alone"),
        PathBuf::from("/b.txt"),
    ];
    let (result, events) = copy(&engine, source.clone(), &sources, dest.clone()).await;

    assert!(source.fault_fired(FaultyOp::OpenReadStream));
    assert!(result.is_ok(), "{result:?}");
    assert!(dest.exists(Path::new("/a.txt")).await && dest.exists(Path::new("/b.txt")).await);
    assert_eq!(skipped(&events), 1);
}

/// The bars finish: a skipped child counts toward the file total.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_skipped_child_still_moves_the_file_bar() {
    let engine = TestOperations::new();
    let source = answering_no_content(source_with_link().await, "/tree/link");
    let (result, events) = copy(&engine, source, &[PathBuf::from("/tree")], no_links()).await;

    assert!(result.is_ok(), "{result:?}");
    let last = events
        .progress
        .lock_ignore_poison()
        .iter()
        .rfind(|e| e.phase == WriteOperationPhase::Copying)
        .cloned()
        .expect("progress");
    assert_eq!(last.files_done, last.files_total);
    assert_eq!(skipped(&events), 1);
}

/// `/real/a.txt`, plus `/tree/link -> ../real` and `/tree/keep.txt`.
fn local_tree(dir: &Path) {
    std::fs::create_dir_all(dir.join("real")).unwrap();
    std::fs::write(dir.join("real/a.txt"), b"aaa").unwrap();
    std::fs::create_dir_all(dir.join("tree")).unwrap();
    std::fs::write(dir.join("tree/keep.txt"), b"kept").unwrap();
    symlink("../real", dir.join("tree/link")).unwrap();
}

/// Local to a link-holding remote: the walk never goes through the link, and its
/// target's bytes don't count.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_local_link_to_a_folder_lands_as_the_link_on_a_destination_that_holds_it() {
    let engine = TestOperations::new();
    let dir = TestDir::new("link-carry-local");
    local_tree(&dir);
    let dest = holds_links();
    let (result, events) = copy(&engine, local_folder(&dir), &[PathBuf::from("/tree")], dest.clone()).await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(
        target_at(&dest, "/tree/link").await.as_deref(),
        Some(Path::new("../real"))
    );
    assert!(!dest.exists(Path::new("/tree/link/a.txt")).await);
    let last = events.progress.lock_ignore_poison().last().cloned().expect("progress");
    assert_eq!(last.files_total, 2, "the file and the link");
    assert_eq!(last.files_done, 2);
    assert_eq!(last.bytes_total, 4, "the file's bytes alone");
}

/// A selected local link to a folder: one leaf, and no walk of what's behind it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_selected_local_link_counts_as_one_leaf_when_it_lands_as_the_link() {
    let engine = TestOperations::new();
    let dir = TestDir::new("link-carry-selected-local");
    local_tree(&dir);
    let dest = holds_links();
    let (result, events) = copy(
        &engine,
        local_folder(&dir),
        &[PathBuf::from("/tree/link")],
        dest.clone(),
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert_eq!(target_at(&dest, "/link").await.as_deref(), Some(Path::new("../real")));
    let last = events.progress.lock_ignore_poison().last().cloned().expect("progress");
    assert_eq!((last.files_total, last.bytes_total), (1, 0));
}

/// Where the link can't land, a source with bytes keeps its documented route: the
/// destination gets what the link points at.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_with_no_links_still_gets_what_a_local_link_points_at() {
    let engine = TestOperations::new();
    let dir = TestDir::new("link-carry-materialized");
    local_tree(&dir);
    let dest = no_links();
    let (result, _) = copy(&engine, local_folder(&dir), &[PathBuf::from("/tree")], dest.clone()).await;

    assert!(result.is_ok(), "{result:?}");
    assert!(dest.exists(Path::new("/tree/link/a.txt")).await);
}

/// A link-capable backend on a filesystem with no links (exFAT under a local
/// volume): the proof fails BEFORE any conflict is decided, so the older route
/// runs whole, and the proof leaves nothing behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_destination_whose_filesystem_refuses_links_takes_the_older_route() {
    let engine = TestOperations::new();
    let dir = TestDir::new("link-carry-refused");
    local_tree(&dir);
    let dest = Arc::new(
        InMemoryVolume::new("exFAT")
            .with_space_info(10_000_000, 10_000_000)
            .with_links_refused(),
    );
    let (result, events) = copy(&engine, local_folder(&dir), &[PathBuf::from("/tree")], dest.clone()).await;

    assert!(result.is_ok(), "{result:?}");
    assert!(dest.exists(Path::new("/tree/link/a.txt")).await);
    assert_eq!(skipped(&events), 0);
    no_temps(&names_at(&*dest, "/").await);
}

/// A move's rule stands (#570): followed, and the sweep takes the link alone.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_still_follows_a_link_onto_a_destination_that_could_hold_it() {
    let engine = TestOperations::new();
    let dir = TestDir::new("link-carry-move");
    local_tree(&dir);
    let dest = holds_links();
    let result = move_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "op-link-carry-move",
        &Arc::new(engine.new_operation_state(Duration::from_millis(50))),
        local_folder(&dir),
        &[PathBuf::from("/tree")],
        Arc::clone(&dest) as Arc<dyn Volume>,
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert!(dest.exists(Path::new("/tree/link/a.txt")).await);
    assert_eq!(
        std::fs::read(dir.join("real/a.txt")).unwrap(),
        b"aaa",
        "the target stays"
    );
}

/// A copy inside one volume is left exactly as it was.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_copy_inside_one_volume_carries_nothing() {
    let engine = TestOperations::new();
    let volume = holds_links();
    volume.create_directory(Path::new("/real")).await.unwrap();
    volume.plant_symlink(Path::new("/alone"), Path::new("/real"));
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
    let as_dyn: Arc<dyn Volume> = volume;
    let carry = super::LinkCarry::for_copy(&state, &as_dyn, &[PathBuf::from("/alone")], &as_dyn, Path::new("/")).await;
    assert_eq!(carry.selected().count(), 0);
}

/// Under a blanket Skip the file at the name stays; under an Overwrite the link
/// replaces it through the safe landing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_link_meeting_a_file_honors_the_policy() {
    for (policy, replaced) in [(ConflictResolution::Skip, false), (ConflictResolution::Overwrite, true)] {
        let engine = TestOperations::new();
        let source = source_with_link().await;
        let dest = holds_links();
        dest.create_directory(Path::new("/tree")).await.unwrap();
        dest.create_file(Path::new("/tree/link"), b"mine").await.unwrap();
        let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
        let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Skip, false));

        let result = copy_volumes_with_progress(
            events,
            "op-link-carry-clash",
            &state,
            source,
            &[PathBuf::from("/tree")],
            Arc::clone(&dest) as Arc<dyn Volume>,
            Path::new("/"),
            &VolumeCopyConfig {
                conflict_resolution: policy,
                progress_interval_ms: 0,
                ..VolumeCopyConfig::default()
            },
        )
        .await;

        assert!(result.is_ok(), "{policy:?}: {result:?}");
        let at_name = target_at(&dest, "/tree/link").await;
        assert_eq!(at_name.is_some(), replaced, "{policy:?}");
        if !replaced {
            let kept = dest.read_range(Path::new("/tree/link"), 0, 16).await.unwrap();
            assert_eq!(kept, b"mine");
        }
        no_temps(&names_at(&*dest, "/tree").await);
    }
}

/// ❗ A link pointing at a folder is no folder to merge with: under a blanket
/// Overwrite it never crosses types (`../../../CLAUDE.md`), so the folder and
/// everything in it stay.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_link_meeting_a_real_folder_leaves_the_folder_alone() {
    let engine = TestOperations::new();
    let source = source_with_link().await;
    let dest = holds_links();
    dest.create_directory(Path::new("/tree")).await.unwrap();
    dest.create_directory(Path::new("/tree/link")).await.unwrap();
    dest.create_file(Path::new("/tree/link/mine.txt"), b"mine")
        .await
        .unwrap();
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Skip, false));

    let result = copy_volumes_with_progress(
        events,
        "op-link-carry-folder",
        &state,
        source,
        &[PathBuf::from("/tree")],
        Arc::clone(&dest) as Arc<dyn Volume>,
        Path::new("/"),
        &VolumeCopyConfig {
            conflict_resolution: ConflictResolution::Overwrite,
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        },
    )
    .await;

    assert!(result.is_ok(), "{result:?}");
    assert!(dest.exists(Path::new("/tree/link/mine.txt")).await);
    assert_eq!(target_at(&dest, "/tree/link").await, None);
}

/// A rollback takes back the link it landed, and leaves one whose target changed
/// since: that's not ours any more.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rollback_removes_the_links_it_landed_and_only_those() {
    let engine = TestOperations::new();
    let dest = holds_links();
    dest.plant_symlink(Path::new("/ours"), Path::new("t"));
    dest.plant_symlink(Path::new("/changed"), Path::new("someone else's"));
    let state = Arc::new(engine.new_operation_state(Duration::from_millis(50)));
    let mut ledger = vec![
        crate::ledger::WrittenFile::volume_link(PathBuf::from("/ours"), PathBuf::from("t")),
        crate::ledger::WrittenFile::volume_link(PathBuf::from("/changed"), PathBuf::from("t")),
    ];

    let as_dyn: Arc<dyn Volume> = Arc::clone(&dest) as Arc<dyn Volume>;
    volume_rollback_with_progress(
        &as_dyn,
        &mut ledger,
        &[],
        &CollectorEventSink::new(),
        "op-link-carry-rollback",
        &WindDown::of(&state),
        1,
        1,
        1,
        1,
    )
    .await;

    assert!(!dest.exists(Path::new("/ours")).await);
    assert_eq!(
        target_at(&dest, "/changed").await.as_deref(),
        Some(Path::new("someone else's"))
    );
}
