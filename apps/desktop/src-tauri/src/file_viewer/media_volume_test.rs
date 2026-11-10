//! A picture or PDF on a volume renders like one on the local disk: sniffed through the
//! volume, served by position, with no temp, and honest when the server goes quiet.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use cmdr_fs::entry::FileEntry;

use tauri::http::{Response, StatusCode, header};

use super::content_kind::ViewerContentKind;
use super::media::MediaSource;
use super::media_protocol::serve_volume;
use super::media_volume::{MEDIA_ON_VOLUME_MAX_BYTES, MediaLoad, MediaLoadStage};
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{ListingProgress, Volume, VolumeError, VolumeRandomReader};
use crate::file_viewer::app_viewer as viewer;

/// `ftypheic`: the bytes the issue was about. Nothing in the app decodes them.
fn a_heic(len: usize) -> Vec<u8> {
    let mut bytes = vec![0, 0, 0, 24];
    bytes.extend_from_slice(b"ftypheic");
    bytes.extend((bytes.len()..len).map(|i| (i % 251) as u8));
    bytes
}

/// How a test's volume reads by position.
#[derive(Clone, Copy, PartialEq)]
enum Reads {
    /// `open_random_reader`, like SMB.
    Held,
    /// `read_range` only, like S3, SFTP, NFS, WebDAV, ADB, and MTP.
    Ranged,
    /// A held reader that can't open: the file sits behind a dead DFS link.
    DeadLink,
}

#[derive(Default)]
struct Counters {
    ranged_reads: AtomicUsize,
    /// A read waits here until a test lets it go.
    hold_reads: AtomicBool,
    fail_reads: AtomicBool,
}

struct OneFileVolume {
    root: PathBuf,
    file: PathBuf,
    bytes: Arc<Vec<u8>>,
    /// What a stat and a reader say, when a test wants more than it stores.
    claimed_size: u64,
    reads: Reads,
    counters: Arc<Counters>,
}

struct HeldReader {
    bytes: Arc<Vec<u8>>,
    size: u64,
    counters: Arc<Counters>,
}

/// The bytes at `[offset, offset + len)`, after whatever the test arranged.
async fn serve(bytes: Arc<Vec<u8>>, counters: Arc<Counters>, offset: u64, len: usize) -> Result<Vec<u8>, VolumeError> {
    while counters.hold_reads.load(Ordering::SeqCst) {
        // allowed-test-sleep: a server that doesn't answer is the subject.
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    if counters.fail_reads.load(Ordering::SeqCst) {
        return Err(VolumeError::ConnectionTimeout("the server dropped".into()));
    }
    let start = (offset as usize).min(bytes.len());
    let end = start.saturating_add(len).min(bytes.len());
    Ok(bytes[start..end].to_vec())
}

impl VolumeRandomReader for HeldReader {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(
        &self,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + '_>> {
        Box::pin(serve(Arc::clone(&self.bytes), Arc::clone(&self.counters), offset, len))
    }

    fn close(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
}

impl Volume for OneFileVolume {
    fn name(&self) -> &str {
        "Server"
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn supports_local_fs_access(&self) -> bool {
        false
    }

    fn paths_are_os_visible(&self) -> bool {
        false
    }

    fn list_directory<'a>(
        &'a self,
        _path: &'a Path,
        _on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, VolumeError>> + Send + 'a>> {
        Box::pin(async { Ok(Vec::new()) })
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<FileEntry, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if path != self.file {
                return Err(VolumeError::NotFound(path.display().to_string()));
            }
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned());
            let mut entry = FileEntry::new(name.unwrap_or_default(), path.display().to_string(), false, false);
            entry.size = Some(self.claimed_size);
            Ok(entry)
        })
    }

    fn open_random_reader<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeRandomReader>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if self.reads == Reads::DeadLink {
                return Err(VolumeError::LinkTargetUnreachable(path.display().to_string()));
            }
            if self.reads != Reads::Held {
                return Err(VolumeError::NotSupported);
            }
            if path != self.file {
                return Err(VolumeError::NotFound(path.display().to_string()));
            }
            Ok(Box::new(HeldReader {
                bytes: Arc::clone(&self.bytes),
                size: self.claimed_size,
                counters: Arc::clone(&self.counters),
            }) as Box<dyn VolumeRandomReader>)
        })
    }

    fn read_range<'a>(
        &'a self,
        _path: &'a Path,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if self.reads != Reads::Ranged {
                return Err(VolumeError::NotSupported);
            }
            self.counters.ranged_reads.fetch_add(1, Ordering::SeqCst);
            serve(Arc::clone(&self.bytes), Arc::clone(&self.counters), offset, len).await
        })
    }
}

/// Registers a volume under `id` holding `bytes` at `<root>/<name>`.
fn a_volume(id: &str, name: &str, bytes: Vec<u8>, reads: Reads) -> (String, Arc<Vec<u8>>, Arc<Counters>) {
    let claimed_size = bytes.len() as u64;
    a_volume_claiming(id, name, bytes, claimed_size, reads)
}

fn a_volume_claiming(
    id: &str,
    name: &str,
    bytes: Vec<u8>,
    claimed_size: u64,
    reads: Reads,
) -> (String, Arc<Vec<u8>>, Arc<Counters>) {
    let root = PathBuf::from(format!("/Volumes/cmdr-test-media-{id}"));
    let file = root.join(name);
    let bytes = Arc::new(bytes);
    let counters = Arc::new(Counters::default());
    let volume = Arc::new(OneFileVolume {
        root,
        file: file.clone(),
        bytes: Arc::clone(&bytes),
        claimed_size,
        reads,
        counters: Arc::clone(&counters),
    });
    get_volume_manager().register(id, volume as Arc<dyn Volume>);
    (file.display().to_string(), bytes, counters)
}
/// The reported bug (getcmdr/cmdr-reports#91): a HEIC on a share opened as text.
#[test]
fn a_heic_on_a_share_opens_as_an_image() {
    let id = "viewer-media-heic";
    let (path, _bytes, _counters) = a_volume(id, "photo.HEIC", a_heic(2048), Reads::Held);
    let result = viewer().open_session(&path, id).expect("opens");
    assert_eq!(result.kind, ViewerContentKind::Image);
    assert!(result.media_token.is_some());
    viewer().close_session(&result.session_id).unwrap();
    get_volume_manager().unregister(id);
}

/// The file's own trouble reaches the window: falling back to the mount could answer
/// `NotFound`, which reads as a deleted file.
#[test]
fn a_file_behind_a_dead_link_says_so() {
    let id = "viewer-media-dead-link";
    let (path, _bytes, _counters) = a_volume(id, "photo.HEIC", a_heic(2048), Reads::DeadLink);
    assert!(matches!(
        viewer().open_session(&path, id),
        Err(super::ViewerError::LinkTargetUnreachable)
    ));
    get_volume_manager().unregister(id);
}

/// A real PNG, padded past `len`, so dimensions have a true header to read.
fn a_png(len: usize) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::RgbaImage::new(3, 2)
        .write_to(&mut std::io::Cursor::new(&mut bytes), image::ImageFormat::Png)
        .unwrap();
    bytes.resize(bytes.len().max(len), 0);
    bytes
}

const STALL_LIMIT: Duration = Duration::from_secs(30);

fn volume_media(token: &str) -> Arc<super::media_volume::VolumeMedia> {
    match viewer().tokens().resolve_token(token).expect("the token resolves") {
        MediaSource::Volume(found) => found,
        _ => panic!("a file on a volume mints a volume token"),
    }
}

/// One `cmdr-media://` request against a session's token, and what it told the window.
fn request(token: &str, range: Option<&str>, stall_limit: Duration) -> (Response<Vec<u8>>, Vec<MediaLoad>) {
    let heard = Arc::new(Mutex::new(Vec::new()));
    let heard_by_emit = Arc::clone(&heard);
    let response = tauri::async_runtime::block_on(serve_volume(
        volume_media(token),
        token.to_string(),
        range.map(str::to_string),
        stall_limit,
        move |load| heard_by_emit.lock().unwrap().push(load),
    ));
    let heard = heard.lock().unwrap().clone();
    (response, heard)
}

/// Both ways of reading by position, end to end: the sniff, the token, a whole-file
/// request (every `<img>`), and a `Range` request (a PDF plugin's).
#[test]
fn both_ways_of_reading_by_position_serve_the_whole_file_and_a_range() {
    for (id, reads) in [
        ("viewer-media-held", Reads::Held),
        ("viewer-media-ranged", Reads::Ranged),
    ] {
        let (path, bytes, _counters) = a_volume(id, "photo.png", a_png(3 * 1024 * 1024), reads);
        let opened = viewer().open_session(&path, id).expect("opens");
        assert_eq!(opened.kind, ViewerContentKind::Image);
        assert!(opened.media_from_volume, "the window follows this request");
        let dimensions = opened.media_dimensions.expect("a PNG header reads through the volume");
        assert_eq!((dimensions.width, dimensions.height), (3, 2));
        let token = opened.media_token.clone().expect("a token");

        let (whole, heard) = request(&token, None, STALL_LIMIT);
        assert_eq!(whole.status(), StatusCode::OK);
        assert_eq!(whole.headers().get(header::CONTENT_TYPE).unwrap(), "image/png");
        assert_eq!(whole.headers().get(header::ACCEPT_RANGES).unwrap(), "bytes");
        assert_eq!(whole.body(), bytes.as_ref());
        let last = heard.last().expect("the window hears how it went");
        assert_eq!(last.stage, MediaLoadStage::Done);
        assert_eq!(last.token, token);
        assert_eq!(last.bytes_done, bytes.len() as u64);

        let (part, _) = request(&token, Some("bytes=1048570-1048579"), STALL_LIMIT);
        assert_eq!(part.status(), StatusCode::PARTIAL_CONTENT);
        assert_eq!(
            part.headers().get(header::CONTENT_RANGE).unwrap(),
            format!("bytes 1048570-1048579/{}", bytes.len()).as_str()
        );
        assert_eq!(part.body().as_slice(), &bytes[1048570..=1048579]);

        let (past_the_end, _) = request(&token, Some("bytes=99999999-"), STALL_LIMIT);
        assert_eq!(past_the_end.status(), StatusCode::RANGE_NOT_SATISFIABLE);

        viewer().close_session(&opened.session_id).unwrap();
        assert!(
            viewer().tokens().resolve_token(&token).is_none(),
            "closing drops the token"
        );
        get_volume_manager().unregister(id);
    }
}

/// A server that reads by position needs no temp: this volume has no stream at all, so a
/// pull could only have failed. And a big PDF costs what's asked for, not the whole file.
#[test]
fn a_pdf_on_a_server_opens_with_no_pull_and_a_range_reads_only_its_blocks() {
    let id = "viewer-media-pdf";
    let mut pdf = b"%PDF-1.7\n".to_vec();
    pdf.resize(40 * 1024 * 1024, b' ');
    let (path, _bytes, counters) = a_volume(id, "scan.pdf", pdf, Reads::Ranged);
    let opened = viewer().open_session(&path, id).expect("opens without a stream");
    assert_eq!(opened.kind, ViewerContentKind::Pdf);
    assert!(opened.media_dimensions.is_none());
    let token = opened.media_token.clone().expect("a token");
    assert!(
        counters.ranged_reads.load(Ordering::SeqCst) <= 2,
        "the open reads a stat's worth and the head"
    );

    let before = counters.ranged_reads.load(Ordering::SeqCst);
    let at = 30 * 1024 * 1024;
    let (part, _) = request(&token, Some(&format!("bytes={at}-{}", at + 99)), STALL_LIMIT);
    assert_eq!(part.body().len(), 100);
    assert_eq!(counters.ranged_reads.load(Ordering::SeqCst) - before, 1);

    viewer().close_session(&opened.session_id).unwrap();
    get_volume_manager().unregister(id);
}

/// Binary and Hex read a media session's file: on a volume, through the same reader.
#[test]
fn raw_bytes_of_a_picture_on_a_volume_read_through_the_volume() {
    for (id, reads) in [
        ("viewer-media-raw-held", Reads::Held),
        ("viewer-media-raw-ranged", Reads::Ranged),
    ] {
        let (path, bytes, _counters) = a_volume(id, "photo.heic", a_heic(4096), reads);
        let opened = viewer().open_session(&path, id).expect("opens");
        let raw = viewer()
            .get_bytes(&opened.session_id, 8, 16)
            .expect("reads through the volume");
        assert_eq!(raw.as_slice(), &bytes[8..24]);
        viewer().close_session(&opened.session_id).unwrap();
        get_volume_manager().unregister(id);
    }
}

/// "View as text" still means text, and an SVG still needs its extension and its root.
#[test]
fn view_as_text_and_svg_keep_their_rules_on_a_volume() {
    let id = "viewer-media-as-text";
    let (path, _bytes, _counters) = a_volume(id, "photo.heic", a_heic(4096), Reads::Held);
    let opened = viewer().open_session_as_text(&path, id).expect("opens");
    assert_eq!(opened.kind, ViewerContentKind::Text);
    assert!(opened.media_token.is_none());
    viewer().close_session(&opened.session_id).unwrap();
    get_volume_manager().unregister(id);

    let id = "viewer-media-svg";
    let svg = br#"<svg xmlns="http://www.w3.org/2000/svg"></svg>"#.to_vec();
    let (path, _bytes, _counters) = a_volume(id, "logo.svg", svg, Reads::Held);
    let opened = viewer().open_session(&path, id).expect("opens");
    assert_eq!(opened.kind, ViewerContentKind::Image);
    let token = opened.media_token.clone().expect("a token");
    let (response, _) = request(&token, None, STALL_LIMIT);
    assert_eq!(response.headers().get(header::CONTENT_TYPE).unwrap(), "image/svg+xml");
    viewer().close_session(&opened.session_id).unwrap();
    get_volume_manager().unregister(id);
}

/// One whole-file request holds the file in memory, so there's a line. Past it a held
/// reader opens the text and hex views it always did.
#[test]
fn a_picture_past_the_memory_line_opens_as_text_on_a_held_reader() {
    let id = "viewer-media-too-big";
    let claimed = MEDIA_ON_VOLUME_MAX_BYTES + 1;
    let (path, _bytes, _counters) = a_volume_claiming(id, "huge.png", a_png(4096), claimed, Reads::Held);
    let opened = viewer().open_session(&path, id).expect("opens");
    assert_eq!(opened.kind, ViewerContentKind::Text);
    viewer().close_session(&opened.session_id).unwrap();
    get_volume_manager().unregister(id);
}

/// A hung server: the request answers 504 once the bytes stop, the window hears it, and
/// the read stops at its next chunk once the server wakes.
#[test]
fn a_server_that_stops_answering_fails_the_request_on_the_stall_limit() {
    let id = "viewer-media-hung";
    let (path, _bytes, counters) = a_volume(id, "photo.png", a_png(3 * 1024 * 1024), Reads::Ranged);
    let opened = viewer().open_session(&path, id).expect("opens");
    let token = opened.media_token.clone().expect("a token");

    counters.hold_reads.store(true, Ordering::SeqCst);
    let (response, heard) = request(&token, None, Duration::from_millis(400));
    assert_eq!(response.status(), StatusCode::GATEWAY_TIMEOUT);
    assert_eq!(heard.last().expect("told").stage, MediaLoadStage::Failed);
    let reads_when_it_gave_up = counters.ranged_reads.load(Ordering::SeqCst);

    counters.hold_reads.store(false, Ordering::SeqCst);
    let (retried, _) = request(&token, Some("bytes=2097152-2097161"), STALL_LIMIT);
    assert_eq!(retried.status(), StatusCode::PARTIAL_CONTENT, "a retry reads again");
    assert!(counters.ranged_reads.load(Ordering::SeqCst) > reads_when_it_gave_up);

    viewer().close_session(&opened.session_id).unwrap();
    get_volume_manager().unregister(id);
}

/// A read the volume refuses (twice: the source reopens once) reports the failure, so
/// the window doesn't blame the file.
#[test]
fn a_read_the_server_refuses_tells_the_window() {
    let id = "viewer-media-refused";
    let (path, _bytes, counters) = a_volume(id, "photo.png", a_png(3 * 1024 * 1024), Reads::Held);
    let opened = viewer().open_session(&path, id).expect("opens");
    let token = opened.media_token.clone().expect("a token");

    counters.fail_reads.store(true, Ordering::SeqCst);
    let (response, heard) = request(&token, None, STALL_LIMIT);
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(heard.last().expect("told").stage, MediaLoadStage::Failed);

    viewer().close_session(&opened.session_id).unwrap();
    get_volume_manager().unregister(id);
}

/// Closing the window stops the read between chunks, and nobody gets told: there's no
/// one left to tell, and Retry isn't on offer in a window that's gone.
#[test]
fn closing_the_session_stops_a_read_in_flight() {
    let id = "viewer-media-closed";
    let (path, _bytes, counters) = a_volume(id, "photo.png", a_png(64 * 1024 * 1024), Reads::Ranged);
    let opened = viewer().open_session(&path, id).expect("opens");
    let token = opened.media_token.clone().expect("a token");
    let media = volume_media(&token);

    counters.hold_reads.store(true, Ordering::SeqCst);
    let heard = Arc::new(Mutex::new(Vec::new()));
    let heard_by_emit = Arc::clone(&heard);
    let in_flight = tauri::async_runtime::spawn(serve_volume(
        media,
        token.clone(),
        None,
        STALL_LIMIT,
        move |load: MediaLoad| heard_by_emit.lock().unwrap().push(load),
    ));
    let reads_before_the_close = counters.ranged_reads.load(Ordering::SeqCst);
    crate::test_support::wait_until(Duration::from_secs(5), "the first read to arrive", || {
        counters.ranged_reads.load(Ordering::SeqCst) > reads_before_the_close
    });

    viewer().close_session(&opened.session_id).unwrap();
    counters.hold_reads.store(false, Ordering::SeqCst);
    let response = tauri::async_runtime::block_on(in_flight).expect("the request answers");

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert!(
        counters.ranged_reads.load(Ordering::SeqCst) - reads_before_the_close <= 2,
        "the read stopped at its next chunk rather than walking the rest of the file"
    );
    assert!(
        heard
            .lock()
            .unwrap()
            .iter()
            .all(|load| load.stage == MediaLoadStage::Reading),
        "a closed window hears no verdict"
    );
    get_volume_manager().unregister(id);
}
