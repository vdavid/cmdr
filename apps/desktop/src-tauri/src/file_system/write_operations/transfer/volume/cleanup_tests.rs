//! Tests for `cleanup.rs`: the three ways this directory is allowed to remove
//! something, and the two that must never take more than one node.
//!
//! A `#[path]` child of `cleanup`, so `super::` here is `cleanup` and
//! `super::super::` is `volume` (the one-level-shallower rule every `*_tests.rs`
//! in this directory follows).

use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::ledger::WrittenIdentity;
use crate::file_system::write_operations::state::cancel_write_operation;
use crate::file_system::write_operations::test_support::TestOperationGuard;

fn make_state() -> Arc<WriteOperationState> {
    Arc::new(WriteOperationState::new(Duration::from_millis(50)))
}

/// Runs a rollback over the given ledger of complete volume files, and reports
/// whether it finished.
async fn roll_back(volume: &Arc<dyn Volume>, copied_paths: &[PathBuf], created_dirs: &[PathBuf]) -> bool {
    let mut ledger: Vec<WrittenFile> = copied_paths
        .iter()
        .map(|path| WrittenFile::volume(path.clone(), 0))
        .collect();
    let events = CollectorEventSink::new();
    let state = make_state();
    volume_rollback_with_progress(
        volume,
        &mut ledger,
        created_dirs,
        &events,
        "cleanup-tests-op",
        &state,
        1,
        1,
        1,
        1,
    )
    .await
}

/// **The destructive one.** A directory that reaches the partial sweep must
/// cost the user nothing.
///
/// `copy_serial.rs` parks every source's destination in `last_dest_path`,
/// directories included, and clears it in both arms of the transfer's result.
/// Whether a directory root can survive that window is a property of the
/// DRIVER (today it awaits the future, so it can't) — and the sweep is not
/// allowed to depend on it. A merged destination directory holds files the
/// user already had; removing it recursively is silent data loss.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn partial_sweep_leaves_a_directory_and_its_contents_alone() {
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    vol.create_file(Path::new("/album/keep-me.jpg"), b"the user's own file")
        .await
        .unwrap();
    let volume: Arc<dyn Volume> = vol.clone();

    clean_partial_writes(&volume, &[PathBuf::from("/album")], "cleanup-tests-op").await;

    assert!(
        vol.exists(Path::new("/album/keep-me.jpg")).await,
        "a directory in the partial sweep must not take the user's files with it"
    );
    assert!(
        vol.exists(Path::new("/album")).await,
        "the merged dest directory survives"
    );
}

/// The same leak through the OTHER feed: `copy.rs`'s RollingBack branch pushes
/// `last_dest_path` into `copied_paths`, and the rollback loop deletes each
/// entry. Same cell, same directory, same loss.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rollback_leaves_a_directory_in_copied_paths_alone() {
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    vol.create_file(Path::new("/album/keep-me.jpg"), b"the user's own file")
        .await
        .unwrap();
    let volume: Arc<dyn Volume> = vol.clone();

    let completed = roll_back(&volume, &[PathBuf::from("/album")], &[]).await;

    assert!(completed, "rollback runs to the end even when a path refuses");
    assert!(
        vol.exists(Path::new("/album/keep-me.jpg")).await,
        "rollback must delete the files this op wrote, never a directory's contents"
    );
}

/// A file the copy wrote still goes, and one that's already gone is not a
/// failure worth logging: the sweep's job is "make sure this isn't there".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn rollback_removes_the_files_the_copy_wrote() {
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    vol.create_file(Path::new("/album/ours.jpg"), b"we wrote this")
        .await
        .unwrap();
    vol.create_file(Path::new("/album/keep-me.jpg"), b"the user's own file")
        .await
        .unwrap();
    let volume: Arc<dyn Volume> = vol.clone();

    roll_back(
        &volume,
        &[
            PathBuf::from("/album/ours.jpg"),
            PathBuf::from("/album/never-landed.jpg"),
        ],
        &[],
    )
    .await;

    assert!(!vol.exists(Path::new("/album/ours.jpg")).await);
    assert!(vol.exists(Path::new("/album/keep-me.jpg")).await);
}

/// **The second destructive one, against a backend that lies.** The created-dirs
/// prune has to establish emptiness ITSELF.
///
/// Every shipping backend refuses to delete a non-empty directory, and a
/// conformance assertion keeps them honest — but a guard that survives only
/// because a promise held breaks the day someone writes a new `Volume`. The
/// user's file here got into a created dir through the one window that allows
/// it (a TOCTOU race against another writer), and it must survive regardless of
/// what `delete` would have done.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn created_dir_prune_checks_emptiness_itself_even_on_a_recursive_backend() {
    use super::super::strategy::test_support::RecursiveDeleteVolume;

    let inner = Arc::new(InMemoryVolume::new("Dest"));
    inner.create_directory(Path::new("/album")).await.unwrap();
    inner
        .create_file(Path::new("/album/keep-me.jpg"), b"the user's own file")
        .await
        .unwrap();
    let volume: Arc<dyn Volume> = RecursiveDeleteVolume::wrapping(Arc::clone(&inner));

    roll_back(&volume, &[], &[PathBuf::from("/album")]).await;

    assert!(
        inner.exists(Path::new("/album/keep-me.jpg")).await,
        "the prune must list the directory before deleting it, not trust the backend to refuse"
    );
}

/// A created dir that really is empty still goes, on that same lying backend:
/// the emptiness check must not overshoot into "never prune anything".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn created_dir_prune_still_removes_an_empty_directory() {
    use super::super::strategy::test_support::RecursiveDeleteVolume;

    let inner = Arc::new(InMemoryVolume::new("Dest"));
    inner.create_directory(Path::new("/album")).await.unwrap();
    inner.create_directory(Path::new("/album/raw")).await.unwrap();
    let volume: Arc<dyn Volume> = RecursiveDeleteVolume::wrapping(Arc::clone(&inner));

    // Creation order is shallowest-first; the prune walks it in reverse, so the
    // leaf empties before its parent is tried.
    roll_back(&volume, &[], &[PathBuf::from("/album"), PathBuf::from("/album/raw")]).await;

    assert!(!inner.exists(Path::new("/album/raw")).await);
    assert!(!inner.exists(Path::new("/album")).await);
}

// ── The in-flight partials this operation owns ─────────────────────────────

/// A write that was still in flight goes into the ledger as this operation's own
/// partial, which is a different thing from a file whose identity is unknown.
///
/// A partial has no size and no complete file to recognize, by construction, so
/// a reversal that skipped whatever it couldn't verify would leave a truncated
/// file at the destination. Nothing else can own a destination path that never
/// held a complete file, so these are removed on sight.
#[test]
fn in_flight_writes_join_the_ledger_as_this_operation_s_own_partials() {
    let mut ledger = vec![WrittenFile::volume(PathBuf::from("/album/done.jpg"), 4096)];

    append_own_partials(
        &mut ledger,
        Some(PathBuf::from("/album/half.jpg")),
        &[PathBuf::from("/album/also-half.jpg")],
    );

    assert_eq!(ledger[0].identity, WrittenIdentity::VolumeFile { size: 4096 });
    for partial in &ledger[1..] {
        assert_eq!(
            partial.identity,
            WrittenIdentity::OwnPartial,
            "{} was still being written",
            partial.path.display()
        );
        assert_ne!(
            partial.identity,
            WrittenIdentity::Unverifiable,
            "a partial that reads as merely unverifiable gets left at the destination"
        );
    }
    assert_eq!(ledger.len(), 3);
}

/// A path the ledger already carries isn't added twice: the completed write is
/// the better record, and a second entry would walk the same path twice.
#[test]
fn a_partial_the_ledger_already_carries_is_not_added_twice() {
    let mut ledger = vec![WrittenFile::volume(PathBuf::from("/album/one.jpg"), 10)];

    append_own_partials(
        &mut ledger,
        Some(PathBuf::from("/album/one.jpg")),
        &[PathBuf::from("/album/two.jpg"), PathBuf::from("/album/two.jpg")],
    );

    let paths: Vec<&PathBuf> = ledger.iter().map(|entry| &entry.path).collect();
    assert_eq!(
        paths,
        vec![&PathBuf::from("/album/one.jpg"), &PathBuf::from("/album/two.jpg")]
    );
    assert_eq!(ledger[0].identity, WrittenIdentity::VolumeFile { size: 10 });
}

/// The volume ledger is a stack too: a reversal the user stops halfway leaves it
/// claiming exactly what's still on the volume.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_stopped_volume_reversal_leaves_the_ledger_claiming_what_is_still_there() {
    let vol = Arc::new(InMemoryVolume::new("Dest"));
    vol.create_directory(Path::new("/album")).await.unwrap();
    let mut ledger = Vec::new();
    for i in 0..4 {
        let path = PathBuf::from(format!("/album/file{i}.bin"));
        vol.create_file(&path, b"payload").await.unwrap();
        ledger.push(WrittenFile::volume(path, 7));
    }
    let volume: Arc<dyn Volume> = vol.clone();

    let state = make_state();
    let guard = TestOperationGuard::register_state("volume-ledger-stop", Arc::clone(&state));
    // Rolling back, and the user stops the reversal before it starts deleting.
    cancel_write_operation(guard.id(), true);
    cancel_write_operation(guard.id(), false);
    let events = CollectorEventSink::new();
    let completed =
        volume_rollback_with_progress(&volume, &mut ledger, &[], &events, guard.id(), &state, 4, 28, 4, 28).await;

    assert!(!completed, "the user stopped it");
    assert_eq!(ledger.len(), 4, "nothing was reversed, so nothing left the ledger");
    for entry in &ledger {
        assert!(
            vol.exists(&entry.path).await,
            "{} is still claimed, so it must still be on the volume",
            entry.path.display()
        );
    }
}
