//! What the volume drivers report while they run: every file's row in the
//! operation's transfer tree (`transfer_tree/`), and every top-level source's
//! verdict on `write-source-item-done`, so a copy to or from a share deselects
//! its sources one by one.
//!
//! Both are observation only. These cells pin what a person sees; the copy's
//! own outcome is pinned by every other suite here, and none of them changes
//! with the tree.

use std::collections::BTreeSet;
use std::sync::atomic::Ordering;

use super::super::faulty_volume::{FaultyOp, FaultyVolume, forward_volume_methods};
use super::tests::{make_state, make_volumes};
use super::wedge_test_support::*;
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::transfer_tree::test_support::row_states;
use crate::types::{ConflictResolution, SourceItemOutcome};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::InMemoryVolume;

/// The tree's rows with their states (`row_states`).
fn rows(state: &WriteOperationState) -> Vec<String> {
    state
        .transfer_tree
        .get()
        .expect("the scan installed a tree")
        .read(row_states)
        .expect("the tree is on")
}

/// Every `write-source-item-done`, in order: `(source, outcome, removed)`.
fn verdicts(events: &CollectorEventSink) -> Vec<(String, SourceItemOutcome, bool)> {
    events
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .map(|event| (event.source_path.clone(), event.outcome, event.source_removed))
        .collect()
}

fn verdict(source: &str, outcome: SourceItemOutcome, removed: bool) -> (String, SourceItemOutcome, bool) {
    (source.to_owned(), outcome, removed)
}

async fn put(volume: &Arc<dyn Volume>, path: &str, bytes: &[u8]) {
    if let Some(parent) = Path::new(path).parent() {
        volume.create_directory_all(parent).await.expect("parent folder");
    }
    volume.create_file(Path::new(path), bytes).await.expect("file");
}

async fn copy(
    events: &Arc<CollectorEventSink>,
    state: &Arc<WriteOperationState>,
    source: &Arc<dyn Volume>,
    sources: &[&str],
    dest: &Arc<dyn Volume>,
    config: &VolumeCopyConfig,
) -> Result<(), WriteFailure> {
    let sources: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
    copy_volumes_with_progress(
        Arc::clone(events) as Arc<dyn OperationEventSink>,
        "op-tree-rows",
        state,
        Arc::clone(source),
        &sources,
        Arc::clone(dest),
        Path::new("/"),
        config,
    )
    .await
}

/// The concurrent driver streams files from several folders at once, and the
/// tree shows every one of them in flight, each in its own folder. When the
/// bytes flow, every row ends done and every source says so.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_concurrent_copy_shows_files_in_flight_across_folders_and_every_row_ends_done() {
    let engine = TestOperations::new();
    let fx = fixture(CHUNK as u64 * 2);
    for path in ["/a/1.bin", "/a/2.bin", "/b/1.bin", "/c/1.bin"] {
        fx.source_inner
            .create_directory_all(Path::new(path).parent().expect("parent"))
            .await
            .expect("folder");
        fx.source_inner
            .create_file(Path::new(path), &vec![0xAB; CHUNK * 2])
            .await
            .expect("file");
    }
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine);
    let config = VolumeCopyConfig::default();

    let running = copy(&events, &state, &fx.source, &["/a", "/b", "/c"], &fx.dest, &config);
    tokio::pin!(running);
    tokio::select! {
        result = &mut running => panic!("the copy must wait on the gate: {result:?}"),
        () = wait_until_async(WAIT, "every file to open its stream", || fx.opened.load(Ordering::SeqCst) >= 4) => {}
    }

    let tree = state.transfer_tree.get().expect("the scan installed a tree");
    let in_flight: Vec<String> = tree
        .read(|model| {
            model
                .active()
                .iter()
                .map(|leaf| {
                    model
                        .name(model.parent(leaf.node).expect("a file has a folder"))
                        .to_owned()
                })
                .collect()
        })
        .expect("the tree is on");
    assert_eq!(in_flight.len(), 4, "four files stream at once: {in_flight:?}");
    assert_eq!(
        in_flight.into_iter().collect::<BTreeSet<_>>(),
        BTreeSet::from(["a".to_owned(), "b".to_owned(), "c".to_owned()]),
        "and they sit in three different folders"
    );

    fx.gate.add_permits(1_000);
    running.await.expect("the copy succeeds once the bytes flow");

    assert_eq!(
        rows(&state),
        vec![
            "a/ done",
            "  1.bin done",
            "  2.bin done",
            "b/ done",
            "  1.bin done",
            "c/ done",
            "  1.bin done"
        ]
    );
    let mut said = verdicts(&events);
    said.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(
        said,
        vec![
            verdict("/a", SourceItemOutcome::Done, false),
            verdict("/b", SourceItemOutcome::Done, false),
            verdict("/c", SourceItemOutcome::Done, false),
        ]
    );
}

/// "Skip all" over clashes the dialog already found: no task ever runs for
/// those sources, so their rows and verdicts come from the drivers' bulk path.
/// Two sources take the serial driver, three the concurrent one.
#[tokio::test]
async fn skipping_every_known_clash_marks_each_row_skipped_and_says_so_per_source() {
    let engine = TestOperations::new();
    for names in [vec!["x.txt", "y.txt"], vec!["x.txt", "y.txt", "z.txt"]] {
        let (source, dest) = make_volumes();
        for name in &names {
            put(&source, &format!("/src/{name}"), b"incoming").await;
            put(&dest, &format!("/{name}"), b"theirs").await;
        }
        let sources: Vec<String> = names.iter().map(|name| format!("/src/{name}")).collect();
        let events = Arc::new(CollectorEventSink::new());
        let state = make_state(&engine);
        let config = VolumeCopyConfig {
            conflict_resolution: ConflictResolution::Skip,
            pre_known_conflicts: names.iter().map(|name| (*name).to_owned()).collect(),
            ..VolumeCopyConfig::default()
        };

        copy(
            &events,
            &state,
            &source,
            &sources.iter().map(String::as_str).collect::<Vec<_>>(),
            &dest,
            &config,
        )
        .await
        .expect("a skip-all copy succeeds");

        let expected_rows: Vec<String> = names.iter().map(|name| format!("{name} skipped")).collect();
        assert_eq!(rows(&state), expected_rows, "{} sources", names.len());
        let mut said = verdicts(&events);
        said.sort_by(|a, b| a.0.cmp(&b.0));
        let expected: Vec<_> = sources
            .iter()
            .map(|source| verdict(source, SourceItemOutcome::Skipped, false))
            .collect();
        assert_eq!(said, expected, "{} sources", names.len());
    }
}

/// A clash met INSIDE a folder, mid-walk, skips that one row; the folder still
/// landed something, so it's done. A folder whose every file clashed landed
/// nothing, so it reads (and reports) skipped.
#[tokio::test]
async fn a_clash_inside_a_folder_skips_its_row_and_a_folder_of_only_clashes_is_skipped() {
    let engine = TestOperations::new();
    let (source, dest) = make_volumes();
    put(&source, "/src/mixed/a.txt", b"incoming").await;
    put(&source, "/src/mixed/b.txt", b"incoming").await;
    put(&source, "/src/clashing/c.txt", b"incoming").await;
    put(&dest, "/mixed/a.txt", b"theirs").await;
    put(&dest, "/clashing/c.txt", b"theirs").await;
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };

    copy(
        &events,
        &state,
        &source,
        &["/src/mixed", "/src/clashing"],
        &dest,
        &config,
    )
    .await
    .expect("the copy succeeds");

    assert_eq!(
        rows(&state),
        vec![
            "clashing/ skipped",
            "  c.txt skipped",
            "mixed/ done",
            "  a.txt skipped",
            "  b.txt done"
        ]
    );
    assert_eq!(
        verdicts(&events),
        vec![
            verdict("/src/clashing", SourceItemOutcome::Skipped, false),
            verdict("/src/mixed", SourceItemOutcome::Done, false),
        ]
    );
}

/// A file that fails deep inside a folder is the row that shows it: the error
/// names it by its typed path, and the event's gateway marks it. The file that
/// landed before it stays done.
#[tokio::test]
async fn a_failure_marks_the_row_of_the_file_it_names() {
    let engine = TestOperations::new();
    let inner = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    let plain: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    put(&plain, "/src/f/a.txt", b"aaa").await;
    put(&plain, "/src/f/b.txt", b"bbb").await;
    let faulty = FaultyVolume::wrapping(Arc::clone(&inner))
        .failing_call(
            FaultyOp::OpenReadStream,
            2,
            VolumeError::IoError {
                message: "unreadable".to_owned().into(),
                raw_os_error: None,
            },
        )
        .arc();
    let source: Arc<dyn Volume> = Arc::clone(&faulty) as Arc<dyn Volume>;
    let (_, dest) = make_volumes();
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine);

    let failure = copy(
        &events,
        &state,
        &source,
        &["/src/f"],
        &dest,
        &VolumeCopyConfig::default(),
    )
    .await
    .expect_err("the second read fails the copy");
    assert!(
        faulty.fault_fired(FaultyOp::OpenReadStream),
        "the fault reached the copy"
    );
    let event = write_error_event_from(&state, "op-tree-rows".to_owned(), WriteOperationType::Copy, failure);

    assert_eq!(event.error.item_path(), Some("/src/f/b.txt"));
    assert_eq!(rows(&state), vec!["f/ failed", "  a.txt done", "  b.txt failed"]);
    assert_eq!(
        verdicts(&events),
        vec![verdict("/src/f", SourceItemOutcome::Failed, false)]
    );
}

/// A cancel leaves every file it stopped back at waiting, ❌ never active
/// forever and never done, and gives the stopped sources no verdict: nothing
/// was decided about them. The file that landed first stays done.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_leaves_the_rows_it_stopped_waiting() {
    let engine = TestOperations::new();
    let _drain = CancelDrainGuard::set(Duration::from_millis(150));
    let fx = fixture(CHUNK as u64 * 4);
    fx.source_inner
        .create_file(Path::new("/landed.txt"), b"")
        .await
        .expect("file");
    for name in ["/wedge-a.bin", "/wedge-b.bin", "/wedge-c.bin"] {
        fx.source_inner
            .create_file(Path::new(name), &vec![0xAB; CHUNK * 4])
            .await
            .expect("file");
    }
    let events = Arc::new(CollectorEventSink::new());
    let op = engine.register_state("tree-rows-cancel", make_state(&engine));
    let config = VolumeCopyConfig::default();

    let running = copy(
        &events,
        op.state(),
        &fx.source,
        &["/landed.txt", "/wedge-a.bin", "/wedge-b.bin", "/wedge-c.bin"],
        &fx.dest,
        &config,
    );
    tokio::pin!(running);
    let landed = || {
        op.state()
            .transfer_tree
            .get()
            .and_then(|tree| tree.read(row_states))
            .is_some_and(|rows| rows.first().is_some_and(|row| row == "landed.txt done"))
    };
    tokio::select! {
        result = &mut running => panic!("the copy must still be running: {result:?}"),
        () = wait_until_async(WAIT, "the empty file to land and the rest to park", || {
            fx.opened.load(Ordering::SeqCst) >= 4 && landed()
        }) => {}
    }
    engine.cancel_write_operation(op.id(), false);
    let result = tokio::time::timeout(Duration::from_secs(5), running)
        .await
        .expect("a cancelled copy returns");
    assert!(matches!(
        result.as_ref().err().map(|failure| &failure.error),
        Some(WriteOperationError::Cancelled { .. })
    ));

    assert_eq!(
        rows(op.state()),
        vec![
            "landed.txt done",
            "wedge-a.bin waiting",
            "wedge-b.bin waiting",
            "wedge-c.bin waiting"
        ]
    );
    assert_eq!(
        verdicts(&events),
        vec![verdict("/landed.txt", SourceItemOutcome::Done, false)]
    );
}

/// A same-volume move renames each item whole, so its tree is the items
/// themselves, each done when its rename lands, and each says so once, with
/// the source gone.
#[tokio::test]
async fn a_same_volume_move_shows_one_row_per_item_and_reports_each_rename() {
    let engine = TestOperations::new();
    let (volume, _) = make_volumes();
    put(&volume, "/src/folder/inner.txt", b"inner").await;
    put(&volume, "/src/file.txt", b"file").await;
    volume.create_directory_all(Path::new("/dest")).await.expect("dest");
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine);

    super::super::move_same::move_within_same_volume_with_progress(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        "op-tree-rows-rename",
        &state,
        Arc::clone(&volume),
        &[PathBuf::from("/src/file.txt"), PathBuf::from("/src/folder")],
        Path::new("/dest"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect("the move succeeds");

    assert_eq!(rows(&state), vec!["folder/ done", "file.txt done"]);
    assert_eq!(
        verdicts(&events),
        vec![
            verdict("/src/folder", SourceItemOutcome::Done, true),
            verdict("/src/file.txt", SourceItemOutcome::Done, true),
        ]
    );
}

/// A backend whose `exists` is the trait default (a stat, any failure "gone",
/// as on ADB, MTP, and SFTP), and whose stat of the source folder starts
/// failing the moment a rename-merge tries to remove it.
struct StatFailsAfterCleanup {
    inner: Arc<InMemoryVolume>,
}

impl Volume for StatFailsAfterCleanup {
    forward_volume_methods!(inner =>
        name, root, list_directory, get_metadata, is_directory, create_file, create_directory,
        create_directory_all, rename, get_space_info, supports_streaming, supports_export,
        operations_are_local, max_concurrent_ops, scan_for_copy, supports_unknown_length_writes,
        write_from_stream, open_read_stream,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let result = self.inner.delete(path).await;
            if path == Path::new("/src/keep") {
                self.inner.set_stat_failing(path);
            }
            result
        })
    }
}

/// A rename-merge that left a skipped child behind keeps its source folder,
/// and a stat that can't answer afterwards is ❌ never evidence it's gone: a
/// wrong `source_removed` drops the folder's whole subtree from search
/// snapshots (`../../DETAILS.md` § "Per-source outcomes").
#[tokio::test]
async fn a_same_volume_merge_that_kept_its_source_never_reports_it_removed_on_a_failed_stat() {
    let engine = TestOperations::new();
    let inner = Arc::new(InMemoryVolume::new("Volume").with_space_info(10_000_000, 10_000_000));
    let plain: Arc<dyn Volume> = Arc::clone(&inner) as Arc<dyn Volume>;
    put(&plain, "/src/keep/a.txt", b"incoming").await;
    put(&plain, "/src/keep/b.txt", b"incoming").await;
    put(&plain, "/dest/keep/a.txt", b"theirs").await;
    let volume: Arc<dyn Volume> = Arc::new(StatFailsAfterCleanup { inner });
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };

    super::super::move_same::move_within_same_volume_with_progress(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        "op-tree-rows-merge-stat",
        &state,
        volume,
        &[PathBuf::from("/src/keep")],
        Path::new("/dest"),
        &config,
    )
    .await
    .expect("the move succeeds");

    assert_eq!(
        verdicts(&events),
        vec![verdict("/src/keep", SourceItemOutcome::Done, false)]
    );
}

/// A cross-volume move speaks twice per source: `Done` once the copy landed,
/// then again once the sweep ran, saying whether the original is gone. The
/// LAST word is the verdict: a folder that kept a clashing child stays, a
/// file that moved is gone.
#[tokio::test]
async fn a_cross_volume_move_reports_each_source_twice_and_the_sweep_has_the_last_word() {
    let engine = TestOperations::new();
    let (source, dest) = make_volumes();
    put(&source, "/src/keep/a.txt", b"incoming").await;
    put(&source, "/src/keep/b.txt", b"incoming").await;
    put(&source, "/src/solo.txt", b"solo").await;
    put(&dest, "/keep/a.txt", b"theirs").await;
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine);
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..VolumeCopyConfig::default()
    };

    super::super::move_cross::move_volumes_with_progress(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        "op-tree-rows-move",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/src/keep"), PathBuf::from("/src/solo.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await
    .expect("the move succeeds");

    assert_eq!(
        verdicts(&events),
        vec![
            verdict("/src/keep", SourceItemOutcome::Done, false),
            verdict("/src/keep", SourceItemOutcome::Done, false),
            verdict("/src/solo.txt", SourceItemOutcome::Done, false),
            verdict("/src/solo.txt", SourceItemOutcome::Done, true),
        ]
    );
    assert_eq!(
        rows(&state),
        vec!["keep/ done", "  a.txt skipped", "  b.txt done", "solo.txt done"]
    );
}

/// A bug while recording the scan's tree costs the tree, never the copy: the
/// scan goes on past it, every file lands, and the operation has no tree.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_tree_that_fails_to_build_leaves_a_volume_copy_as_it_would_run_without_one() {
    use crate::transfer_tree::test_support::TREE_BUG_NAME;

    let engine = TestOperations::new();

    let (source, dest) = make_volumes();
    let bug = format!("/a/{TREE_BUG_NAME}");
    put(&source, "/a/1.bin", b"one").await;
    put(&source, &bug, b"two").await;
    put(&source, "/b.bin", b"three").await;
    let events = Arc::new(CollectorEventSink::new());
    let state = make_state(&engine);

    copy(
        &events,
        &state,
        &source,
        &["/a", "/b.bin"],
        &dest,
        &VolumeCopyConfig::default(),
    )
    .await
    .expect("the copy succeeds");

    for path in ["/a/1.bin", bug.as_str(), "/b.bin"] {
        assert!(dest.exists(Path::new(path)).await, "{path} landed");
    }
    assert!(
        state.transfer_tree.get().is_none(),
        "no tree, so views answer `ready: false`"
    );
}
