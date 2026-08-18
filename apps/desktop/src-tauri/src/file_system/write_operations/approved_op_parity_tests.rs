//! The governing rule, made machine-checkable.
//!
//! **Once someone approves a suggested operation, it IS a user-started operation
//! — because they started it.** Same queue, same conflict handling, same folder
//! creation, same overwrite behaviour, same everything. A special case in the
//! execution path for approved work is not extra safety; it is a second execution
//! path that will drift from the real one, and the drift will only be discovered
//! by the person whose files it mishandled.
//!
//! So each case here runs the SAME transfer twice — once with no source binding
//! (a user picking files in a pane) and once bound to exactly what is on disk (a
//! group somebody just approved) — and demands the two agree on what happened to
//! the filesystem AND on what the sink reported. The binding is the only
//! difference an approved operation carries, so if a per-caller behaviour ever
//! creeps in, it lands here.
//!
//! ❌ These are not "the agent's tests". Nothing in the engine knows who called
//! it, and that is exactly the property being pinned.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::event_sinks::{CollectorEventSink, OperationEventSink};
use super::source_binding::{ExpectedSources, SourceFingerprint};
use super::types::{ConflictResolution, WriteOperationConfig, WriteOperationType};
use super::{copy_files_start, move_files_start};
use crate::ignore_poison::IgnorePoison;
use crate::operation_log::types::Initiator;
use crate::test_support::TestDir;

/// What one run of a transfer did, in the terms the two runs have to agree on.
#[derive(Debug, PartialEq, Eq)]
struct RunOutcome {
    files_processed: usize,
    files_skipped: usize,
    bytes_processed: u64,
    /// Every path under the destination root, relative and sorted, with its
    /// bytes. This is the answer that actually matters: what ended up on disk.
    dest_tree: Vec<(PathBuf, Vec<u8>)>,
    /// `(relative source path, source_removed, outcome)` per emitted item, sorted.
    source_items: Vec<(PathBuf, bool, super::types::SourceItemOutcome)>,
}

/// The binding an approved group carries: every source, fingerprinted exactly as
/// it is right now, so nothing is skipped for having changed. The whole point of
/// these cases is what happens to sources that DID survive the check.
fn bind_all(sources: &[PathBuf]) -> ExpectedSources {
    ExpectedSources::new(sources.iter().map(|source| {
        (
            source.clone(),
            SourceFingerprint::capture_local(source).expect("the fixture source is stat-able"),
        )
    }))
}

fn read_tree(root: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    fn walk(dir: &Path, root: &Path, into: &mut Vec<(PathBuf, Vec<u8>)>) {
        let Ok(entries) = std::fs::read_dir(dir) else { return };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                walk(&path, root, into);
            } else if let Ok(bytes) = std::fs::read(&path) {
                into.push((path.strip_prefix(root).expect("under the root").to_path_buf(), bytes));
            }
        }
    }
    let mut tree = Vec::new();
    walk(root, root, &mut tree);
    tree.sort();
    tree
}

async fn collect(collector: &Arc<CollectorEventSink>, dest_root: &Path, src_root: &Path) -> RunOutcome {
    crate::test_support::wait_until_async(Duration::from_secs(10), "the operation to settle", || {
        !collector.settled.lock_ignore_poison().is_empty()
    })
    .await;
    let complete = collector.complete.lock_ignore_poison();
    let complete = complete
        .first()
        .expect("the transfer must complete; a parity case is not about failures");
    let mut source_items: Vec<_> = collector
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .map(|item| {
            (
                Path::new(&item.source_path)
                    .strip_prefix(src_root)
                    .unwrap_or(Path::new(&item.source_path))
                    .to_path_buf(),
                item.source_removed,
                item.outcome,
            )
        })
        .collect();
    source_items.sort_by(|left, right| left.0.cmp(&right.0).then(left.1.cmp(&right.1)));
    RunOutcome {
        files_processed: complete.files_processed,
        files_skipped: complete.files_skipped,
        bytes_processed: complete.bytes_processed,
        dest_tree: read_tree(dest_root),
        source_items,
    }
}

/// Seeds one fixture pair and runs a copy over it, with or without a binding.
async fn run_copy(seed: impl Fn(&Path, &Path), config: WriteOperationConfig, bind: bool, tag: &str) -> RunOutcome {
    let src_root = TestDir::new(&format!("parity_src_{tag}"));
    let dst_root = TestDir::new(&format!("parity_dst_{tag}"));
    seed(&src_root, &dst_root);

    let sources: Vec<PathBuf> = std::fs::read_dir(&*src_root)
        .expect("read the seeded sources")
        .flatten()
        .map(|entry| entry.path())
        .collect::<std::collections::BTreeSet<_>>()
        .into_iter()
        .collect();
    let expected = bind.then(|| bind_all(&sources));

    let collector = Arc::new(CollectorEventSink::new());
    let events: Arc<dyn OperationEventSink> = collector.clone();
    // The destination the transfer is aimed at is a CHILD of the fixture root, so
    // "does the engine create a missing destination folder?" is a real question
    // for every case here.
    let destination = dst_root.join("incoming");
    copy_files_start(
        events,
        sources,
        destination.clone(),
        config,
        vec![],
        None,
        Initiator::User,
        expected,
    )
    .await
    .expect("the copy starts");

    collect(&collector, &destination, &src_root).await
}

/// The plan's flagship case: a suggestion whose destination folder does not exist
/// yet. Refusing to create it would be an agent-specific safety behaviour, and
/// the copy engine calls `ensure_destination_dir` on purpose.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_copy_creates_a_missing_destination_folder_exactly_as_a_user_started_one_does() {
    let seed = |src: &Path, _dst: &Path| {
        std::fs::write(src.join("report.pdf"), b"reviewed bytes").expect("seed source");
    };

    let by_user = run_copy(seed, WriteOperationConfig::default(), false, "mkdir_user").await;
    let approved = run_copy(seed, WriteOperationConfig::default(), true, "mkdir_approved").await;

    assert_eq!(
        approved.dest_tree,
        vec![(PathBuf::from("report.pdf"), b"reviewed bytes".to_vec())],
        "the destination folder was created and the file landed in it"
    );
    assert_eq!(approved, by_user);
}

/// Overwrite is the other refusal the guiding principle forbids. An approved
/// group that overwrites is a person choosing to overwrite.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_copy_overwrites_exactly_as_a_user_started_one_does() {
    let seed = |src: &Path, dst: &Path| {
        std::fs::write(src.join("report.pdf"), b"the new document").expect("seed source");
        let incoming = dst.join("incoming");
        std::fs::create_dir_all(&incoming).expect("seed destination");
        std::fs::write(incoming.join("report.pdf"), b"the old one").expect("seed clash");
    };
    let config = WriteOperationConfig {
        conflict_resolution: ConflictResolution::Overwrite,
        ..WriteOperationConfig::default()
    };

    let by_user = run_copy(seed, config.clone(), false, "overwrite_user").await;
    let approved = run_copy(seed, config, true, "overwrite_approved").await;

    assert_eq!(
        approved.dest_tree,
        vec![(PathBuf::from("report.pdf"), b"the new document".to_vec())],
        "the approved copy replaced the destination, because that is what was approved"
    );
    assert_eq!(approved, by_user);
}

/// Skip is the reverse check: the approved run must not quietly become MORE
/// permissive than the user-started one either.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_copy_skips_exactly_where_a_user_started_one_skips() {
    let seed = |src: &Path, dst: &Path| {
        std::fs::write(src.join("report.pdf"), b"the new document").expect("seed source");
        std::fs::write(src.join("notes.txt"), b"no clash here").expect("seed second source");
        let incoming = dst.join("incoming");
        std::fs::create_dir_all(&incoming).expect("seed destination");
        std::fs::write(incoming.join("report.pdf"), b"the old one").expect("seed clash");
    };
    let config = WriteOperationConfig {
        conflict_resolution: ConflictResolution::Skip,
        ..WriteOperationConfig::default()
    };

    let by_user = run_copy(seed, config.clone(), false, "skip_user").await;
    let approved = run_copy(seed, config, true, "skip_approved").await;

    assert_eq!(
        approved.dest_tree,
        vec![
            (PathBuf::from("notes.txt"), b"no clash here".to_vec()),
            (PathBuf::from("report.pdf"), b"the old one".to_vec()),
        ],
        "the clash was skipped and the clean source landed"
    );
    assert_eq!(approved, by_user);
}

/// A directory source, because a suggestion's op may be one whole folder rather
/// than a list of files, and a directory's fingerprint is deliberately weaker
/// than a file's.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_copy_of_a_whole_folder_matches_the_user_started_one() {
    let seed = |src: &Path, _dst: &Path| {
        let folder = src.join("photos");
        std::fs::create_dir_all(folder.join("2026")).expect("seed folder");
        std::fs::write(folder.join("a.jpg"), b"aaa").expect("seed child");
        std::fs::write(folder.join("2026").join("b.jpg"), b"bbbb").expect("seed nested child");
    };

    let by_user = run_copy(seed, WriteOperationConfig::default(), false, "folder_user").await;
    let approved = run_copy(seed, WriteOperationConfig::default(), true, "folder_approved").await;

    assert_eq!(
        approved.dest_tree,
        vec![
            (PathBuf::from("photos/2026/b.jpg"), b"bbbb".to_vec()),
            (PathBuf::from("photos/a.jpg"), b"aaa".to_vec()),
        ]
    );
    assert_eq!(approved, by_user);
}

/// Move gets its own case: it has a source-delete phase a copy doesn't, and that
/// phase is where a per-caller "be careful" would be most tempting to add.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_move_takes_the_source_exactly_as_a_user_started_one_does() {
    async fn run(bind: bool, tag: &str) -> (RunOutcome, bool) {
        let src_root = TestDir::new(&format!("parity_move_src_{tag}"));
        let dst_root = TestDir::new(&format!("parity_move_dst_{tag}"));
        let source = src_root.join("report.pdf");
        std::fs::write(&source, b"reviewed bytes").expect("seed source");
        let sources = vec![source.clone()];
        let expected = bind.then(|| bind_all(&sources));

        let collector = Arc::new(CollectorEventSink::new());
        let events: Arc<dyn OperationEventSink> = collector.clone();
        let destination = dst_root.join("incoming");
        move_files_start(
            events,
            sources,
            destination.clone(),
            WriteOperationConfig::default(),
            vec![],
            None,
            Initiator::User,
            expected,
        )
        .await
        .expect("the move starts");

        let outcome = collect(&collector, &destination, &src_root).await;
        (outcome, source.exists())
    }

    let (by_user, user_source_left) = run(false, "user").await;
    let (approved, approved_source_left) = run(true, "approved").await;

    assert!(
        !approved_source_left,
        "an approved move takes the source, like any move"
    );
    assert_eq!(approved_source_left, user_source_left);
    assert_eq!(
        approved.dest_tree,
        vec![(PathBuf::from("report.pdf"), b"reviewed bytes".to_vec())]
    );
    assert_eq!(approved, by_user);
}

/// The one thing an approved operation may do differently, stated positively so
/// nobody reads the cases above as "the binding does nothing": a source that
/// changed since the approval is dropped, and everything ELSE about the run is
/// still the ordinary path.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn only_a_source_that_changed_is_treated_differently() {
    let src_root = TestDir::new("parity_changed_src");
    let dst_root = TestDir::new("parity_changed_dst");
    let stale = src_root.join("stale.pdf");
    let fresh = src_root.join("fresh.pdf");
    std::fs::write(&stale, b"as reviewed").expect("seed");
    std::fs::write(&fresh, b"as reviewed too").expect("seed");
    let sources = vec![stale.clone(), fresh.clone()];
    let expected = bind_all(&sources);

    // Somebody edits one of them between the approval and the operation's turn.
    std::fs::write(&stale, b"edited since the approval").expect("rewrite");

    let collector = Arc::new(CollectorEventSink::new());
    let events: Arc<dyn OperationEventSink> = collector.clone();
    let destination = dst_root.join("incoming");
    copy_files_start(
        events,
        sources,
        destination.clone(),
        WriteOperationConfig::default(),
        vec![],
        None,
        Initiator::User,
        Some(expected),
    )
    .await
    .expect("the copy starts");

    let outcome = collect(&collector, &destination, &src_root).await;

    assert_eq!(
        outcome.dest_tree,
        vec![(PathBuf::from("fresh.pdf"), b"as reviewed too".to_vec())],
        "the unchanged source copied; the edited one did not"
    );
    assert_eq!(
        outcome.source_items,
        vec![
            (PathBuf::from("fresh.pdf"), false, super::types::SourceItemOutcome::Done),
            (
                PathBuf::from("stale.pdf"),
                false,
                super::types::SourceItemOutcome::Skipped
            ),
        ]
    );
    assert_eq!(
        std::fs::read(&stale).expect("the edited source is untouched"),
        b"edited since the approval"
    );
}

/// A guard on the guard: an approved COPY still writes with `WriteOperationType::Copy`,
/// so nothing here has quietly routed approved work through a different verb.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_approved_copy_settles_as_an_ordinary_copy() {
    let src_root = TestDir::new("parity_type_src");
    let dst_root = TestDir::new("parity_type_dst");
    let source = src_root.join("report.pdf");
    std::fs::write(&source, b"reviewed").expect("seed");
    let sources = vec![source];

    let collector = Arc::new(CollectorEventSink::new());
    let events: Arc<dyn OperationEventSink> = collector.clone();
    copy_files_start(
        events,
        sources.clone(),
        dst_root.join("incoming"),
        WriteOperationConfig::default(),
        vec![],
        None,
        Initiator::User,
        Some(bind_all(&sources)),
    )
    .await
    .expect("the copy starts");

    crate::test_support::wait_until_async(Duration::from_secs(10), "the operation to settle", || {
        !collector.settled.lock_ignore_poison().is_empty()
    })
    .await;

    let settled = collector.settled.lock_ignore_poison();
    assert_eq!(settled[0].operation_type, WriteOperationType::Copy);
}

// ============================================================================
// The volume route, end to end
// ============================================================================

/// A lane nothing else in the suite shares, so this op is admitted immediately rather than
/// queueing behind another test's transfer.
fn unique_lane(label: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static N: AtomicU64 = AtomicU64::new(0);
    let n = N.fetch_add(1, Ordering::Relaxed);
    format!("bound-{label}-{n}-{:?}", std::thread::current().id())
}

async fn seed(volume: &dyn cmdr_fs::volume::Volume, path: &str, bytes: &[u8]) {
    let path = Path::new(path);
    if volume.exists(path).await {
        volume.delete(path).await.expect("clear");
    }
    volume.create_file(path, bytes).await.expect("seed");
}

/// The whole chain, on the route an approved SMB or MTP group actually takes: a binding
/// captured at review time, a source rewritten while the operation waited, and the engine
/// noticing at admission rather than at approval.
///
/// Until this existed, "does the deferred actually call the pre-flight?" rested on the
/// compiler and a grep. The unit tests prove the binding refuses a changed file; only this
/// proves `copy_between_volumes` asks it before it writes.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_bound_cross_volume_copy_skips_the_source_that_changed_while_it_waited() {
    use cmdr_fs::volume::{InMemoryVolume, Volume};

    let source: Arc<dyn Volume> = Arc::new(
        InMemoryVolume::new("Share")
            .with_lane_key(unique_lane("src"))
            .with_space_info(1_000_000, 900_000),
    );
    let dest: Arc<dyn Volume> = Arc::new(
        InMemoryVolume::new("Phone")
            .with_lane_key(unique_lane("dst"))
            .with_space_info(1_000_000, 900_000),
    );
    dest.create_directory(Path::new("/incoming"))
        .await
        .expect("seed the destination folder");
    seed(source.as_ref(), "/holiday.jpg", b"as reviewed").await;
    seed(source.as_ref(), "/invoice.pdf", b"as reviewed too").await;

    // What preflight saw, captured live through the volume that owns these paths.
    let reviewed = PathBuf::from("/holiday.jpg");
    let stale = PathBuf::from("/invoice.pdf");
    let mut entries = Vec::new();
    for path in [&reviewed, &stale] {
        entries.push((
            path.clone(),
            SourceFingerprint::capture_remote(source.as_ref(), path)
                .await
                .expect("capture"),
        ));
    }
    let expected = ExpectedSources::new(entries);

    // Somebody rewrites one of them while the operation sits in the queue.
    seed(source.as_ref(), "/invoice.pdf", b"edited while it waited for its lane").await;

    let collector = Arc::new(CollectorEventSink::new());
    let events: Arc<dyn OperationEventSink> = collector.clone();
    super::transfer::volume::copy_between_volumes(
        events,
        "share".to_string(),
        Arc::clone(&source),
        vec![reviewed.clone(), stale.clone()],
        "phone".to_string(),
        Arc::clone(&dest),
        PathBuf::from("/incoming"),
        super::types::VolumeCopyConfig::default(),
        Initiator::Agent,
        Some(expected),
    )
    .await
    .expect("the copy starts");

    crate::test_support::wait_until_async(Duration::from_secs(10), "the bound copy to settle", || {
        !collector.settled.lock_ignore_poison().is_empty()
    })
    .await;

    let verdicts: Vec<_> = collector
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .map(|item| (item.source_path.clone(), item.outcome))
        .collect();
    assert!(
        verdicts.contains(&(stale.display().to_string(), super::types::SourceItemOutcome::Skipped)),
        "the rewritten source must be reported skipped, got {verdicts:?}"
    );
    let errors: Vec<_> = collector
        .errors
        .lock_ignore_poison()
        .iter()
        .map(|e| format!("{:?}", e.error))
        .collect();
    assert!(
        dest.exists(Path::new("/incoming/holiday.jpg")).await,
        "the untouched source still copies: a binding filters, it does not refuse the batch. \
         verdicts={verdicts:?} errors={errors:?}"
    );
    assert!(
        !dest.exists(Path::new("/incoming/invoice.pdf")).await,
        "and the file the user never approved in this state is not written"
    );
}
