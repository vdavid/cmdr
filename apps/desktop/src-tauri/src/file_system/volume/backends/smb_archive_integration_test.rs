//! Integration tests for remote-backed archives on a live SMB share (M5).
//!
//! The remote counterpart to the in-memory `remote_backed_archive_*` unit tests
//! in `archive/volume_test.rs`: a zip living on a REAL SMB share browses and
//! extracts through `SmbVolume::read_range`, write-routing detects a zip-inner
//! path over SMB, an extract-out materializes to local disk, and a remote EDIT
//! (pull → apply → upload → swap) commits, while a cancel before the swap leaves
//! the remote original byte-for-byte intact.
//!
//! Every test here is `#[ignore]`d so default runs skip it. Start the
//! containers with `./apps/desktop/test/smb-servers/start.sh`, then run
//! `cargo nextest run smb_integration --run-ignored all`. Declared as a
//! `#[cfg(test)]` submodule of `smb` alongside `smb_integration_test`; shared
//! helpers come from `super::smb_test_support`.

use super::smb_test_support::*;
use super::*;

/// End-to-end proof that a zip living on a REAL SMB share browses and extracts
/// through `SmbVolume::read_range` (backed by `smb2::FileReader`) — the remote
/// counterpart to the in-memory `remote_backed_archive_*` unit tests in
/// `archive/volume_test.rs`. This is the integration link the ranged-read
/// primitive exists for.
///
/// Writes a small zip to the share, wraps the live `SmbVolume` as an
/// `ArchiveVolume` parent (a direct-SMB volume reports
/// `supports_local_fs_access() == false`, so the archive takes the ranged-read
/// path, not a local `pread`), then lists the root and extracts a STORED and a
/// DEFLATED entry, checking the decompressed bytes.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_archive_browse_and_extract_via_read_range() {
    use crate::file_system::volume::backends::archive::{ArchiveFormat, ArchiveVolume};
    use std::io::Write as _;

    async fn drain_archive(archive: &ArchiveVolume, inner: &str) -> Vec<u8> {
        let mut stream = archive.open_read_stream(Path::new(inner)).await.unwrap();
        let mut out = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            out.extend_from_slice(&chunk.expect("archive extract chunk"));
        }
        out
    }

    let vol = Arc::new(make_docker_volume().await);

    // Build a small zip: one STORED entry at the root, one DEFLATED entry in a
    // subdirectory (so the synthetic-directory browse path runs too).
    let zip_bytes = {
        let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
        let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
        let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
        w.start_file("a.txt", stored).unwrap();
        w.write_all(b"hello").unwrap();
        w.start_file("dir/b.txt", deflated).unwrap();
        w.write_all(b"world from a deflated entry").unwrap();
        w.finish().unwrap().into_inner()
    };

    // Unique root-level name so the no-clobber `create_file` never collides and
    // no directory setup is needed.
    let zip_path = PathBuf::from(format!("/{}.zip", test_dir_name()));
    vol.create_file(&zip_path, &zip_bytes).await.unwrap();

    // Wrap the live SMB volume as the archive's parent. Direct SMB has no local
    // FS access, so every archive read flows through `SmbVolume::read_range`.
    assert!(!vol.supports_local_fs_access());
    let archive = ArchiveVolume::new(
        Arc::clone(&vol) as Arc<dyn Volume>,
        zip_path.clone(),
        ArchiveFormat::Zip,
    );

    // Browse: root shows the synthetic `dir` first, then the file.
    let root = archive.list_directory(Path::new(""), None).await.unwrap();
    let names: Vec<String> = root.iter().map(|e| e.name.clone()).collect();
    assert_eq!(names, vec!["dir", "a.txt"], "unexpected archive root listing");

    // Extract both entries, pulling every byte through the ranged-read seam.
    assert_eq!(drain_archive(&archive, "a.txt").await, b"hello");
    assert_eq!(
        drain_archive(&archive, "dir/b.txt").await,
        b"world from a deflated entry"
    );

    // Cleanup: remove the zip from the share.
    let _ = vol.delete(&zip_path).await;
}

// ── Remote-backed archive write-routing + edit (M5) ─────────────────
//
// The read-path counterpart above (`smb_integration_archive_browse_and_extract_via_read_range`)
// proves a zip on a share BROWSES and EXTRACTS. These prove the WRITE side end
// to end on a real share: the async parent-aware routing predicate detects a
// zip-inner path over SMB, an extract-out materializes to local disk, and a
// remote EDIT (pull → apply → upload → swap) commits — while a cancel before the
// swap leaves the remote original byte-for-byte intact. The data-safety contract
// unit-tested with an `InMemoryVolume` in `archive_remote_edit_tests`, now over
// real SMB.

/// A no-op `MutationHooks` for the mutator (never pauses/cancels here).
struct RemoteEditNoHooks;
impl crate::file_system::volume::backends::archive::mutator::MutationHooks for RemoteEditNoHooks {}

/// Streams a zip back off the share via `open_read_stream` and parses it into a
/// `name -> contents` map, so assertions re-verify the archive THROUGH THE SHARE
/// (not the local working copy). A corrupt swap fails loudly here.
async fn read_share_zip(vol: &SmbVolume, path: &Path) -> std::collections::HashMap<String, Vec<u8>> {
    use std::io::Read as _;
    let mut stream = vol.open_read_stream(path).await.expect("open remote archive");
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk.expect("read archive chunk"));
    }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("share archive parses");
    let mut out = std::collections::HashMap::new();
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).expect("entry");
        let name = entry.name().to_string();
        let mut buf = Vec::new();
        entry.read_to_end(&mut buf).expect("entry bytes");
        out.insert(name, buf);
    }
    out
}

/// True if a `<zip>.cmdr-tmp-*` upload temp for `zip_path` lingers in the share
/// root (a debris check scoped to this test's unique zip name, so it ignores
/// other parallel tests' artifacts).
async fn upload_temp_lingers(vol: &SmbVolume, zip_path: &Path) -> bool {
    let temp_prefix = format!(
        "{}.cmdr-tmp-",
        zip_path.file_name().expect("zip name").to_string_lossy()
    );
    vol.list_directory_impl(Path::new("/"))
        .await
        .map(|entries| entries.iter().any(|e| e.name.starts_with(&temp_prefix)))
        .unwrap_or(false)
}

/// Two entries: a stored one to keep and a deflated one to drop, so a delete edit
/// has something to remove and something to retain (verbatim raw-copy).
fn two_entry_zip() -> Vec<u8> {
    use std::io::Write as _;
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    w.start_file("keep.txt", stored).unwrap();
    w.write_all(b"keep me").unwrap();
    w.start_file("drop.txt", deflated).unwrap();
    w.write_all(b"delete me from the share").unwrap();
    w.finish().unwrap().into_inner()
}

/// The async, parent-aware write-routing predicate detects a zip-INNER path on a
/// real SMB share (the `std::fs`-only sync predicate would wrongly return false),
/// and an extract-out streams an entry to LOCAL disk through the ranged reads.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_archive_routing_detection_and_extract_out() {
    use crate::file_system::get_volume_manager;
    use crate::file_system::volume::backends::archive::{ArchiveFormat, ArchiveVolume};
    use std::io::Write as _;

    let vol = Arc::new(make_docker_volume().await);
    let zip_path = PathBuf::from(format!("/{}.zip", test_dir_name()));
    vol.create_file(&zip_path, &two_entry_zip()).await.unwrap();

    // Register the live SMB volume so the parent-aware predicate confirms the
    // boundary through the SMB volume's OWN `get_metadata` + `read_range`.
    let vol_id = "smb-archive-routing-test";
    get_volume_manager().register(vol_id, Arc::clone(&vol) as Arc<dyn Volume>);
    assert!(!vol.supports_local_fs_access(), "direct SMB is not local-FS-backed");

    // A genuinely-inner path routes; the `.zip` file itself is a plain file.
    assert!(
        get_volume_manager()
            .path_is_inside_archive(vol_id, &zip_path.join("drop.txt"))
            .await,
        "a zip-inner path on a real SMB share must be detected (write-routing reaches the edit driver)"
    );
    assert!(
        !get_volume_manager().path_is_inside_archive(vol_id, &zip_path).await,
        "the `.zip` file itself is a plain file, not archive-inner"
    );

    // Extract-out: stream a DEFLATED entry through the archive (over `read_range`)
    // and materialize it to LOCAL disk, then read it back.
    let archive = ArchiveVolume::new(
        Arc::clone(&vol) as Arc<dyn Volume>,
        zip_path.clone(),
        ArchiveFormat::Zip,
    );
    let local_dir = tempfile::tempdir().unwrap();
    let out_path = local_dir.path().join("drop.txt");
    {
        let mut stream = archive.open_read_stream(Path::new("drop.txt")).await.unwrap();
        let mut f = std::fs::File::create(&out_path).unwrap();
        while let Some(chunk) = stream.next_chunk().await {
            f.write_all(&chunk.expect("extract chunk")).unwrap();
        }
        f.flush().unwrap();
    }
    assert_eq!(std::fs::read(&out_path).unwrap(), b"delete me from the share");

    get_volume_manager().unregister(vol_id);
    let _ = vol.delete(&zip_path).await;
}

/// A remote EDIT (delete an inner entry) commits through pull → apply → upload →
/// swap against a real SMB share, and the result is re-verified BY RE-READING the
/// zip off the share. No upload temp lingers.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_remote_zip_edit_deletes_an_entry_through_the_share() {
    use crate::file_system::volume::backends::archive::mutator::{self, Changeset};
    use crate::file_system::write_operations::{RemoteEditError, WriteOperationState, pull_apply_upload_swap};
    use std::time::Duration;

    let vol = Arc::new(make_docker_volume().await);
    let zip_path = PathBuf::from(format!("/{}.zip", test_dir_name()));
    vol.create_file(&zip_path, &two_entry_zip()).await.unwrap();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let result = pull_apply_upload_swap(
        Arc::clone(&vol) as Arc<dyn Volume>,
        zip_path.clone(),
        state,
        move |working: &Path| -> Result<(), RemoteEditError> {
            let changeset = Changeset {
                deletes: vec!["drop.txt".to_string()],
                ..Default::default()
            };
            mutator::apply(working, &changeset, &RemoteEditNoHooks).expect("local mutator apply");
            Ok(())
        },
    )
    .await;
    assert!(result.is_ok(), "a remote SMB zip edit should commit");

    // Re-verify through the share: the deleted entry is gone, the kept one survives.
    let back = read_share_zip(vol.as_ref(), &zip_path).await;
    assert!(
        back.contains_key("keep.txt"),
        "the retained entry survives on the share"
    );
    assert!(
        !back.contains_key("drop.txt"),
        "the deleted entry is gone on the share, got: {:?}",
        back.keys().collect::<Vec<_>>()
    );
    assert_eq!(back.get("keep.txt").map(Vec::as_slice), Some(b"keep me".as_slice()));

    assert!(
        !upload_temp_lingers(vol.as_ref(), &zip_path).await,
        "no leftover upload temp after the swap"
    );

    let _ = vol.delete(&zip_path).await;
}

/// A cancel landing AFTER the local apply but BEFORE the remote swap leaves the
/// share's original byte-for-byte intact (both entries still present) and drops no
/// upload temp — the core M5 data-safety guarantee, over real SMB.
#[tokio::test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_remote_zip_edit_cancel_before_swap_keeps_original() {
    use crate::file_system::volume::backends::archive::mutator::{self, Changeset};
    use crate::file_system::write_operations::{
        OperationIntent, RemoteEditError, WriteOperationState, pull_apply_upload_swap,
    };
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    let vol = Arc::new(make_docker_volume().await);
    let zip_path = PathBuf::from(format!("/{}.zip", test_dir_name()));
    vol.create_file(&zip_path, &two_entry_zip()).await.unwrap();

    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let state_for_closure = Arc::clone(&state);
    // Apply the delete to the LOCAL working copy, then flip the op to cancelled so
    // the orchestrator's pre-upload cancel check trips before touching the share.
    let result = pull_apply_upload_swap(
        Arc::clone(&vol) as Arc<dyn Volume>,
        zip_path.clone(),
        state,
        move |working: &Path| -> Result<(), RemoteEditError> {
            let changeset = Changeset {
                deletes: vec!["drop.txt".to_string()],
                ..Default::default()
            };
            mutator::apply(working, &changeset, &RemoteEditNoHooks).expect("local mutator apply");
            state_for_closure
                .intent
                .store(OperationIntent::Stopped as u8, Ordering::Relaxed);
            Ok(())
        },
    )
    .await;
    assert!(
        matches!(result, Err(RemoteEditError::Cancelled)),
        "a cancel before the swap must report Cancelled"
    );

    // The share's original is intact: BOTH entries still present, no temp debris.
    let back = read_share_zip(vol.as_ref(), &zip_path).await;
    assert!(back.contains_key("keep.txt"), "the original keep.txt survives");
    assert!(
        back.contains_key("drop.txt"),
        "the cancelled delete never reached the share"
    );
    assert!(
        !upload_temp_lingers(vol.as_ref(), &zip_path).await,
        "a cancelled edit leaves no upload temp on the share"
    );

    let _ = vol.delete(&zip_path).await;
}
