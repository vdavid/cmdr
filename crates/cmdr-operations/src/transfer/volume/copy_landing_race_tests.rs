//! A name another writer takes while our bytes are still on their way is theirs,
//! and a copy that fails over it must not delete it on the way out.
//!
//! The copy looked, found the name free, and staged its bytes on a
//! `.cmdr-tmp-*` sibling. Before the landing rename, someone else (another
//! app, another person on the share) creates a file at that name. The landing
//! rightly refuses (`staged_write.rs::land`, `LandingName::ExpectedFree`), and
//! the copy fails naming the clash. What these pin is the step AFTER: the
//! driver's post-loop "partial cleanup" used to treat the failed file's FINAL
//! name as our partial and delete it, which on this path is the other writer's
//! file. Under ordinary staging our bytes never touch the final name, so there
//! is nothing of ours there to clean.
//!
//! The live-server twins are `operations_host/backend_suites/network_safety_test_support.rs`'s
//! `a_name_taken_mid_upload_is_never_replaced`, which found this on WebDAV.
//!
//! The other race at the landing is with OUR OWN file: a rename the server
//! performed whose reply is a late error from a session someone else already
//! replaced. That file is ours and must count as landed once, never as a clash.

use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use super::tests::make_state;
use super::*;
use crate::event_sinks::CollectorEventSink;
use crate::testing::TestOperations;
use crate::transfer::outage_volume_test_support::{OutageVolume, RenameDrop};
use crate::transfer::volume::forward_volume_methods;
use crate::types::{ConflictResolution, WriteOperationError};
use cmdr_fs::volume::{InMemoryVolume, Volume, VolumeError};
use cmdr_local::LocalPosixVolume;

/// What the other writer puts at the name.
const THEIRS: &[u8] = b"someone else's file, written while ours was in flight";

/// A destination where another writer takes `taken` the moment our landing
/// rename arrives for it, so the rename meets an occupied name exactly as it
/// would on a live share.
struct TakenAtLanding {
    inner: InMemoryVolume,
    taken: &'static str,
}

impl Volume for TakenAtLanding {
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
            if to.file_name().is_some_and(|n| n == self.taken) && !self.inner.exists(to).await {
                self.inner.create_file(to, THEIRS).await?;
            }
            self.inner.rename(from, to, force).await
        })
    }
}

/// Copies `names` from a fresh source into `/dest` on a destination where
/// another writer takes `taken` at landing, and returns the destination and the
/// copy's failure.
async fn copy_with_a_name_taken_at_landing(
    engine: &TestOperations,
    names: &[&str],
    taken: &'static str,
    policy: ConflictResolution,
) -> (Arc<TakenAtLanding>, WriteOperationError) {
    let source = Arc::new(InMemoryVolume::new("Source"));
    for name in names {
        source
            .create_file(&Path::new("/").join(name), format!("SRC-{name}").as_bytes())
            .await
            .expect("seeding the source");
    }
    let dest = Arc::new(TakenAtLanding {
        inner: InMemoryVolume::new("Dest"),
        taken,
    });
    dest.create_directory(Path::new("/dest"))
        .await
        .expect("the destination folder");

    let sources: Vec<PathBuf> = names.iter().map(|n| Path::new("/").join(n)).collect();
    let result = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "test-op-name-taken-at-landing",
        &make_state(engine),
        source as Arc<dyn Volume>,
        &sources,
        Arc::clone(&dest) as Arc<dyn Volume>,
        Path::new("/dest"),
        &VolumeCopyConfig {
            conflict_resolution: policy,
            ..VolumeCopyConfig::default()
        },
    )
    .await;
    let Err(failure) = result else {
        panic!("a copy whose landing met a name nobody resolved must not report success");
    };
    (dest, failure.error)
}

async fn assert_theirs_survives(dest: &TakenAtLanding, error: &WriteOperationError, taken: &str) {
    assert!(
        matches!(error, WriteOperationError::DestinationExists { .. }),
        "the copy fails naming the clash, got {error:?}"
    );
    let path = Path::new("/dest").join(taken);
    let meta = dest.inner.get_metadata(&path).await;
    assert!(
        meta.is_ok(),
        "❗ the other writer's {taken} must survive the failed copy; it was deleted"
    );
    assert_eq!(
        meta.ok().and_then(|m| m.size),
        Some(THEIRS.len() as u64),
        "and it must be exactly what they wrote"
    );
    // The refused landing takes our complete-but-unplaced temp away at once:
    // the source still holds those bytes, and a `.cmdr-tmp-*` in the user's
    // folder would otherwise wait an hour for the stale-temp reap.
    let leftovers: Vec<String> = dest
        .inner
        .list_directory(Path::new("/dest"), None)
        .await
        .expect("listing the destination")
        .into_iter()
        .map(|e| e.name)
        .filter(|n| n.contains(".cmdr-tmp-"))
        .collect();
    assert!(leftovers.is_empty(), "no staging may be left behind: {leftovers:?}");
}

/// One source, so the SERIAL driver runs it.
#[tokio::test]
async fn serial_a_name_taken_at_landing_is_never_deleted_by_the_failed_copy() {
    let engine = TestOperations::new();
    let (dest, error) =
        copy_with_a_name_taken_at_landing(&engine, &["photo.jpg"], "photo.jpg", ConflictResolution::Skip).await;
    assert_theirs_survives(&dest, &error, "photo.jpg").await;
}

/// Three sources, so the CONCURRENT driver runs them.
#[tokio::test]
async fn concurrent_a_name_taken_at_landing_is_never_deleted_by_the_failed_copy() {
    let engine = TestOperations::new();
    let (dest, error) = copy_with_a_name_taken_at_landing(
        &engine,
        &["a.jpg", "photo.jpg", "z.jpg"],
        "photo.jpg",
        ConflictResolution::Skip,
    )
    .await;
    assert_theirs_survives(&dest, &error, "photo.jpg").await;
}

/// The same under Rename: the name was free when the copy looked, so no
/// resolution ran and nothing claimed it.
#[tokio::test]
async fn serial_a_name_taken_at_landing_survives_under_rename_too() {
    let engine = TestOperations::new();
    let (dest, error) =
        copy_with_a_name_taken_at_landing(&engine, &["photo.jpg"], "photo.jpg", ConflictResolution::Rename).await;
    assert_theirs_survives(&dest, &error, "photo.jpg").await;
}

/// Copies `names` into `/dest` on a share whose landing rename number
/// `at_rename` the server PERFORMS, while its reply is a late error: the session
/// dropped under it and someone else reconnected before the error came back, so
/// the share already reads live on a newer session.
async fn copy_through_a_late_error_at_landing(
    engine: &TestOperations,
    lane: &str,
    names: &[&str],
    at_rename: usize,
) -> (Arc<InMemoryVolume>, Arc<OutageVolume>, Result<(), WriteOperationError>) {
    let source = Arc::new(InMemoryVolume::new("Source"));
    for name in names {
        source
            .create_file(&Path::new("/").join(name), format!("SRC-{name}").as_bytes())
            .await
            .expect("seeding the source");
    }
    let inner = Arc::new(InMemoryVolume::new("Naspolya"));
    inner
        .create_directory(Path::new("/dest"))
        .await
        .expect("the destination folder");
    let nas = OutageVolume::wrap(Arc::clone(&inner), lane);
    nas.stale_error_at_rename(at_rename, RenameDrop::AfterTheServerPerformsIt);

    let sources: Vec<PathBuf> = names.iter().map(|n| Path::new("/").join(n)).collect();
    let outcome = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        lane,
        &make_state(engine),
        source as Arc<dyn Volume>,
        &sources,
        Arc::clone(&nas) as Arc<dyn Volume>,
        Path::new("/dest"),
        &VolumeCopyConfig::default(),
    )
    .await
    .map(|_| ())
    .map_err(|failure| failure.error);
    (inner, nas, outcome)
}

async fn assert_each_landed_once(inner: &InMemoryVolume, nas: &OutageVolume, names: &[&str]) {
    assert_eq!(nas.generation(), 1, "the session really was replaced mid-landing");
    let mut listed: Vec<String> = inner
        .list_directory(Path::new("/dest"), None)
        .await
        .expect("listing the destination")
        .into_iter()
        .map(|e| e.name)
        .collect();
    listed.sort();
    let mut expected: Vec<String> = names.iter().map(|n| (*n).to_owned()).collect();
    expected.sort();
    assert_eq!(listed, expected, "each file exactly once, no staging left behind");
    for name in names {
        let entry = inner
            .get_metadata(&Path::new("/dest").join(name))
            .await
            .expect("it landed");
        assert_eq!(entry.size, Some(format!("SRC-{name}").len() as u64), "{name} is whole");
    }
}

/// A landing the server performed, answered by a replaced session's late
/// error: the share is back on a newer session at once, so the settle runs and
/// finds our file whole at its name with the temp gone. That needs the temp to
/// still be OURS when the settle looks: `land` must not discard it on a
/// transport fault (on the fresh session that delete answers `NotFound`, which
/// releases the temp, and the settle can then no longer prove the landing ours;
/// the re-run met our own file and failed with `DestinationExists`).
#[tokio::test]
async fn serial_a_landing_answered_by_a_replaced_session_counts_as_landed_once() {
    let engine = TestOperations::new();
    let names = ["photo.jpg"];
    let (inner, nas, outcome) =
        copy_through_a_late_error_at_landing(&engine, "late-error-landing-serial", &names, 0).await;

    outcome.expect("a landing the server performed is ours, not a clash");
    assert_each_landed_once(&inner, &nas, &names).await;
}

/// The same in the concurrent window.
#[tokio::test]
async fn concurrent_a_landing_answered_by_a_replaced_session_counts_as_landed_once() {
    let engine = TestOperations::new();
    let names = ["a.jpg", "photo.jpg", "z.jpg"];
    let (inner, nas, outcome) =
        copy_through_a_late_error_at_landing(&engine, "late-error-landing-concurrent", &names, 1).await;

    outcome.expect("a landing the server performed is ours, not a clash");
    assert_each_landed_once(&inner, &nas, &names).await;
}

// ── A name the copy claimed ─────────────────────────────────────────────────

/// A local destination where someone saves at `taken` the instant our landing
/// cleared it: the delete of our `Rename` placeholder goes through, then their
/// file is there before our rename arrives. Once: a later delete of the name
/// is a delete of their file.
struct SavedOverOurClearedName {
    inner: LocalPosixVolume,
    taken: PathBuf,
    saved: AtomicBool,
}

impl Volume for SavedOverOurClearedName {
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
        rename,
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

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.inner.delete(path).await?;
            if path == self.taken && !self.saved.swap(true, Ordering::SeqCst) {
                self.inner.create_file(path, THEIRS).await?;
            }
            Ok(())
        })
    }
}

/// Copies `names` into a local `/dest` that already holds `photo.jpg`, under
/// Rename, so `photo.jpg`'s copy claims `photo (1).jpg` with a placeholder, and
/// someone saves there the instant our landing clears it.
async fn copy_onto_a_claimed_name_someone_saves_over(
    engine: &TestOperations,
    names: &[&str],
) -> (tempfile::TempDir, Result<(), WriteOperationError>) {
    let source = Arc::new(InMemoryVolume::new("Source"));
    for name in names {
        source
            .create_file(&Path::new("/").join(name), format!("SRC-{name}").as_bytes())
            .await
            .expect("seeding the source");
    }
    let dest_dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(dest_dir.path().join("photo.jpg"), b"the user's photo").unwrap();
    let dest = Arc::new(SavedOverOurClearedName {
        inner: LocalPosixVolume::local_folder(
            "Dest",
            dest_dir.path().to_path_buf(),
            crate::host::testing::pane_order_host(),
        ),
        taken: PathBuf::from("/photo (1).jpg"),
        saved: AtomicBool::new(false),
    });

    let sources: Vec<PathBuf> = names.iter().map(|n| Path::new("/").join(n)).collect();
    let outcome = copy_volumes_with_progress(
        Arc::new(CollectorEventSink::new()),
        "test-op-claimed-name-saved-over",
        &make_state(engine),
        source as Arc<dyn Volume>,
        &sources,
        dest as Arc<dyn Volume>,
        Path::new("/"),
        &VolumeCopyConfig {
            conflict_resolution: ConflictResolution::Rename,
            ..VolumeCopyConfig::default()
        },
    )
    .await
    .map(|_| ())
    .map_err(|failure| failure.error);
    (dest_dir, outcome)
}

/// ❗ Once a landing has cleared its claimed name, the name stops being a place
/// the copy's failure may clean: their file stays, and our bytes are kept beside
/// it under ` (recovered)`. The post-loop's partial cleanup used to delete
/// whatever stood at the claimed name, which here is their save, and after a
/// kept landing the reconnect sweep has finished, our own landed file (`#575`).
async fn assert_their_save_and_our_bytes_both_stay(dest: &Path, outcome: &Result<(), WriteOperationError>) {
    assert!(
        matches!(outcome, Err(WriteOperationError::NewDataKeptAt { .. })),
        "the landing couldn't take the name and says where the bytes went, got {outcome:?}"
    );
    assert_eq!(
        std::fs::read(dest.join("photo (1).jpg")).ok().as_deref(),
        Some(THEIRS),
        "❗ the file someone saved at the cleared name survives the failed copy"
    );
    assert_eq!(
        std::fs::read(dest.join("photo (1) (recovered).jpg")).ok().as_deref(),
        Some(&b"SRC-photo.jpg"[..]),
        "our bytes are kept beside it"
    );
}

/// One source, so the SERIAL driver runs it.
#[tokio::test]
async fn serial_a_save_at_a_claimed_name_the_landing_cleared_survives_the_failed_copy() {
    let engine = TestOperations::new();
    let (dest, outcome) = copy_onto_a_claimed_name_someone_saves_over(&engine, &["photo.jpg"]).await;
    assert_their_save_and_our_bytes_both_stay(dest.path(), &outcome).await;
}

/// Three sources, so the CONCURRENT driver runs them.
#[tokio::test]
async fn concurrent_a_save_at_a_claimed_name_the_landing_cleared_survives_the_failed_copy() {
    let engine = TestOperations::new();
    let (dest, outcome) = copy_onto_a_claimed_name_someone_saves_over(&engine, &["a.jpg", "photo.jpg", "z.jpg"]).await;
    assert_their_save_and_our_bytes_both_stay(dest.path(), &outcome).await;
}
