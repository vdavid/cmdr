//! A picture, a HEIC, and a PDF on a live server render through the viewer's media path:
//! sniffed through the volume, then served whole and by `Range` with the bytes intact.
//! The scenario is backend-blind; the cells stay per backend because the integration lane
//! selects them by name prefix. `file_viewer/DETAILS.md` § "Media on a volume".

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::Volume;
use tauri::http::StatusCode;

use super::network_transfer_test_support::clean_deep;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_viewer::app_viewer as viewer;
use crate::file_viewer::content_kind::ViewerContentKind;
use crate::file_viewer::media::MediaSource;
use crate::file_viewer::media_protocol::serve_volume;

/// Past one read chunk and one cached block, so reassembly is really exercised.
const FILE_BYTES: usize = 3 * 1024 * 1024 + 123;

fn padded(head: &[u8]) -> Vec<u8> {
    let mut bytes = head.to_vec();
    bytes.extend((bytes.len()..FILE_BYTES).map(|i| (i % 251) as u8));
    bytes
}

fn a_png() -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::new(3, 2)
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .expect("a PNG in memory");
    padded(&bytes)
}

/// Opens `path` the way F3 does, and asks for it the way the webview does.
fn renders(id: &str, path: &Path, bytes: &[u8], kind: ViewerContentKind, mime: &str) {
    let opened = viewer()
        .open_session(&path.display().to_string(), id)
        .expect("the file opens");
    assert_eq!(opened.kind, kind, "{}", path.display());
    assert!(opened.media_from_volume, "served from the volume, not a temp");
    let token = opened.media_token.clone().expect("a media token");
    let Some(MediaSource::Volume(served)) = viewer().tokens().resolve_token(&token) else {
        panic!("a volume token");
    };
    let ask = |range: Option<&str>| {
        tauri::async_runtime::block_on(serve_volume(
            Arc::clone(&served),
            token.clone(),
            range.map(str::to_string),
            Duration::from_secs(45),
            |_load| {},
        ))
    };

    let whole = ask(None);
    assert_eq!(whole.status(), StatusCode::OK);
    assert_eq!(whole.headers().get("content-type").expect("a type"), mime);
    assert!(whole.body().as_slice() == bytes, "every byte, whole");

    let part = ask(Some("bytes=2097000-2097999"));
    assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
    assert!(
        part.body().as_slice() == &bytes[2_097_000..=2_097_999],
        "every byte, by range"
    );

    let raw = viewer()
        .get_bytes(&opened.session_id, 0, 8)
        .expect("Binary and Hex read it too");
    assert_eq!(raw.as_slice(), &bytes[..8]);

    viewer().close_session(&opened.session_id).expect("closes");
}

/// The three files, each written to `dir` and viewed there.
fn the_files() -> [(&'static str, Vec<u8>, ViewerContentKind, &'static str); 3] {
    [
        ("photo.png", a_png(), ViewerContentKind::Image, "image/png"),
        (
            "photo.heic",
            padded(b"\0\0\0\x18ftypheic\0\0\0\0mif1heic"),
            ViewerContentKind::Image,
            "image/heic",
        ),
        (
            "scan.pdf",
            padded(b"%PDF-1.7\n"),
            ViewerContentKind::Pdf,
            "application/pdf",
        ),
    ]
}

/// `write_dir` is where the volume writes; `view_dir` is how a pane names the same folder
/// (SMB's differ: relative to the share, under the mount).
fn pictures_and_a_pdf_render_through_the_volume(id: &str, remote: Arc<dyn Volume>, write_dir: &Path, view_dir: &Path) {
    get_volume_manager().register(id, Arc::clone(&remote));
    for (name, bytes, kind, mime) in the_files() {
        tauri::async_runtime::block_on(remote.create_file(&write_dir.join(name), &bytes)).expect("the fixture file");
        renders(id, &view_dir.join(name), &bytes, kind, mime);
    }
    get_volume_manager().unregister(id);
    // allowed-discarded-outcome: `clean_deep` answers `()`; a leftover scratch folder fails nothing.
    tauri::async_runtime::block_on(clean_deep(remote.as_ref(), write_dir));
}

#[test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
fn smb_integration_pictures_and_a_pdf_render_through_the_volume() {
    let (remote, dir) = tauri::async_runtime::block_on(super::smb_test_support::fixture());
    let view_dir = PathBuf::from(cmdr_smb::volume::testing::share_path(&dir.display().to_string()));
    pictures_and_a_pdf_render_through_the_volume("viewer-media-smb", remote, &dir, &view_dir);
}

#[test]
#[ignore = "needs the SFTP fixture stack: sftp-servers/start.sh (sftp-fixture)"]
fn sftp_integration_pictures_and_a_pdf_render_through_the_volume() {
    let (remote, dir) = tauri::async_runtime::block_on(super::sftp_test_support::fixture("viewer-media"));
    pictures_and_a_pdf_render_through_the_volume("viewer-media-sftp", remote, &dir, &dir);
}

#[test]
#[ignore = "needs the WebDAV fixture stack: apps/desktop/test/webdav-servers/start.sh (webdav-fixture)"]
fn webdav_integration_pictures_and_a_pdf_render_through_the_volume() {
    let (remote, dir) = tauri::async_runtime::block_on(super::webdav_test_support::fixture());
    pictures_and_a_pdf_render_through_the_volume("viewer-media-webdav", remote, &dir, &dir);
}

#[test]
#[ignore = "needs the NFS fixture stack: apps/desktop/test/nfs-servers/start.sh (nfs-fixture)"]
fn nfs_integration_pictures_and_a_pdf_render_through_the_volume() {
    let (remote, dir) = tauri::async_runtime::block_on(super::nfs_test_support::fixture("viewer-media"));
    pictures_and_a_pdf_render_through_the_volume("viewer-media-nfs", remote, &dir, &dir);
}

#[test]
#[ignore = "needs the S3 fixture stack: apps/desktop/test/s3-servers/start.sh (s3-fixture)"]
fn s3_integration_pictures_and_a_pdf_render_through_the_volume() {
    use cmdr_s3::volume::testing::{S3Target, VERSITYGW};
    let target = S3Target::Fixture(VERSITYGW);
    let volume = tauri::async_runtime::block_on(target.connect(Some(target.bucket())));
    let dir = volume.root().join(target.prefix("viewer-media").trim_end_matches('/'));
    pictures_and_a_pdf_render_through_the_volume("viewer-media-s3", Arc::new(volume), &dir, &dir);
}

/// Against the crate's fake server, so it needs no Docker and runs in the unit lane.
#[test]
fn adb_pictures_and_a_pdf_render_through_the_volume() {
    use super::adb_transfer_test::dialed_phone_with;
    let phone = tauri::async_runtime::block_on(dialed_phone_with(
        "VIEWERMEDIA",
        "viewer-media",
        cmdr_adb::testing::FakeTree::new(),
    ));
    for (name, bytes, kind, mime) in the_files() {
        let path = phone.dir.join(name);
        tauri::async_runtime::block_on(phone.volume.create_file(&path, &bytes)).expect("the fixture file");
        renders(&phone.volume_id, &path, &bytes, kind, mime);
    }
}

/// Against the virtual device: `read_range` is one `GetPartialObject64` per call.
#[cfg(feature = "virtual-mtp")]
#[test]
fn mtp_pictures_and_a_pdf_render_through_the_volume() {
    use crate::mtp::test_support::{connect_fixture, device_lock, teardown, volume_for};

    let id = "viewer-media-mtp";
    let files = the_files();
    let (guard, device, remote) = tauri::async_runtime::block_on(async {
        let guard = device_lock().await;
        let fixture = cmdr_mtp::virtual_device::setup_virtual_mtp_device();
        for (name, bytes, ..) in &files {
            std::fs::write(fixture.root().join("internal").join(name), bytes).expect("seed the device");
        }
        cmdr_mtp::virtual_device::rescan_virtual_device();
        let device = connect_fixture(fixture).await;
        let remote: Arc<dyn Volume> = Arc::new(volume_for(&device, None).await);
        (guard, device, remote)
    });
    get_volume_manager().register(id, Arc::clone(&remote));
    for (name, bytes, kind, mime) in &files {
        renders(id, &Path::new("/").join(name), bytes, *kind, mime);
    }
    get_volume_manager().unregister(id);
    drop(remote);
    tauri::async_runtime::block_on(teardown(device));
    drop(guard);
}
