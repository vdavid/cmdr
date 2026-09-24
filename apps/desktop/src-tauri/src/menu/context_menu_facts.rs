//! The file context menu's SLOW facts (macOS): everything the menu asks the disk, a
//! provider, or LaunchServices about the right-clicked rows, gathered off the main thread.
//!
//! The menu used to ask all of it on the main thread before it popped, and on a network
//! share every answer is a round trip: a sample of the running app on an SMB share
//! (2026-09-23, five right-clicks) spent 3.4 s in the Google Drive xattr read, 2.1 s in the
//! Share enumeration, and 1.0 s in the Finder-tag reads, so each menu took about 1.3 s to
//! appear, and once 10 s. Now each fact is a job on a bounded pool of 8 MB-stack threads
//! ([`crate::file_system::framework_pool`]), the menu waits at most [`GRACE`] for them, and
//! builds with whatever answered. Every later answer goes to the menu while it's open
//! (`context_menu_live.rs`), and one that lands after it closed is dropped.
//!
//! The core ([`start_with`], [`Gathering::collect`]) knows nothing about AppKit or the
//! pool, so the grace-then-late routing is unit-tested with plain threads.

use std::collections::BTreeSet;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, LazyLock, Mutex};
use std::time::{Duration, Instant};

use cmdr_fs::ignore_poison::IgnorePoison;

use super::file_context_menu::{FileContextInfo, Slow};
use crate::file_system::file_provider_actions::ProviderOffer;
use crate::file_system::framework_pool::{Pool, PoolConfig};
use crate::file_system::google_drive::DriveItemLinks;
use crate::file_system::open_with::OpenWithChoices;
use crate::file_system::share::ShareOffer;
use crate::file_system::sync_status::SyncStatus;

/// How long a right-click waits for the slow facts before the menu goes up with whatever
/// answered. On a local disk every fact answers well inside it, so the menu is complete on
/// its first frame; on a slow mount the rest fill in while the menu is open.
pub const GRACE: Duration = Duration::from_millis(100);

/// How long the iCloud sync-status read may take. The menu no longer waits on it, so this
/// only bounds how long a stuck provider holds a pool worker.
const SYNC_STATUS_BUDGET: Duration = Duration::from_millis(500);

/// How long File Provider may take to name the rows' actions. Same story: the menu shows the
/// group whenever it arrives, so this bounds the worker, not the menu.
const FILE_PROVIDER_ACTIONS_BUDGET: Duration = Duration::from_secs(1);

/// The menu's own pool, separate from sync status's so a provider wedging one can't starve
/// the other. Up to six jobs per right-click run at once; the ceiling bounds the threads a
/// mount that stops answering can hold for the process's life.
const POOL: PoolConfig = PoolConfig {
    name: "menu_facts",
    target_workers: 6,
    max_workers: 24,
    wedged_after: Duration::from_secs(10),
};

static FACT_POOL: LazyLock<Pool> = LazyLock::new(|| Pool::new(POOL));

/// Which fact an answer is.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum FactKind {
    SyncStatus,
    DriveLinks,
    ProviderOffer,
    OpenWith,
    Share,
    Tags,
}

/// One slow fact's answer.
pub enum Fact {
    /// The primary row's iCloud sync status, which picks the eviction item.
    SyncStatus(SyncStatus),
    /// The primary row's Google Drive links, `None` when it isn't a Drive item.
    DriveLinks(Option<DriveItemLinks>),
    /// The rows' File Provider actions, `None` when their provider offers none.
    ProviderOffer(Option<ProviderOffer>),
    /// The apps that can open every row.
    OpenWith(OpenWithChoices),
    /// What macOS offers to share the rows with, `None` when no row makes a file URL.
    Share(Option<ShareOffer>),
    /// Which of the seven Finder tag colors every row carries, indexed by color.
    Tags([bool; 8]),
}

impl Fact {
    pub fn kind(&self) -> FactKind {
        match self {
            Fact::SyncStatus(_) => FactKind::SyncStatus,
            Fact::DriveLinks(_) => FactKind::DriveLinks,
            Fact::ProviderOffer(_) => FactKind::ProviderOffer,
            Fact::OpenWith(_) => FactKind::OpenWith,
            Fact::Share(_) => FactKind::Share,
            Fact::Tags(_) => FactKind::Tags,
        }
    }
}

/// What the right-click asks about. Which facts it needs follows from it
/// ([`FactsRequest::kinds`]), so a fact the menu can't show is never asked.
pub struct FactsRequest {
    /// The right-clicked row, which drives the single-row facts (sync status, Drive).
    pub primary: PathBuf,
    /// Every row the menu acts on.
    pub paths: Vec<PathBuf>,
    pub is_directory: bool,
    /// iCloud Drive only: the eviction pair is the one thing sync status picks between.
    pub is_icloud_drive: bool,
    /// Whether the rows are real OS paths a share service can take.
    pub can_share: bool,
}

impl FactsRequest {
    /// The facts this menu can use. "Open with" is a file-only submenu, `Share` needs OS
    /// paths, and sync status matters only in iCloud Drive.
    ///
    /// On sync status: the probe is provider-agnostic and answers correctly for third-party
    /// providers too (a streamed Google Drive file carries `SF_DATALESS` like any other stub,
    /// verified 2026-09-07 with `stat -f %Sf`). It's skipped off iCloud because the only
    /// thing it picks between is the eviction pair, which is iCloud-only; widening the menu,
    /// not this call, is what a third-party provider would need.
    pub fn kinds(&self) -> BTreeSet<FactKind> {
        let mut kinds = BTreeSet::from([FactKind::DriveLinks, FactKind::ProviderOffer, FactKind::Tags]);
        if self.is_icloud_drive {
            kinds.insert(FactKind::SyncStatus);
        }
        if !self.is_directory {
            kinds.insert(FactKind::OpenWith);
        }
        if self.can_share {
            kinds.insert(FactKind::Share);
        }
        kinds
    }
}

/// A job: work that answers one fact.
type FactJob = Box<dyn FnOnce() -> Fact + Send + 'static>;

/// Where an answer goes once the menu has stopped waiting.
type LateSink = Arc<dyn Fn(Fact) + Send + Sync + 'static>;

/// Facts in flight for one right-click.
pub struct Gathering {
    shared: Arc<Shared>,
}

struct Shared {
    state: Mutex<State>,
    arrived: Condvar,
    /// Set once the menu closed: jobs still queued skip their work, and late answers drop.
    cancelled: AtomicBool,
}

struct State {
    /// Asked and not yet answered.
    pending: BTreeSet<FactKind>,
    /// Answered while the menu was still waiting.
    ready: Vec<Fact>,
    /// `None` while the menu waits; the late route once it stopped.
    late: Option<LateSink>,
}

/// Cancels a gathering's unfinished work when the menu that asked is gone.
#[derive(Clone)]
pub struct CancelGathering(Arc<Shared>);

impl CancelGathering {
    pub fn cancel(&self) {
        self.0.cancelled.store(true, Ordering::Release);
    }
}

/// What the menu has to build with when it stops waiting.
pub struct Collected {
    /// Every fact that answered in time.
    pub ready: Vec<Fact>,
    /// The facts still out, each answered later through the late route (or never, if the
    /// menu closes first).
    pub pending: BTreeSet<FactKind>,
    pub cancel: CancelGathering,
}

/// Starts every job `request` needs on the menu's pool.
pub fn start(request: FactsRequest) -> Gathering {
    start_with(jobs_for(request), |job| FACT_POOL.submit(job))
}

/// Starts `jobs` through `submit`, which runs each one somewhere other than the caller.
fn start_with(jobs: Vec<(FactKind, FactJob)>, submit: impl Fn(Box<dyn FnOnce() + Send + 'static>)) -> Gathering {
    let shared = Arc::new(Shared {
        state: Mutex::new(State {
            pending: jobs.iter().map(|(kind, _)| *kind).collect(),
            ready: Vec::new(),
            late: None,
        }),
        arrived: Condvar::new(),
        cancelled: AtomicBool::new(false),
    });
    for (_, job) in jobs {
        let shared = Arc::clone(&shared);
        submit(Box::new(move || {
            // A job queued behind a mount that stopped answering may only start after its
            // menu closed; its answer would go nowhere, so it doesn't ask at all.
            if shared.cancelled.load(Ordering::Acquire) {
                return;
            }
            let fact = job();
            deliver(&shared, fact);
        }));
    }
    Gathering { shared }
}

/// Routes one answer: to the waiting menu, or down the late route once it stopped waiting.
fn deliver(shared: &Shared, fact: Fact) {
    let mut state = shared.state.lock_ignore_poison();
    state.pending.remove(&fact.kind());
    match state.late.clone() {
        None => {
            state.ready.push(fact);
            drop(state);
            shared.arrived.notify_all();
        }
        Some(late) => {
            drop(state);
            if !shared.cancelled.load(Ordering::Acquire) {
                late(fact);
            }
        }
    }
}

impl Gathering {
    /// Waits until every fact answered or `deadline` passed, whichever comes first, and hands
    /// every answer after that to `late`, on the thread that produced it.
    pub fn collect(self, deadline: Instant, late: impl Fn(Fact) + Send + Sync + 'static) -> Collected {
        let mut state = self.shared.state.lock_ignore_poison();
        while !state.pending.is_empty() {
            let Some(left) = deadline.checked_duration_since(Instant::now()) else {
                break;
            };
            state = self
                .shared
                .arrived
                .wait_timeout(state, left)
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .0;
        }
        // Flipped under the same lock the jobs deliver under, so every answer lands in
        // exactly one place: `ready` before this line, the late route after it.
        state.late = Some(Arc::new(late));
        Collected {
            ready: std::mem::take(&mut state.ready),
            pending: state.pending.clone(),
            cancel: CancelGathering(Arc::clone(&self.shared)),
        }
    }
}

/// What the menu builds from: every answered fact in place, every pending one marked
/// [`Slow::Pending`], and every fact nobody asked for at its empty answer. Also hands back
/// the `Share` offer when it answered, for the caller to arm on the main thread.
pub fn file_context_info(
    is_icloud_drive: bool,
    ready: Vec<Fact>,
    pending: &BTreeSet<FactKind>,
) -> (FileContextInfo, Option<ShareOffer>) {
    let mut info = FileContextInfo {
        is_icloud_drive,
        ..FileContextInfo::default()
    };
    for kind in pending {
        match kind {
            FactKind::SyncStatus => info.sync_status = Slow::Pending,
            FactKind::DriveLinks => info.google_drive_links = Slow::Pending,
            FactKind::ProviderOffer => info.file_provider_offer = Slow::Pending,
            FactKind::OpenWith => info.open_with = Slow::Pending,
            FactKind::Share => info.share_services = Slow::Pending,
            FactKind::Tags => info.applied_tag_colors = Slow::Pending,
        }
    }
    let mut share_offer = None;
    for fact in ready {
        match fact {
            Fact::SyncStatus(status) => info.sync_status = Slow::Ready(status),
            Fact::DriveLinks(links) => info.google_drive_links = Slow::Ready(links),
            Fact::ProviderOffer(offer) => info.file_provider_offer = Slow::Ready(offer),
            Fact::OpenWith(choices) => info.open_with = Slow::Ready(choices),
            Fact::Share(offer) => {
                let services = offer
                    .as_ref()
                    .map(|offer| offer.services().to_vec())
                    .unwrap_or_default();
                info.share_services = Slow::Ready(services);
                share_offer = offer;
            }
            Fact::Tags(applied) => info.applied_tag_colors = Slow::Ready(applied),
        }
    }
    (info, share_offer)
}

/// The production jobs for `request`, one per fact it needs.
fn jobs_for(request: FactsRequest) -> Vec<(FactKind, FactJob)> {
    let FactsRequest {
        primary,
        paths,
        is_directory,
        ..
    } = &request;
    let kinds = request.kinds();
    let mut jobs: Vec<(FactKind, FactJob)> = Vec::new();
    for kind in kinds {
        let primary = primary.clone();
        let paths = paths.clone();
        let is_directory = *is_directory;
        let job: FactJob = match kind {
            FactKind::SyncStatus => Box::new(move || {
                Fact::SyncStatus(crate::file_system::sync_status::status_within_blocking(
                    &primary.to_string_lossy(),
                    SYNC_STATUS_BUDGET,
                ))
            }),
            FactKind::DriveLinks => {
                Box::new(move || Fact::DriveLinks(crate::file_system::google_drive::item_links(&primary, is_directory)))
            }
            FactKind::ProviderOffer => Box::new(move || {
                Fact::ProviderOffer(crate::file_system::file_provider_actions::offer_for(
                    &paths,
                    FILE_PROVIDER_ACTIONS_BUDGET,
                ))
            }),
            FactKind::OpenWith => {
                Box::new(move || Fact::OpenWith(crate::file_system::open_with::compute_open_with_choices(paths)))
            }
            FactKind::Share => Box::new(move || Fact::Share(crate::file_system::share::enumerate_offer(&paths))),
            FactKind::Tags => Box::new(move || {
                // Which colors the WHOLE selection carries: each path's tags read once, and a
                // color counts only when every path has it.
                let per_path: Vec<_> = paths
                    .iter()
                    .map(|path| crate::file_system::tags::read_tags(path))
                    .collect();
                Fact::Tags(crate::file_system::tags::applied_colors(&per_path))
            }),
        };
        jobs.push((kind, job));
    }
    jobs
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::mpsc;

    const LONG: Duration = Duration::from_secs(5);

    /// Runs each job on a thread of its own, like the pool does.
    fn on_threads(job: Box<dyn FnOnce() + Send + 'static>) {
        std::thread::spawn(job);
    }

    fn quick(kind: FactKind) -> (FactKind, FactJob) {
        (kind, Box::new(move || fact_of(kind)))
    }

    /// A job that answers only once `release` fires.
    fn held(kind: FactKind) -> ((FactKind, FactJob), mpsc::Sender<()>) {
        let (release, wait) = mpsc::channel::<()>();
        let job: FactJob = Box::new(move || {
            let _ = wait.recv();
            fact_of(kind)
        });
        ((kind, job), release)
    }

    fn fact_of(kind: FactKind) -> Fact {
        match kind {
            FactKind::SyncStatus => Fact::SyncStatus(SyncStatus::Synced),
            FactKind::DriveLinks => Fact::DriveLinks(None),
            FactKind::ProviderOffer => Fact::ProviderOffer(None),
            FactKind::OpenWith => Fact::OpenWith(OpenWithChoices::default()),
            FactKind::Share => Fact::Share(None),
            FactKind::Tags => Fact::Tags([false; 8]),
        }
    }

    fn kinds(facts: &[Fact]) -> BTreeSet<FactKind> {
        facts.iter().map(Fact::kind).collect()
    }

    #[test]
    fn when_every_fact_answers_in_time_the_menu_gets_them_all_and_nothing_is_late() {
        let gathering = start_with(vec![quick(FactKind::Tags), quick(FactKind::DriveLinks)], on_threads);
        let (late_tx, late_rx) = mpsc::channel();
        let collected = gathering.collect(Instant::now() + LONG, move |fact| {
            let _ = late_tx.send(fact.kind());
        });
        assert_eq!(
            kinds(&collected.ready),
            BTreeSet::from([FactKind::Tags, FactKind::DriveLinks])
        );
        assert!(collected.pending.is_empty());
        assert!(
            late_rx.try_recv().is_err(),
            "a fact that answered in time must not also arrive late"
        );
    }

    /// The menu stops waiting at the deadline, builds with what answered, and the slow
    /// fact reaches the late route when it finally answers.
    #[test]
    fn a_slow_fact_misses_the_deadline_and_then_arrives_through_the_late_route() {
        let (slow, release) = held(FactKind::Share);
        let gathering = start_with(vec![quick(FactKind::Tags), slow], on_threads);
        let (late_tx, late_rx) = mpsc::channel();
        let started = Instant::now();
        let collected = gathering.collect(started + Duration::from_millis(50), move |fact| {
            let _ = late_tx.send(fact.kind());
        });
        assert!(
            started.elapsed() < LONG,
            "the wait must end at the deadline, not when the slow fact answers"
        );
        assert_eq!(kinds(&collected.ready), BTreeSet::from([FactKind::Tags]));
        assert_eq!(collected.pending, BTreeSet::from([FactKind::Share]));
        release.send(()).expect("the held job is waiting");
        assert_eq!(late_rx.recv_timeout(LONG), Ok(FactKind::Share));
    }

    /// Once the menu closed, a late answer goes nowhere, and a job that hadn't started yet
    /// doesn't do its work at all.
    #[test]
    fn a_cancelled_gathering_drops_late_answers_and_skips_queued_work() {
        let (slow, release) = held(FactKind::ProviderOffer);
        let gathering = start_with(vec![slow], on_threads);
        let (late_tx, late_rx) = mpsc::channel();
        let collected = gathering.collect(Instant::now(), move |fact| {
            let _ = late_tx.send(fact.kind());
        });
        collected.cancel.cancel();
        release.send(()).expect("the held job is waiting");
        assert!(late_rx.recv_timeout(Duration::from_millis(200)).is_err());

        // A job that only starts after the cancel never runs.
        let ran = Arc::new(AtomicBool::new(false));
        let ran_in_job = Arc::clone(&ran);
        let (queued, run_queued) = mpsc::channel::<Box<dyn FnOnce() + Send>>();
        let gathering = start_with(
            vec![(
                FactKind::Tags,
                Box::new(move || {
                    ran_in_job.store(true, Ordering::SeqCst);
                    Fact::Tags([false; 8])
                }),
            )],
            |job| queued.send(job).expect("the queue is open"),
        );
        let collected = gathering.collect(Instant::now(), |_| {});
        collected.cancel.cancel();
        (run_queued.recv().expect("one job was queued"))();
        assert!(!ran.load(Ordering::SeqCst), "work for a closed menu must be skipped");
    }

    /// The menu builds a pending fact's part to be filled in later, an answered one in place,
    /// and one nobody asked about at its empty answer (a folder's "Open with" stays empty).
    #[test]
    fn the_info_marks_pending_facts_fills_answered_ones_and_leaves_the_rest_empty() {
        let mut applied = [false; 8];
        applied[3] = true;
        let (info, share_offer) = file_context_info(
            true,
            vec![Fact::Tags(applied), Fact::SyncStatus(SyncStatus::OnlineOnly)],
            &BTreeSet::from([FactKind::Share, FactKind::ProviderOffer]),
        );
        assert!(info.is_icloud_drive);
        assert_eq!(info.applied_tag_colors.ready(), Some(&applied));
        assert_eq!(info.sync_status.ready(), Some(&SyncStatus::OnlineOnly));
        assert!(info.share_services.is_pending());
        assert!(info.file_provider_offer.is_pending());
        assert!(share_offer.is_none(), "a pending Share has no offer to arm yet");
        // Never asked: answered, and empty.
        assert!(
            info.open_with
                .ready()
                .is_some_and(|choices| choices.candidates.is_empty())
        );
        assert_eq!(info.google_drive_links.ready(), Some(&None));
    }

    #[test]
    fn a_right_click_asks_only_for_the_facts_its_menu_can_show() {
        let request = |is_directory, is_icloud_drive, can_share| FactsRequest {
            primary: PathBuf::from("/x"),
            paths: vec![PathBuf::from("/x")],
            is_directory,
            is_icloud_drive,
            can_share,
        };
        // A folder on a phone: no Open with, no Share, no eviction pair.
        assert_eq!(
            request(true, false, false).kinds(),
            BTreeSet::from([FactKind::DriveLinks, FactKind::ProviderOffer, FactKind::Tags])
        );
        // A file in iCloud Drive: everything.
        assert_eq!(
            request(false, true, true).kinds(),
            BTreeSet::from([
                FactKind::SyncStatus,
                FactKind::DriveLinks,
                FactKind::ProviderOffer,
                FactKind::OpenWith,
                FactKind::Share,
                FactKind::Tags,
            ])
        );
    }
}
