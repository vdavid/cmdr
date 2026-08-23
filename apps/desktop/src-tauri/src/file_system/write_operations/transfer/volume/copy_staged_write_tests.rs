//! A destination file must never carry its FINAL name until its last byte has
//! landed.
//!
//! The 2026-07-31 wedge was force-quit mid-transfer and left
//! `sms-20260726002817.xml` at zero bytes and `sms-20260725002819.xml` truncated
//! at 4 MiB, both at their final names, indistinguishable from complete files
//! (`docs/notes/incidents/2026-07-31-transfer-wedge/README.md`). Neither had a
//! conflict, so neither took the safe-replace temp; they streamed straight to the
//! destination path.
//!
//! These tests reproduce that by ABANDONING the copy future mid-stream — the
//! in-process equivalent of a force-quit, since no error path, no cleanup, and no
//! `Drop` on the backend writer gets to run. Doubles: `wedge_test_support`.

use super::tests::make_state;
use super::wedge_test_support::*;
use super::*;
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::ConflictResolution;
use cmdr_fs::testing::wait_until_async;
use std::sync::atomic::Ordering;

// ============================================================================
// M2: a killed transfer leaves nothing at a final name
// ============================================================================

/// A FRESH copy (no conflict, so no safe-replace temp) abandoned mid-stream must
/// leave nothing at the destination's final name. Pre-staging the bytes streamed
/// straight to `/notes.txt`, so a force-quit left a truncated file there that
/// looked complete.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abandoned_fresh_copy_leaves_no_partial_at_the_final_name() {
    let Fixture {
        source,
        source_inner,
        gate,
        dest,
        dest_inner,
        written,
        ..
    } = fixture(CHUNK as u64 * 4);
    source_inner
        .create_file(Path::new("/notes.txt"), &vec![0xAB; CHUNK * 4])
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    {
        let sources = [PathBuf::from("/notes.txt")];
        let copy = copy_volumes_with_progress(
            events.clone(),
            "test-op-abandon-fresh",
            &state,
            Arc::clone(&source),
            &sources,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        );
        tokio::pin!(copy);

        // Let exactly one chunk through, then abandon the copy where it parks
        // waiting for the second. Dropping the future is the in-process
        // equivalent of the force-quit: no cleanup of any kind runs.
        gate.add_permits(1);
        tokio::select! {
            r = &mut copy => panic!("the gated copy must not run to completion: {r:?}"),
            () = wait_until_async(WAIT, "the first chunk to land at the destination", || {
                written.load(Ordering::SeqCst) > 0
            }) => {}
        }
    }

    let names = dest_names(&dest_inner).await;
    assert!(
        !names.iter().any(|n| n == "notes.txt"),
        "an abandoned transfer must leave NOTHING at the file's final name; found {names:?}"
    );
    assert!(
        names.iter().any(|n| n.contains(".cmdr-tmp-")),
        "the abandoned partial must survive under a recognizable .cmdr-tmp-* name; found {names:?}"
    );
}

/// The OVERWRITE path's counterpart: abandoning mid-stream must leave the
/// original destination file untouched and complete, and must not park partial
/// bytes at the final name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abandoned_overwrite_leaves_the_original_intact() {
    let Fixture {
        source,
        source_inner,
        gate,
        dest,
        dest_inner,
        written,
        ..
    } = fixture(CHUNK as u64 * 4);
    source_inner
        .create_file(Path::new("/notes.txt"), &vec![0xAB; CHUNK * 4])
        .await
        .unwrap();
    dest_inner
        .create_file(Path::new("/notes.txt"), b"ORIGINAL DEST DATA")
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    };

    {
        let sources = [PathBuf::from("/notes.txt")];
        let copy = copy_volumes_with_progress(
            events.clone(),
            "test-op-abandon-overwrite",
            &state,
            Arc::clone(&source),
            &sources,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        );
        tokio::pin!(copy);

        gate.add_permits(1);
        tokio::select! {
            r = &mut copy => panic!("the gated copy must not run to completion: {r:?}"),
            () = wait_until_async(WAIT, "the first chunk to land at the destination", || {
                written.load(Ordering::SeqCst) > 0
            }) => {}
        }
    }

    assert_eq!(
        read_dest(&dest_inner, "/notes.txt").await.as_deref(),
        Some(&b"ORIGINAL DEST DATA"[..]),
        "an abandoned overwrite must leave the original destination file complete"
    );
}

/// A file inside a DIRECTORY source gets the same guarantee. Directory sources
/// never entered the driver's in-flight partial ledger, so their children were
/// the least protected case of all.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn abandoned_directory_child_leaves_no_partial_at_the_final_name() {
    let Fixture {
        source,
        source_inner,
        gate,
        dest,
        dest_inner,
        written,
        ..
    } = fixture(CHUNK as u64 * 4);
    source_inner.create_directory(Path::new("/folder")).await.unwrap();
    source_inner
        .create_file(Path::new("/folder/notes.txt"), &vec![0xAB; CHUNK * 4])
        .await
        .unwrap();

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    {
        let sources = [PathBuf::from("/folder")];
        let copy = copy_volumes_with_progress(
            events.clone(),
            "test-op-abandon-dir-child",
            &state,
            Arc::clone(&source),
            &sources,
            Arc::clone(&dest),
            Path::new("/"),
            &config,
        );
        tokio::pin!(copy);

        gate.add_permits(1);
        tokio::select! {
            r = &mut copy => panic!("the gated copy must not run to completion: {r:?}"),
            () = wait_until_async(WAIT, "the first chunk to land at the destination", || {
                written.load(Ordering::SeqCst) > 0
            }) => {}
        }
    }

    let names: Vec<String> = dest_inner
        .list_directory(Path::new("/folder"), None)
        .await
        .unwrap()
        .into_iter()
        .map(|e| e.name)
        .collect();
    assert!(
        !names.iter().any(|n| n == "notes.txt"),
        "an abandoned merge child must leave NOTHING at its final name; found {names:?}"
    );
}

/// The staging must be invisible on the happy path: a completed fresh copy lands
/// the full content at the final name and leaves no temp behind.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_completed_copy_lands_at_the_final_name_with_no_temp_left() {
    let Fixture {
        source,
        source_inner,
        gate,
        dest,
        dest_inner,
        ..
    } = fixture(CHUNK as u64 * 2);
    source_inner
        .create_file(Path::new("/notes.txt"), &vec![0xAB; CHUNK * 2])
        .await
        .unwrap();
    gate.add_permits(8); // enough for the whole file

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let config = VolumeCopyConfig::default();

    copy_volumes_with_progress(
        events.clone(),
        "test-op-staged-happy",
        &state,
        Arc::clone(&source),
        &[PathBuf::from("/notes.txt")],
        Arc::clone(&dest),
        Path::new("/"),
        &config,
    )
    .await
    .expect("the copy should succeed");

    let names = dest_names(&dest_inner).await;
    assert_eq!(names, vec!["notes.txt".to_string()], "exactly the final file, no temp");
    assert_eq!(
        read_dest(&dest_inner, "/notes.txt").await.map(|b| b.len()),
        Some(CHUNK * 2),
        "the landed file must hold every byte"
    );
}
