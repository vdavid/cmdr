//! Remote-backed archive tests over a virtual MTP device (M5).
//!
//! The MTP counterpart to the live-SMB archive tests (`smb_archive_integration_test`):
//! a zip living on a virtual MTP device browses and extracts through
//! `MtpVolume::read_range` (GetPartialObject64), and a remote EDIT (pull → apply
//! → upload → swap) commits through the device's delete-then-rename swap (MTP
//! allows same-name siblings, so it must NOT attempt an atomic rename-overwrite).
//!
//! The whole module requires the `virtual-mtp` feature, so it's a sibling test
//! module of `mtp_test`, gated on the feature in `backends/mod.rs`; `super::*`
//! reaches the backend re-exports (`MtpVolume`, `Volume`).

use super::*;
use crate::mtp::connection::connection_manager;
use crate::mtp::virtual_device::VirtualDeviceFixture;
use std::path::Path;

/// Builds a small zip: one STORED root entry, one DEFLATED entry in a subdir.
#[cfg(feature = "virtual-mtp")]
fn archive_test_zip() -> Vec<u8> {
    use std::io::Write as _;
    let mut w = zip::ZipWriter::new(std::io::Cursor::new(Vec::new()));
    let stored = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
    let deflated = zip::write::SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    w.start_file("a.txt", stored).unwrap();
    w.write_all(b"hello from mtp").unwrap();
    w.start_file("dir/b.txt", deflated).unwrap();
    w.write_all(b"deflated over usb").unwrap();
    w.finish().unwrap().into_inner()
}

/// Connects a virtual MTP device seeded with `zip_bytes` at `internal/bundle.zip`
/// and returns `(device_id, storage_id)` of the writable internal storage, with
/// the root path cache primed.
#[cfg(feature = "virtual-mtp")]
async fn connect_virtual_device_with_zip(zip_bytes: &[u8]) -> (String, u32, VirtualDeviceFixture) {
    use crate::mtp::virtual_device::{rescan_virtual_device, setup_virtual_mtp_device};

    let fixture = setup_virtual_mtp_device();
    // Seed the zip into the writable internal storage's backing dir, then rescan
    // so the device's object tree picks it up (the watcher is off in tests).
    std::fs::write(fixture.root().join("internal/bundle.zip"), zip_bytes).expect("seed zip on device");
    rescan_virtual_device();

    let device_id = crate::mtp::list_mtp_devices()
        .into_iter()
        .find(|d| d.location_id == fixture.location_id)
        .map(|d| d.id)
        .expect("the virtual device must appear in discovery");
    let info = connection_manager()
        .connect(&device_id, None)
        .await
        .expect("virtual-mtp connect should succeed");
    let storage_id = info.storages.first().expect("a storage").id;
    connection_manager()
        .list_directory(&device_id, storage_id, "/")
        .await
        .expect("list root should succeed");
    (device_id, storage_id, fixture)
}

/// Disconnects and unregisters, so the next test doesn't inherit this device's
/// registration under the shared virtual-device id.
#[cfg(feature = "virtual-mtp")]
async fn teardown(device_id: &str, fixture: VirtualDeviceFixture) {
    connection_manager()
        .disconnect(device_id, None, crate::mtp::connection::MtpDisconnectReason::User)
        .await
        .ok();
    crate::mtp::virtual_device::unregister_virtual_mtp_device(fixture.location_id);
}

/// Streams a virtual-MTP zip back off the device and parses it into a
/// `name -> contents` map (re-verifies edits THROUGH the device, not a local copy).
#[cfg(feature = "virtual-mtp")]
async fn read_device_zip(vol: &MtpVolume, path: &Path) -> std::collections::HashMap<String, Vec<u8>> {
    use std::io::Read as _;
    let mut stream = vol.open_read_stream(path).await.expect("open device archive");
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend_from_slice(&chunk.expect("read archive chunk"));
    }
    let mut archive = zip::ZipArchive::new(std::io::Cursor::new(bytes)).expect("device archive parses");
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

/// A zip on a virtual MTP device BROWSES and EXTRACTS through `read_range`
/// (GetPartialObject64) — the MTP counterpart to the SMB read-path test.
#[cfg(feature = "virtual-mtp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn virtual_mtp_archive_browses_and_extracts_via_read_range() {
    use crate::file_system::volume::backends::archive::{ArchiveFormat, ArchiveVolume};
    use std::sync::Arc;

    async fn drain(archive: &ArchiveVolume, inner: &str) -> Vec<u8> {
        let mut stream = archive
            .open_read_stream(Path::new(inner))
            .await
            .expect("open inner entry");
        let mut out = Vec::new();
        while let Some(chunk) = stream.next_chunk().await {
            out.extend_from_slice(&chunk.expect("extract chunk"));
        }
        out
    }

    let _guard = crate::mtp::virtual_device::virtual_device_test_lock().lock().await;
    let (device_id, storage_id, fixture) = connect_virtual_device_with_zip(&archive_test_zip()).await;

    let vol = Arc::new(MtpVolume::new(&device_id, storage_id, "Internal"));
    assert!(!vol.supports_local_fs_access(), "MTP is not local-FS-backed");

    let archive = ArchiveVolume::new(
        Arc::clone(&vol) as Arc<dyn Volume>,
        std::path::PathBuf::from("/bundle.zip"),
        ArchiveFormat::Zip,
    );

    // Browse: synthetic `dir` first, then the root file.
    let root_listing = archive
        .list_directory(Path::new(""), None)
        .await
        .expect("browse archive root");
    let names: Vec<String> = root_listing.iter().map(|e| e.name.clone()).collect();
    assert_eq!(
        names,
        vec!["dir", "a.txt"],
        "unexpected archive root listing: {names:?}"
    );

    // Extract a STORED and a DEFLATED entry, pulled through GetPartialObject64.
    assert_eq!(drain(&archive, "a.txt").await, b"hello from mtp");
    assert_eq!(drain(&archive, "dir/b.txt").await, b"deflated over usb");

    teardown(&device_id, fixture).await;
}

/// A remote EDIT (delete an inner entry) commits on a virtual MTP device through
/// the delete-then-rename swap, re-verified by re-reading the zip off the device.
#[cfg(feature = "virtual-mtp")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn virtual_mtp_remote_zip_edit_deletes_an_entry_through_the_device() {
    use crate::file_system::volume::backends::archive::mutator::{self, Changeset, MutationHooks};
    use crate::file_system::write_operations::{RemoteEditError, WriteOperationState, pull_apply_upload_swap};
    use std::sync::Arc;
    use std::time::Duration;

    struct NoHooks;
    impl MutationHooks for NoHooks {}

    let _guard = crate::mtp::virtual_device::virtual_device_test_lock().lock().await;
    let (device_id, storage_id, fixture) = connect_virtual_device_with_zip(&archive_test_zip()).await;

    let vol = Arc::new(MtpVolume::new(&device_id, storage_id, "Internal"));
    // MTP allows same-name siblings, so the swap MUST take delete-then-rename.
    assert!(
        !vol.create_directory_errors_on_existing_dir(),
        "MTP allows same-name siblings"
    );

    let archive_path = std::path::PathBuf::from("/bundle.zip");
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let result = pull_apply_upload_swap(
        Arc::clone(&vol) as Arc<dyn Volume>,
        archive_path.clone(),
        state,
        move |working: &Path| -> Result<(), RemoteEditError> {
            let changeset = Changeset {
                deletes: vec!["a.txt".to_string()],
                ..Default::default()
            };
            mutator::apply(working, &changeset, &NoHooks).expect("local mutator apply");
            Ok(())
        },
    )
    .await;
    assert!(result.is_ok(), "a remote MTP zip edit should commit");

    // Re-verify through the device: a.txt gone, dir/b.txt retained.
    let back = read_device_zip(vol.as_ref(), &archive_path).await;
    assert!(!back.contains_key("a.txt"), "the deleted entry is gone on the device");
    assert_eq!(
        back.get("dir/b.txt").map(Vec::as_slice),
        Some(b"deflated over usb".as_slice())
    );

    // Exactly one bundle.zip remains (no sibling duplicate from the swap), no temp.
    let entries = connection_manager()
        .list_directory(&device_id, storage_id, "/")
        .await
        .expect("list root");
    assert_eq!(
        entries.iter().filter(|e| e.name == "bundle.zip").count(),
        1,
        "exactly one archive object remains (no duplicate)"
    );
    assert!(
        !entries.iter().any(|e| e.name.contains(".cmdr-tmp-")),
        "no leftover upload temp on the device"
    );

    teardown(&device_id, fixture).await;
}
