//! The concurrent driver's own contract, driven through
//! `drive_transfer_concurrent` directly rather than through
//! `copy_volumes_with_progress`.
//!
//! **Why this seam and not the end-to-end one.** Two of the driver's data-safety
//! rules are invisible from outside `copy_volumes_with_progress`, because a
//! second, independent defense stands behind them: the post-loop's cleanup and
//! rollback can only reach `delete_written_file` / `prune_created_dir_if_empty`,
//! which list before they delete, so handing them a merged destination ROOT
//! fails to delete it instead of destroying the user's files (`DETAILS.md` §
//! "Three ways to delete, and who may use each"). That capability split is
//! deliberate, and it means an end-to-end assertion about surviving files stays
//! green whether the driver's rollback ledger is right or wrong. Asserting on
//! what the driver HANDS BACK is what actually pins the ledger.
//!
//! The finalize-failure arm of `cleanup_temp` (a temp holding the only complete
//! copy of the new data must be left on disk) is pinned end to end instead, in
//! `copy_crashsafe_tests.rs`; the stream-failure arm below is its sibling.
//!
//! Beyond the ledger, this is also where the pieces the phase runner can't reach
//! get pinned: what `prepare_source` decides for one source before any task
//! exists (a pre-skip costs nothing, a Skip credits both progress axes once, an
//! Overwrite becomes a staged write plus a swap), and that a directory source
//! takes no slot from the file window, so a window narrower than the batch can't
//! deadlock on itself.
//!
//! Fixtures are local: the driver takes ~30 fields the phase runner assembles,
//! and `Harness` below is the one place a test has to know that.

use super::*;
use crate::file_system::volume::{InMemoryVolume, VolumeError, VolumeReadStream};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
use crate::file_system::write_operations::ledger::{WrittenFile, WrittenIdentity};
use crate::file_system::write_operations::types::ConflictResolution;
use cmdr_fs::staging::STAGING_TEMP_MARKER;
use std::collections::HashSet;

use super::super::faulty_volume::forward_volume_methods;

// ============================================================================
// Doubles
// ============================================================================

/// A source volume whose read stream fails for ONE named path, so a single
/// source (or a single child inside a directory source) fails while its
/// siblings copy normally. `IoError` with no errno is deliberately not
/// retryable (`retry.rs::is_retryable`), so the failure lands on the first
/// attempt.
struct FailReadForPathVolume {
    inner: Arc<InMemoryVolume>,
    fail_for: PathBuf,
}

impl Volume for FailReadForPathVolume {
    forward_volume_methods!(inner =>
        name, root, list_directory, get_metadata, exists, is_directory, create_file, create_directory,
        create_directory_all, delete, rename, get_space_info, supports_streaming, supports_export,
        operations_are_local, max_concurrent_ops, scan_for_copy, write_from_stream,
    );

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }

    fn open_read_stream<'a>(
        &'a self,
        path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = Result<Box<dyn VolumeReadStream>, VolumeError>> + Send + 'a>> {
        if path == self.fail_for {
            return Box::pin(async move {
                Err(VolumeError::IoError {
                    message: "simulated source read failure".to_string(),
                    raw_os_error: None,
                })
            });
        }
        self.inner.open_read_stream(path)
    }
}

// ============================================================================
// Harness
// ============================================================================

/// Everything a `ConcurrentCopy` borrows, owned in one place so a test can hand
/// out a context and then read the ledgers back.
///
/// The phase runner computes all of this before it reaches the driver; here it
/// starts at its "nothing known upfront" value — no destination index (so the
/// pre-check probes), no preflight hints (so `resolve_source_is_directory` asks
/// the volume), no journal, no probe — and a test overrides only the field it is
/// about.
struct Harness {
    state: Arc<WriteOperationState>,
    events: Arc<dyn OperationEventSink>,
    config: VolumeCopyConfig,
    source_paths: Vec<PathBuf>,
    pre_skip_paths: HashSet<PathBuf>,
    source_hints: HashMap<PathBuf, SourceHint>,
    dest_index: Option<DestNameIndex>,
    journal_volumes: Option<(String, String)>,
    op_probe: Option<Arc<OperationProbe>>,
    files_done: Arc<AtomicUsize>,
    bytes_done: Arc<AtomicU64>,
    files_skipped: Arc<AtomicUsize>,
    bytes_skipped: Arc<AtomicU64>,
    last_progress: Arc<std::sync::Mutex<Instant>>,
    apply_to_all: Arc<std::sync::Mutex<ApplyToAll>>,
    copied_paths: Arc<std::sync::Mutex<Vec<WrittenFile>>>,
    created_dirs: Arc<std::sync::Mutex<Vec<PathBuf>>>,
    in_flight_partials: Arc<std::sync::Mutex<Vec<PathBuf>>>,
}

impl Harness {
    fn new(source_paths: &[PathBuf]) -> Self {
        Self {
            state: Arc::new(WriteOperationState::new(Duration::from_millis(50))),
            events: Arc::new(CollectorEventSink::new()),
            config: merge_config(),
            source_paths: source_paths.to_vec(),
            pre_skip_paths: HashSet::new(),
            source_hints: HashMap::new(),
            dest_index: None,
            journal_volumes: None,
            op_probe: None,
            files_done: Arc::new(AtomicUsize::new(0)),
            bytes_done: Arc::new(AtomicU64::new(0)),
            files_skipped: Arc::new(AtomicUsize::new(0)),
            bytes_skipped: Arc::new(AtomicU64::new(0)),
            last_progress: Arc::new(std::sync::Mutex::new(Instant::now())),
            apply_to_all: Arc::new(std::sync::Mutex::new(ApplyToAll::default())),
            copied_paths: Arc::new(std::sync::Mutex::new(Vec::new())),
            created_dirs: Arc::new(std::sync::Mutex::new(Vec::new())),
            in_flight_partials: Arc::new(std::sync::Mutex::new(Vec::new())),
        }
    }

    fn ctx(&self, source: Arc<dyn Volume>, dest: Arc<dyn Volume>, concurrency: usize) -> ConcurrentCopy<'_> {
        ConcurrentCopy {
            events: Arc::clone(&self.events),
            operation_id: "driver-unit-test",
            state: &self.state,
            source_volume: source,
            source_paths: &self.source_paths,
            dest_volume: dest,
            dest_path: Path::new("/"),
            config: &self.config,
            concurrency,
            file_window: super::super::strategy::FileWindow::new(concurrency),
            dest_dir_is_ours: false,
            dest_index: &self.dest_index,
            pre_skip_paths: &self.pre_skip_paths,
            source_hints: &self.source_hints,
            total_files: self.source_paths.len(),
            total_bytes: 0,
            progress_interval: Duration::from_millis(0),
            journal_volumes: &self.journal_volumes,
            op_probe: &self.op_probe,
            files_done_atomic: Arc::clone(&self.files_done),
            atomic_bytes_done: Arc::clone(&self.bytes_done),
            files_skipped_atomic: Arc::clone(&self.files_skipped),
            bytes_skipped_atomic: Arc::clone(&self.bytes_skipped),
            last_progress_mutex: Arc::clone(&self.last_progress),
            apply_to_all_cell: Arc::clone(&self.apply_to_all),
            copied_paths: Arc::clone(&self.copied_paths),
            created_dirs: Arc::clone(&self.created_dirs),
            in_flight_partials: Arc::clone(&self.in_flight_partials),
            deep_skipped_files: Arc::new(AtomicUsize::new(0)),
            deep_skipped_bytes: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Runs the whole window over the harness's sources and reports what the
    /// driver left behind.
    async fn drive(
        &self,
        source: Arc<dyn Volume>,
        dest: Arc<dyn Volume>,
        concurrency: usize,
    ) -> Result<DriverRun, WriteFailure> {
        let outcome = drive_transfer_concurrent(self.ctx(source, dest, concurrency)).await?;
        Ok(DriverRun {
            outcome,
            copied: self.copied_paths.lock_ignore_poison().clone(),
            created_dirs: self.created_dirs.lock_ignore_poison().clone(),
            in_flight_partials: self.in_flight_partials.lock_ignore_poison().clone(),
        })
    }

    /// Fills `source_hints` from a REAL preflight scan, the way the phase runner
    /// does before it reaches the driver.
    ///
    /// ❌ Never hand-build a `SourceHint`: a literal records the test author's
    /// assumptions rather than a shape production emits, which is the whole
    /// point of `no-hand-rolled-fixture`.
    async fn seed_hints_from_preflight(&mut self, source: &Arc<dyn Volume>) {
        let preflight = super::super::preflight::scan_volume_sources(
            source,
            &self.source_paths,
            &self.config,
            "driver-unit-test",
            WriteOperationType::Copy,
            &self.state,
            &*self.events,
        )
        .await
        .expect("a preflight scan of an in-memory source can't fail");
        self.source_hints = preflight.source_hints;
    }

    /// `(files_done, bytes_done, files_skipped, bytes_skipped)`.
    fn counters(&self) -> (usize, u64, usize, u64) {
        (
            self.files_done.load(Ordering::Relaxed),
            self.bytes_done.load(Ordering::Relaxed),
            self.files_skipped.load(Ordering::Relaxed),
            self.bytes_skipped.load(Ordering::Relaxed),
        )
    }
}

/// What one driver run left behind: its own outcome plus the three shared
/// ledgers the phase runner reads after it returns.
struct DriverRun {
    outcome: ConcurrentOutcome,
    copied: Vec<WrittenFile>,
    created_dirs: Vec<PathBuf>,
    in_flight_partials: Vec<PathBuf>,
}

impl DriverRun {
    /// Just the paths of the rollback ledger, for the assertions that are about
    /// WHICH destinations were recorded.
    fn copied_paths(&self) -> Vec<PathBuf> {
        self.copied.iter().map(|entry| entry.path.clone()).collect()
    }
}

/// A source directory of `n` files, merging into a pre-existing destination
/// directory of the same name that holds a sentinel the operation must never
/// touch, plus two sibling files so the batch is a real multi-source fan-out.
async fn merge_batch(n: usize) -> (Arc<InMemoryVolume>, Arc<dyn Volume>, Vec<PathBuf>) {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));

    source.create_directory(Path::new("/album")).await.unwrap();
    for i in 0..n {
        source
            .create_file(Path::new(&format!("/album/new{i}.bin")), &vec![0u8; 20_000])
            .await
            .unwrap();
    }
    source
        .create_file(Path::new("/sib1.bin"), &vec![1u8; 20_000])
        .await
        .unwrap();
    source
        .create_file(Path::new("/sib2.bin"), &vec![2u8; 20_000])
        .await
        .unwrap();

    dest.create_directory(Path::new("/album")).await.unwrap();
    dest.create_file(Path::new("/album/sentinel.txt"), b"precious user data")
        .await
        .unwrap();

    let sources = vec![
        PathBuf::from("/album"),
        PathBuf::from("/sib1.bin"),
        PathBuf::from("/sib2.bin"),
    ];
    (source, dest, sources)
}

fn merge_config() -> VolumeCopyConfig {
    VolumeCopyConfig {
        // Overwrite ⇒ dir-vs-dir merges into the pre-existing dest directory.
        conflict_resolution: ConflictResolution::Overwrite,
        progress_interval_ms: 0,
        ..VolumeCopyConfig::default()
    }
}

// ============================================================================
// The rollback ledger: a directory source is recorded per FILE, never by root
// ============================================================================

/// A directory source that copied SUCCESSFULLY hands back the individual files
/// it wrote and the subdirectories it newly created — never the destination
/// directory root.
///
/// The root is what the user already had: "Overwrite means merge for dirs", so a
/// merged destination legitimately holds dest-only files the operation never
/// touched. Record the root and a later Rollback either destroys them or (behind
/// the delete-capability split) silently fails to undo anything the copy wrote.
/// The serial driver's version of this is pinned end to end by
/// `copy_rollback_tests.rs::rollback_of_merged_directory_preserves_preexisting_dest_files`;
/// the concurrent driver reaches it through a different arm.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_completed_directory_source_is_recorded_file_by_file_never_by_its_root() {
    let (source, dest, sources) = merge_batch(3).await;
    let harness = Harness::new(&sources);

    let run = harness
        .drive(source as Arc<dyn Volume>, Arc::clone(&dest), 4)
        .await
        .expect("the driver only errors when conflict resolution itself fails");

    assert!(
        run.outcome.copy_error.is_none(),
        "the batch should have copied cleanly: {:?}",
        run.outcome.copy_error
    );
    assert!(
        !run.copied_paths().contains(&PathBuf::from("/album")),
        "the merged destination ROOT must never be recorded for rollback: {:?}",
        run.copied_paths()
    );
    for i in 0..3 {
        let leaf = PathBuf::from(format!("/album/new{i}.bin"));
        assert!(
            run.copied_paths().contains(&leaf),
            "the directory source's own file {} must be recorded, so Rollback can undo it: {:?}",
            leaf.display(),
            run.copied_paths()
        );
    }
    assert!(
        !run.copied_paths().contains(&PathBuf::from("/album/sentinel.txt")),
        "a dest-only file the copy never wrote must not be in the rollback ledger: {:?}",
        run.copied_paths()
    );
    // Both FILE sources record their landed path, which is the whole ledger for
    // them (a file source has no `created_*`).
    assert!(run.copied_paths().contains(&PathBuf::from("/sib1.bin")));
    assert!(run.copied_paths().contains(&PathBuf::from("/sib2.bin")));
    assert!(
        !run.created_dirs.contains(&PathBuf::from("/album")),
        "`/album` already existed, so the copy did not create it: {:?}",
        run.created_dirs
    );
}

/// Every entry in the rollback ledger carries the size it was written with —
/// the identity an in-flight reversal rechecks the destination against.
///
/// The size is on hand at every recording site (the leaf copier reports the bytes
/// it piped), and the journal's own row for the same leaf carries it. A ledger of
/// bare paths would leave the reversal nothing to recognize the file by.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn every_recorded_destination_carries_the_size_it_was_written_with() {
    let (source, dest, sources) = merge_batch(3).await;
    let harness = Harness::new(&sources);

    let run = harness
        .drive(source as Arc<dyn Volume>, Arc::clone(&dest), 4)
        .await
        .expect("the driver only errors when conflict resolution itself fails");

    assert_eq!(run.copied.len(), 5, "three merged children plus two file sources");
    for entry in &run.copied {
        assert_eq!(
            entry.identity,
            WrittenIdentity::VolumeFile { size: 20_000 },
            "{} lost the size it was written with",
            entry.path.display()
        );
    }
}

/// The same ledger has to flow out of the FAILURE arm, not just the success one.
/// A directory source whose subtree copy fails part-way hands back the files it
/// managed to write and leaves `last_dest_path` empty — the dest directory root
/// is never designated as a partial to clean.
///
/// The cancel-shaped sibling of this is pinned end to end by
/// `copy_rollback_tests.rs::cancel_mid_merge_stream_concurrent_preserves_preexisting_dest_file`.
/// A genuine transport failure takes the same arm and had nothing watching it.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_failed_directory_source_records_its_files_and_never_its_root_as_a_partial() {
    let (source_inner, dest, sources) = merge_batch(3).await;
    let source: Arc<dyn Volume> = Arc::new(FailReadForPathVolume {
        inner: source_inner,
        fail_for: PathBuf::from("/album/new1.bin"),
    });
    let harness = Harness::new(&sources);

    let run = harness
        .drive(source, Arc::clone(&dest), 4)
        .await
        .expect("a task failure comes back in the outcome, not as an Err");

    assert!(
        run.outcome.copy_error.is_some(),
        "the failing child must surface as the operation's error"
    );
    assert_eq!(
        run.outcome.last_dest_path, None,
        "a DIRECTORY source's dest root must never be handed to the partial-cleanup sweep: {:?}",
        run.outcome.last_dest_path
    );
    assert!(
        !run.copied_paths().contains(&PathBuf::from("/album")),
        "the merged destination ROOT must never be recorded, on the failure arm either: {:?}",
        run.copied_paths()
    );
    assert!(
        run.copied_paths().iter().any(|p| p.starts_with("/album/")),
        "the files the interrupted directory source did write must be recorded per file: {:?}",
        run.copied_paths()
    );
    assert!(
        !run.in_flight_partials.contains(&PathBuf::from("/album")),
        "a directory source is never an in-flight partial: {:?}",
        run.in_flight_partials
    );
}

// ============================================================================
// `cleanup_temp`: a stream failure's partial IS cleaned
// ============================================================================

/// A FILE source resolved to a safe-replace Overwrite whose STREAM fails hands
/// its temp back as `last_dest_path`, so the post-loop sweep removes it.
///
/// This is the `cleanup_temp = true` arm. Its opposite number — a FINALIZE
/// failure after a successful write, where the temp holds the only complete copy
/// of the new data and MUST be left on disk — is pinned by
/// `copy_crashsafe_tests.rs::cross_volume_overwrite_concurrent_preserves_new_data_on_finalize_failure`.
/// Swapping the two arms is total data loss in one direction and a stray temp in
/// the other, so both need something watching them.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_stream_failure_hands_its_staged_partial_back_for_cleanup() {
    let source_inner = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source_inner.create_file(Path::new("/a.txt"), b"AAA").await.unwrap();
    source_inner.create_file(Path::new("/b.txt"), b"BBB-new").await.unwrap();
    source_inner.create_file(Path::new("/c.txt"), b"CCC").await.unwrap();
    let source: Arc<dyn Volume> = Arc::new(FailReadForPathVolume {
        inner: source_inner,
        fail_for: PathBuf::from("/b.txt"),
    });

    // `/b.txt` clashes, so an Overwrite resolves to a temp sibling plus a
    // finalize — and the stream into that temp is the thing that fails.
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest.create_file(Path::new("/b.txt"), b"BBB-old").await.unwrap();

    let sources = vec![
        PathBuf::from("/a.txt"),
        PathBuf::from("/b.txt"),
        PathBuf::from("/c.txt"),
    ];
    let harness = Harness::new(&sources);

    let run = harness.drive(source, Arc::clone(&dest), 4).await.unwrap();

    assert!(
        run.outcome.copy_error.is_some(),
        "the failing source must surface an error"
    );
    let partial = run
        .outcome
        .last_dest_path
        .as_ref()
        .expect("a FILE source's half-written partial must be designated for cleanup");
    assert!(
        partial.to_string_lossy().contains(STAGING_TEMP_MARKER),
        "the partial to clean is the staged temp sibling, not the user's file: {}",
        partial.display()
    );
    // And the original the Overwrite was replacing is still whole: the temp is
    // what failed, so nothing has swapped over it.
    let mut stream = dest.open_read_stream(Path::new("/b.txt")).await.unwrap();
    assert_eq!(
        stream.next_chunk().await.unwrap().unwrap(),
        b"BBB-old",
        "a stream failure must leave the destination it was replacing untouched"
    );
}

// ============================================================================
// Preparing one source
// ============================================================================

/// A source the bulk skip already accounted is dropped without touching
/// anything: no probe, no ledger entry, and — the part that would go wrong
/// quietly — no second credit to the progress counters.
///
/// `pre_skip_paths` holds clashes resolved and counted BEFORE the driver
/// started. Crediting them again here would report more files done than the
/// operation has.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_pre_skipped_source_is_dropped_without_being_counted_again() {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_file(Path::new("/a.txt"), b"AAA").await.unwrap();
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));

    let sources = vec![PathBuf::from("/a.txt")];
    let mut harness = Harness::new(&sources);
    harness.pre_skip_paths.insert(PathBuf::from("/a.txt"));

    let ctx = harness.ctx(source as Arc<dyn Volume>, dest, 4);
    let prepared = ctx
        .prepare_source(0, Path::new("/a.txt"))
        .await
        .expect("a pre-skip is not a failure");

    assert!(prepared.is_none(), "a pre-skipped source must not be spawned");
    assert_eq!(
        harness.counters(),
        (0, 0, 0, 0),
        "the bulk skip already counted this source; counting it again over-reports progress"
    );
    assert!(
        harness.in_flight_partials.lock_ignore_poison().is_empty(),
        "nothing was written, so nothing is in flight"
    );
}

/// A clash the resolver answers with Skip advances BOTH progress axes exactly
/// once, using the preflight hint's size.
///
/// Without it a "Skip all" choice runs through dozens of conflicts with the bar
/// pinned at 0%, which reads as a hung operation: the user can see it working
/// through their files.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_conflict_resolved_to_skip_credits_both_progress_axes_once() {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_file(Path::new("/a.txt"), b"AAA").await.unwrap();
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest.create_file(Path::new("/a.txt"), b"OLD").await.unwrap();

    let sources = vec![PathBuf::from("/a.txt")];
    let source: Arc<dyn Volume> = source;
    let mut harness = Harness::new(&sources);
    harness.config.conflict_resolution = ConflictResolution::Skip;
    harness.seed_hints_from_preflight(&source).await;

    let ctx = harness.ctx(Arc::clone(&source), dest, 4);
    let prepared = ctx.prepare_source(0, Path::new("/a.txt")).await.unwrap();

    assert!(prepared.is_none(), "Skip means no task");
    assert_eq!(
        harness.counters(),
        (1, 3, 1, 3),
        "a skipped file counts once on the done axis and once on the skipped axis, at its scanned size"
    );
}

/// A file→file Overwrite is prepared as a STAGED write plus a swap, and it is
/// the temp — never the user's file — that goes on the in-flight partial list.
///
/// That is what makes a mid-stream failure survivable: the original is untouched
/// until the temp is complete, and the cleanup sweep that runs on cancel or
/// error can only reach the temp.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_overwrite_is_prepared_as_a_staged_write_plus_a_swap() {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    source.create_file(Path::new("/a.txt"), b"NEW").await.unwrap();
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));
    dest.create_file(Path::new("/a.txt"), b"OLD").await.unwrap();

    let sources = vec![PathBuf::from("/a.txt")];
    let harness = Harness::new(&sources);

    let ctx = harness.ctx(source as Arc<dyn Volume>, dest, 4);
    let task = ctx
        .prepare_source(0, Path::new("/a.txt"))
        .await
        .unwrap()
        .expect("an Overwrite spawns a task");

    assert!(
        task.dest_path.to_string_lossy().contains(STAGING_TEMP_MARKER),
        "the bytes go to a temp sibling, not onto the file being replaced: {}",
        task.dest_path.display()
    );
    assert_eq!(
        task.replace_after_write,
        Some(PathBuf::from("/a.txt")),
        "the swap has to name the original, or the new data never takes its place"
    );
    assert_eq!(
        *harness.in_flight_partials.lock_ignore_poison(),
        vec![task.dest_path.clone()],
        "the TEMP is the partial cleanup may remove; the original must never be on this list"
    );
}

// ============================================================================
// The file window
// ============================================================================

/// A DIRECTORY source takes no slot from the operation's file window, so a
/// window narrower than the batch can't deadlock on itself.
///
/// Two directory sources in a two-wide window: if each held a permit while
/// walking, both permits would be gone and every leaf underneath them would wait
/// forever for one. A top-level FILE source is a leaf and does take its slot,
/// which is why the two can't share one rule.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn a_directory_source_takes_no_slot_from_the_file_window() {
    let source = Arc::new(InMemoryVolume::new("Source").with_space_info(10_000_000, 10_000_000));
    for dir in ["d1", "d2"] {
        source.create_directory(Path::new(&format!("/{dir}"))).await.unwrap();
        for i in 0..3 {
            source
                .create_file(Path::new(&format!("/{dir}/leaf{i}.bin")), &vec![7u8; 10_000])
                .await
                .unwrap();
        }
    }
    source
        .create_file(Path::new("/f.bin"), &vec![8u8; 10_000])
        .await
        .unwrap();
    let dest: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("Dest").with_space_info(10_000_000, 10_000_000));

    let sources = vec![PathBuf::from("/d1"), PathBuf::from("/d2"), PathBuf::from("/f.bin")];
    let harness = Harness::new(&sources);

    // A deadlock here would otherwise run until nextest kills the process with
    // nothing to say; the deadline turns it into a readable failure.
    let run = tokio::time::timeout(
        Duration::from_secs(5),
        harness.drive(source as Arc<dyn Volume>, Arc::clone(&dest), 2),
    )
    .await
    .expect("a two-wide window over two directory sources must not deadlock")
    .unwrap();

    assert!(run.outcome.copy_error.is_none(), "{:?}", run.outcome.copy_error);
    for dir in ["d1", "d2"] {
        for i in 0..3 {
            let leaf = format!("/{dir}/leaf{i}.bin");
            assert!(dest.exists(Path::new(&leaf)).await, "{leaf} never landed");
        }
    }
    assert!(dest.exists(Path::new("/f.bin")).await);
}
