//! A transfer that names its items one by one (`TransferLeaves::per_source`,
//! `transfer/DETAILS.md` § Named destinations) on the VOLUME engines, through
//! the routed starts: the serial and concurrent copy drivers, the cross-volume
//! move, the same-volume move (by rename, and by copy where renames copy), from
//! the local disk onto a remote volume, and within the local volume. What lands
//! where, a clash at a renamed leaf, every refusal, a source pruned after
//! admission, and undo. The native engines' half is `named_leaves_tests.rs`.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use super::event_sinks::{CollectorEventSink, OperationEventSink};
use super::manager::{OperationDescriptor, OperationSummaryText};
use super::types::{
    ConflictResolution, ConflictResolutionOutcome, LifecycleStatus, NamedLeavesRefusal, SourcesGoneAnswerOutcome,
    VolumeCopyConfig, WriteOperationError, WriteOperationStartResult, WriteOperationType,
};
use crate::rollback::Reversal;
use crate::testing::{TestOperationLog, TestOperations};
use crate::types::TransferLeaves;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::{TestDir, wait_until_async};
use cmdr_fs::volume::{InMemoryVolume, LaneKey, Volume};
use cmdr_operation_log::rollback::{execute_rollback, rollback_operation};
use cmdr_operation_log::types::{Initiator, OperationId};

const WAIT: Duration = Duration::from_secs(10);

/// Where a per-source map runs on a volume engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Route {
    /// Between two remote volumes; three or more sources take the concurrent
    /// driver, fewer the serial one.
    CopyAcross,
    MoveAcross,
    /// Within one remote volume: `Volume::rename` per item.
    MoveWithin,
    /// Within one remote volume whose renames copy (an object store): copy, then delete.
    MoveWithinByCopy,
    /// From the local disk onto a remote volume: filing onto a NAS.
    CopyUp,
    MoveUp,
    /// Within the local volume through `start_volume_move`, which runs on the
    /// volume engine (`Volume::rename`), not the native move.
    MoveWithinLocal,
}

const ROUTES: [Route; 7] = [
    Route::CopyAcross,
    Route::MoveAcross,
    Route::MoveWithin,
    Route::MoveWithinByCopy,
    Route::CopyUp,
    Route::MoveUp,
    Route::MoveWithinLocal,
];

impl Route {
    fn moves(self) -> bool {
        !matches!(self, Route::CopyAcross | Route::CopyUp)
    }
}

fn in_memory(name: &str, lane: Option<&str>) -> InMemoryVolume {
    let volume = InMemoryVolume::new(name).with_space_info(10_000_000, 9_000_000);
    match lane {
        Some(lane) => volume.with_lane_key(lane),
        None => volume,
    }
}

/// One route's two ends, registered on an engine journaling into its own log,
/// with a source folder and a destination folder in each volume's own paths.
struct Rig {
    engine: TestOperations,
    log: TestOperationLog,
    route: Route,
    source: Arc<dyn Volume>,
    dest: Arc<dyn Volume>,
    source_id: &'static str,
    dest_id: &'static str,
    src_dir: PathBuf,
    dest_dir: PathBuf,
    _tmp: TestDir,
}

impl Rig {
    async fn new(route: Route) -> Self {
        Self::on_lane(route, None).await
    }

    /// Remote volumes on `lane`, so a test can hold the operation in the queue.
    async fn on_lane(route: Route, lane: Option<&str>) -> Self {
        let log = TestOperationLog::new("named-leaves-volume");
        let engine = TestOperations::journaling_into(&log);
        let tmp = TestDir::new("named_leaves_volume");
        let local: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
            "Test root",
            "/",
            crate::host::testing::pane_order_host(),
        ));
        engine.volumes().register("root", Arc::clone(&local));
        let mem: Arc<dyn Volume> = Arc::new(match route {
            // An object store's double: no rename, and every write publishes whole.
            Route::MoveWithinByCopy => in_memory("Mem", lane).with_renames_by_copy().with_whole_publish(),
            _ => in_memory("Mem", lane),
        });
        let other: Arc<dyn Volume> = Arc::new(in_memory("Other", lane));
        engine.volumes().register("mem", Arc::clone(&mem));
        engine.volumes().register("other", Arc::clone(&other));
        let (source, source_id, dest, dest_id) = match route {
            Route::CopyAcross | Route::MoveAcross => (mem, "mem", other, "other"),
            Route::MoveWithin | Route::MoveWithinByCopy => (Arc::clone(&mem), "mem", mem, "mem"),
            Route::CopyUp | Route::MoveUp => (local, "root", mem, "mem"),
            Route::MoveWithinLocal => (Arc::clone(&local), "root", local, "root"),
        };
        let local_source = source.local_path().is_some();
        let local_dest = dest.local_path().is_some();
        let src_dir = if local_source {
            tmp.join("src")
        } else {
            PathBuf::from("/src")
        };
        let dest_dir = if local_dest {
            tmp.join("dst")
        } else {
            PathBuf::from("/dst")
        };
        source.create_directory_all(&src_dir).await.expect("the source folder");
        dest.create_directory_all(&dest_dir)
            .await
            .expect("the destination folder");
        Self {
            engine,
            log,
            route,
            source,
            dest,
            source_id,
            dest_id,
            src_dir,
            dest_dir,
            _tmp: tmp,
        }
    }

    /// A source file at `rel` under the source folder.
    async fn file(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        put(&self.source, &self.src_dir.join(rel), bytes).await
    }

    /// A file already at `rel` under the destination folder.
    async fn dest_file(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        put(&self.dest, &self.dest_dir.join(rel), bytes).await
    }

    /// A source file at `rel` under the DESTINATION folder's path, on the source
    /// volume: within one volume, a source already in the destination.
    async fn file_at_dest_path(&self, rel: &str, bytes: &[u8]) -> PathBuf {
        put(&self.source, &self.dest_dir.join(rel), bytes).await
    }

    async fn dest_names(&self) -> Vec<String> {
        names_on(&self.dest, &self.dest_dir).await
    }

    async fn read_dest(&self, rel: &str) -> Vec<u8> {
        read_all(&self.dest, &self.dest_dir.join(rel)).await
    }

    async fn start(
        &self,
        events: Arc<dyn OperationEventSink>,
        sources: Vec<PathBuf>,
        leaves: TransferLeaves,
        policy: ConflictResolution,
    ) -> Result<WriteOperationStartResult, WriteOperationError> {
        let config = VolumeCopyConfig {
            leaves,
            conflict_resolution: policy,
            progress_interval_ms: 0,
            ..VolumeCopyConfig::default()
        };
        let (from, to) = (self.source_id.to_string(), self.dest_id.to_string());
        let dest = self.dest_dir.display().to_string();
        if self.route.moves() {
            self.engine
                .start_volume_move(events, from, sources, to, dest, config, Initiator::User, None)
                .await
        } else {
            self.engine
                .start_volume_copy(events, from, sources, to, dest, config, Initiator::User, None)
                .await
        }
    }

    /// Starts and waits for the operation to settle without an error.
    async fn run(&self, sources: Vec<PathBuf>, leaves: TransferLeaves) -> String {
        let events = Arc::new(CollectorEventSink::new());
        let started = self
            .start(events.clone(), sources, leaves, ConflictResolution::Stop)
            .await
            .unwrap_or_else(|e| panic!("{:?}: the named transfer starts: {e:?}", self.route));
        settled(&events, self.route).await;
        assert_eq!(
            events.complete.lock_ignore_poison().len(),
            1,
            "{:?}: one operation",
            self.route
        );
        started.operation_id
    }

    /// Undoes `op_id` the way the app does: the gate decides, then the engine runs.
    async fn undo(&self, op_id: &str) {
        let writer = self.log.writer();
        let vm = self.engine.volumes();
        let plan = rollback_operation(vm.as_ref(), writer, op_id, |_plan| Ok(()))
            .unwrap_or_else(|refusal| panic!("{:?}: the named transfer reverses: {refusal:?}", self.route));
        let reversal = Reversal::on(&self.engine, "named-leaves-volume-undo");
        let report = execute_rollback(
            vm.as_ref(),
            writer,
            &plan.original,
            &plan.inverse_op_id,
            Initiator::User,
            reversal.runner(),
        )
        .await;
        assert_eq!(report.skipped, 0, "{:?}: nothing skipped, got {report:?}", self.route);
    }
}

async fn put(volume: &Arc<dyn Volume>, path: &Path, bytes: &[u8]) -> PathBuf {
    let parent = path.parent().expect("a parent");
    volume.create_directory_all(parent).await.expect("parents");
    volume.create_file(path, bytes).await.expect("a fixture file");
    path.to_path_buf()
}

async fn names_on(volume: &Arc<dyn Volume>, dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = volume
        .list_directory(dir, None)
        .await
        .unwrap_or_else(|e| panic!("list {}: {e:?}", dir.display()))
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    names.sort();
    names
}

async fn read_all(volume: &Arc<dyn Volume>, path: &Path) -> Vec<u8> {
    let mut reader = volume
        .open_read_stream(path)
        .await
        .unwrap_or_else(|e| panic!("open {}: {e:?}", path.display()));
    let mut bytes = Vec::new();
    while let Some(chunk) = reader.next_chunk().await {
        bytes.extend(chunk.unwrap_or_else(|e| panic!("read {}: {e:?}", path.display())));
    }
    bytes
}

async fn settled(events: &CollectorEventSink, route: Route) {
    wait_until_async(WAIT, "the operation to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(
        events.errors.lock_ignore_poison().is_empty(),
        "{route:?}: the operation must succeed: {:?}",
        events.errors.lock_ignore_poison()
    );
}

fn names(pairs: &[(&Path, &str)]) -> TransferLeaves {
    TransferLeaves::per_source(pairs.iter().map(|(p, n)| (p.to_path_buf(), (*n).to_string())))
}

// ============================================================================
// Landing
// ============================================================================

/// Three sources, two renamed (a file and a folder) and one keeping its name,
/// land as one operation on every volume route; the folder's children and an
/// empty subfolder land under its new name. Three sources take the concurrent
/// copy driver; two, the serial one.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn sources_renamed_and_kept_land_named_on_every_volume_route() {
    for route in ROUTES {
        for with_kept in [true, false] {
            let rig = Rig::new(route).await;
            let invoice = rig.file("Invoice-X.pdf", b"invoice").await;
            let receipt = rig.file("receipt.pdf", b"receipt").await;
            let folder = rig.src_dir.join("scans");
            rig.file("scans/page.png", b"page").await;
            rig.source
                .create_directory_all(&folder.join("empty"))
                .await
                .expect("an empty subfolder");
            let leaves = names(&[
                (&invoice, "2026-09-27 OpenAI invoice Testco.pdf"),
                (&folder, "2026-09 scans"),
            ]);
            let mut sources = vec![invoice.clone(), folder.clone()];
            if with_kept {
                sources.push(receipt.clone());
            }
            let ctx = format!("{route:?} with_kept={with_kept}");

            rig.run(sources, leaves).await;

            let mut want = vec!["2026-09 scans", "2026-09-27 OpenAI invoice Testco.pdf"];
            if with_kept {
                want.push("receipt.pdf");
            }
            assert_eq!(rig.dest_names().await, want, "{ctx}");
            assert_eq!(
                rig.read_dest("2026-09-27 OpenAI invoice Testco.pdf").await,
                b"invoice",
                "{ctx}"
            );
            assert_eq!(rig.read_dest("2026-09 scans/page.png").await, b"page", "{ctx}");
            assert_eq!(
                names_on(&rig.dest, &rig.dest_dir.join("2026-09 scans")).await,
                vec!["empty", "page.png"],
                "{ctx}: the empty subfolder lands named too"
            );
            assert_eq!(
                rig.source.exists(&invoice).await,
                !route.moves(),
                "{ctx}: a move takes the original, a copy keeps it"
            );
            assert_eq!(rig.source.exists(&folder).await, !route.moves(), "{ctx}");
        }
    }
}

/// A source renamed to another source's ORIGINAL name, while that one is
/// renamed onward, lands each where it was asked.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_taken_from_another_source_that_moves_on_lands_each_where_asked() {
    for route in ROUTES {
        let rig = Rig::new(route).await;
        let a = rig.src_dir.join("one/a");
        rig.file("one/a/from-a.txt", b"a").await;
        let b = rig.src_dir.join("two/b");
        rig.file("two/b/from-b.txt", b"b").await;

        rig.run(vec![a.clone(), b.clone()], names(&[(&a, "b"), (&b, "c")]))
            .await;

        assert_eq!(rig.dest_names().await, vec!["b", "c"], "{route:?}");
        assert_eq!(rig.read_dest("b/from-a.txt").await, b"a", "{route:?}");
        assert_eq!(rig.read_dest("c/from-b.txt").await, b"b", "{route:?}");
    }
}

/// Two sources sharing an ORIGINAL name, each named apart, land apart: the
/// volume engines key every landing on the source PATH, so nothing confuses
/// the two (the native engines, keyed on `<destination>/<own name>`, refuse
/// this in the started operation instead).
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_sources_sharing_an_original_name_land_apart_when_named_apart() {
    for route in ROUTES {
        let rig = Rig::new(route).await;
        let one = rig.file("one/scan.pdf", b"one").await;
        let two = rig.file("two/scan.pdf", b"two").await;

        rig.run(
            vec![one.clone(), two.clone()],
            names(&[(&one, "2026-01 scan.pdf"), (&two, "2026-02 scan.pdf")]),
        )
        .await;

        assert_eq!(
            rig.dest_names().await,
            vec!["2026-01 scan.pdf", "2026-02 scan.pdf"],
            "{route:?}"
        );
        assert_eq!(rig.read_dest("2026-01 scan.pdf").await, b"one", "{route:?}");
        assert_eq!(rig.read_dest("2026-02 scan.pdf").await, b"two", "{route:?}");
    }
}

/// `src/a.txt` lands as `named.txt`, where an older `named.txt` already sits,
/// beside a bystander under `a.txt`'s ORIGINAL name; the rest keep their names.
/// Every policy meets the clash at `named.txt`, and nothing touches `a.txt`.
/// Two sources and three, so a copy meets it on both drivers.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_clash_at_a_renamed_leaf_meets_the_policy_at_the_renamed_path() {
    // (configured policy, what a Stop prompt is answered with)
    let cases = [
        (ConflictResolution::Stop, ConflictResolution::Overwrite),
        (ConflictResolution::Stop, ConflictResolution::Skip),
        (ConflictResolution::Skip, ConflictResolution::Skip),
        (ConflictResolution::Overwrite, ConflictResolution::Overwrite),
    ];
    for route in ROUTES {
        for (policy, answer) in cases {
            for count in [2, 3] {
                let ctx = format!("{route:?} {policy:?}/{answer:?} sources={count}");
                let rig = Rig::new(route).await;
                let a = rig.file("a.txt", b"new a").await;
                let mut sources = vec![a.clone(), rig.file("b.txt", b"b").await];
                if count == 3 {
                    sources.push(rig.file("c.txt", b"c").await);
                }
                rig.dest_file("named.txt", b"old").await;
                rig.dest_file("a.txt", b"bystander").await;

                let events = Arc::new(CollectorEventSink::new());
                let started = rig
                    .start(events.clone(), sources, names(&[(&a, "named.txt")]), policy)
                    .await
                    .unwrap_or_else(|e| panic!("{ctx}: {e:?}"));
                if policy == ConflictResolution::Stop {
                    wait_until_async(WAIT, "the clash or the end", || {
                        !events.conflicts.lock_ignore_poison().is_empty()
                            || !events.settled.lock_ignore_poison().is_empty()
                    })
                    .await;
                    let prompt = events.conflicts.lock_ignore_poison().first().cloned();
                    let prompt = prompt.unwrap_or_else(|| panic!("{ctx}: the clash was never asked"));
                    assert_eq!(
                        Path::new(&prompt.destination_path),
                        rig.dest_dir.join("named.txt"),
                        "{ctx}: asked at the renamed path"
                    );
                    let outcome = rig.engine.resolve_write_conflict(
                        &OperationId::from(started.operation_id.clone()),
                        prompt.conflict_id,
                        answer,
                        false,
                    );
                    assert_eq!(outcome, ConflictResolutionOutcome::Resolved, "{ctx}");
                }
                settled(&events, route).await;
                let prompts = events.conflicts.lock_ignore_poison().len();
                let want_prompts = usize::from(policy == ConflictResolution::Stop);
                assert_eq!(prompts, want_prompts, "{ctx}: one prompt, and only when asked to stop");

                let kept_old = answer == ConflictResolution::Skip;
                let want: &[u8] = if kept_old { b"old" } else { b"new a" };
                assert_eq!(rig.read_dest("named.txt").await, want, "{ctx}");
                assert_eq!(rig.read_dest("a.txt").await, b"bystander", "{ctx}");
                assert_eq!(rig.read_dest("b.txt").await, b"b", "{ctx}: kept its name");
                assert_eq!(
                    rig.source.exists(&a).await,
                    !route.moves() || kept_old,
                    "{ctx}: a skipped move keeps its source"
                );
            }
        }
    }
}

/// A source removed while the operation waited in the queue is pruned after
/// admission: the rest land named, and its unused name is no error.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_source_pruned_after_admission_leaves_the_rest_landing_named() {
    for route in [Route::CopyAcross, Route::MoveAcross, Route::MoveWithin] {
        let rig = Rig::on_lane(route, Some("lane")).await;
        let a = rig.file("a.pdf", b"a").await;
        let b = rig.file("b.pdf", b"b").await;
        let c = rig.file("c.pdf", b"c").await;

        let (release, released) = tokio::sync::oneshot::channel::<()>();
        let holder = OperationId::from("holder");
        let settle_holder = holder.clone();
        rig.engine.manager().spawn_managed(
            OperationDescriptor {
                operation_id: holder,
                operation_type: WriteOperationType::Copy,
                lanes: vec![LaneKey::new("lane")],
                volume_ids: vec![],
                summary: OperationSummaryText::default(),
                rollback_support: crate::types::RollbackSupport::Supported,
                preview_id: None,
                reverses: None,
            },
            rig.engine.new_state(),
            Box::new(move |ops: crate::Operations| {
                Box::pin(async move {
                    let _ = released.await;
                    ops.manager().on_settled(&settle_holder);
                })
            }),
        );

        let events = Arc::new(CollectorEventSink::new());
        let leaves = names(&[(&a, "alpha.pdf"), (&b, "beta.pdf")]);
        let started = rig
            .start(events.clone(), vec![a, b.clone(), c], leaves, ConflictResolution::Stop)
            .await
            .unwrap_or_else(|e| panic!("{route:?}: admitted, queued: {e:?}"));
        let op_id = OperationId::from(started.operation_id);
        assert_eq!(
            rig.engine.manager().lifecycle_status(&op_id),
            Some(LifecycleStatus::Queued),
            "{route:?}"
        );

        // A renamed source leaves while the transfer waits its turn.
        rig.source.delete(&b).await.expect("b leaves");
        let _ = release.send(());
        wait_until_async(WAIT, "the transfer to ask about its sources", || {
            rig.engine.pending_sources_gone(&op_id).is_some()
        })
        .await;
        assert_eq!(
            rig.engine.continue_without_gone_sources(&op_id),
            SourcesGoneAnswerOutcome::Continued,
            "{route:?}"
        );
        settled(&events, route).await;

        assert_eq!(rig.dest_names().await, vec!["alpha.pdf", "c.pdf"], "{route:?}");
        assert_eq!(rig.read_dest("alpha.pdf").await, b"a", "{route:?}: landed named");
    }
}

// ============================================================================
// Refusals, all before anything is admitted
// ============================================================================

/// Runs `case` on each of `routes` and expects `want`, with nothing written.
/// `case` gets a fresh rig and returns the sources and names.
async fn refuses_on<F, Fut>(
    routes: &[Route],
    want: impl Fn(&Result<WriteOperationStartResult, WriteOperationError>) -> bool,
    case: F,
) where
    F: Fn(Arc<Rig>) -> Fut,
    Fut: Future<Output = (Vec<PathBuf>, TransferLeaves)>,
{
    for &route in routes {
        let rig = Arc::new(Rig::new(route).await);
        let (sources, leaves) = case(Arc::clone(&rig)).await;
        let dest_before = rig.dest_names().await;
        let events = Arc::new(CollectorEventSink::new());
        let result = rig
            .start(events, sources.clone(), leaves, ConflictResolution::Stop)
            .await;
        assert!(want(&result), "{route:?}: got {result:?}");
        assert_eq!(rig.dest_names().await, dest_before, "{route:?}: nothing landed");
        for source in &sources {
            assert!(rig.source.exists(source).await, "{route:?}: {} stays", source.display());
        }
    }
}

/// The routes whose source volume can hold a file at the destination folder's
/// path: every one but the local disk's onto a remote volume, whose destination
/// path would name a real folder on this machine.
const SOURCE_REACHES_DEST_PATH: [Route; 5] = [
    Route::CopyAcross,
    Route::MoveAcross,
    Route::MoveWithin,
    Route::MoveWithinByCopy,
    Route::MoveWithinLocal,
];

fn refused(want: NamedLeavesRefusal) -> impl Fn(&Result<WriteOperationStartResult, WriteOperationError>) -> bool {
    move |result| matches!(result, Err(WriteOperationError::NamedLeavesRefused { reason, .. }) if *reason == want)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn two_items_named_alike_refuse() {
    for (one, other) in [
        ("same.pdf", "same.pdf"),
        ("Invoice.pdf", "invoice.pdf"),
        ("R\u{e9}sum\u{e9}.pdf", "Re\u{301}sume\u{301}.pdf"),
    ] {
        refuses_on(&ROUTES, refused(NamedLeavesRefusal::SameName), |rig| async move {
            let a = rig.file("a.pdf", b"a").await;
            let b = rig.file("b.pdf", b"b").await;
            (vec![a.clone(), b.clone()], names(&[(&a, one), (&b, other)]))
        })
        .await;
    }
}

/// A rename can collide with an item that keeps its name just as well.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_equal_to_an_item_that_keeps_its_own_refuses() {
    refuses_on(&ROUTES, refused(NamedLeavesRefusal::SameName), |rig| async move {
        let a = rig.file("one/a.pdf", b"a").await;
        let b = rig.file("two/B.pdf", b"b").await;
        (vec![a.clone(), b], names(&[(&a, "b.pdf")]))
    })
    .await;
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_for_an_item_the_transfer_does_not_carry_refuses() {
    refuses_on(&ROUTES, refused(NamedLeavesRefusal::NotASource), |rig| async move {
        let a = rig.file("a.pdf", b"a").await;
        let ghost = rig.src_dir.join("ghost.pdf");
        (vec![a.clone()], names(&[(&a, "alpha.pdf"), (&ghost, "ghost.pdf")]))
    })
    .await;
}

/// Names go to OTHER folders: a source already in the destination makes the
/// whole group a rename, whether it's the renamed item or one keeping its name.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn names_into_a_folder_that_already_holds_a_source_refuse() {
    for renamed_is_inside in [true, false] {
        refuses_on(
            &SOURCE_REACHES_DEST_PATH,
            refused(NamedLeavesRefusal::AlreadyInDestination),
            |rig| async move {
                // Across two remote volumes the paths only coincide, which the
                // folded comparison refuses all the same: they could be one
                // share under two addresses, and a refusal costs a retry.
                let inside = rig.file_at_dest_path("inside.pdf", b"inside").await;
                let outside = rig.file("outside.pdf", b"outside").await;
                let renamed = if renamed_is_inside { &inside } else { &outside };
                let leaves = names(&[(renamed, "renamed.pdf")]);
                (vec![inside.clone(), outside.clone()], leaves)
            },
        )
        .await;
    }
}

/// The chain: `a` lands as `b`, itself a source renamed onward. In whichever
/// order it runs, one overwrites the other before it's read (a copy) or moved.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_chain_onto_another_source_refuses_for_move_and_copy() {
    refuses_on(
        &SOURCE_REACHES_DEST_PATH,
        refused(NamedLeavesRefusal::OntoASource),
        |rig| async move {
            let a = rig.file("a.txt", b"a").await;
            let b = rig.file_at_dest_path("b.txt", b"b").await;
            (vec![a.clone(), b.clone()], names(&[(&a, "b.txt"), (&b, "c.txt")]))
        },
    )
    .await;
}

/// A name that renders as something else, or holds something invisible, is a
/// spoof the review couldn't show.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_name_with_a_bidi_override_or_a_zero_width_joiner_refuses() {
    for leaf in [
        "invoice\u{202E}fdp.exe",
        "in\u{200D}voice.pdf",
        " padded.pdf",
        "a/b.pdf",
        "..",
    ] {
        refuses_on(
            &ROUTES,
            |r: &Result<WriteOperationStartResult, WriteOperationError>| {
                matches!(r, Err(WriteOperationError::InvalidName { .. }))
            },
            |rig| async move {
                let a = rig.file("a.pdf", b"a").await;
                (vec![a.clone()], names(&[(&a, leaf)]))
            },
        )
        .await;
    }
}

// ============================================================================
// Undo
// ============================================================================

/// One undo takes back the whole named transfer on every volume route: a copy's
/// named copies go (and only they: a bystander under an ORIGINAL name stays), a
/// move's items go home under their original names.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn undoing_a_transfer_naming_each_item_restores_every_original() {
    for route in ROUTES {
        for count in [2, 3] {
            let ctx = format!("{route:?} sources={count}");
            let rig = Rig::new(route).await;
            let invoice = rig.file("Invoice-X.pdf", b"invoice").await;
            let folder = rig.src_dir.join("scans");
            rig.file("scans/page.png", b"page").await;
            let mut sources = vec![invoice.clone(), folder.clone()];
            if count == 3 {
                sources.push(rig.file("receipt.pdf", b"receipt").await);
            }
            rig.dest_file("Invoice-X.pdf", b"bystander").await;
            let leaves = names(&[(&invoice, "2026-09-27 invoice.pdf"), (&folder, "2026-09 scans")]);

            let op_id = rig.run(sources.clone(), leaves).await;
            assert_eq!(rig.read_dest("2026-09-27 invoice.pdf").await, b"invoice", "{ctx}");

            rig.undo(&op_id).await;

            assert_eq!(
                rig.dest_names().await,
                vec!["Invoice-X.pdf"],
                "{ctx}: only the bystander stays"
            );
            assert_eq!(rig.read_dest("Invoice-X.pdf").await, b"bystander", "{ctx}");
            assert_eq!(
                read_all(&rig.source, &invoice).await,
                b"invoice",
                "{ctx}: the original name"
            );
            assert_eq!(
                read_all(&rig.source, &folder.join("page.png")).await,
                b"page",
                "{ctx}: the folder's original name"
            );
            for source in &sources {
                assert!(rig.source.exists(source).await, "{ctx}: {} is back", source.display());
            }
        }
    }
}
