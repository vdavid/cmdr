//! A queued operation whose sources left while it waited asks before carrying on
//! (ERR-962JD). Each case queues a real local move behind a lane holder on an
//! engine of its own, changes the sources while it waits, and lets it in.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::oneshot;

use super::super::event_sinks::{CollectorEventSink, OperationEventSink};
use super::super::manager::{OperationDescriptor, OperationSummaryText};
use super::super::testing::TestOperations;
use super::super::types::{
    LifecycleStatus, SourceItemOutcome, SourcesGoneAnswerOutcome, SourcesGoneNext, WriteOperationConfig,
    WriteOperationError, WriteOperationType,
};
use super::kept_after_queue_on;
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::testing::{TestDir, wait_until_async};
use cmdr_fs::volume::{InMemoryVolume, LaneKey, Volume};
use cmdr_operation_log::types::{Initiator, OperationId};

const WAIT: Duration = Duration::from_secs(5);

/// A running operation that owns `lane` until dropped, so whatever registers on
/// the same lane after it waits `Queued`.
struct LaneHolder {
    release: Option<oneshot::Sender<()>>,
}

impl LaneHolder {
    fn hold(engine: &TestOperations, lane: &LaneKey) -> Self {
        let id = OperationId::from("holder");
        let (release, released) = oneshot::channel::<()>();
        let settle_id = id.clone();
        engine.manager().spawn_managed(
            OperationDescriptor {
                operation_id: id.clone(),
                operation_type: WriteOperationType::Copy,
                lanes: vec![lane.clone()],
                volume_ids: vec![],
                summary: OperationSummaryText::default(),
                rollback_support: crate::types::RollbackSupport::Supported,
                preview_id: None,
                reverses: None,
            },
            engine.new_state(),
            Box::new(move |ops: crate::Operations| {
                Box::pin(async move {
                    let _ = released.await;
                    ops.manager().on_settled(&settle_id);
                })
            }),
        );
        assert_eq!(engine.manager().lifecycle_status(&id), Some(LifecycleStatus::Running));
        Self { release: Some(release) }
    }

    fn release(&mut self) {
        if let Some(release) = self.release.take() {
            let _ = release.send(());
        }
    }
}

impl Drop for LaneHolder {
    fn drop(&mut self) {
        self.release();
    }
}

/// A source folder holding `names` (each file's bytes are its own name) and an
/// empty destination beside it.
struct Fixture {
    src: TestDir,
    dst: TestDir,
    sources: Vec<PathBuf>,
}

fn fixture(tag: &str, names: &[&str]) -> Fixture {
    let src = TestDir::new(&format!("queued_src_{tag}"));
    let dst = TestDir::new(&format!("queued_dst_{tag}"));
    let sources = names
        .iter()
        .map(|name| {
            let path = src.join(name);
            std::fs::write(&path, name.as_bytes()).expect("seed a source");
            path
        })
        .collect();
    Fixture { src, dst, sources }
}

/// Starts a move of every fixture source on `lane`, returning its id.
async fn start_move(
    engine: &TestOperations,
    collector: &Arc<CollectorEventSink>,
    fx: &Fixture,
    lane: &LaneKey,
) -> String {
    let events: Arc<dyn OperationEventSink> = collector.clone();
    engine
        .move_files_start(
            events,
            fx.sources.clone(),
            fx.dst.to_path_buf(),
            WriteOperationConfig::default(),
            vec![],
            Some(vec![lane.clone()]),
            Initiator::User,
            None,
            None,
        )
        .await
        .expect("the move registers")
        .operation_id
}

async fn settled(collector: &CollectorEventSink) {
    wait_until_async(WAIT, "the operation to settle", || {
        !collector.settled.lock_ignore_poison().is_empty()
    })
    .await;
}

fn names_in(dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .expect("read the folder")
        .flatten()
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_queued_move_whose_sources_partly_left_asks_then_moves_the_rest() {
    let engine = TestOperations::new();
    let fx = fixture("partly", &["a.mp3", "b.mp3", "c.mp3"]);
    let lane = LaneKey::new("lane");
    let mut holder = LaneHolder::hold(&engine, &lane);
    let collector = Arc::new(CollectorEventSink::new());
    let op_id = OperationId::from(start_move(&engine, &collector, &fx, &lane).await);
    assert_eq!(engine.manager().lifecycle_status(&op_id), Some(LifecycleStatus::Queued));

    // The person moves one of the files away while the move waits its turn.
    std::fs::remove_file(&fx.sources[1]).expect("move b away");
    holder.release();

    wait_until_async(WAIT, "the move to ask about its sources", || {
        engine.pending_sources_gone(&op_id).is_some()
    })
    .await;
    let asked = engine.pending_sources_gone(&op_id).expect("it is asking");
    assert_eq!(asked.next, SourcesGoneNext::AwaitingAnswer);
    assert_eq!((asked.gone_count, asked.total_count), (1, 3), "an honest count: 1 of 3");
    assert_eq!(asked.gone_sample, vec![fx.sources[1].display().to_string()]);
    assert_eq!(
        collector.sources_gone.lock_ignore_poison().clone(),
        vec![asked],
        "the question went out once"
    );
    assert!(names_in(&fx.dst).is_empty(), "nothing moves before the answer");

    assert_eq!(
        engine.continue_without_gone_sources(&op_id),
        SourcesGoneAnswerOutcome::Continued
    );
    settled(&collector).await;

    assert!(
        collector.errors.lock_ignore_poison().is_empty(),
        "no failure: the rest moved"
    );
    assert_eq!(collector.complete.lock_ignore_poison().len(), 1);
    assert_eq!(names_in(&fx.dst), vec!["a.mp3", "c.mp3"]);
    let gone_item = collector
        .source_items_done
        .lock_ignore_poison()
        .iter()
        .find(|item| item.source_path == fx.sources[1].display().to_string())
        .cloned()
        .expect("the gone source gets its own verdict");
    assert_eq!(gone_item.outcome, SourceItemOutcome::Skipped);
    assert!(gone_item.source_removed, "it really is gone from where the move looked");
    drop(fx.src);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn cancelling_a_queued_move_while_it_asks_ends_it_as_cancelled() {
    let engine = TestOperations::new();
    let fx = fixture("cancel", &["a.mp3", "b.mp3"]);
    let lane = LaneKey::new("lane");
    let mut holder = LaneHolder::hold(&engine, &lane);
    let collector = Arc::new(CollectorEventSink::new());
    let op_id = OperationId::from(start_move(&engine, &collector, &fx, &lane).await);

    std::fs::remove_file(&fx.sources[0]).expect("move a away");
    holder.release();
    wait_until_async(WAIT, "the move to ask", || {
        engine.pending_sources_gone(&op_id).is_some()
    })
    .await;

    engine.cancel_operation(&op_id);
    settled(&collector).await;

    assert_eq!(
        collector.cancelled.lock_ignore_poison().len(),
        1,
        "a cancel, not a failure"
    );
    assert!(collector.errors.lock_ignore_poison().is_empty());
    assert!(collector.complete.lock_ignore_poison().is_empty());
    assert!(names_in(&fx.dst).is_empty(), "nothing moved");
    assert!(fx.sources[1].exists(), "the source still there stays put");
    assert_eq!(
        engine.continue_without_gone_sources(&op_id),
        SourcesGoneAnswerOutcome::UnknownOperation,
        "a late answer finds nothing to carry on"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_queued_move_whose_sources_all_left_completes_and_says_why() {
    let engine = TestOperations::new();
    let fx = fixture("all", &["a.mp3", "b.mp3"]);
    let lane = LaneKey::new("lane");
    let mut holder = LaneHolder::hold(&engine, &lane);
    let collector = Arc::new(CollectorEventSink::new());
    let op_id = OperationId::from(start_move(&engine, &collector, &fx, &lane).await);

    for source in &fx.sources {
        std::fs::remove_file(source).expect("move it away");
    }
    holder.release();
    settled(&collector).await;

    assert!(
        collector.errors.lock_ignore_poison().is_empty(),
        "no error tone: nothing went wrong"
    );
    let notices = collector.sources_gone.lock_ignore_poison().clone();
    assert_eq!(notices.len(), 1);
    assert_eq!(notices[0].next, SourcesGoneNext::NothingLeft);
    assert_eq!((notices[0].gone_count, notices[0].total_count), (2, 2));
    let complete = collector.complete.lock_ignore_poison();
    assert_eq!(complete.len(), 1, "it completes with nothing to do");
    assert_eq!((complete[0].files_processed, complete[0].files_skipped), (0, 2));
    assert_eq!(engine.pending_sources_gone(&op_id), None, "and asks nobody anything");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_move_admitted_at_once_is_not_checked() {
    let engine = TestOperations::new();
    let fx = fixture("unqueued", &["a.mp3"]);
    std::fs::remove_file(&fx.sources[0]).expect("remove the source before the move starts");
    let collector = Arc::new(CollectorEventSink::new());
    // A fresh lane: nothing ahead, so it never waited.
    start_move(&engine, &collector, &fx, &LaneKey::new("lane")).await;
    settled(&collector).await;

    assert!(
        collector.sources_gone.lock_ignore_poison().is_empty(),
        "nothing to ask about"
    );
    let errors = collector.errors.lock_ignore_poison();
    assert!(
        matches!(
            errors.first().map(|e| &e.error),
            Some(WriteOperationError::SourceNotFound { .. })
        ),
        "the engine's own typed refusal stands: {errors:?}"
    );
}

#[test]
fn an_answer_to_an_operation_that_is_not_asking_changes_nothing() {
    let engine = TestOperations::new();
    let op = engine.register("not-asking");
    assert_eq!(
        engine.continue_without_gone_sources(op.id()),
        SourcesGoneAnswerOutcome::NotAsking
    );
    assert_eq!(
        engine.continue_without_gone_sources(&OperationId::from("never-registered")),
        SourcesGoneAnswerOutcome::UnknownOperation
    );
}

/// The volume probe counts only a POSITIVE absence: a source whose stat fails
/// for another reason is left for the engine, so a dead connection can never
/// read as "all your files were moved".
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn on_a_volume_only_a_source_that_is_really_missing_counts_as_gone() {
    let engine = TestOperations::new();
    let volume = InMemoryVolume::new("queued-sources-vol");
    for name in ["/kept.txt", "/unreadable.txt"] {
        volume.create_file(Path::new(name), b"x").await.expect("seed");
    }
    volume.set_stat_failing(Path::new("/unreadable.txt"));
    let sources = vec![
        PathBuf::from("/kept.txt"),
        PathBuf::from("/missing.txt"),
        PathBuf::from("/unreadable.txt"),
    ];
    let op = engine.register("volume-probe");
    op.state().note_waited_in_queue();
    let collector = CollectorEventSink::new();

    let op_id = op.id().clone();
    let check = kept_after_queue_on(
        &volume as &dyn Volume,
        &collector,
        op_id.as_str(),
        WriteOperationType::Copy,
        op.state(),
        &sources,
    );
    let answer = async {
        wait_until_async(WAIT, "the copy to ask", || {
            engine.pending_sources_gone(&op_id).is_some()
        })
        .await;
        let asked = engine.pending_sources_gone(&op_id).expect("asking");
        assert_eq!((asked.gone_count, asked.total_count), (1, 3));
        assert_eq!(
            engine.continue_without_gone_sources(&op_id),
            SourcesGoneAnswerOutcome::Continued
        );
    };
    let (kept, ()) = tokio::join!(check, answer);
    assert_eq!(
        kept,
        Some(vec![0, 2]),
        "the unreadable source stays for the engine to report"
    );
}
