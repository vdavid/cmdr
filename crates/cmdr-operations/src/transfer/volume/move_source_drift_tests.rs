//! What a cross-volume move's source sweep does with an original that changed
//! after its copy, and with an item that turned up in a source folder while the
//! folder was copying.
//!
//! The engine copies one top-level source, then removes it from the source
//! volume. For a folder that copy can take as long as the folder is big, and
//! anything written into it meanwhile (a save over a file already copied, a new
//! download) exists ONLY in the source. So the sweep removes exactly the files
//! the copy carried, and only while they still look the way the copy found them.
//!
//! The window is reproduced by a destination that runs an edit on the source the
//! moment a named file lands: the copy of that file is done, and the sweep
//! hasn't started.

use std::collections::HashMap;
use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Mutex;

use super::test_support::make_state;
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::transfer::volume::forward_volume_methods;
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};

type Edit = Box<dyn FnOnce() -> Pin<Box<dyn Future<Output = ()> + Send>> + Send>;

/// A destination that runs an edit on the source right after the file named in
/// `on_landing` takes its final name.
struct EditSourceAtLanding {
    inner: InMemoryVolume,
    on_landing: Mutex<HashMap<&'static str, Edit>>,
}

impl EditSourceAtLanding {
    fn new(on_landing: Vec<(&'static str, Edit)>) -> Self {
        Self {
            inner: InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000),
            on_landing: Mutex::new(on_landing.into_iter().collect()),
        }
    }

    fn edits_left(&self) -> usize {
        self.on_landing.lock_ignore_poison().len()
    }
}

impl Volume for EditSourceAtLanding {
    forward_volume_methods!(
        inner => name,
        root,
        lane_key,
        list_directory,
        get_metadata,
        exists,
        is_directory,
        create_file,
        create_directory,
        create_directory_all,
        delete,
        get_space_info,
        local_path,
        supports_streaming,
        supports_export,
        supports_local_fs_access,
        operations_are_local,
        max_concurrent_ops,
        create_directory_errors_on_existing_dir,
        scan_for_copy,
        scan_for_copy_batch,
        scan_for_conflicts,
        open_read_stream,
        supports_unknown_length_writes,
        write_from_stream,
        write_is_single_shot,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn rename<'a>(
        &'a self,
        from: &'a Path,
        to: &'a Path,
        force: bool,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.inner.rename(from, to, force).await?;
            let edit = to
                .file_name()
                .and_then(|name| name.to_str())
                .and_then(|name| self.on_landing.lock_ignore_poison().remove(name));
            if let Some(edit) = edit {
                edit().await;
            }
            Ok(())
        })
    }
}

fn source_volume() -> Arc<InMemoryVolume> {
    Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000))
}

/// An edit that saves `bytes` over `path` on `source` and moves its mtime on.
fn save_over(source: &Arc<InMemoryVolume>, path: &'static str, bytes: &'static [u8]) -> Edit {
    let source = Arc::clone(source);
    Box::new(move || {
        Box::pin(async move {
            let path = Path::new(path);
            let modified = source.get_metadata(path).await.unwrap().modified_at.unwrap();
            source.delete(path).await.unwrap();
            source.create_file(path, bytes).await.unwrap();
            source.set_modified_at(path, Some(modified + 5));
        })
    })
}

/// An edit that creates `path` on `source`.
fn create_on(source: &Arc<InMemoryVolume>, path: &'static str, bytes: &'static [u8]) -> Edit {
    let source = Arc::clone(source);
    Box::new(move || Box::pin(async move { source.create_file(Path::new(path), bytes).await.unwrap() }))
}

async fn read(volume: &dyn Volume, path: &str) -> Vec<u8> {
    let mut stream = volume.open_read_stream(Path::new(path)).await.unwrap();
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend(chunk.unwrap());
    }
    bytes
}

/// Moves `sources` from `source` into `/` on `dest` and returns the events.
async fn run_move(
    engine: &TestOperations,
    source: &Arc<InMemoryVolume>,
    dest: &Arc<EditSourceAtLanding>,
    sources: &[&str],
    op_id: &str,
) -> Arc<CollectorEventSink> {
    let events = Arc::new(CollectorEventSink::new());
    let sources: Vec<PathBuf> = sources.iter().map(PathBuf::from).collect();
    let result = move_volumes_with_progress(
        events.clone(),
        op_id,
        &make_state(engine),
        Arc::clone(source) as Arc<dyn Volume>,
        &sources,
        Arc::clone(dest) as Arc<dyn Volume>,
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await;
    assert!(result.is_ok(), "the move must succeed: {result:?}");
    assert_eq!(dest.edits_left(), 0, "precondition: every edit ran before the sweep");
    events
}

/// CRITICAL data-loss regression (#139). A file saved over after its copy
/// landed has its new bytes only in the source; the sweep must leave it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_folder_original_saved_over_after_its_copy_survives_the_sweep() {
    let engine = TestOperations::new();
    let source = source_volume();
    source.create_directory(Path::new("/Work")).await.unwrap();
    source
        .create_file(Path::new("/Work/draft.txt"), b"first draft")
        .await
        .unwrap();
    source
        .create_file(Path::new("/Work/report.txt"), b"final report")
        .await
        .unwrap();
    let dest = Arc::new(EditSourceAtLanding::new(vec![(
        "draft.txt",
        // Same length, so only the timestamp gives it away.
        save_over(&source, "/Work/draft.txt", b"final draft"),
    )]));

    let events = run_move(&engine, &source, &dest, &["/Work"], "op-vol-drift-folder").await;

    assert_eq!(read(&*source, "/Work/draft.txt").await, b"final draft");
    assert_eq!(read(&*dest, "/Work/draft.txt").await, b"first draft");
    assert!(
        !source.exists(Path::new("/Work/report.txt")).await,
        "an original nobody touched still goes"
    );
    let complete = events.complete.lock_ignore_poison();
    let left = complete[0]
        .appeared_during_move
        .as_ref()
        .expect("the move kept a changed original, so it must say so");
    assert_eq!(left.changed_count, 1);
    assert_eq!(left.item_count, 0);
    assert_eq!(left.folder_name, "Work");
}

/// A file (or a whole folder) that turned up in the source folder while it was
/// copying was never carried, so it stays, and the toast counts it once per
/// unknown subtree.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn items_that_appeared_while_a_folder_copied_survive_the_sweep() {
    let engine = TestOperations::new();
    let source = source_volume();
    source.create_directory(Path::new("/Work")).await.unwrap();
    source.create_directory(Path::new("/Work/notes")).await.unwrap();
    source
        .create_file(Path::new("/Work/report.txt"), b"final report")
        .await
        .unwrap();
    source
        .create_file(Path::new("/Work/notes/todo.md"), b"todo")
        .await
        .unwrap();
    let new_folder = Arc::clone(&source);
    let dest = Arc::new(EditSourceAtLanding::new(vec![
        (
            "report.txt",
            create_on(&source, "/Work/download.zip", b"arrived mid-move"),
        ),
        (
            "todo.md",
            Box::new(move || {
                Box::pin(async move {
                    new_folder.create_directory(Path::new("/Work/notes/new")).await.unwrap();
                    new_folder
                        .create_file(Path::new("/Work/notes/new/a.txt"), b"a")
                        .await
                        .unwrap();
                })
            }),
        ),
    ]));

    let events = run_move(&engine, &source, &dest, &["/Work"], "op-vol-drift-appeared").await;

    assert_eq!(read(&*source, "/Work/download.zip").await, b"arrived mid-move");
    assert_eq!(read(&*source, "/Work/notes/new/a.txt").await, b"a");
    assert!(!source.exists(Path::new("/Work/report.txt")).await);
    assert!(!source.exists(Path::new("/Work/notes/todo.md")).await);
    let complete = events.complete.lock_ignore_poison();
    let left = complete[0].appeared_during_move.as_ref().expect("newcomers are news");
    assert_eq!(left.item_count, 2, "a new file and a new folder, each counted once");
    assert_eq!(left.changed_count, 0);
    assert_eq!(left.folder_name, "Work");
}

/// A top-level file saved over after its copy stays, named by the folder it
/// sits in.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_top_level_file_saved_over_after_its_copy_survives() {
    let engine = TestOperations::new();
    let source = source_volume();
    source.create_directory(Path::new("/Documents")).await.unwrap();
    source
        .create_file(Path::new("/Documents/budget.xlsx"), b"old numbers")
        .await
        .unwrap();
    let dest = Arc::new(EditSourceAtLanding::new(vec![(
        "budget.xlsx",
        save_over(&source, "/Documents/budget.xlsx", b"new numbers, and more"),
    )]));

    let events = run_move(
        &engine,
        &source,
        &dest,
        &["/Documents/budget.xlsx"],
        "op-vol-drift-file",
    )
    .await;

    assert_eq!(read(&*source, "/Documents/budget.xlsx").await, b"new numbers, and more");
    assert_eq!(read(&*dest, "/budget.xlsx").await, b"old numbers");
    let complete = events.complete.lock_ignore_poison();
    let left = complete[0]
        .appeared_during_move
        .as_ref()
        .expect("a kept original is news");
    assert_eq!(left.changed_count, 1);
    assert_eq!(left.folder_name, "Documents");
}

/// The ordinary move still takes everything and reports nothing.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_clean_folder_move_takes_everything_and_reports_nothing() {
    let engine = TestOperations::new();
    let source = source_volume();
    source.create_directory(Path::new("/Work")).await.unwrap();
    source.create_directory(Path::new("/Work/empty")).await.unwrap();
    source
        .create_file(Path::new("/Work/report.txt"), b"final report")
        .await
        .unwrap();
    let dest = Arc::new(EditSourceAtLanding::new(Vec::new()));

    let events = run_move(&engine, &source, &dest, &["/Work"], "op-vol-drift-clean").await;

    assert!(!source.exists(Path::new("/Work")).await, "the whole folder went");
    assert!(dest.exists(Path::new("/Work/empty")).await);
    let complete = events.complete.lock_ignore_poison();
    assert!(complete[0].appeared_during_move.is_none());
}

/// #1148: a landed file's original waits for the batch's barrier, so the window
/// between its copy and its sweep spans the copies after it. A save over it in
/// that window, here while the NEXT file lands, keeps it all the same.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_original_saved_over_while_its_sweep_is_held_survives() {
    let engine = TestOperations::new();
    let source = source_volume();
    source.create_file(Path::new("/a.txt"), b"old a").await.unwrap();
    source.create_file(Path::new("/b.txt"), b"b").await.unwrap();
    let dest = Arc::new(EditSourceAtLanding::new(vec![(
        "b.txt",
        save_over(&source, "/a.txt", b"new a"),
    )]));
    // One batch, so `/a.txt` is still held when `/b.txt` lands.
    let mut state = engine.new_operation_state(std::time::Duration::from_millis(50));
    state.barriers = cmdr_fs::durable::DeviceBarriers::with_interval(std::time::Duration::MAX);
    let events = Arc::new(CollectorEventSink::new());

    move_volumes_with_progress(
        events.clone(),
        "op-vol-drift-held",
        &Arc::new(state),
        Arc::clone(&source) as Arc<dyn Volume>,
        &[PathBuf::from("/a.txt"), PathBuf::from("/b.txt")],
        Arc::clone(&dest) as Arc<dyn Volume>,
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
    .expect("the move completes");

    assert_eq!(dest.edits_left(), 0, "precondition: the save ran before the sweep");
    assert_eq!(read(&*source, "/a.txt").await, b"new a");
    assert_eq!(read(&*dest, "/a.txt").await, b"old a");
    assert!(
        !source.exists(Path::new("/b.txt")).await,
        "an untouched original still goes"
    );
    let complete = events.complete.lock_ignore_poison();
    let left = complete[0]
        .appeared_during_move
        .as_ref()
        .expect("a kept original is news");
    assert_eq!(left.changed_count, 1);
}
