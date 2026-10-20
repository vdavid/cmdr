//! Tests for the volume-backed byte source (#303): a big file on a share opens through
//! the volume's held random reader, jumps cost a bounded number of reads over ONE open,
//! and a slow share answers a scrollbar drag inside the fetch deadline.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use cmdr_fs::entry::FileEntry;

use super::source::ByteSource;
use super::volume_source::{BLOCK_BYTES, VolumeSource};
use super::{SeekTarget, ViewerError, session};
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_system::volume::{ListingProgress, Volume, VolumeError, VolumeRandomReader};

/// Every synthetic row is 15 digits plus a newline.
const ROW_BYTES: u64 = 16;
const FIVE_GIB: u64 = 5 * 1024 * 1024 * 1024;
/// The viewer's per-fetch deadline (`commands/file_viewer.rs::VIEWER_TIMEOUT`).
const FETCH_DEADLINE: Duration = Duration::from_secs(2);

/// The byte at `offset` of the synthetic file: row `n` is `format!("{n:015}\n")`.
fn synthetic_byte(offset: u64) -> u8 {
    let row = offset / ROW_BYTES;
    let column = offset % ROW_BYTES;
    if column == ROW_BYTES - 1 {
        return b'\n';
    }
    let digit = (row / 10u64.pow((ROW_BYTES - 2 - column) as u32)) % 10;
    b'0' + digit as u8
}

/// What a test reads off the share after the fact.
#[derive(Default)]
struct ShareState {
    size: AtomicU64,
    opens: AtomicUsize,
    reads: AtomicUsize,
    bytes_read: AtomicU64,
    fail_next_read: AtomicBool,
    latency_ms: AtomicU64,
}

/// A share holding one synthetic file, generated on read, so a 5 GiB file costs nothing.
struct SyntheticShare {
    root: PathBuf,
    file: PathBuf,
    os_visible: bool,
    state: Arc<ShareState>,
}

struct SyntheticReader {
    size: u64,
    state: Arc<ShareState>,
}

impl VolumeRandomReader for SyntheticReader {
    fn size(&self) -> u64 {
        self.size
    }

    fn read_at(
        &self,
        offset: u64,
        len: usize,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<u8>, VolumeError>> + Send + '_>> {
        Box::pin(async move {
            let latency = self.state.latency_ms.load(Ordering::SeqCst);
            if latency > 0 {
                // allowed-test-sleep: the share's round-trip latency is the subject.
                tokio::time::sleep(Duration::from_millis(latency)).await;
            }
            self.state.reads.fetch_add(1, Ordering::SeqCst);
            if self.state.fail_next_read.swap(false, Ordering::SeqCst) {
                return Err(VolumeError::ConnectionTimeout("the share dropped".into()));
            }
            let end = (offset + len as u64).min(self.size);
            let bytes: Vec<u8> = (offset.min(end)..end).map(synthetic_byte).collect();
            self.state.bytes_read.fetch_add(bytes.len() as u64, Ordering::SeqCst);
            Ok(bytes)
        })
    }

    fn close(self: Box<Self>) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send>> {
        Box::pin(async { Ok(()) })
    }
}

impl Volume for SyntheticShare {
    fn name(&self) -> &str {
        "Share"
    }

    fn root(&self) -> &Path {
        &self.root
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
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
            let mut entry = FileEntry::new("big.log".to_string(), path.display().to_string(), false, false);
            entry.size = Some(self.state.size.load(Ordering::SeqCst));
            Ok(entry)
        })
    }

    fn paths_are_os_visible(&self) -> bool {
        self.os_visible
    }

    fn open_random_reader<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeRandomReader>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if path != self.file {
                return Err(VolumeError::NotFound(path.display().to_string()));
            }
            self.state.opens.fetch_add(1, Ordering::SeqCst);
            Ok(Box::new(SyntheticReader {
                size: self.state.size.load(Ordering::SeqCst),
                state: Arc::clone(&self.state),
            }) as Box<dyn VolumeRandomReader>)
        })
    }
}

/// Registers a share under `id` holding one `size`-byte synthetic `big.log`, and returns
/// the file's path, the share itself, and its counters.
fn a_share(id: &str, size: u64, os_visible: bool) -> (String, Arc<SyntheticShare>, Arc<ShareState>) {
    // Under `/Volumes/` like a real share, and nowhere on disk: only the volume can
    // serve it, whether or not it claims an OS mount.
    let root = PathBuf::from(format!("/Volumes/cmdr-test-share-{id}"));
    let file = root.join("big.log");
    let state = Arc::new(ShareState::default());
    state.size.store(size, Ordering::SeqCst);
    let share = Arc::new(SyntheticShare {
        root,
        file: file.clone(),
        os_visible,
        state: Arc::clone(&state),
    });
    get_volume_manager().register(id, Arc::clone(&share) as Arc<dyn Volume>);
    (file.display().to_string(), share, state)
}

/// The row number a synthetic row's text spells.
fn row_number(text: &str) -> u64 {
    text.parse().expect("a synthetic row is 15 digits")
}

/// A 5 GiB file on a share whose mount is gone opens straight away and serves rows at
/// the start, middle, and end, each jump reading a bounded few blocks over ONE open.
/// Before #303 the open pulled the file into a temp first and refused anything over the
/// 256 MiB preview cap.
#[test]
fn a_multi_gigabyte_file_on_a_share_opens_and_jumps_through_one_held_reader() {
    let (path, _share, state) = a_share("viewer-vsrc-5gib", FIVE_GIB, false);

    let opened = session::open_session(&path, "viewer-vsrc-5gib").expect("a 5 GiB file on a share opens");
    assert_eq!(opened.total_bytes, FIVE_GIB);
    assert_eq!(row_number(&opened.initial_lines.rows[0].text), 0);
    let after_open = state.bytes_read.load(Ordering::SeqCst);

    let last_row = FIVE_GIB / ROW_BYTES - 1;
    for (fraction, near_row) in [(0.5, last_row / 2), (0.999, last_row * 999 / 1000), (0.0, 0)] {
        let before = state.bytes_read.load(Ordering::SeqCst);
        let chunk = session::get_lines(
            &opened.session_id,
            SeekTarget::Fraction(fraction),
            500,
            &AtomicBool::new(false),
        )
        .expect("a jump reads");
        let first = row_number(&chunk.rows[0].text);
        assert!(
            first.abs_diff(near_row) <= 1,
            "a jump to {fraction} landed on row {first}"
        );
        let read = state.bytes_read.load(Ordering::SeqCst) - before;
        assert!(
            read <= 2 * BLOCK_BYTES,
            "a jump to {fraction} read {read} B, over the two-block budget"
        );
    }

    assert_eq!(
        state.opens.load(Ordering::SeqCst),
        1,
        "one open serves the whole session"
    );
    assert!(
        after_open <= 2 * BLOCK_BYTES,
        "the open read {after_open} B, over the two-block budget"
    );
    session::close_session(&opened.session_id).unwrap();
}

/// A scrollbar drag on a share with a 200 ms round trip: fetches fire every 100 ms,
/// overlapping, and every one answers inside the 2 s fetch deadline. Over a live mount
/// this used to be an OS open plus 64 KiB reads per fetch, which ran out of time.
#[test]
fn a_scrollbar_drag_on_a_slow_share_answers_every_fetch_inside_the_deadline() {
    let (path, _share, state) = a_share("viewer-vsrc-drag", FIVE_GIB, true);
    let opened = session::open_session(&path, "viewer-vsrc-drag").expect("opens");
    state.latency_ms.store(200, Ordering::SeqCst);

    let fetches: Vec<_> = (0..10)
        .map(|i| {
            let session_id = opened.session_id.clone();
            let handle = std::thread::spawn(move || {
                let started = Instant::now();
                let result = session::get_lines(
                    &session_id,
                    SeekTarget::Fraction(f64::from(i) / 10.0 + 0.05),
                    500,
                    &AtomicBool::new(false),
                );
                (result.map(|_| ()), started.elapsed())
            });
            // allowed-test-sleep: a drag fires a fetch every 100 ms; the spacing is the subject.
            std::thread::sleep(Duration::from_millis(100));
            handle
        })
        .collect();
    for (i, fetch) in fetches.into_iter().enumerate() {
        let (result, took) = fetch.join().expect("the fetch thread finishes");
        result.unwrap_or_else(|e| panic!("fetch {i} failed: {e}"));
        assert!(took < FETCH_DEADLINE, "fetch {i} took {took:?}");
    }
    assert_eq!(state.opens.load(Ordering::SeqCst), 1);
    session::close_session(&opened.session_id).unwrap();
}

/// A read that fails (the share reconnected under the held handle) reopens once and
/// answers from the fresh handle instead of surfacing an error.
#[test]
fn a_failed_read_reopens_the_handle_once() {
    let (_path, share, state) = a_share("viewer-vsrc-reopen", 4 * BLOCK_BYTES, false);
    let (source, _) = VolumeSource::open(share.clone(), &share.file).unwrap();
    state.fail_next_read.store(true, Ordering::SeqCst);

    let mut buf = vec![0u8; 32];
    assert_eq!(source.read_at(BLOCK_BYTES, &mut buf).unwrap(), 32);
    assert_eq!(buf[15], b'\n');
    assert_eq!(state.opens.load(Ordering::SeqCst), 2);
}

/// Bytes appended after open are read through a fresh handle: a held reader answers
/// only up to the size it opened at, and the backends ask past it only once tail mode
/// told them the file grew.
#[test]
fn an_append_is_read_through_a_fresh_handle() {
    let (_path, share, state) = a_share("viewer-vsrc-grow", 100 * ROW_BYTES, false);
    let (source, size) = VolumeSource::open(share.clone(), &share.file).unwrap();
    assert_eq!(size, 100 * ROW_BYTES);
    state.size.store(200 * ROW_BYTES, Ordering::SeqCst);

    let mut buf = vec![0u8; ROW_BYTES as usize];
    assert_eq!(source.read_at(150 * ROW_BYTES, &mut buf).unwrap(), buf.len());
    assert_eq!(&buf[..15], b"000000000000150");
    assert_eq!(state.opens.load(Ordering::SeqCst), 2);
}

/// Reading the same rows again is served from the block cache, so scrolling back and
/// forth within a few blocks costs no round trips.
#[test]
fn rereading_nearby_bytes_costs_no_round_trip() {
    let (_path, share, state) = a_share("viewer-vsrc-cache", 64 * BLOCK_BYTES, false);
    let (source, _) = VolumeSource::open(share.clone(), &share.file).unwrap();

    let mut buf = vec![0u8; 64 * 1024];
    source.read_at(10 * BLOCK_BYTES + 100, &mut buf).unwrap();
    let reads = state.reads.load(Ordering::SeqCst);
    source.read_at(10 * BLOCK_BYTES, &mut buf).unwrap();
    source.read_at(10 * BLOCK_BYTES + 5000, &mut buf).unwrap();
    assert_eq!(state.reads.load(Ordering::SeqCst), reads);
}

/// The errors a test double hands back still map to the viewer's typed errors.
#[test]
fn a_missing_file_on_the_share_is_not_found() {
    let (_path, share, _state) = a_share("viewer-vsrc-missing", ROW_BYTES, false);
    let missing = share.root.join("nope.log");
    let result = session::open_session(&missing.display().to_string(), "viewer-vsrc-missing");
    assert!(
        matches!(result, Err(ViewerError::NotFound { .. })),
        "got {:?}",
        result.err()
    );
}

/// End to end against a real Samba share (requires Docker SMB containers): a file past
/// the background-index limit opens through smb2 and serves rows at the start, middle,
/// and end, with the real `SmbVolume`, the real `block_on` bridge, and the reader closed
/// on the runtime when the session goes.
#[test]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
fn smb_integration_viewer_jumps_in_a_big_file_on_a_share() {
    use cmdr_smb::volume::testing::{ensure_clean, make_docker_volume, share_path, test_dir_name};

    let volume = tauri::async_runtime::block_on(make_docker_volume());
    let dir = test_dir_name();
    tauri::async_runtime::block_on(async {
        ensure_clean(&volume, &dir).await;
        volume.create_directory(Path::new(&dir)).await.unwrap();
    });
    let size = 40 * 1024 * 1024;
    let bytes: Vec<u8> = (0..size).map(synthetic_byte).collect();
    let relative = format!("{dir}/big.log");
    tauri::async_runtime::block_on(volume.create_file(Path::new(&relative), &bytes)).unwrap();
    let volume = Arc::new(volume);
    let id = format!("viewer-vsrc-smb-{dir}");
    get_volume_manager().register(&id, Arc::clone(&volume) as Arc<dyn Volume>);

    let opened = session::open_session(&share_path(&relative), &id).expect("opens through smb2");
    assert_eq!(opened.total_bytes, size);
    let last_row = size / ROW_BYTES - 1;
    for (fraction, near_row) in [(0.5, last_row / 2), (0.999, last_row * 999 / 1000), (0.0, 0)] {
        let chunk = session::get_lines(
            &opened.session_id,
            SeekTarget::Fraction(fraction),
            500,
            &AtomicBool::new(false),
        )
        .expect("a jump reads");
        let first = row_number(&chunk.rows[0].text);
        assert!(
            first.abs_diff(near_row) <= 1,
            "a jump to {fraction} landed on row {first}"
        );
    }
    session::close_session(&opened.session_id).unwrap();

    get_volume_manager().unregister(&id);
    tauri::async_runtime::block_on(ensure_clean(&volume, &dir));
}
