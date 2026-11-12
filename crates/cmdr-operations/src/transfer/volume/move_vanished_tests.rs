//! What a cross-volume move does when a LOCAL drive on one of its sides leaves
//! mid-flight: the twin of `../move_op/move_vanished_tests.rs`, through the
//! held sweeps (`held_sweeps.rs`, `local_mounts.rs`).
//!
//! The mount table is answered by `transfer_sides::test_hook`, which is
//! thread-local, so these run on a current-thread runtime: every question the
//! sweeps ask then comes from the test's own thread.

use std::cell::Cell;
use std::rc::Rc;

use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::transfer_sides::test_hook;
use crate::types::TransferRole;
use cmdr_fs::volume::InMemoryVolume;
use cmdr_local::LocalPosixVolume;

fn state(engine: &TestOperations) -> Arc<WriteOperationState> {
    Arc::new(
        engine
            .new_operation_state(std::time::Duration::from_millis(50))
            .with_journal_volumes("vol-source".to_string(), "vol-destination".to_string()),
    )
}

fn local(name: &str, root: &Path) -> Arc<dyn Volume> {
    Arc::new(LocalPosixVolume::local_folder(
        name,
        root.to_path_buf(),
        crate::host::testing::pane_order_host(),
    ))
}

fn phone() -> Arc<InMemoryVolume> {
    Arc::new(InMemoryVolume::new("Tëstphone").with_space_info(10_000_000, 10_000_000))
}

async fn run_move(
    state: &Arc<WriteOperationState>,
    events: &Arc<CollectorEventSink>,
    source: Arc<dyn Volume>,
    sources: &[PathBuf],
    dest: Arc<dyn Volume>,
) -> Result<(), WriteFailure> {
    move_volumes_with_progress(
        Arc::clone(events) as Arc<dyn OperationEventSink>,
        "op-vanished",
        state,
        source,
        sources,
        dest,
        Path::new("/"),
        &VolumeCopyConfig::default(),
    )
    .await
}

/// The LAST `source_removed` each source got, which is its verdict.
fn last_removed(events: &CollectorEventSink, source: &str) -> Option<bool> {
    events
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .rev()
        .find(|event| event.source_path == source)
        .map(|event| event.source_removed)
}

/// Answers "listed" for `root` the first `listed_for` times it's asked (the
/// first is the capture as the move starts), then `after`. `on_last_listed`
/// runs on the last "listed" answer.
fn mount_answers(
    root: &Path,
    listed_for: usize,
    after: Option<bool>,
    on_last_listed: impl Fn() + 'static,
) -> test_hook::MountTableHook {
    let root = root.to_path_buf();
    let asked = Rc::new(Cell::new(0_usize));
    test_hook::answer(move |path| {
        if path != root {
            return None;
        }
        asked.set(asked.get() + 1);
        if asked.get() == listed_for {
            on_last_listed();
        }
        Some(if asked.get() <= listed_for { Some(true) } else { after })
    })
}

async fn two_phone_files(source: &InMemoryVolume) -> Vec<PathBuf> {
    source.create_file(Path::new("/a.jpg"), b"one").await.unwrap();
    source.create_file(Path::new("/b.jpg"), b"two").await.unwrap();
    vec![PathBuf::from("/a.jpg"), PathBuf::from("/b.jpg")]
}

/// The gate: a barrier can answer `Ok` for writes the kernel took moments
/// before the stick left, so the mount table is asked between the barrier and
/// the first delete. Without it, the sweep deletes the only copies.
#[tokio::test(flavor = "current_thread")]
async fn a_destination_that_left_before_the_sweep_keeps_every_source() {
    let engine = TestOperations::new();
    let stick = tempfile::tempdir().expect("tempdir");
    let source = phone();
    let sources = two_phone_files(&source).await;
    let events = Arc::new(CollectorEventSink::new());
    let _hook = mount_answers(stick.path(), 1, Some(false), || {});

    let failure = run_move(
        &state(&engine),
        &events,
        Arc::clone(&source) as Arc<dyn Volume>,
        &sources,
        local("Tëststick", stick.path()),
    )
    .await
    .expect_err("a destination that left stops the move");

    match failure.error {
        WriteOperationError::DeviceDisconnected { side: Some(side), .. } => {
            assert_eq!(side.role, TransferRole::Destination);
            assert_eq!(side.volume_id, "vol-destination");
            assert_eq!(side.volume_name, "Tëststick");
            assert_eq!(side.counterpart_name, "Tëstphone");
        }
        other => panic!("expected the stick named as disconnected, got {other:?}"),
    }
    for path in &sources {
        assert!(source.exists(path).await, "{} stays on the phone", path.display());
        assert_ne!(last_removed(&events, &path.display().to_string()), Some(true));
    }
}

/// The twin: a destination still listed completes, so the gate can't be
/// answering "gone" for every drive.
#[tokio::test(flavor = "current_thread")]
async fn a_mounted_destination_still_completes() {
    let engine = TestOperations::new();
    let stick = tempfile::tempdir().expect("tempdir");
    let source = phone();
    let sources = two_phone_files(&source).await;
    let events = Arc::new(CollectorEventSink::new());
    let _hook = mount_answers(stick.path(), usize::MAX, None, || {});

    run_move(
        &state(&engine),
        &events,
        Arc::clone(&source) as Arc<dyn Volume>,
        &sources,
        local("Tëststick", stick.path()),
    )
    .await
    .expect("the move completes");

    for path in &sources {
        assert!(!source.exists(path).await, "{} moved", path.display());
        assert_eq!(last_removed(&events, &path.display().to_string()), Some(true));
    }
}

/// An unreadable mount table is not evidence of leaving: the sweep runs.
#[tokio::test(flavor = "current_thread")]
async fn an_unreadable_mount_table_lets_the_sweep_run() {
    let engine = TestOperations::new();
    let stick = tempfile::tempdir().expect("tempdir");
    let source = phone();
    let sources = two_phone_files(&source).await;
    let _hook = mount_answers(stick.path(), 1, None, || {});

    run_move(
        &state(&engine),
        &Arc::new(CollectorEventSink::new()),
        Arc::clone(&source) as Arc<dyn Volume>,
        &sources,
        local("Tëststick", stick.path()),
    )
    .await
    .expect("the move completes");

    for path in &sources {
        assert!(!source.exists(path).await, "{} moved", path.display());
    }
}

/// A local folder that was never a mount point (a cloud-sync folder) can't
/// leave the table, so it's never asked: asking would read it as gone.
#[tokio::test(flavor = "current_thread")]
async fn a_destination_that_was_never_a_mount_point_is_not_watched() {
    let engine = TestOperations::new();
    let folder = tempfile::tempdir().expect("tempdir");
    let source = phone();
    let sources = two_phone_files(&source).await;
    let _hook = test_hook::pull(folder.path());

    run_move(
        &state(&engine),
        &Arc::new(CollectorEventSink::new()),
        Arc::clone(&source) as Arc<dyn Volume>,
        &sources,
        local("Dropbox", folder.path()),
    )
    .await
    .expect("the move completes");

    for path in &sources {
        assert!(!source.exists(path).await, "{} moved", path.display());
    }
}

/// The sweep's "already gone" readings: a source drive that left answers
/// `NotFound` exactly as a file this move already removed does, so a folder
/// whose drive left mid-sweep must not read as moved.
#[tokio::test(flavor = "current_thread")]
async fn a_source_drive_that_left_is_not_reported_as_removed() {
    let engine = TestOperations::new();
    let usb = tempfile::tempdir().expect("tempdir");
    let folder = usb.path().join("photos");
    std::fs::create_dir(&folder).unwrap();
    std::fs::write(folder.join("a.jpg"), b"one").unwrap();
    std::fs::write(folder.join("b.jpg"), b"two").unwrap();
    let dest = phone();
    let events = Arc::new(CollectorEventSink::new());
    // Listed at the capture and at the gate after the barrier; the drive goes
    // right after that (its files with it), so the sweep reads `NotFound`.
    let vanishing = folder.clone();
    let _hook = mount_answers(usb.path(), 2, Some(false), move || {
        std::fs::remove_dir_all(&vanishing).unwrap();
    });

    let failure = run_move(
        &state(&engine),
        &events,
        local("Tëstusb", usb.path()),
        &[PathBuf::from("/photos")],
        Arc::clone(&dest) as Arc<dyn Volume>,
    )
    .await
    .expect_err("a source that left stops the move");

    match failure.error {
        WriteOperationError::DeviceDisconnected { side: Some(side), .. } => {
            assert_eq!(side.role, TransferRole::Source);
            assert_eq!(side.volume_id, "vol-source");
            assert_eq!(side.volume_name, "Tëstusb");
        }
        other => panic!("expected the source drive named as disconnected, got {other:?}"),
    }
    assert!(dest.exists(Path::new("/photos/a.jpg")).await, "the copy landed");
    assert_ne!(
        last_removed(&events, &usb.path().join("photos").display().to_string()),
        Some(true),
        "the folder isn't reported moved"
    );
    assert_ne!(
        last_removed(&events, "/photos"),
        Some(true),
        "the folder isn't reported moved"
    );
}
