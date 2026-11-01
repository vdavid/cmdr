//! The live read's three layers: the gate over real temp dirs (links included),
//! and the bounded run with injected reads that are slow, stuck, or stoppable.

use std::os::unix::fs::symlink;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use cmdr_fs::testing::unique_test_id;
use tokio::sync::Notify;

use super::*;
use crate::test_support::{TestDir, wait_until_async};

/// A place the injected privacy rule guards: any path with this component.
const GUARDED: &str = "Guarded";

fn guarded(path: &Path) -> bool {
    path.components().any(|c| c.as_os_str() == GUARDED)
}

fn never(_: &Path) -> bool {
    false
}

/// The Full Disk Access question is open and `Guarded` folders are protected.
fn pending() -> GatePolicy {
    GatePolicy {
        fda_pending: true,
        is_protected: guarded,
        denial_is_privacy: never,
    }
}

/// The question is answered: the same folders are readable.
fn answered() -> GatePolicy {
    GatePolicy {
        fda_pending: false,
        ..pending()
    }
}

/// A temp dir spelled the way the kernel resolves it (`/var` is a link on macOS),
/// the form a mount root under `/Volumes` already has.
fn physical(dir: &TestDir) -> String {
    std::fs::canonicalize(dir)
        .expect("temp dir resolves")
        .to_string_lossy()
        .into_owned()
}

/// The real scanner policy, for a volume the index has never heard of: the junk
/// basenames and root pseudo-filesystems every volume skips.
fn scanner_policy() -> ScanSkips {
    Arc::new(|path: &str| index().scan_skips("live-gate-test", path))
}

fn target(root: &str, path: &str) -> LiveTarget {
    LiveTarget {
        root: root.to_string(),
        path: path.to_string(),
        others: Vec::new(),
        skips: scanner_policy(),
    }
}

// ── The gate ──────────────────────────────────────────────────────────────────

#[test]
fn a_folder_the_index_skips_is_refused_and_so_is_anything_inside_it() {
    let dir = TestDir::new("live-skipped");
    std::fs::create_dir_all(dir.join(".Spotlight-V100/Store-V2")).expect("dirs");
    let root = physical(&dir);
    for path in [
        format!("{root}/.Spotlight-V100"),
        format!("{root}/.Spotlight-V100/Store-V2"),
    ] {
        assert_eq!(
            gate_local(&target(&root, &path), &answered()),
            Err(LiveMiss::Excluded {
                name: ".Spotlight-V100".to_string()
            }),
            "{path}"
        );
    }
}

#[test]
fn a_folder_the_index_lists_is_listed_live_too_even_one_searches_hide() {
    // The index walks `node_modules` (only search hides it by default), so a live
    // read must answer for it the same way, or one tool answers two ways.
    let dir = TestDir::new("live-node-modules");
    std::fs::create_dir_all(dir.join("app/node_modules/react")).expect("dirs");
    let root = physical(&dir);
    let path = format!("{root}/app/node_modules");
    assert_eq!(gate_local(&target(&root, &path), &answered()), Ok(path));
}

#[test]
fn a_protected_folder_is_refused_while_the_disk_access_question_is_open() {
    let dir = TestDir::new("live-guarded");
    let root = physical(&dir);
    // It doesn't exist, so the refusal can only have come before a stat.
    let path = format!("{root}/{GUARDED}/taxes");
    assert_eq!(
        gate_local(&target(&root, &path), &pending()),
        Err(LiveMiss::NeedsDiskAccess(PrivacyGate::Pending))
    );
    // Answered, the same place is just missing.
    assert_eq!(gate_local(&target(&root, &path), &answered()), Err(LiveMiss::NotFound));
}

#[test]
fn a_link_into_a_protected_folder_is_refused_before_its_target_is_touched() {
    let volume = TestDir::new("live-link-guarded");
    let elsewhere = TestDir::new("live-link-guarded-target");
    let root = physical(&volume);
    // The link's target doesn't exist: reaching it at all would answer `NotFound`.
    symlink(elsewhere.join(GUARDED).join("secret"), volume.join("inbox")).expect("link");
    assert_eq!(
        gate_local(&target(&root, &format!("{root}/inbox")), &pending()),
        Err(LiveMiss::NeedsDiskAccess(PrivacyGate::Pending))
    );
}

#[test]
fn a_link_onto_another_volume_is_refused_and_names_where_it_lands() {
    let ours = TestDir::new("live-link-ours");
    let theirs = TestDir::new("live-link-theirs");
    std::fs::create_dir(theirs.join("photos")).expect("dir");
    symlink(theirs.join("photos"), ours.join("photos")).expect("link");
    let (root, their_root) = (physical(&ours), physical(&theirs));
    let other = OtherVolume {
        volume_id: "theirs".to_string(),
        name: "Their drive".to_string(),
        root: their_root.clone(),
    };
    let ask = LiveTarget {
        others: vec![other.clone()],
        ..target(&root, &format!("{root}/photos"))
    };
    assert_eq!(
        gate_local(&ask, &answered()),
        Err(LiveMiss::LeadsOffVolume {
            to: Some(other),
            path: format!("{their_root}/photos"),
        })
    );
    // With no known volume there, it's refused all the same.
    assert!(matches!(
        gate_local(&target(&root, &format!("{root}/photos")), &answered()),
        Err(LiveMiss::LeadsOffVolume { to: None, .. })
    ));
}

#[test]
fn a_link_on_the_boot_disk_onto_a_deeper_volume_is_refused() {
    // The boot disk's root holds every path, so "under our root" isn't enough: a
    // deeper live root is another volume's ground.
    let share = TestDir::new("live-deeper-share");
    let disk = TestDir::new("live-deeper-disk");
    symlink(&*share, disk.join("nas")).expect("link");
    let share_root = physical(&share);
    let ask = LiveTarget {
        others: vec![OtherVolume {
            volume_id: "nas".to_string(),
            name: "naspi".to_string(),
            root: share_root.clone(),
        }],
        ..target("/", &format!("{}/nas", physical(&disk)))
    };
    assert!(matches!(
        gate_local(&ask, &answered()),
        Err(LiveMiss::LeadsOffVolume { to: Some(OtherVolume { ref volume_id, .. }), .. }) if volume_id == "nas"
    ));
}

#[test]
fn a_link_inside_the_volume_lists_the_folder_it_points_to_under_the_volumes_own_root() {
    let dir = TestDir::new("live-link-inside");
    std::fs::create_dir_all(dir.join("papers/invoices")).expect("dirs");
    symlink("papers/invoices", dir.join("invoices")).expect("link");
    // The root as the volume spells it, NOT resolved: the answer keeps that spelling.
    let root = dir.to_string_lossy().into_owned();
    assert_eq!(
        gate_local(&target(&root, &format!("{root}/invoices")), &answered()),
        Ok(format!("{root}/papers/invoices"))
    );
}

#[test]
fn a_missing_folder_and_a_link_loop_are_not_found() {
    let dir = TestDir::new("live-missing");
    let root = physical(&dir);
    assert_eq!(
        gate_local(&target(&root, &format!("{root}/nope")), &answered()),
        Err(LiveMiss::NotFound)
    );
    symlink(dir.join("b"), dir.join("a")).expect("link");
    symlink(dir.join("a"), dir.join("b")).expect("link");
    assert_eq!(
        gate_local(&target(&root, &format!("{root}/a")), &answered()),
        Err(LiveMiss::NotFound)
    );
}

// ── The bounded run ───────────────────────────────────────────────────────────

/// Millisecond budgets, so a deadline test takes a moment.
const FAST: LiveReadConfig = LiveReadConfig {
    timeout: Duration::from_millis(150),
    cancel_grace: Duration::from_millis(50),
};

/// A read that never answers until the test releases it, ignoring its stop
/// token: a thread stuck in a kernel call. `returned` flips when it finally does.
struct Stuck {
    release: Arc<Notify>,
    returned: Arc<AtomicBool>,
}

impl Stuck {
    fn new() -> Self {
        Self {
            release: Arc::new(Notify::new()),
            returned: Arc::new(AtomicBool::new(false)),
        }
    }

    fn read(&self) -> impl FnOnce(CancellationToken) -> BoxedRead {
        let (release, returned) = (self.release.clone(), self.returned.clone());
        move |_stop| {
            Box::pin(async move {
                release.notified().await;
                returned.store(true, Ordering::SeqCst);
                Ok(Vec::new())
            })
        }
    }
}

type BoxedRead = std::pin::Pin<Box<dyn Future<Output = Result<Vec<FileEntry>, LiveMiss>> + Send>>;

#[tokio::test]
async fn a_read_past_its_deadline_is_abandoned_and_holds_its_slot_until_it_returns() {
    let volume_id = unique_test_id("live-deadline");
    let stuck = Stuck::new();
    let answer = run_bounded(&volume_id, FAST, &CancellationToken::new(), stuck.read()).await;
    assert_eq!(answer.err(), Some(LiveMiss::Unreachable(Stall::Deadline)));
    assert!(!stuck.returned.load(Ordering::SeqCst), "abandoned, not stopped");

    // The thread is still parked, so the volume's slot is too.
    assert!(IN_FLIGHT.lock_ignore_poison().contains(&volume_id));
    stuck.release.notify_one();
    wait_until_async(Duration::from_secs(5), "the abandoned read to free its slot", || {
        !IN_FLIGHT.lock_ignore_poison().contains(&volume_id)
    })
    .await;
}

#[tokio::test]
async fn a_second_read_while_the_first_is_stuck_answers_at_once() {
    let volume_id = unique_test_id("live-busy");
    let stuck = Stuck::new();
    let first = run_bounded(&volume_id, FAST, &CancellationToken::new(), stuck.read()).await;
    assert_eq!(first.err(), Some(LiveMiss::Unreachable(Stall::Deadline)));

    let started = Instant::now();
    let never_runs = |_stop| -> BoxedRead { panic!("a second read must not start behind a stuck one") };
    let second = run_bounded(&volume_id, FAST, &CancellationToken::new(), never_runs).await;
    assert_eq!(second.err(), Some(LiveMiss::Unreachable(Stall::Busy)));
    assert!(started.elapsed() < FAST.timeout, "busy answers without waiting");
    stuck.release.notify_one();
}

#[tokio::test]
async fn a_read_that_stops_when_asked_still_reads_as_the_deadline() {
    let volume_id = unique_test_id("live-stoppable");
    let stoppable = |stop: CancellationToken| -> BoxedRead {
        Box::pin(async move {
            stop.cancelled().await;
            Err(LiveMiss::Cancelled)
        })
    };
    let answer = run_bounded(&volume_id, FAST, &CancellationToken::new(), stoppable).await;
    assert_eq!(answer.err(), Some(LiveMiss::Unreachable(Stall::Deadline)));
    wait_until_async(Duration::from_secs(5), "the stopped read to free its slot", || {
        !IN_FLIGHT.lock_ignore_poison().contains(&volume_id)
    })
    .await;
}

#[tokio::test]
async fn stop_answers_cancelled_mid_read_and_the_slot_frees_when_the_read_returns() {
    let volume_id = unique_test_id("live-cancel");
    let stuck = Stuck::new();
    let turn = CancellationToken::new();
    let budget = LiveReadConfig {
        timeout: Duration::from_secs(30),
        ..FAST
    };
    let stopper = turn.clone();
    tokio::spawn(async move { stopper.cancel() });
    let started = Instant::now();
    let answer = run_bounded(&volume_id, budget, &turn, stuck.read()).await;
    assert_eq!(answer.err(), Some(LiveMiss::Cancelled));
    assert!(started.elapsed() < budget.timeout, "Stop doesn't wait out the read");
    assert!(
        IN_FLIGHT.lock_ignore_poison().contains(&volume_id),
        "the read is still parked"
    );

    stuck.release.notify_one();
    wait_until_async(Duration::from_secs(5), "the cancelled read to free its slot", || {
        !IN_FLIGHT.lock_ignore_poison().contains(&volume_id)
    })
    .await;
}

#[tokio::test]
async fn dropping_the_call_tells_the_read_to_stop() {
    let volume_id = unique_test_id("live-drop");
    let told = Arc::new(AtomicBool::new(false));
    let seen = told.clone();
    let read = move |stop: CancellationToken| -> BoxedRead {
        Box::pin(async move {
            stop.cancelled().await;
            seen.store(true, Ordering::SeqCst);
            Err(LiveMiss::Cancelled)
        })
    };
    let turn = CancellationToken::new();
    let budget = LiveReadConfig {
        timeout: Duration::from_secs(30),
        ..FAST
    };
    // An AI client hanging up drops the call mid-read.
    let call = run_bounded(&volume_id, budget, &turn, read);
    assert!(tokio::time::timeout(Duration::from_millis(20), call).await.is_err());
    wait_until_async(
        Duration::from_secs(5),
        "the dropped call's read to be told to stop",
        || told.load(Ordering::SeqCst),
    )
    .await;
}

#[tokio::test]
async fn a_read_that_answers_in_time_is_the_answer() {
    let volume_id = unique_test_id("live-ok");
    let quick = |_stop| -> BoxedRead { Box::pin(async { Err(LiveMiss::NotFound) }) };
    let answer = run_bounded(&volume_id, FAST, &CancellationToken::new(), quick).await;
    assert_eq!(answer.err(), Some(LiveMiss::NotFound));
}
