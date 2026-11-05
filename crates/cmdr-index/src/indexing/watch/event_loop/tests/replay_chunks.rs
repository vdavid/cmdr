//! The cold-start replay commits in chunks (`replay_chunks.rs`), and what a reader
//! sees between two chunks is exactly what a crash at that moment leaves on disk.
//! So these pin the chunk boundary from a separate read connection: rows and the
//! journal position land together at a drain, nothing past it does, and a replay
//! that resumes from there ends with every change applied once.

use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use cmdr_fs::testing::wait_until_async;

use super::*;
use crate::RescanReason;
use crate::indexing::hold::VolumeWork;
use crate::indexing::lifecycle::cover;
use crate::indexing::store::REPLAY_DEFERRED_RESCANS_KEY;
use crate::indexing::watch::event_loop::ReplayChunks;
use crate::indexing::watch::event_loop::replay_cost::ReplayBudget;
use crate::indexing::writer::IndexWriter;

const SINCE: u64 = 4_000_000_000;
const WAIT: Duration = Duration::from_secs(5);

/// Three paths to a window, two to a commit: the window's drain lands mid-way
/// through a commit's worth, so both kinds of boundary sit inside a handful of
/// events.
const SMALL: ReplayChunks = ReplayChunks {
    window_paths: 3,
    commit_every_paths: 2,
};

/// A volume mounted at a temp dir, its index, and its writer.
struct Fixture {
    mount: tempfile::TempDir,
    db: tempfile::TempDir,
    writer: IndexWriter,
}

impl Fixture {
    fn new() -> Self {
        let mount = tempfile::tempdir().expect("mount dir");
        let db = tempfile::tempdir().expect("db dir");
        IndexStore::open(&db.path().join("index.db")).expect("open store");
        let writer =
            IndexWriter::spawn(&db.path().join("index.db"), crate::NoopEventSink::shared()).expect("spawn writer");
        Self { mount, db, writer }
    }

    fn space(&self) -> IndexPathSpace {
        IndexPathSpace::mount_rooted(self.mount.path().to_string_lossy().into_owned())
    }

    fn path(&self, name: &str) -> PathBuf {
        self.mount.path().join(name)
    }

    fn file(&self, name: &str) -> PathBuf {
        let path = self.path(name);
        std::fs::write(&path, name).expect("write file");
        path
    }

    fn reader(&self) -> Connection {
        IndexStore::open_read_connection(&self.db.path().join("index.db")).expect("read connection")
    }

    /// What a crash right now would leave: the committed `last_event_id`.
    fn stored_position(&self) -> Option<u64> {
        IndexStore::get_meta(&self.reader(), "last_event_id")
            .expect("readable")
            .map(|id| id.parse().expect("an id"))
    }

    fn stored_rescans(&self) -> Option<Vec<String>> {
        IndexStore::get_meta(&self.reader(), REPLAY_DEFERRED_RESCANS_KEY)
            .expect("readable")
            .map(|stored| serde_json::from_str(&stored).expect("a JSON list"))
    }

    fn is_indexed(&self, name: &str) -> bool {
        self.space()
            .resolve_abs(&self.reader(), &self.path(name).to_string_lossy())
            .expect("readable")
            .is_some()
    }

    fn rows_named(&self, name: &str) -> i64 {
        self.reader()
            .query_row("SELECT COUNT(*) FROM entries WHERE name = ?1", [name], |row| row.get(0))
            .expect("count")
    }

    fn replay(&self, volume_id: &str, since: u64, chunks: ReplayChunks, budget: ReplayBudget) -> Replay {
        let (sender, watched) = tokio::sync::mpsc::unbounded_channel();
        let (fallback_tx, fallback) = tokio::sync::oneshot::channel();
        let work = VolumeWork::for_test(volume_id);
        let task = crate::indexing::host::runtime::spawn(run_replay_event_loop(
            watched,
            self.writer.clone(),
            crate::NoopEventSink::shared(),
            ReplayConfig {
                volume_id: volume_id.to_string(),
                space: self.space(),
                since_event_id: since,
                estimated_total: None,
                heal_after_replay: false,
                work: work.clone(),
                ground: cover::Claim::take(
                    volume_id,
                    vec![self.mount.path().to_string_lossy().into_owned()],
                    cover::Holder::Rewriting,
                ),
                budget,
                chunks,
            },
            fallback_tx,
            None,
            Arc::new(AtomicBool::new(true)),
        ));
        Replay {
            sender: Some(sender),
            task,
            fallback,
            _work: work,
        }
    }
}

struct Replay {
    sender: Option<tokio::sync::mpsc::UnboundedSender<watcher::FsChangeEvent>>,
    task: tokio::task::JoinHandle<Result<(), String>>,
    fallback: tokio::sync::oneshot::Receiver<RescanReason>,
    _work: VolumeWork,
}

impl Replay {
    fn send(&self, path: &Path, event_id: u64, flags: watcher::FsEventFlags) {
        self.sender
            .as_ref()
            .expect("the stream is open")
            .send(make_event(&path.to_string_lossy(), event_id, flags))
            .expect("send");
    }

    fn created(&self, path: &Path, event_id: u64) {
        self.send(
            path,
            event_id,
            watcher::FsEventFlags {
                item_created: true,
                item_is_file: true,
                ..Default::default()
            },
        );
    }

    fn history_done(&self, root: &Path, event_id: u64) {
        self.send(
            root,
            event_id,
            watcher::FsEventFlags {
                history_done: true,
                ..Default::default()
            },
        );
    }

    /// The stream closes, the way it does when the volume stops mid-replay, and the
    /// loop ends.
    async fn close(mut self) {
        self.sender.take();
        self.task
            .await
            .expect("the replay task ends")
            .expect("a closed stream is an ordinary exit");
    }
}

/// The chunk boundary, seen from outside: a drained window's rows and its journal
/// position become visible together while the replay is still running, and nothing
/// still in the next window does. Then the stream closes before `HistoryDone` (a
/// quit, a pause for disk space): the position must stay at the drain, because the
/// undrained event was never processed. It used to commit the last RECEIVED id, so
/// a quit mid-replay skipped up to a whole window of changes for good.
#[tokio::test(flavor = "multi_thread")]
async fn a_drained_window_commits_its_rows_with_its_position_and_nothing_past_it() {
    let fx = Fixture::new();
    let [a, b, c, d] = ["a.txt", "b.txt", "c.txt", "d.txt"].map(|name| fx.file(name));
    let replay = fx.replay("chunks-boundary", SINCE, SMALL, ReplayBudget::with_limit(u64::MAX));

    replay.created(&a, SINCE + 1);
    replay.created(&b, SINCE + 2);
    replay.created(&c, SINCE + 3); // the window holds three paths: it drains here
    replay.created(&d, SINCE + 4); // the next window's first path

    wait_until_async(WAIT, "the first window's commit", || {
        fx.stored_position() == Some(SINCE + 3)
    })
    .await;
    assert!(
        fx.is_indexed("a.txt") && fx.is_indexed("b.txt") && fx.is_indexed("c.txt"),
        "the drained window's rows commit with its position"
    );
    assert!(
        !fx.is_indexed("d.txt"),
        "a path still in the window is not processed yet"
    );

    replay.close().await;
    fx.writer.flush().await.expect("flush");
    assert_eq!(
        fx.stored_position(),
        Some(SINCE + 3),
        "a replay that stops before HistoryDone keeps the position of its last drain"
    );
    assert!(!fx.is_indexed("d.txt"));
    fx.writer.shutdown();
}

/// The resume half: a replay that starts from the stored position finishes the job,
/// and a path the journal delivers again (FSEvents re-sends from `sinceWhen`, and a
/// crash mid-window means a window processed twice) lands as the same row, not a
/// second one.
#[tokio::test(flavor = "multi_thread")]
async fn a_replay_that_resumes_from_the_last_chunk_applies_every_change_once() {
    let fx = Fixture::new();
    let [a, b, c, d] = ["a.txt", "b.txt", "c.txt", "d.txt"].map(|name| fx.file(name));

    let first = fx.replay("chunks-resume", SINCE, SMALL, ReplayBudget::with_limit(u64::MAX));
    first.created(&a, SINCE + 1);
    first.created(&b, SINCE + 2);
    first.created(&c, SINCE + 3);
    first.created(&d, SINCE + 4);
    wait_until_async(WAIT, "the first window's commit", || {
        fx.stored_position() == Some(SINCE + 3)
    })
    .await;
    first.close().await;

    let resumed = fx.replay("chunks-resume", SINCE + 3, SMALL, ReplayBudget::with_limit(u64::MAX));
    resumed.created(&c, SINCE + 3); // delivered again
    resumed.created(&d, SINCE + 4);
    resumed.history_done(fx.mount.path(), SINCE + 5);
    wait_until_async(WAIT, "the resumed replay's final commit", || {
        fx.stored_position() == Some(SINCE + 5)
    })
    .await;
    resumed.close().await;

    for name in ["a.txt", "b.txt", "c.txt", "d.txt"] {
        assert_eq!(fx.rows_named(name), 1, "{name} is indexed exactly once");
    }
    fx.writer.shutdown();
}

/// A `MustScanSubDirs` the replay has seen runs only after the replay, so once a
/// chunk moves the position past its event, the rescan exists nowhere else but in
/// the store. It has to commit WITH that position, survive the replay stopping,
/// and come off the disk once a replay finishes and hands it to the live drain.
#[tokio::test(flavor = "multi_thread")]
async fn a_deferred_rescan_commits_with_the_position_that_passes_its_event() {
    let fx = Fixture::new();
    let [a, b] = ["a.txt", "b.txt"].map(|name| fx.file(name));
    let churned = fx.path("churned");
    let two_paths = ReplayChunks {
        window_paths: 2,
        commit_every_paths: 1,
    };

    let first = fx.replay("chunks-rescan", SINCE, two_paths, ReplayBudget::with_limit(u64::MAX));
    first.send(
        &churned,
        SINCE + 1,
        watcher::FsEventFlags {
            must_scan_sub_dirs: true,
            ..Default::default()
        },
    );
    first.created(&a, SINCE + 2);
    first.created(&b, SINCE + 3);
    wait_until_async(WAIT, "the window that passes the rescan's event", || {
        fx.stored_position() == Some(SINCE + 3)
    })
    .await;
    assert_eq!(
        fx.stored_rescans(),
        Some(vec!["/churned".to_string()]),
        "the rescan is on disk in the same commit as the position past its event"
    );
    first.close().await;
    assert_eq!(
        fx.stored_rescans(),
        Some(vec!["/churned".to_string()]),
        "a stop keeps it"
    );

    let resumed = fx.replay(
        "chunks-rescan",
        SINCE + 3,
        two_paths,
        ReplayBudget::with_limit(u64::MAX),
    );
    resumed.history_done(fx.mount.path(), SINCE + 4);
    wait_until_async(WAIT, "the resumed replay's final commit", || {
        fx.stored_position() == Some(SINCE + 4)
    })
    .await;
    assert_eq!(
        fx.stored_rescans(),
        None,
        "a finished replay hands it to the live drain"
    );
    resumed.close().await;
    fx.writer.shutdown();
}

/// A replay that gives up for a full walk owes no rescan: the walk reads all of it.
/// Left on disk, the hint would send the next replay re-walking a subtree the walk
/// already brought current.
#[tokio::test(flavor = "multi_thread")]
async fn a_replay_that_falls_back_to_a_walk_drops_its_deferred_rescans() {
    let fx = Fixture::new();
    let [a, b] = ["a.txt", "b.txt"].map(|name| fx.file(name));
    let two_paths = ReplayChunks {
        window_paths: 2,
        commit_every_paths: 1,
    };
    let first = fx.replay("chunks-fallback", SINCE, two_paths, ReplayBudget::with_limit(u64::MAX));
    first.send(
        &fx.path("churned"),
        SINCE + 1,
        watcher::FsEventFlags {
            must_scan_sub_dirs: true,
            ..Default::default()
        },
    );
    first.created(&a, SINCE + 2);
    first.created(&b, SINCE + 3);
    wait_until_async(WAIT, "the rescan on disk", || fx.stored_rescans().is_some()).await;
    first.close().await;

    let mut over_budget = fx.replay("chunks-fallback", SINCE + 3, two_paths, ReplayBudget::with_limit(0));
    over_budget.created(&a, SINCE + 4);
    assert_eq!(
        (&mut over_budget.fallback).await.expect("the replay asks for a walk"),
        RescanReason::ReplayOverflow
    );
    over_budget.close().await;
    fx.writer.flush().await.expect("flush");
    assert_eq!(fx.stored_rescans(), None);
    fx.writer.shutdown();
}
