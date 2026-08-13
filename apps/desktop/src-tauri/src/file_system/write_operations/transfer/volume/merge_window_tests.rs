//! What the operation-wide file-copy window does to a DIRECTORY source: how
//! wide it opens, what bounds it, and what stays exactly as it was when the
//! subtree walk was strictly serial.
//!
//! The defect these pin: selecting one folder is ONE top-level source, so it
//! takes the serial driver, and before the window existed nothing inside it ever
//! overlapped — `network.smbConcurrency` (advertised 1-32) did nothing at all for
//! the commonest copy there is.
//!
//! ## How overlap is observed
//!
//! [`WindowWatchingDest`] counts how many `write_from_stream` calls are live at
//! once and remembers the peak. On its own that proves nothing: an in-memory
//! write finishes in microseconds, so two genuinely concurrent leaves can still
//! never be live at the same instant and the peak reads 1 whatever the code does.
//! So each write LINGERS until `rendezvous` others have joined it or a short
//! deadline passes. A working window reaches the rendezvous immediately; a serial
//! walk pays the deadline per file and reports a peak of 1. ❌ Don't drop the
//! linger to make these faster — it's what makes the number mean anything.
//!
//! Shared fixtures `make_state` / `make_volumes` live in `volume/copy_tests.rs`
//! (`super::tests`).

use super::super::super::conflict_responder_test_support::ConflictResponderSink;
use super::super::faulty_volume::forward_volume_methods;
use super::tests::{make_state, make_volumes};
use super::*;
use crate::file_system::volume::InMemoryVolume;
use crate::file_system::write_operations::state::cancel_write_operation;
use crate::file_system::write_operations::test_support::TestOperationGuard;
use crate::file_system::write_operations::types::{
    CollectorEventSink, ConflictResolution, ScanProgressEvent, WriteCancelledEvent, WriteCompleteEvent,
    WriteConflictEvent, WriteErrorEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};
use std::sync::atomic::AtomicU8;

/// How long a leaf lingers inside `write_from_stream` waiting for siblings to
/// join it. Long enough that a real window fills, short enough that a serial walk
/// (which never reaches the rendezvous) finishes the suite in a couple of seconds.
const LINGER: Duration = Duration::from_millis(250);

/// A destination that reports a chosen concurrency cap and records how many
/// writes it ever had in flight at once.
struct WindowWatchingDest {
    inner: Arc<InMemoryVolume>,
    cap: usize,
    live: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    /// How many concurrent writes a leaf waits for before it stops lingering.
    /// Set ABOVE the window when the test wants every leaf to linger for the
    /// whole deadline, which is how the peak gets its best chance to be seen.
    rendezvous: usize,
    /// Writes that were live while a `create_directory` ran. MTP's cap of 1 has
    /// to mean "one operation on the transport", not merely "one write".
    dir_creates_during_a_write: Arc<AtomicUsize>,
}

impl WindowWatchingDest {
    fn new(cap: usize, rendezvous: usize) -> Arc<Self> {
        Arc::new(Self {
            inner: Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000)),
            cap,
            live: Arc::new(AtomicUsize::new(0)),
            peak: Arc::new(AtomicUsize::new(0)),
            rendezvous,
            dir_creates_during_a_write: Arc::new(AtomicUsize::new(0)),
        })
    }

    fn peak(&self) -> usize {
        self.peak.load(Ordering::Relaxed)
    }

    fn still_live(&self) -> usize {
        self.live.load(Ordering::Relaxed)
    }
}

impl Volume for WindowWatchingDest {
    forward_volume_methods!(
        inner => name, root, lane_key, list_directory, get_metadata, exists, is_directory, create_file,
        create_directory_all, delete, rename, get_space_info, supports_streaming, supports_export,
        create_directory_errors_on_existing_dir, scan_for_copy, scan_for_copy_batch, open_read_stream,
        write_is_single_shot,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    /// A real transport limit, so `transfer_concurrency` reads it as the binding
    /// cap for the pair (the `InMemoryVolume` source declares itself local).
    fn operations_are_local(&self) -> bool {
        false
    }

    fn max_concurrent_ops(&self) -> usize {
        self.cap
    }

    fn create_directory<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<(), VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            if self.live.load(Ordering::Relaxed) > 0 {
                self.dir_creates_during_a_write.fetch_add(1, Ordering::Relaxed);
            }
            self.inner.create_directory(path).await
        })
    }

    fn write_from_stream<'a>(
        &'a self,
        dest: &'a Path,
        size: u64,
        stream: Box<dyn crate::file_system::volume::VolumeReadStream>,
        on_progress: &'a (dyn Fn(u64, u64) -> std::ops::ControlFlow<()> + Sync),
    ) -> Pin<Box<dyn Future<Output = Result<u64, VolumeError>> + Send + 'a>> {
        Box::pin(async move {
            let live = self.live.fetch_add(1, Ordering::Relaxed) + 1;
            self.peak.fetch_max(live, Ordering::Relaxed);
            // Linger so overlap is observable at all; see the module doc.
            let deadline = Instant::now() + LINGER;
            while self.live.load(Ordering::Relaxed) < self.rendezvous && Instant::now() < deadline {
                // allowed-test-sleep: the linger IS the subject — it's the fake write latency that makes overlap observable at all.
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
            let result = self.inner.write_from_stream(dest, size, stream, on_progress).await;
            self.live.fetch_sub(1, Ordering::Relaxed);
            result
        })
    }
}

/// A source that refuses to open ONE named file, so a single leaf deep inside a
/// subtree fails while its siblings succeed.
struct PoisonedLeafSource {
    inner: Arc<InMemoryVolume>,
    poisoned: String,
}

impl Volume for PoisonedLeafSource {
    forward_volume_methods!(
        inner => name, root, lane_key, list_directory, get_metadata, exists, is_directory, create_file,
        create_directory, create_directory_all, delete, rename, get_space_info, supports_streaming,
        supports_export, operations_are_local, max_concurrent_ops, create_directory_errors_on_existing_dir,
        scan_for_copy, scan_for_copy_batch, write_from_stream, write_is_single_shot,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<
        Box<
            dyn Future<Output = Result<Box<dyn crate::file_system::volume::VolumeReadStream>, VolumeError>> + Send + 'a,
        >,
    > {
        Box::pin(async move {
            if path.to_string_lossy() == self.poisoned {
                return Err(VolumeError::IoError {
                    message: "Injected read failure".into(),
                    raw_os_error: Some(5),
                });
            }
            self.inner.open_read_stream(path).await
        })
    }
}

/// Builds `/album` with `count` files on a fresh in-memory source.
async fn folder_of(count: usize) -> Arc<dyn Volume> {
    let (source, _) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    for index in 0..count {
        let name = format!("/album/f-{index:02}.bin");
        source
            .create_file(Path::new(&name), &vec![index as u8; 4_096])
            .await
            .unwrap();
    }
    source
}

fn overwrite_config() -> VolumeCopyConfig {
    VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..VolumeCopyConfig::default()
    }
}

// ============================================================================
// The defect
// ============================================================================

/// ONE folder is ONE top-level source, so this runs on the SERIAL driver — and
/// its subtree still has to fill the operation's window. Before the window
/// existed this peaked at 1 no matter what `network.smbConcurrency` said.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn one_folders_subtree_fills_the_operation_window() {
    let source = folder_of(12).await;
    let dest = WindowWatchingDest::new(4, 4);
    let dest_volume: Arc<dyn Volume> = Arc::clone(&dest) as Arc<dyn Volume>;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let result = copy_volumes_with_progress(
        events,
        "merge-window-one-folder",
        &state,
        source,
        &[PathBuf::from("/album")],
        Arc::clone(&dest_volume),
        Path::new("/out"),
        &overwrite_config(),
    )
    .await;
    assert!(result.is_ok(), "the copy must succeed, got {result:?}");

    assert_eq!(
        dest.peak(),
        4,
        "a 12-file folder copied through a window of 4 must have had 4 writes in flight at once, saw {}",
        dest.peak()
    );
    for index in 0..12 {
        let landed = format!("/out/album/f-{index:02}.bin");
        assert!(
            dest_volume.exists(Path::new(&landed)).await,
            "{landed} must be at the destination"
        );
    }
}

/// The N² trap. The top-level driver already fans out `W` ways over sources, so
/// a `W`-wide window PER DIRECTORY would put `W²` files on one connection — 100
/// at the shipped default. Four folder sources through a window of 4 must still
/// carry four writes, never sixteen.
#[tokio::test(flavor = "multi_thread", worker_threads = 8)]
async fn four_folder_sources_share_one_window_rather_than_taking_one_each() {
    let (source, _) = make_volumes();
    let mut sources = Vec::new();
    for folder in 0..4 {
        let dir = format!("/album-{folder}");
        source.create_directory(Path::new(&dir)).await.unwrap();
        for index in 0..6 {
            let name = format!("{dir}/f-{index:02}.bin");
            source
                .create_file(Path::new(&name), &vec![index as u8; 4_096])
                .await
                .unwrap();
        }
        sources.push(PathBuf::from(dir));
    }
    // A rendezvous well past the window, so every leaf lingers the full deadline
    // and the peak gets the best chance any test can give it to exceed 4.
    let dest = WindowWatchingDest::new(4, 32);
    let dest_volume: Arc<dyn Volume> = Arc::clone(&dest) as Arc<dyn Volume>;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let result = copy_volumes_with_progress(
        events,
        "merge-window-n-squared",
        &state,
        source,
        &sources,
        Arc::clone(&dest_volume),
        Path::new("/out"),
        &overwrite_config(),
    )
    .await;
    assert!(result.is_ok(), "the copy must succeed, got {result:?}");

    assert_eq!(
        dest.peak(),
        4,
        "four directory sources must SHARE the operation's window of 4, not open one each; peak was {}",
        dest.peak()
    );
    for folder in 0..4 {
        for index in 0..6 {
            let landed = format!("/out/album-{folder}/f-{index:02}.bin");
            assert!(
                dest_volume.exists(Path::new(&landed)).await,
                "{landed} must be at the destination"
            );
        }
    }
}

/// ❌ The one that must never regress. `MtpVolume::max_concurrent_ops()` is 1
/// because MTP is a single USB bulk transport, and a subtree walk must not
/// overlap ANY two operations on it — not two writes, and not a directory create
/// against a live write.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cap_of_one_keeps_a_subtree_strictly_one_operation_at_a_time() {
    let (source, _) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    source.create_directory(Path::new("/album/sub")).await.unwrap();
    for index in 0..3 {
        source
            .create_file(Path::new(&format!("/album/f-{index}.bin")), b"top")
            .await
            .unwrap();
        source
            .create_file(Path::new(&format!("/album/sub/g-{index}.bin")), b"deep")
            .await
            .unwrap();
    }
    let dest = WindowWatchingDest::new(1, 2);
    let dest_volume: Arc<dyn Volume> = Arc::clone(&dest) as Arc<dyn Volume>;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let result = copy_volumes_with_progress(
        events,
        "merge-window-mtp-shape",
        &state,
        source,
        &[PathBuf::from("/album")],
        Arc::clone(&dest_volume),
        Path::new("/out"),
        &overwrite_config(),
    )
    .await;
    assert!(result.is_ok(), "the copy must succeed, got {result:?}");

    assert_eq!(dest.peak(), 1, "a cap of 1 must never put two writes on the transport");
    assert_eq!(
        dest.dir_creates_during_a_write.load(Ordering::Relaxed),
        0,
        "a cap of 1 must not create a directory while a write is live either"
    );
}

// ============================================================================
// What must not change
// ============================================================================

/// Conflict resolution stays on the walker, in listing order, so the user answers
/// prompts in the order they'd expect however wide the byte copies run.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn deep_prompts_keep_listing_order_while_the_copies_overlap() {
    let source = folder_of(8).await;
    let dest = WindowWatchingDest::new(8, 8);
    let dest_volume: Arc<dyn Volume> = Arc::clone(&dest) as Arc<dyn Volume>;
    // Every source file clashes, so every one prompts.
    dest_volume.create_directory(Path::new("/out")).await.unwrap();
    dest_volume.create_directory(Path::new("/out/album")).await.unwrap();
    for index in 0..8 {
        dest_volume
            .create_file(Path::new(&format!("/out/album/f-{index:02}.bin")), b"old")
            .await
            .unwrap();
    }

    let guard = TestOperationGuard::register("merge-window-prompt-order");
    let state = Arc::clone(guard.state());
    let events = Arc::new(ConflictResponderSink::new(&state, ConflictResolution::Overwrite, false));
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Stop,
        ..VolumeCopyConfig::default()
    };
    let result = copy_volumes_with_progress(
        Arc::clone(&events) as Arc<dyn OperationEventSink>,
        guard.id(),
        &state,
        source,
        &[PathBuf::from("/album")],
        Arc::clone(&dest_volume),
        Path::new("/out"),
        &config,
    )
    .await;
    assert!(result.is_ok(), "the copy must succeed, got {result:?}");

    let prompted: Vec<String> = events
        .inner
        .conflicts
        .lock_ignore_poison()
        .iter()
        .map(|c| c.source_path.clone())
        .collect();
    let expected: Vec<String> = (0..8).map(|index| format!("/album/f-{index:02}.bin")).collect();
    assert_eq!(
        prompted, expected,
        "deep prompts must arrive in the walker's listing order, not in whatever order the writes finish"
    );
    assert!(
        dest.peak() > 1,
        "the copies must still have overlapped; peak was {}",
        dest.peak()
    );
}

/// A cancel mid-subtree stops promptly and leaves NOTHING writing behind it: the
/// walk drains every leaf it started before it returns, so the driver's cleanup
/// never races a live write.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_cancel_mid_subtree_leaves_no_leaf_still_writing() {
    let source = folder_of(40).await;
    let dest = WindowWatchingDest::new(8, 8);
    let dest_volume: Arc<dyn Volume> = Arc::clone(&dest) as Arc<dyn Volume>;

    let guard = TestOperationGuard::register("merge-window-cancel");
    let state = Arc::clone(guard.state());
    let events = Arc::new(CollectorEventSink::new());
    let operation_id = guard.id().to_owned();

    let copy = tokio::spawn({
        let state = Arc::clone(&state);
        let dest_volume = Arc::clone(&dest_volume);
        let operation_id = operation_id.clone();
        let events = Arc::clone(&events) as Arc<dyn OperationEventSink>;
        async move {
            copy_volumes_with_progress(
                events,
                &operation_id,
                &state,
                source,
                &[PathBuf::from("/album")],
                dest_volume,
                Path::new("/out"),
                &overwrite_config(),
            )
            .await
        }
    });

    // Let the window fill, then cancel through the real user-facing path.
    // allowed-test-sleep: the canceller's head start IS the subject — the cancel has to land while several leaves are mid-write.
    tokio::time::sleep(Duration::from_millis(60)).await;
    cancel_write_operation(&operation_id, false);

    let result = tokio::time::timeout(Duration::from_secs(20), copy)
        .await
        .expect("a cancelled subtree copy must not hang")
        .expect("the copy task must not panic");
    assert!(result.is_err(), "a cancelled copy reports a failure, got {result:?}");
    assert_eq!(
        dest.still_live(),
        0,
        "every leaf the walk started must have finished before the copy returned"
    );
}

/// Rollback of a wide-window subtree copy leaves the destination as it found it.
///
/// This is the ledger-completeness property, stated as an outcome: up to `W`
/// leaves at several depths were producing into ONE `CreatedPaths` at once, and
/// `prune_created_dir_if_empty` only removes a directory whose listing comes back
/// empty. So a file the ledger missed, or a `.cmdr-tmp-*` from a leaf still
/// running when the driver moved on to cleanup, both show up here as a directory
/// that refused to go.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_rolled_back_wide_window_copy_leaves_no_directory_it_created() {
    struct RollbackOnceCopying {
        inner: CollectorEventSink,
        intent: Arc<AtomicU8>,
        seen: AtomicUsize,
    }
    impl OperationEventSink for RollbackOnceCopying {
        fn emit_settled(&self, e: WriteSettledEvent) {
            self.inner.emit_settled(e);
        }
        fn emit_progress(&self, event: WriteProgressEvent) {
            // Late enough that several leaves are in flight at several depths.
            if event.phase == WriteOperationPhase::Copying && self.seen.fetch_add(1, Ordering::Relaxed) >= 6 {
                // RollingBack = 1
                self.intent.store(1, Ordering::Relaxed);
            }
            self.inner.emit_progress(event);
        }
        fn emit_complete(&self, e: WriteCompleteEvent) {
            self.inner.emit_complete(e);
        }
        fn emit_cancelled(&self, e: WriteCancelledEvent) {
            self.inner.emit_cancelled(e);
        }
        fn emit_error(&self, e: WriteErrorEvent) {
            self.inner.emit_error(e);
        }
        fn emit_conflict(&self, e: WriteConflictEvent) {
            self.inner.emit_conflict(e);
        }
        fn emit_source_item_done(&self, _e: WriteSourceItemDoneEvent) {}
        fn emit_scan_progress(&self, _e: ScanProgressEvent) {}
        fn emit_scan_conflict(&self, _c: crate::file_system::write_operations::types::ConflictInfo) {}
        fn emit_dry_run_complete(&self, _r: crate::file_system::write_operations::types::DryRunResult) {}
    }

    let (source, _) = make_volumes();
    source.create_directory(Path::new("/album")).await.unwrap();
    for sub in 0..3 {
        let dir = format!("/album/sub-{sub}");
        source.create_directory(Path::new(&dir)).await.unwrap();
        for index in 0..8 {
            source
                .create_file(Path::new(&format!("{dir}/f-{index:02}.bin")), &vec![index as u8; 8_192])
                .await
                .unwrap();
        }
    }
    let dest = WindowWatchingDest::new(6, 6);
    let dest_volume: Arc<dyn Volume> = Arc::clone(&dest) as Arc<dyn Volume>;

    let state = make_state();
    let events = Arc::new(RollbackOnceCopying {
        inner: CollectorEventSink::new(),
        intent: Arc::clone(&state.intent),
        seen: AtomicUsize::new(0),
    });
    let config = VolumeCopyConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    };
    let result = copy_volumes_with_progress(
        events,
        "merge-window-rollback",
        &state,
        source,
        &[PathBuf::from("/album")],
        Arc::clone(&dest_volume),
        Path::new("/out"),
        &config,
    )
    .await;
    assert!(result.is_err(), "a rolled-back copy reports a failure, got {result:?}");
    assert_eq!(
        dest.still_live(),
        0,
        "no leaf may outlive the copy into the cleanup phase"
    );

    for sub in 0..3 {
        let dir = format!("/out/album/sub-{sub}");
        assert!(
            !dest_volume.exists(Path::new(&dir)).await,
            "{dir} survived the rollback, so something it holds was never recorded"
        );
    }
    assert!(
        !dest_volume.exists(Path::new("/out/album")).await,
        "/out/album survived the rollback"
    );
}

/// One file failing names ITSELF (not the folder the user selected), and the
/// siblings that already landed stay recorded — the ledger a rollback reads is
/// complete even though several leaves were producing into it at once.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failing_leaf_names_itself_and_its_landed_siblings_stay_recorded() {
    let inner_source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    inner_source.create_directory(Path::new("/album")).await.unwrap();
    for index in 0..8 {
        inner_source
            .create_file(Path::new(&format!("/album/f-{index:02}.bin")), &vec![7u8; 4_096])
            .await
            .unwrap();
    }
    let source: Arc<dyn Volume> = Arc::new(PoisonedLeafSource {
        inner: Arc::clone(&inner_source),
        poisoned: "/album/f-05.bin".to_string(),
    });
    let dest = WindowWatchingDest::new(4, 4);
    let dest_volume: Arc<dyn Volume> = Arc::clone(&dest) as Arc<dyn Volume>;

    let events = Arc::new(CollectorEventSink::new());
    let state = make_state();
    let result = copy_volumes_with_progress(
        events,
        "merge-window-leaf-failure",
        &state,
        source,
        &[PathBuf::from("/album")],
        Arc::clone(&dest_volume),
        Path::new("/out"),
        &overwrite_config(),
    )
    .await;
    assert!(result.is_err(), "the poisoned leaf must fail the copy, got {result:?}");

    // The failure must name the FILE that broke, not the folder the user
    // selected: with a whole subtree behind one top-level item, the folder's name
    // is the entire content of an unusable report.
    let failure = format!("{:?}", result.unwrap_err().error);
    assert!(
        failure.contains("f-05.bin"),
        "the failure must name the file that broke; got {failure}"
    );

    assert!(
        !dest_volume.exists(Path::new("/out/album/f-05.bin")).await,
        "the poisoned file must not be at the destination"
    );
    assert_eq!(
        dest.still_live(),
        0,
        "the walk must drain its in-flight leaves before reporting the failure"
    );
}
