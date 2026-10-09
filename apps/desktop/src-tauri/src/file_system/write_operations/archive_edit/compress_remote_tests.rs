//! Fresh compression onto a remote parent (SMB / MTP, modeled by a non-local
//! `InMemoryVolume`). These pin private spool production followed by staged
//! upload, fresh replacement rather than mutation, backend-specific publication,
//! and cleanup of unpublished stages.

use super::compress::compress_start;
use super::test_support::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::WriteOperationPhase;
use uuid::Uuid;

/// Registers a NON-local `InMemoryVolume` (the remote-parent stand-in) with `dir`
/// created but NO target zip — compress must create it. `mtp_style` allows same-name
/// siblings (`create_directory_errors_on_existing_dir() == false`), so the swap
/// takes MTP's delete-then-rename path instead of SMB's atomic rename-replace.
/// Unregister with `get_volume_manager().unregister(&id)` when done.
async fn register_remote_parent(dir: &Path, mtp_style: bool) -> (String, Arc<InMemoryVolume>) {
    let id = format!("remote-parent-{}", Uuid::new_v4());
    let mut vol = InMemoryVolume::new("Remote").with_lane_key(id.clone());
    if mtp_style {
        vol = vol.with_sibling_duplicates_allowed();
    }
    vol.create_directory(dir).await.expect("seed parent dir");
    let vol = Arc::new(vol);
    get_volume_manager().register(&id, Arc::clone(&vol) as Arc<dyn Volume>);
    (id, vol)
}

/// A local source volume over a temp dir holding `files` at its root. The common
/// case: compress LOCAL files onto a remote share.
fn local_source_with(files: &[(&str, &[u8])]) -> (tempfile::TempDir, Arc<dyn Volume>) {
    use crate::file_system::volume::backends::LocalPosixVolume;
    let tmp = tempfile::tempdir().expect("tempdir");
    for (name, bytes) in files {
        std::fs::write(tmp.path().join(name), bytes).expect("write source file");
    }
    let vol: Arc<dyn Volume> = Arc::new(LocalPosixVolume::local_folder("src", tmp.path().to_path_buf()));
    (tmp, vol)
}

/// Names of every entry in the archive's parent dir, to assert no leftover
/// `.cmdr-tmp-*` upload temp remains after the swap.
async fn sibling_names(parent: &dyn Volume, archive_path: &Path) -> Vec<String> {
    let dir = archive_path.parent().expect("archive has a parent dir");
    parent
        .list_directory(dir, None)
        .await
        .expect("list parent dir")
        .into_iter()
        .map(|e| e.name)
        .collect()
}

#[tokio::test]
async fn compress_onto_a_remote_parent_spools_then_packs_local_files() {
    let payload = vec![b'z'; 128 * 1024];
    let (_src_tmp, source_volume) = local_source_with(&[("one.txt", &payload), ("two.txt", b"second")]);
    let archive_path = PathBuf::from("/share/bundle.zip");
    let (parent_id, parent) = register_remote_parent(Path::new("/share"), false).await;

    let events = PhaseGateSink::new(WriteOperationPhase::FinishingTransfer);
    let start = compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("one.txt"), PathBuf::from("two.txt")],
        archive_path.clone(),
        parent_id.clone(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start remote compress");
    assert_eq!(start.operation_type, WriteOperationType::Compress);

    wait_until_async(Duration::from_secs(5), "finishing-transfer phase gate", || {
        events.entered()
    })
    .await;
    assert!(
        events.inner.complete.lock_ignore_poison().is_empty(),
        "completion must wait for remote publication"
    );
    assert!(
        read_remote_entry(parent.as_ref(), &archive_path, "one.txt")
            .await
            .is_none(),
        "the uploaded archive must stay unpublished while final transfer work is gated"
    );
    events.release();

    wait_until_async(Duration::from_secs(5), "a terminal event (complete or error)", || {
        !events.inner.complete.lock_ignore_poison().is_empty() || !events.inner.errors.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        !events.inner.complete.lock_ignore_poison().is_empty(),
        "remote compress should complete, errors: {:?}",
        events.inner.errors.lock_ignore_poison()
    );

    assert_eq!(
        read_remote_entry(parent.as_ref(), &archive_path, "one.txt")
            .await
            .as_deref(),
        Some(payload.as_slice()),
        "the first source must land in the remote zip"
    );
    assert_eq!(
        read_remote_entry(parent.as_ref(), &archive_path, "two.txt")
            .await
            .as_deref(),
        Some(b"second".as_slice()),
        "the second source must land in the remote zip"
    );
    // No upload temp debris left at the user's destination.
    let names = sibling_names(parent.as_ref(), &archive_path).await;
    assert!(
        !names.iter().any(|n| n.contains(".cmdr-tmp-")),
        "no upload temp should remain after the swap, got: {names:?}"
    );

    let progress = events.inner.progress.lock_ignore_poison();
    let transferring = progress
        .iter()
        .filter(|event| event.phase == WriteOperationPhase::Transferring)
        .collect::<Vec<_>>();
    assert!(
        !transferring.is_empty(),
        "remote compress must report produced-archive transfer bytes"
    );
    assert!(
        transferring
            .iter()
            .all(|event| event.operation_type == WriteOperationType::Compress)
    );
    let output_bytes = transferring[0].bytes_total;
    assert_ne!(
        output_bytes,
        (payload.len() + 6) as u64,
        "transfer total is output, not source bytes"
    );
    assert!(
        transferring
            .iter()
            .all(|event| event.bytes_done < event.bytes_total || event.bytes_total == 0),
        "transfer must become indeterminate before publication instead of displaying 100%"
    );
    let finishing = progress
        .iter()
        .find(|event| event.phase == WriteOperationPhase::FinishingTransfer)
        .expect("finishing-transfer phase");
    assert_eq!((finishing.files_done, finishing.files_total), (0, 0));
    assert_eq!((finishing.bytes_done, finishing.bytes_total), (0, 0));

    // The spool route is two steps a person can see: zip here, then upload.
    use crate::file_system::write_operations::types::ProgressStep;
    for event in progress.iter() {
        let expected = match event.phase {
            WriteOperationPhase::Compressing | WriteOperationPhase::FinishingCompression => {
                Some(ProgressStep { number: 1, total: 2 })
            }
            WriteOperationPhase::Transferring | WriteOperationPhase::FinishingTransfer => {
                Some(ProgressStep { number: 2, total: 2 })
            }
            _ => None,
        };
        assert_eq!(event.step, expected, "{:?} numbers its step", event.phase);
    }

    get_volume_manager().unregister(&parent_id);
}

#[tokio::test]
async fn cancel_during_remote_close_does_not_publish_or_complete() {
    let (_src_tmp, source_volume) = local_source_with(&[("one.txt", &[b'x'; 4096])]);
    let archive_path = PathBuf::from("/share/cancelled.zip");
    let (parent_id, parent) = register_remote_parent(Path::new("/share"), false).await;

    // The first finishing event replaces the final 100% transfer tick; the
    // second starts after `write_from_stream` has closed successfully.
    // Cancellation at that gate must leave the completed stage unpublished.
    let events = PhaseGateSink::new_nth(WriteOperationPhase::FinishingTransfer, 2);
    let start = compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("one.txt")],
        archive_path.clone(),
        parent_id.clone(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start remote compress for close-boundary cancellation");

    wait_until_async(
        Duration::from_secs(5),
        "post-close finishing-transfer phase gate",
        || events.entered(),
    )
    .await;
    crate::file_system::write_operations::manager::cancel_operation(&start.operation_id);
    events.release();

    wait_until_async(Duration::from_secs(5), "cancelled terminal event", || {
        !events.inner.cancelled.lock_ignore_poison().is_empty()
            || !events.inner.complete.lock_ignore_poison().is_empty()
            || !events.inner.errors.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        events.inner.complete.lock_ignore_poison().is_empty(),
        "a cancellation after backend close must not publish or report success"
    );
    assert_eq!(events.inner.cancelled.lock_ignore_poison().len(), 1);
    assert!(
        read_remote_entry(parent.as_ref(), &archive_path, "one.txt")
            .await
            .is_none(),
        "the completed remote temp must remain unpublished after cancellation"
    );
    let names = sibling_names(parent.as_ref(), &archive_path).await;
    assert!(
        !names.iter().any(|name| name.contains(".cmdr-tmp-")),
        "cancellation must remove the unpublished remote temp, got: {names:?}"
    );

    get_volume_manager().unregister(&parent_id);
}

#[tokio::test]
async fn compress_onto_a_remote_parent_overwrites_an_existing_zip_with_a_fresh_archive() {
    // The target already holds a zip on the remote. Compress-overwrite REPLACES it
    // with a fresh archive of just the sources — it never merges into the old one
    // because the dedicated producer contains only the selected sources.
    let (_src_tmp, source_volume) = local_source_with(&[("new.txt", b"brand new")]);
    let archive_path = PathBuf::from("/share/existing.zip");
    let (parent_id, parent) = register_remote_zip(&archive_path, &[("stale.txt", b"old content")]).await;

    let events = Arc::new(CollectorEventSink::new());
    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("new.txt")],
        archive_path.clone(),
        parent_id.clone(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start remote compress over existing");

    wait_until_async(Duration::from_secs(5), "a terminal event (complete or error)", || {
        !events.complete.lock_ignore_poison().is_empty() || !events.errors.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        !events.complete.lock_ignore_poison().is_empty(),
        "remote compress-overwrite should complete, errors: {:?}",
        events.errors.lock_ignore_poison()
    );

    assert_eq!(
        read_remote_entry(parent.as_ref(), &archive_path, "new.txt")
            .await
            .as_deref(),
        Some(b"brand new".as_slice()),
        "the new source must be in the fresh archive"
    );
    assert!(
        read_remote_entry(parent.as_ref(), &archive_path, "stale.txt")
            .await
            .is_none(),
        "the pre-existing entry must be gone — compress-overwrite creates a fresh zip, not a merge"
    );

    get_volume_manager().unregister(&parent_id);
}

#[tokio::test]
async fn a_publication_refusal_recovers_and_names_the_displaced_original() {
    let (_src_tmp, source_volume) = local_source_with(&[("new.txt", b"brand new")]);
    let archive_path = PathBuf::from("/share/existing.zip");
    let (parent_id, parent) = register_remote_zip(&archive_path, &[("old.txt", b"only old copy")]).await;
    parent.set_rename_to_failing(&archive_path);
    let events = Arc::new(CollectorEventSink::new());

    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("new.txt")],
        archive_path.clone(),
        parent_id.clone(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start publication-failure compress");
    wait_until_async(Duration::from_secs(5), "publication failure", || {
        !events.errors.lock_ignore_poison().is_empty()
    })
    .await;

    let recovered_path = {
        let errors = events.errors.lock_ignore_poison();
        let WriteOperationError::OriginalsKeptAside { recovered, .. } = &errors[0].error else {
            panic!(
                "publication failure must name the recovered original: {:?}",
                errors[0].error
            );
        };
        assert_eq!(recovered.len(), 1);
        PathBuf::from(&recovered[0].kept_at)
    };
    assert_eq!(
        read_remote_entry(parent.as_ref(), &recovered_path, "old.txt")
            .await
            .as_deref(),
        Some(b"only old copy".as_slice())
    );
    assert!(
        sibling_names(parent.as_ref(), &archive_path)
            .await
            .iter()
            .all(|name| !name.contains(".cmdr-tmp-") && !name.contains(".cmdr-aside-")),
        "the only surviving original must have a stable, non-reapable name"
    );
    get_volume_manager().unregister(&parent_id);
}

#[tokio::test]
async fn compress_onto_an_mtp_style_remote_parent_spools_and_packs() {
    // An MTP-shaped parent allows same-name siblings, so its swap is
    // delete-then-rename, not atomic rename-replace. Publication over a brand-new
    // target must tolerate there being no original to displace.
    let (_src_tmp, source_volume) = local_source_with(&[("photo.raw", b"pixels")]);
    let archive_path = PathBuf::from("/device/DCIM/album.zip");
    let (parent_id, parent) = register_remote_parent(Path::new("/device/DCIM"), true).await;

    let events = Arc::new(CollectorEventSink::new());
    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("photo.raw")],
        archive_path.clone(),
        parent_id.clone(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start mtp-style remote compress");

    wait_until_async(Duration::from_secs(5), "a terminal event (complete or error)", || {
        !events.complete.lock_ignore_poison().is_empty() || !events.errors.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        !events.complete.lock_ignore_poison().is_empty(),
        "mtp-style remote compress should complete, errors: {:?}",
        events.errors.lock_ignore_poison()
    );
    assert_eq!(
        read_remote_entry(parent.as_ref(), &archive_path, "photo.raw")
            .await
            .as_deref(),
        Some(b"pixels".as_slice()),
        "the source must land in the zip on an MTP-style parent"
    );
    let names = sibling_names(parent.as_ref(), &archive_path).await;
    assert!(
        !names.iter().any(|n| n.contains(".cmdr-tmp-")),
        "no upload temp should remain after the delete-then-rename swap, got: {names:?}"
    );

    get_volume_manager().unregister(&parent_id);
}

#[tokio::test]
async fn a_remote_source_streams_into_a_direct_local_archive() {
    let (source_id, source) = register_remote_source(&[("folder/remote.bin", &[b'r'; 192 * 1024])]).await;
    let tmp = tempfile::tempdir().expect("tempdir");
    let dest = tmp.path().join("remote-source.zip");
    let events = Arc::new(CollectorEventSink::new());

    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source as Arc<dyn Volume>,
        vec![PathBuf::from("/folder")],
        dest.clone(),
        unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start remote-source compress");
    wait_until_async(Duration::from_secs(5), "remote-source compress terminal", || {
        !events.complete.lock_ignore_poison().is_empty() || !events.errors.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(events.errors.lock_ignore_poison().is_empty());
    assert_eq!(
        read_entry(&dest, "folder/remote.bin").as_deref(),
        Some([b'r'; 192 * 1024].as_slice())
    );
    get_volume_manager().unregister(&source_id);
}

#[tokio::test]
async fn same_device_mtp_source_finishes_spooling_before_the_upload_path() {
    use crate::file_system::volume::BackendKind;

    let id = format!("mtp-same-device-{}", Uuid::new_v4());
    let volume = Arc::new(
        InMemoryVolume::new("Phone")
            .with_backend_kind(BackendKind::Mtp)
            .with_sibling_duplicates_allowed()
            .with_lane_key(id.clone()),
    );
    volume.create_directory(Path::new("/DCIM")).await.expect("create DCIM");
    volume
        .create_file(Path::new("/DCIM/photo.raw"), &[b'p'; 192 * 1024])
        .await
        .expect("create source");
    get_volume_manager().register(&id, Arc::clone(&volume) as Arc<dyn Volume>);
    let events = Arc::new(CollectorEventSink::new());
    let archive = PathBuf::from("/DCIM/photos.zip");

    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        Arc::clone(&volume) as Arc<dyn Volume>,
        vec![PathBuf::from("/DCIM/photo.raw")],
        archive.clone(),
        id.clone(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start same-device MTP compress");
    wait_until_async(Duration::from_secs(5), "same-device MTP terminal", || {
        !events.complete.lock_ignore_poison().is_empty() || !events.errors.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "fallback must avoid an unknown-length destination write: {:?}",
        events.errors.lock_ignore_poison()
    );
    assert_eq!(
        read_remote_entry(volume.as_ref(), &archive, "photo.raw")
            .await
            .as_deref(),
        Some([b'p'; 192 * 1024].as_slice())
    );
    get_volume_manager().unregister(&id);
}

const CAFE_ZIP_NFC: &str = "caf\u{e9}.zip";
const CAFE_ZIP_NFD: &str = "cafe\u{301}.zip";

/// A registered SMB-shaped parent: byte-exact, composing new names, holding
/// `/share` and, when `existing` names one, a zip with one `stale.txt` entry.
async fn register_share(existing: Option<&str>) -> (String, Arc<InMemoryVolume>) {
    let id = format!("remote-share-{}", Uuid::new_v4());
    let vol = InMemoryVolume::new("Share")
        .with_lane_key(id.clone())
        .with_composed_new_names();
    vol.create_directory(Path::new("/share"))
        .await
        .expect("seed parent dir");
    if let Some(name) = existing {
        vol.create_file(&Path::new("/share").join(name), &zip_bytes(&[("stale.txt", b"old")]))
            .await
            .expect("seed existing zip");
    }
    let vol = Arc::new(vol);
    get_volume_manager().register(&id, Arc::clone(&vol) as Arc<dyn Volume>);
    (id, vol)
}

/// Starts a compress of one local `new.txt` into `/share/<name>` and, when it
/// starts, waits for it to complete.
async fn compress_to_share(parent_id: &str, name: &str) -> Result<(), WriteOperationError> {
    let (_src_tmp, source_volume) = local_source_with(&[("new.txt", b"brand new")]);
    let events = Arc::new(CollectorEventSink::new());
    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("new.txt")],
        Path::new("/share").join(name),
        parent_id.to_string(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await?;
    wait_until_async(Duration::from_secs(5), "a terminal event (complete or error)", || {
        !events.complete.lock_ignore_poison().is_empty() || !events.errors.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        !events.complete.lock_ignore_poison().is_empty(),
        "the compress should complete, errors: {:?}",
        events.errors.lock_ignore_poison()
    );
    Ok(())
}

async fn names_in_share(parent: &dyn Volume) -> Vec<String> {
    let mut names = sibling_names(parent, Path::new("/share/x.zip")).await;
    names.sort();
    names
}

#[tokio::test]
async fn a_new_archive_on_a_share_is_named_composed() {
    let (parent_id, parent) = register_share(None).await;

    compress_to_share(&parent_id, CAFE_ZIP_NFD)
        .await
        .expect("start compress");

    assert_eq!(names_in_share(parent.as_ref()).await, vec![CAFE_ZIP_NFC.to_string()]);
    let archive = Path::new("/share").join(CAFE_ZIP_NFC);
    assert_eq!(
        read_remote_entry(parent.as_ref(), &archive, "new.txt").await.as_deref(),
        Some(b"brand new".as_slice())
    );
    get_volume_manager().unregister(&parent_id);
}

/// A target the share holds in another spelling is the archive the dialog warned
/// about (`destination_exists` counts a look-alike), so it's replaced IN PLACE:
/// one archive, under the share's own spelling, ❌ never a twin beside it.
#[tokio::test]
async fn a_compress_onto_a_look_alike_of_an_existing_archive_replaces_it_in_place() {
    let (parent_id, parent) = register_share(Some(CAFE_ZIP_NFD)).await;

    compress_to_share(&parent_id, CAFE_ZIP_NFC)
        .await
        .expect("start compress");

    assert_eq!(names_in_share(parent.as_ref()).await, vec![CAFE_ZIP_NFD.to_string()]);
    let archive = Path::new("/share").join(CAFE_ZIP_NFD);
    assert_eq!(
        read_remote_entry(parent.as_ref(), &archive, "new.txt").await.as_deref(),
        Some(b"brand new".as_slice())
    );
    assert_eq!(
        read_remote_entry(parent.as_ref(), &archive, "stale.txt").await,
        None,
        "a fresh archive, never a merge into the old one"
    );
    get_volume_manager().unregister(&parent_id);
}

/// The mirror case: the target composes onto a name the share holds exactly.
/// Same entry, same answer: replaced in place, still one archive.
#[tokio::test]
async fn a_compress_whose_composed_name_the_share_holds_replaces_that_archive() {
    let (parent_id, parent) = register_share(Some(CAFE_ZIP_NFC)).await;

    compress_to_share(&parent_id, CAFE_ZIP_NFD)
        .await
        .expect("start compress");

    assert_eq!(names_in_share(parent.as_ref()).await, vec![CAFE_ZIP_NFC.to_string()]);
    let archive = Path::new("/share").join(CAFE_ZIP_NFC);
    assert_eq!(
        read_remote_entry(parent.as_ref(), &archive, "new.txt").await.as_deref(),
        Some(b"brand new".as_slice())
    );
    get_volume_manager().unregister(&parent_id);
}

/// Two archives fit the target and neither is spelled as asked: which one to
/// replace is a guess, so compression refuses before writing.
#[tokio::test]
async fn a_compress_two_stored_archives_fit_is_refused_and_leaves_both_alone() {
    let composed = "\u{e9}l\u{151}.zip";
    let decomposed = "e\u{301}lo\u{30b}.zip";
    let (parent_id, parent) = register_share(Some(composed)).await;
    parent
        .create_file(
            &Path::new("/share").join(decomposed),
            &zip_bytes(&[("stale.txt", b"old")]),
        )
        .await
        .expect("seed the second archive");

    let refused = compress_to_share(&parent_id, "\u{e9}lo\u{30b}.zip").await;

    assert!(
        matches!(refused, Err(WriteOperationError::DestinationExists { .. })),
        "{refused:?}"
    );
    let mut expected = vec![composed.to_string(), decomposed.to_string()];
    expected.sort();
    assert_eq!(names_in_share(parent.as_ref()).await, expected);
    for name in [composed, decomposed] {
        assert_eq!(
            read_remote_entry(parent.as_ref(), &Path::new("/share").join(name), "stale.txt")
                .await
                .as_deref(),
            Some(b"old".as_slice()),
            "{name:?} keeps its bytes"
        );
    }
    get_volume_manager().unregister(&parent_id);
}

/// The exact stored spelling is the name the dialog warned about: it's replaced
/// in place, under its own bytes, as it always was.
#[tokio::test]
async fn a_compress_onto_the_exact_stored_spelling_still_replaces_it_in_place() {
    let (parent_id, parent) = register_share(Some(CAFE_ZIP_NFD)).await;

    compress_to_share(&parent_id, CAFE_ZIP_NFD)
        .await
        .expect("start compress");

    assert_eq!(names_in_share(parent.as_ref()).await, vec![CAFE_ZIP_NFD.to_string()]);
    let archive = Path::new("/share").join(CAFE_ZIP_NFD);
    assert_eq!(
        read_remote_entry(parent.as_ref(), &archive, "new.txt").await.as_deref(),
        Some(b"brand new".as_slice())
    );
    get_volume_manager().unregister(&parent_id);
}
