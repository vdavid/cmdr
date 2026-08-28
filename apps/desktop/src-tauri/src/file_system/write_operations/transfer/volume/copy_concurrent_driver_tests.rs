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
//! Fixtures are local: the driver takes ~30 fields the phase runner assembles,
//! and `drive` below is the one place a test has to know that.

use super::*;
use crate::file_system::volume::{InMemoryVolume, VolumeReadStream};
use crate::file_system::write_operations::event_sinks::CollectorEventSink;
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

/// What one driver run left behind: its own outcome plus the three shared
/// ledgers the phase runner reads after it returns.
struct DriverRun {
    outcome: ConcurrentOutcome,
    copied_paths: Vec<PathBuf>,
    created_dirs: Vec<PathBuf>,
    in_flight_partials: Vec<PathBuf>,
}

/// Runs the concurrent driver over `sources`, with a window of 4 and everything
/// the phase runner would otherwise have computed set to its "nothing known
/// upfront" value: no destination index (so the pre-check probes), no preflight
/// hints (so `resolve_source_is_directory` asks the volume), no journal, no
/// probe.
async fn drive(
    source: Arc<dyn Volume>,
    dest: Arc<dyn Volume>,
    sources: &[PathBuf],
    config: &VolumeCopyConfig,
) -> Result<DriverRun, WriteFailure> {
    let state = Arc::new(WriteOperationState::new(Duration::from_millis(50)));
    let events: Arc<dyn OperationEventSink> = Arc::new(CollectorEventSink::new());
    let copied_paths = Arc::new(std::sync::Mutex::new(Vec::new()));
    let created_dirs = Arc::new(std::sync::Mutex::new(Vec::new()));
    let in_flight_partials = Arc::new(std::sync::Mutex::new(Vec::new()));
    let pre_skip_paths = HashSet::new();
    let source_hints = HashMap::new();
    let dest_index = None;
    let journal_volumes = None;
    let op_probe = None;

    let outcome = drive_transfer_concurrent(ConcurrentCopy {
        events,
        operation_id: "driver-unit-test",
        state: &state,
        source_volume: source,
        source_paths: sources,
        dest_volume: dest,
        dest_path: Path::new("/"),
        config,
        concurrency: 4,
        file_window: super::super::strategy::FileWindow::new(4),
        dest_dir_is_ours: false,
        dest_index: &dest_index,
        pre_skip_paths: &pre_skip_paths,
        source_hints: &source_hints,
        total_files: sources.len(),
        total_bytes: 0,
        progress_interval: Duration::from_millis(0),
        journal_volumes: &journal_volumes,
        op_probe: &op_probe,
        files_done_atomic: Arc::new(AtomicUsize::new(0)),
        atomic_bytes_done: Arc::new(AtomicU64::new(0)),
        files_skipped_atomic: Arc::new(AtomicUsize::new(0)),
        bytes_skipped_atomic: Arc::new(AtomicU64::new(0)),
        last_progress_mutex: Arc::new(std::sync::Mutex::new(Instant::now())),
        apply_to_all_cell: Arc::new(std::sync::Mutex::new(ApplyToAll::default())),
        copied_paths: Arc::clone(&copied_paths),
        created_dirs: Arc::clone(&created_dirs),
        in_flight_partials: Arc::clone(&in_flight_partials),
        deep_skipped_files: Arc::new(AtomicUsize::new(0)),
        deep_skipped_bytes: Arc::new(AtomicU64::new(0)),
    })
    .await?;

    Ok(DriverRun {
        outcome,
        copied_paths: copied_paths.lock_ignore_poison().clone(),
        created_dirs: created_dirs.lock_ignore_poison().clone(),
        in_flight_partials: in_flight_partials.lock_ignore_poison().clone(),
    })
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
    let config = merge_config();

    let run = drive(source as Arc<dyn Volume>, Arc::clone(&dest), &sources, &config)
        .await
        .expect("the driver only errors when conflict resolution itself fails");

    assert!(
        run.outcome.copy_error.is_none(),
        "the batch should have copied cleanly: {:?}",
        run.outcome.copy_error
    );
    assert!(
        !run.copied_paths.contains(&PathBuf::from("/album")),
        "the merged destination ROOT must never be recorded for rollback: {:?}",
        run.copied_paths
    );
    for i in 0..3 {
        let leaf = PathBuf::from(format!("/album/new{i}.bin"));
        assert!(
            run.copied_paths.contains(&leaf),
            "the directory source's own file {} must be recorded, so Rollback can undo it: {:?}",
            leaf.display(),
            run.copied_paths
        );
    }
    assert!(
        !run.copied_paths.contains(&PathBuf::from("/album/sentinel.txt")),
        "a dest-only file the copy never wrote must not be in the rollback ledger: {:?}",
        run.copied_paths
    );
    // Both FILE sources record their landed path, which is the whole ledger for
    // them (a file source has no `created_*`).
    assert!(run.copied_paths.contains(&PathBuf::from("/sib1.bin")));
    assert!(run.copied_paths.contains(&PathBuf::from("/sib2.bin")));
    assert!(
        !run.created_dirs.contains(&PathBuf::from("/album")),
        "`/album` already existed, so the copy did not create it: {:?}",
        run.created_dirs
    );
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
    let config = merge_config();

    let run = drive(source, Arc::clone(&dest), &sources, &config)
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
        !run.copied_paths.contains(&PathBuf::from("/album")),
        "the merged destination ROOT must never be recorded, on the failure arm either: {:?}",
        run.copied_paths
    );
    assert!(
        run.copied_paths.iter().any(|p| p.starts_with("/album/")),
        "the files the interrupted directory source did write must be recorded per file: {:?}",
        run.copied_paths
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
    let config = merge_config();

    let run = drive(source, Arc::clone(&dest), &sources, &config).await.unwrap();

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
