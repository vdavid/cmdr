//! Compress onto a REMOTE parent (SMB / MTP, modeled by a non-local
//! `InMemoryVolume`). The net-new surface here is the SEED: a remote target must
//! be seeded THROUGH the parent volume, because `route_archive_copy_into`'s remote
//! path PULLS the target before editing — a local-FS seed would be invisible to
//! it. These pin: a fresh remote target gets seeded and packed, an overwrite
//! replaces the remote file with a fresh zip (never merges into it), the MTP swap
//! shape (delete-then-rename over a brand-new target) works, and no temp debris is
//! left at the user's destination.

use super::compress::compress_start;
use super::test_support::*;
use crate::file_system::volume::InMemoryVolume;
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
    let vol: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", tmp.path().to_path_buf()));
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
async fn compress_onto_a_remote_parent_seeds_and_packs_local_files() {
    // A local-FS seed at `/share/bundle.zip` is invisible to the remote parent's
    // pull, so pre-seed-through-Volume the copy-into pulls a missing file and the
    // entries never land — this test is RED until the seed goes through the volume.
    let (_src_tmp, source_volume) = local_source_with(&[("one.txt", b"first"), ("two.txt", b"second")]);
    let archive_path = PathBuf::from("/share/bundle.zip");
    let (parent_id, parent) = register_remote_parent(Path::new("/share"), false).await;

    let events = Arc::new(CollectorEventSink::new());
    compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source_volume,
        vec![PathBuf::from("one.txt"), PathBuf::from("two.txt")],
        archive_path.clone(),
        parent_id.clone(),
        ConflictResolution::Overwrite,
        0,
        None,
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start remote compress");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
        "remote compress should complete, errors: {:?}",
        events.errors.lock_ignore_poison()
    );

    assert_eq!(
        read_remote_entry(parent.as_ref(), &archive_path, "one.txt")
            .await
            .as_deref(),
        Some(b"first".as_slice()),
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

    get_volume_manager().unregister(&parent_id);
}

#[tokio::test]
async fn compress_onto_a_remote_parent_overwrites_an_existing_zip_with_a_fresh_archive() {
    // The target already holds a zip on the remote. Compress-overwrite REPLACES it
    // with a fresh archive of just the sources — it never merges into the old one
    // (the seed clears it to empty before the copy-into).
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
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start remote compress over existing");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
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
async fn compress_onto_an_mtp_style_remote_parent_seeds_and_packs() {
    // An MTP-shaped parent allows same-name siblings, so its swap is
    // delete-then-rename, not the atomic rename-replace. The seed's swap over a
    // BRAND-NEW target must tolerate the missing original (nothing to delete) — this
    // exercises that path without a virtual MTP device.
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
        crate::operation_log::types::Initiator::User,
    )
    .await
    .expect("start mtp-style remote compress");

    assert!(
        wait_until(|| !events.complete.lock_ignore_poison().is_empty()).await,
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
