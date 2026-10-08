//! Fresh compression coordinator shutdown and stage-validation tests.

use super::*;
use crate::file_system::volume::{InMemoryVolume, ListingProgress, SpaceInfo};
use crate::file_system::write_operations::archive_edit::fresh_plan::FreshPlan;
use crate::file_system::write_operations::archive_edit::fresh_zip::{FreshZipEntry, FreshZipSource};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::types::{ArchiveNameRefusal, WriteOperationPhase};
use crate::ignore_poison::IgnorePoison;
use crate::test_support::wait_until_async;

struct RefuseAfterProducerProgress {
    inner: InMemoryVolume,
    producer_progressed: Arc<tokio::sync::Notify>,
}

impl Volume for RefuseAfterProducerProgress {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
    {
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }

    fn supports_unknown_length_writes(&self) -> bool {
        true
    }

    fn write_from_stream<'a>(
        &'a self,
        _dest: &'a Path,
        _mode: WriteMode,
        _length: StreamLength,
        stream: Box<dyn VolumeReadStream>,
        _on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            self.producer_progressed.notified().await;
            drop(stream);
            Err(VolumeError::IoError {
                message: "injected destination refusal".to_string(),
                raw_os_error: None,
            })
        })
    }
}

#[tokio::test]
async fn backend_refusal_unblocks_a_producer_parked_by_pause() {
    let state = Arc::new(WriteOperationState::new(Duration::ZERO));
    state.pause_gate.pause();
    let producer_progressed = Arc::new(tokio::sync::Notify::new());
    let progress_signal = Arc::clone(&producer_progressed);
    let state_for_progress = Arc::clone(&state);
    let state_for_wake = Arc::clone(&state);
    let cancellation = FreshZipCancellation::new(Some(Arc::new(move || state_for_wake.pause_gate.wake())));
    let cancellation_for_progress = cancellation.clone();
    let progress: FreshZipProgressObserver = Arc::new(move |_| {
        progress_signal.notify_one();
        state_for_progress
            .pause_gate
            .wait_while_paused_sync_until(&|| cancellation_for_progress.is_requested());
        cancellation_for_progress.is_requested()
    });
    let plan = FreshPlan {
        source_volume: Arc::new(InMemoryVolume::new("source")),
        entries: vec![FreshZipEntry {
            name: "one.txt".to_string(),
            source: FreshZipSource::Bytes(b"one".to_vec()),
            size: 3,
            is_directory: false,
            modified: None,
            unix_mode: None,
        }],
        remote_feeds: Vec::new(),
        source_bytes: 3,
        skipped: 0,
    };
    let destination: Arc<dyn Volume> = Arc::new(RefuseAfterProducerProgress {
        inner: InMemoryVolume::new("destination"),
        producer_progressed,
    });

    let outcome = tokio::time::timeout(
        Duration::from_millis(250),
        produce_into(
            plan,
            destination,
            PathBuf::from("archive.zip"),
            None,
            progress,
            cancellation,
        ),
    )
    .await;
    state.pause_gate.resume();

    assert!(
        outcome.is_ok(),
        "destination refusal must wake and join the paused producer"
    );
}

/// What a [`GatedDestination`] does once its first chunk has landed.
#[derive(Clone, Copy)]
enum AfterFirstChunk {
    /// Hold the acknowledgement until the gate opens, then report progress.
    HoldThenReport,
    /// Stop reading and claim success: a writer bug.
    ClaimSuccess,
    /// Hold until the gate opens, then fail like a dropped connection.
    HoldThenFail,
}

/// A destination that holds its first acknowledged write in flight until the
/// test opens `gate`, the shape of a server that is slow to answer one request.
/// It leaves its partial behind on every exit, so only the coordinator's own
/// cleanup can remove it. `direct` decides whether it accepts unknown-length
/// writes (the direct route) or only known-length ones (the spool route).
struct GatedDestination {
    inner: InMemoryVolume,
    gate: tokio::sync::watch::Receiver<bool>,
    direct: bool,
    script: AfterFirstChunk,
    at_gate: std::sync::atomic::AtomicBool,
    stopped_by_break: std::sync::atomic::AtomicBool,
    closed_on_eof: std::sync::atomic::AtomicBool,
}

impl GatedDestination {
    fn new(
        inner: InMemoryVolume,
        gate: tokio::sync::watch::Receiver<bool>,
        direct: bool,
        script: AfterFirstChunk,
    ) -> Self {
        Self {
            inner,
            gate,
            direct,
            script,
            at_gate: Default::default(),
            stopped_by_break: Default::default(),
            closed_on_eof: Default::default(),
        }
    }
}

impl Volume for GatedDestination {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn lane_key(&self) -> LaneKey {
        self.inner.lane_key()
    }

    fn backend_kind(&self) -> BackendKind {
        self.inner.backend_kind()
    }

    fn is_writable(&self) -> bool {
        self.inner.is_writable()
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
    {
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }

    fn delete<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        self.inner.delete(path)
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        self.inner.open_read_stream(path)
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }

    fn supports_unknown_length_writes(&self) -> bool {
        self.direct
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        _mode: WriteMode,
        length: StreamLength,
        mut stream: Box<dyn VolumeReadStream>,
        on_progress: &'a (dyn Fn(StreamWriteProgress) -> ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        use std::sync::atomic::Ordering;
        Box::pin(async move {
            let mut written = 0u64;
            let mut gate = self.gate.clone();
            loop {
                let Some(chunk) = stream.next_chunk().await else {
                    self.closed_on_eof.store(true, Ordering::SeqCst);
                    return Ok(written);
                };
                let chunk = chunk?;
                if written == 0 {
                    self.inner.create_file(dest, &chunk).await?;
                }
                written += chunk.len() as u64;
                if matches!(self.script, AfterFirstChunk::ClaimSuccess) {
                    return Ok(written);
                }
                self.at_gate.store(true, Ordering::SeqCst);
                let _ = gate.wait_for(|open| *open).await;
                if matches!(self.script, AfterFirstChunk::HoldThenFail) {
                    return Err(VolumeError::IoError {
                        message: "injected connection loss".to_string(),
                        raw_os_error: None,
                    });
                }
                if on_progress(StreamWriteProgress {
                    bytes_written: written,
                    expected_length: length,
                })
                .is_break()
                {
                    self.stopped_by_break.store(true, Ordering::SeqCst);
                    return Err(VolumeError::Cancelled(dest.display().to_string()));
                }
            }
        })
    }
}

/// A remote source whose file opens fine and then never answers a read: the
/// shape of a share that went quiet mid-transfer.
struct HungSource {
    inner: InMemoryVolume,
    reading: Arc<std::sync::atomic::AtomicBool>,
}

struct NeverAnsweringStream {
    reading: Arc<std::sync::atomic::AtomicBool>,
    size: u64,
}

impl VolumeReadStream for NeverAnsweringStream {
    fn next_chunk(&mut self) -> Pin<Box<dyn Future<Output = Option<Result<Vec<u8>, VolumeError>>> + Send + '_>> {
        self.reading.store(true, std::sync::atomic::Ordering::SeqCst);
        Box::pin(std::future::pending())
    }

    fn total_size(&self) -> StreamLength {
        StreamLength::Known(self.size)
    }

    fn bytes_read(&self) -> u64 {
        0
    }

    fn modified_at(&self) -> Option<std::time::SystemTime> {
        None
    }
}

impl Volume for HungSource {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn root(&self) -> &Path {
        self.inner.root()
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn lane_key(&self) -> LaneKey {
        self.inner.lane_key()
    }

    fn list_directory<'a>(
        &'a self,
        path: &'a Path,
        on_progress: Option<&'a (dyn Fn(ListingProgress) + Sync)>,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<crate::file_system::listing::FileEntry>, VolumeError>> + Send + 'a>>
    {
        self.inner.list_directory(path, on_progress)
    }

    fn get_metadata<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<crate::file_system::listing::FileEntry, VolumeError>> + Send + 'a>> {
        self.inner.get_metadata(path)
    }

    fn exists<'a>(&'a self, path: &'a Path) -> Pin<Box<dyn Future<Output = bool> + Send + 'a>> {
        self.inner.exists(path)
    }

    fn is_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<bool, VolumeError>> + Send + 'a>> {
        self.inner.is_directory(path)
    }

    fn get_space_info<'a>(&'a self) -> Pin<Box<dyn Future<Output = Result<SpaceInfo, VolumeError>> + Send + 'a>> {
        self.inner.get_space_info()
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let size = self.inner.get_metadata(path).await?.size.unwrap_or(0);
            Ok(Box::new(NeverAnsweringStream {
                reading: Arc::clone(&self.reading),
                size,
            }) as Box<dyn VolumeReadStream>)
        })
    }
}

/// Bytes deflate can't shrink, so the ZIP outgrows every bounded queue.
fn incompressible(len: usize) -> Vec<u8> {
    let mut seed = 0x2545_f491_4f6c_dd1d_u64;
    (0..len)
        .map(|_| {
            seed ^= seed << 13;
            seed ^= seed >> 7;
            seed ^= seed << 17;
            seed as u8
        })
        .collect()
}

fn sibling_temps(dir: &[crate::file_system::listing::FileEntry]) -> Vec<String> {
    dir.iter()
        .map(|entry| entry.name.clone())
        .filter(|name| name.contains(".cmdr-tmp-"))
        .collect()
}

async fn wait_for_terminal(events: &CollectorEventSink) {
    wait_until_async(Duration::from_secs(5), "a terminal event", || {
        !events.complete.lock_ignore_poison().is_empty()
            || !events.errors.lock_ignore_poison().is_empty()
            || !events.cancelled.lock_ignore_poison().is_empty()
    })
    .await;
}

const ORIGINAL_ARCHIVE: &[u8] = b"the original archive bytes";

/// A registered SMB-shaped share holding `/share/archive.zip` (placeholder
/// bytes the compress must never touch unless it publishes) behind a
/// [`GatedDestination`], plus a started compress of one 3 MiB local file onto it.
struct GatedRun {
    id: String,
    destination: Arc<GatedDestination>,
    open_gate: tokio::sync::watch::Sender<bool>,
    events: Arc<CollectorEventSink>,
    operation_id: String,
    _source_dir: tempfile::TempDir,
}

impl GatedRun {
    async fn start(direct: bool, script: AfterFirstChunk) -> Self {
        let source_dir = tempfile::tempdir().expect("tempdir");
        std::fs::write(source_dir.path().join("big.bin"), incompressible(3 * 1024 * 1024)).expect("write source");
        let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", source_dir.path().to_path_buf()));
        let id = format!("gated-share-{}", uuid::Uuid::new_v4());
        let inner = InMemoryVolume::new("Share")
            .with_lane_key(id.clone())
            .with_backend_kind(BackendKind::Smb);
        inner.create_directory(Path::new("/share")).await.expect("seed dir");
        inner
            .create_file(Path::new("/share/archive.zip"), ORIGINAL_ARCHIVE)
            .await
            .expect("seed original");
        let (open_gate, gate) = tokio::sync::watch::channel(false);
        let destination = Arc::new(GatedDestination::new(inner, gate, direct, script));
        get_volume_manager().register(&id, Arc::clone(&destination) as Arc<dyn Volume>);
        let events = Arc::new(CollectorEventSink::new());
        let start = super::super::compress::compress_start(
            Arc::clone(&events) as Arc<dyn OperationEventSink>,
            source,
            vec![PathBuf::from("big.bin")],
            PathBuf::from("/share/archive.zip"),
            id.clone(),
            ConflictResolution::Overwrite,
            0,
            None,
            None,
            Initiator::User,
        )
        .await
        .expect("start gated compress");
        Self {
            id,
            destination,
            open_gate,
            events,
            operation_id: start.operation_id,
            _source_dir: source_dir,
        }
    }

    async fn wait_at_gate(&self) {
        wait_until_async(Duration::from_secs(5), "the destination holding a write", || {
            self.destination.at_gate.load(std::sync::atomic::Ordering::SeqCst)
        })
        .await;
    }

    fn open_gate(&self) {
        let _ = self.open_gate.send(true);
    }

    /// The original stays byte-exact and no stage survives, then unregisters.
    async fn assert_destination_untouched_and_finish(self) {
        assert_eq!(
            read_remote_file(&self.destination.inner, Path::new("/share/archive.zip")).await,
            ORIGINAL_ARCHIVE,
            "the original archive stays byte-exact"
        );
        let listing = self
            .destination
            .inner
            .list_directory(Path::new("/share"), None)
            .await
            .expect("list share");
        assert!(sibling_temps(&listing).is_empty(), "the stage is removed: {listing:?}");
        get_volume_manager().unregister(&self.id);
    }
}

#[tokio::test]
async fn cancel_reaches_a_direct_write_held_by_a_gated_destination() {
    use std::sync::atomic::Ordering;
    let run = GatedRun::start(true, AfterFirstChunk::HoldThenReport).await;
    run.wait_at_gate().await;
    manager::cancel_operation(&run.operation_id);
    run.open_gate();
    wait_for_terminal(&run.events).await;

    assert_eq!(
        run.events.cancelled.lock_ignore_poison().len(),
        1,
        "the op ends cancelled"
    );
    assert!(run.events.complete.lock_ignore_poison().is_empty());
    assert!(
        run.destination.stopped_by_break.load(Ordering::SeqCst),
        "the backend must hear the cancel at its next acknowledgement"
    );
    assert!(
        !run.destination.closed_on_eof.load(Ordering::SeqCst),
        "a cancelled ZIP must never reach the backend as a complete stream"
    );
    run.assert_destination_untouched_and_finish().await;
}

#[tokio::test]
async fn a_destination_that_stops_reading_early_is_a_write_error_not_a_cancel() {
    let run = GatedRun::start(true, AfterFirstChunk::ClaimSuccess).await;
    wait_for_terminal(&run.events).await;

    assert!(
        run.events.cancelled.lock_ignore_poison().is_empty(),
        "nobody cancelled: a writer that quits early must not read as a user cancel"
    );
    assert_eq!(run.events.errors.lock_ignore_poison().len(), 1);
    assert!(run.events.complete.lock_ignore_poison().is_empty());
    run.assert_destination_untouched_and_finish().await;
}

#[tokio::test]
async fn an_upload_failure_while_paused_reports_without_waiting_for_resume() {
    let run = GatedRun::start(false, AfterFirstChunk::HoldThenFail).await;
    run.wait_at_gate().await;
    let _ = manager::pause_operation(&run.operation_id);
    run.open_gate();
    wait_for_terminal(&run.events).await;
    let _ = manager::resume_operation(&run.operation_id);

    assert_eq!(
        run.events.errors.lock_ignore_poison().len(),
        1,
        "the upload's own failure is the answer, paused or not"
    );
    assert!(run.events.cancelled.lock_ignore_poison().is_empty());
    run.assert_destination_untouched_and_finish().await;
}

#[tokio::test]
async fn cancel_reaches_a_producer_waiting_on_a_hung_remote_source() {
    use std::sync::atomic::Ordering;
    let lane = format!("hung-source-{}", uuid::Uuid::new_v4());
    let inner = InMemoryVolume::new("HungSource").with_lane_key(lane);
    inner
        .create_file(Path::new("/stuck.bin"), &[b's'; 4096])
        .await
        .expect("seed source");
    let reading = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let source: Arc<dyn Volume> = Arc::new(HungSource {
        inner,
        reading: Arc::clone(&reading),
    });
    let dest_dir = tempfile::tempdir().expect("tempdir");
    let archive = dest_dir.path().join("archive.zip");
    let original = b"the original archive bytes".to_vec();
    std::fs::write(&archive, &original).expect("seed original");
    let events = Arc::new(CollectorEventSink::new());

    let start = super::super::compress::compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source,
        vec![PathBuf::from("/stuck.bin")],
        archive.clone(),
        super::super::test_support::unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        Initiator::User,
    )
    .await
    .expect("start hung-source compress");
    wait_until_async(Duration::from_secs(5), "the source read in flight", || {
        reading.load(Ordering::SeqCst)
    })
    .await;
    manager::cancel_operation(&start.operation_id);
    wait_for_terminal(&events).await;

    assert_eq!(events.cancelled.lock_ignore_poison().len(), 1, "the op ends cancelled");
    assert!(events.complete.lock_ignore_poison().is_empty());
    assert_eq!(std::fs::read(&archive).expect("read original"), original);
    let leftovers = std::fs::read_dir(dest_dir.path())
        .expect("list dest")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name != "archive.zip")
        .collect::<Vec<_>>();
    assert!(leftovers.is_empty(), "the stage is removed: {leftovers:?}");
}

async fn read_remote_file(volume: &dyn Volume, path: &Path) -> Vec<u8> {
    let mut stream = volume.open_read_stream(path).await.expect("open");
    let mut bytes = Vec::new();
    while let Some(chunk) = stream.next_chunk().await {
        bytes.extend(chunk.expect("chunk"));
    }
    bytes
}

#[tokio::test]
async fn a_file_whose_name_holds_a_backslash_compresses_and_validates() {
    // `\` is an ordinary filename byte on macOS, but the archive reader treats it
    // as a separator, so `a\b.txt` reads back as `a/` + `b.txt`.
    let source_dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(source_dir.path().join("a\\b.txt"), b"backslash").expect("write source");
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", source_dir.path().to_path_buf()));
    let dest_dir = tempfile::tempdir().expect("tempdir");
    let archive = dest_dir.path().join("odd.zip");
    let events = Arc::new(CollectorEventSink::new());

    super::super::compress::compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source,
        vec![PathBuf::from("a\\b.txt")],
        archive.clone(),
        super::super::test_support::unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        Initiator::User,
    )
    .await
    .expect("start compress");
    wait_for_terminal(&events).await;

    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "the archive holds exactly the planned file: {:?}",
        events.errors.lock_ignore_poison()
    );
    assert_eq!(
        super::super::test_support::read_entry(&archive, "a\\b.txt").as_deref(),
        Some(b"backslash".as_slice())
    );
}

/// Compresses `selected` (relative to `source_dir`) into a new local archive and
/// waits for the terminal event.
async fn compress_local_selection(
    source_dir: &Path,
    selected: &[&str],
) -> (Arc<CollectorEventSink>, tempfile::TempDir, PathBuf) {
    let source: Arc<dyn Volume> = Arc::new(LocalPosixVolume::new("src", source_dir.to_path_buf()));
    let dest_dir = tempfile::tempdir().expect("tempdir");
    let archive = dest_dir.path().join("out.zip");
    let events = Arc::new(CollectorEventSink::new());
    super::super::compress::compress_start(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        source,
        selected.iter().map(PathBuf::from).collect(),
        archive.clone(),
        super::super::test_support::unique_lane_id(),
        ConflictResolution::Overwrite,
        0,
        None,
        None,
        Initiator::User,
    )
    .await
    .expect("start compress");
    wait_for_terminal(&events).await;
    (events, dest_dir, archive)
}

/// The op failed from its plan: no byte was compressed and no stage exists.
fn assert_refused_before_producing(events: &CollectorEventSink, dest_dir: &Path) {
    assert_eq!(events.errors.lock_ignore_poison().len(), 1, "the op fails");
    assert!(
        !events
            .progress
            .lock_ignore_poison()
            .iter()
            .any(|event| event.phase == WriteOperationPhase::Compressing),
        "it fails from the plan, before compressing a byte"
    );
    let leftovers = std::fs::read_dir(dest_dir).expect("list dest").count();
    assert_eq!(leftovers, 0, "nothing lands at the destination, not even a stage");
}

#[tokio::test]
async fn a_name_the_archive_reader_would_hide_fails_before_compressing() {
    // `..\evil.txt` is an ordinary macOS file name; inside a zip it reads as
    // `../evil.txt`, which the reader quarantines (and other tools may extract
    // outside the target folder).
    let source_dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(source_dir.path().join("..\\evil.txt"), b"x").expect("write source");

    let (events, dest_dir, _archive) = compress_local_selection(source_dir.path(), &["..\\evil.txt"]).await;

    assert_refused_before_producing(&events, dest_dir.path());
    assert!(matches!(
        &events.errors.lock_ignore_poison()[0].error,
        WriteOperationError::ArchiveEntryNameRefused {
            entry,
            reason: ArchiveNameRefusal::ParentTraversal,
        } if entry == "..\\evil.txt"
    ));
}

#[tokio::test]
async fn two_entries_that_read_back_as_one_path_fail_before_compressing() {
    // `a\b.txt` and `a/b.txt` are different files on disk and the same entry
    // to the archive reader: one would silently shadow the other.
    let source_dir = tempfile::tempdir().expect("tempdir");
    std::fs::write(source_dir.path().join("a\\b.txt"), b"backslash").expect("write the look-alike");
    std::fs::create_dir(source_dir.path().join("a")).expect("mkdir");
    std::fs::write(source_dir.path().join("a/b.txt"), b"real").expect("write the real one");

    let (events, dest_dir, _archive) = compress_local_selection(source_dir.path(), &["a\\b.txt", "a"]).await;

    assert_refused_before_producing(&events, dest_dir.path());
    assert!(matches!(
        &events.errors.lock_ignore_poison()[0].error,
        WriteOperationError::ArchiveEntryNamesCollide { archive_path, .. } if archive_path == "a/b.txt"
    ));
}

fn plan_of(names: &[&str]) -> FreshPlan {
    FreshPlan {
        source_volume: Arc::new(InMemoryVolume::new("source")),
        entries: names
            .iter()
            .map(|name| FreshZipEntry {
                name: (*name).to_string(),
                source: FreshZipSource::Bytes(Vec::new()),
                size: 0,
                is_directory: name.ends_with('/'),
                modified: None,
                unix_mode: None,
            })
            .collect(),
        remote_feeds: Vec::new(),
        source_bytes: 0,
        skipped: 0,
    }
}

#[test]
fn every_reader_quarantine_reason_is_refused_from_the_plan() {
    // Past the reader's 256-component limit (`MAX_COMPONENT_DEPTH`).
    let deep = vec!["d"; 300].join("/");
    for (name, reason) in [
        ("x/..\\y.txt", ArchiveNameRefusal::ParentTraversal),
        (deep.as_str(), ArchiveNameRefusal::TooDeep),
        ("\\.\\", ArchiveNameRefusal::Empty),
    ] {
        assert!(
            matches!(
                check_entry_names(&plan_of(&["ok.txt", name])),
                Err(WriteOperationError::ArchiveEntryNameRefused { reason: got, .. }) if got == reason
            ),
            "{reason:?}"
        );
    }
    assert!(check_entry_names(&plan_of(&["ok.txt", "a\\b.txt"])).is_ok());
}

#[test]
fn a_file_where_another_entry_needs_a_folder_collides_but_folders_merge() {
    assert!(matches!(
        check_entry_names(&plan_of(&["a", "a\\b.txt"])),
        Err(WriteOperationError::ArchiveEntryNamesCollide { entry, other, archive_path })
            if entry == "a\\b.txt" && other == "a" && archive_path == "a"
    ));
    assert!(check_entry_names(&plan_of(&["a\\b/", "a/", "a/b/", "a/b/c.txt"])).is_ok());
}

#[test]
fn the_expected_index_names_what_the_reader_synthesizes() {
    let expected = ExpectedIndex::of_names([("folder/", true), ("folder/a\\b.txt", false)]);
    assert_eq!(
        expected.nodes.into_iter().collect::<Vec<_>>(),
        vec![
            ("folder".to_string(), true),
            ("folder/a".to_string(), true),
            ("folder/a/b.txt".to_string(), false),
        ]
    );
    assert_eq!(ExpectedIndex::of_names([("../escape.txt", false)]).quarantined, 1);
}

#[tokio::test]
async fn validation_rejects_byte_count_drift_before_parsing() {
    let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("stage"));
    volume
        .create_file(Path::new("archive.zip"), b"not a ZIP")
        .await
        .expect("create stage");

    assert!(
        validate_stage(&volume, Path::new("archive.zip"), 9, 8, &ExpectedIndex::of_names([]))
            .await
            .is_err()
    );
}

#[tokio::test]
async fn validation_rejects_corrupt_zip_bytes_even_when_counts_agree() {
    let volume: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("stage"));
    let corrupt = b"PK\x03\x04truncated";
    volume
        .create_file(Path::new("archive.zip"), corrupt)
        .await
        .expect("create stage");

    let size = corrupt.len() as u64;
    assert!(
        validate_stage(
            &volume,
            Path::new("archive.zip"),
            size,
            size,
            &ExpectedIndex::of_names([])
        )
        .await
        .is_err()
    );
}
