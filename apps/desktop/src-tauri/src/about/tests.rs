//! The host end to end with plain threads: what the card's channel carries, in
//! what order, and what a cancel or a budget does to it.

use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

#[cfg(target_os = "macos")]
use cmdr_about::actions::{AboutAction, ActionCaps, RemoveBy};
use cmdr_about::card::{AboutFact, OsCaps};
use cmdr_about::fact::{AboutTarget, Fact, FactKind, UnknownWhy};
use cmdr_about::gather::{Job, Task};
#[cfg(target_os = "macos")]
use cmdr_about::provenance::{WebSite, WhereFrom};
use cmdr_about::text::ItemText;
use tokio_util::sync::CancellationToken;

use super::event::AboutUpdate;
use super::jobs::Sources;
use super::probe::{Probe, probe};
use super::route::{Way, local_gathered, reach};
use super::session::{AboutSink, Gathered, Submit, Timing, run};
use crate::test_support::{TestDir, wait_until, wait_until_async};
use crate::tools::resources::volumes::VolumeKind;

/// Far longer than any step here takes.
pub(super) const LONG: Duration = Duration::from_secs(10);

#[derive(Default)]
pub(super) struct Recorder(Mutex<Vec<AboutUpdate>>);

impl AboutSink for Recorder {
    fn send(&self, update: AboutUpdate) -> bool {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .push(update);
        true
    }
}

impl Recorder {
    pub(super) fn updates(&self) -> Vec<AboutUpdate> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    pub(super) fn facts(&self) -> Vec<AboutFact> {
        self.updates()
            .into_iter()
            .filter_map(|update| match update {
                AboutUpdate::Fact { fact, .. } => Some(fact),
                _ => None,
            })
            .collect()
    }
}

pub(super) fn on_threads() -> Submit {
    Arc::new(|task: Task| {
        std::thread::spawn(task);
    })
}

fn quick() -> Timing {
    Timing {
        grace: Duration::from_millis(100),
        row_budget: LONG,
        volume_budget: LONG,
    }
}

fn macos() -> OsCaps {
    OsCaps {
        use_history: true,
        code_signing: true,
        default_app: true,
        install_receipts: true,
    }
}

fn kinds(facts: &[AboutFact]) -> Vec<FactKind> {
    let mut kinds: Vec<FactKind> = facts.iter().map(AboutFact::kind).collect();
    kinds.sort();
    kinds
}

pub(super) async fn gather(gathered: Gathered, timing: Timing) -> Arc<Recorder> {
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn AboutSink> = Arc::clone(&recorder) as Arc<dyn AboutSink>;
    run(gathered, sink, CancellationToken::new(), on_threads(), timing).await;
    recorder
}

/// Every stream opens with the plan and closes with `factsSettled`, and every
/// fact in between has a row in that plan, exactly once. A volume that stopped
/// answering says so once, among them.
pub(super) fn assert_well_formed(updates: &[AboutUpdate]) {
    let Some(AboutUpdate::Planned { plan, .. }) = updates.first() else {
        panic!("the first event is the plan: {updates:?}");
    };
    assert!(matches!(updates.last(), Some(AboutUpdate::FactsSettled)), "{updates:?}");
    let mut seen = Vec::new();
    let mut stopped = 0;
    for update in &updates[1..updates.len() - 1] {
        if matches!(update, AboutUpdate::VolumeStoppedAnswering) {
            stopped += 1;
            assert_eq!(stopped, 1, "a volume stops answering once: {updates:?}");
            continue;
        }
        let AboutUpdate::Fact { fact, .. } = update else {
            panic!("only facts sit between the plan and the end: {update:?}");
        };
        assert!(plan.has(fact.kind()), "{:?} has no row", fact.kind());
        assert!(!seen.contains(&fact.kind()), "{:?} landed twice", fact.kind());
        seen.push(fact.kind());
    }
    let mut planned = plan.kinds();
    planned.sort();
    seen.sort();
    assert_eq!(seen, planned, "every planned row lands");
}

fn local(path: &std::path::Path) -> Gathered {
    let path = path.to_string_lossy().into_owned();
    let probed = probe(&path, None).expect("the fixture is there");
    local_gathered(
        &path,
        probed,
        ItemText::new("Macintosh HD"),
        super::platform::os_caps(),
        Sources::none(),
    )
}

// ── A real file on this disk ────────────────────────────────────────────────

/// What Chrome leaves on a download (synthesized, the spike's shape): the file's
/// address then the page's, and a quarantine naming Chrome and the time.
#[cfg(target_os = "macos")]
fn write_chrome_download(path: &std::path::Path) {
    let sources = plist::Value::Array(vec![
        plist::Value::String("https://dl.example.org/releases/tool-1.2.dmg".to_string()),
        plist::Value::String("https://example.org/download".to_string()),
    ]);
    let mut where_froms = Vec::new();
    sources.to_writer_binary(&mut where_froms).expect("a plist writes");
    xattr::set(path, "com.apple.metadata:kMDItemWhereFroms", &where_froms).expect("set WhereFroms");
    xattr::set(
        path,
        "com.apple.quarantine",
        b"0081;6af6442f;Chrome;6B1F2E8C-0000-4000-8000-000000000000",
    )
    .expect("set quarantine");
}

#[cfg(target_os = "macos")]
#[tokio::test]
async fn a_downloaded_disk_image_streams_the_plan_then_each_fact_then_settles() {
    let dir = TestDir::new("about-download");
    let file = dir.as_ref().join("tool-1.2.dmg");
    // Bytes Cmdr can't place (no trailer read, not text): its type stays `cantTell`, so
    // the footer is a plain download's.
    std::fs::write(&file, b"\0\x01not really a disk image").expect("write the fixture");
    write_chrome_download(&file);

    let caps = ActionCaps {
        file_manager: true,
        removal: Some(RemoveBy::Trash),
        app_folder: None,
        ask_cmdr: false,
    };
    let recorder = gather(
        Gathered {
            actions: caps,
            ..local(&file)
        },
        quick(),
    )
    .await;
    let updates = recorder.updates();
    assert_well_formed(&updates);
    // The footer comes with every landing, from the facts: the page the file was
    // downloaded from (the record's last address, never the file's own), and Move
    // to Trash last.
    let last_actions = updates.iter().rev().find_map(|update| match update {
        AboutUpdate::Fact { derived, .. } => Some(derived.actions.clone()),
        _ => None,
    });
    let last_actions = last_actions.expect("facts landed");
    let offered: Vec<AboutAction> = last_actions.shown.into_iter().chain(last_actions.more).collect();
    assert!(
        offered.iter().any(|action| matches!(
            action,
            AboutAction::OpenDownloadPage { host, url }
                if host == "example.org" && url == "https://example.org/download"
        )),
        "{offered:?}"
    );
    assert_eq!(offered.last(), Some(&AboutAction::MoveToTrash));
    let facts = recorder.facts();
    assert_eq!(
        kinds(&facts),
        [
            FactKind::Identity,
            FactKind::RealType,
            FactKind::Signature,
            FactKind::Download,
            FactKind::LastOpened,
            FactKind::DefaultApp,
        ]
    );
    let download = facts.iter().find_map(|fact| match fact {
        AboutFact::Download(Fact::Known(download)) => Some(download.clone()),
        _ => None,
    });
    let download = download.expect("the quarantine and WhereFroms read as a download");
    assert_eq!(download.downloaded_by, Some(ItemText::new("Chrome")));
    assert_eq!(download.downloaded_at, Some(0x6af6_442f));
    let Fact::Known(sources) = download.sources else {
        panic!("the sources decode");
    };
    assert!(
        sources.iter().any(
            |source| matches!(source, WhereFrom::Web { site: WebSite::Domain { host }, .. } if host == "dl.example.org")
        ),
        "{sources:?}"
    );
    // A file written a moment ago has no Spotlight use record: unknown, never "never".
    assert!(
        facts.iter().any(|fact| matches!(
            fact,
            AboutFact::LastOpened(Fact::Unknown(unknown)) if unknown.why() == UnknownWhy::NoRecord
        )),
        "{facts:?}"
    );
}

#[tokio::test]
async fn a_file_with_no_download_record_says_so_as_a_routine_absence() {
    let dir = TestDir::new("about-plain");
    let file = dir.as_ref().join("notes.txt");
    std::fs::write(&file, "plain words").expect("write the fixture");

    let recorder = gather(local(&file), quick()).await;
    assert_well_formed(&recorder.updates());
    let facts = recorder.facts();
    assert!(facts.iter().any(|fact| matches!(
        fact,
        AboutFact::Download(Fact::Unknown(unknown)) if unknown.why() == UnknownWhy::NoRecord
    )));
    assert!(
        !kinds(&facts).contains(&FactKind::Signature),
        "a text file plans no signature"
    );
    let identity = facts.iter().find_map(|fact| match fact {
        AboutFact::Identity(Fact::Known(identity)) => Some(identity.clone()),
        _ => None,
    });
    let identity = identity.expect("the probe answers identity");
    assert_eq!(identity.name, ItemText::new("notes.txt"));
    assert_eq!(identity.logical_size, Some(11));
    #[cfg(unix)]
    assert_eq!(identity.hard_links, Some(1));
}

#[tokio::test]
async fn a_hard_linked_file_counts_its_other_name() {
    let dir = TestDir::new("about-hard-link");
    let file = dir.as_ref().join("a.txt");
    std::fs::write(&file, "shared").expect("write the fixture");
    std::fs::hard_link(&file, dir.as_ref().join("b.txt")).expect("link it");
    let probed = probe(&file.to_string_lossy(), None).expect("the fixture is there");
    let Fact::Known(identity) = probed.identity else {
        panic!("identity is known");
    };
    #[cfg(unix)]
    assert_eq!(identity.hard_links, Some(2));
}

#[test]
fn a_missing_path_is_unavailable_not_a_card_of_unknowns() {
    let dir = TestDir::new("about-missing");
    let missing = dir.as_ref().join("gone.txt");
    assert_eq!(
        probe(&missing.to_string_lossy(), None).map(|_| ()),
        Err(super::event::AboutUnavailable::NotFound)
    );
}

#[test]
fn a_symlink_is_about_the_link_and_reads_nothing_through_it() {
    let dir = TestDir::new("about-symlink");
    let link = dir.as_ref().join("link");
    #[cfg(unix)]
    std::os::unix::fs::symlink("/nowhere/at/all", &link).expect("make the link");
    #[cfg(unix)]
    {
        let probed = probe(&link.to_string_lossy(), None).expect("a dangling link is still there");
        assert_eq!(probed.target, AboutTarget::Symlink);
        let Fact::Known(identity) = &probed.identity else {
            panic!("identity is known");
        };
        assert_eq!(identity.symlink_target, Some(ItemText::new("/nowhere/at/all")));
        let gathered = local_gathered(
            &link.to_string_lossy(),
            probed,
            ItemText::new("Macintosh HD"),
            macos(),
            Sources::none(),
        );
        assert!(
            gathered.jobs.iter().all(|(kind, _)| *kind == FactKind::Download),
            "a link plans no row that would follow it"
        );
    }
}

/// ❗ A link into a mount that isn't a local disk is described from its own `lstat`,
/// on the disk it lives on, and nothing follows it: no `statfs` through it, so a link
/// to a dead share can't hold the card past its budget. A card Cmdr started counts
/// it as the share. A pseudo-filesystem stands in for the share: the mount table
/// calls it no local disk.
#[cfg(unix)]
#[test]
fn a_link_into_a_mount_that_isnt_a_local_disk_is_described_as_a_link() {
    let elsewhere = if cfg!(target_os = "macos") { "/dev" } else { "/proc" };
    let dir = TestDir::new("about-link-off-disk");
    let link = dir.as_ref().join("nas");
    std::os::unix::fs::symlink(elsewhere, &link).expect("make the link");
    let path = link.to_string_lossy();
    let probed = probe(&path, None).expect("the link is there");
    assert_eq!(probed.target, AboutTarget::Symlink);
    assert!(probed.on_local_disk, "a link lives where its parent lives");
    assert!(
        super::place::reaches_a_share(&path),
        "reading through it reaches the other mount"
    );
    assert!(!super::place::reaches_a_share(&dir.as_ref().to_string_lossy()));
}

// ── Routing ─────────────────────────────────────────────────────────────────

/// Which way each volume kind is reached: only a live local disk reads locally,
/// and a share the OS mounted is told from one Cmdr dials by whether its paths
/// are the OS's own, never by its path.
#[test]
fn each_volume_kind_is_reached_its_own_way() {
    assert_eq!(reach(VolumeKind::Local, true, true), Way::Local);
    assert_eq!(reach(VolumeKind::Smb, true, true), Way::OsMountedShare);
    assert_eq!(reach(VolumeKind::Network, true, true), Way::OsMountedShare);
    for kind in [
        VolumeKind::Smb,
        VolumeKind::Sftp,
        VolumeKind::Webdav,
        VolumeKind::S3,
        VolumeKind::Network,
    ] {
        assert_eq!(reach(kind, true, false), Way::Server, "{kind:?}");
    }
    for kind in [VolumeKind::Mtp, VolumeKind::Adb, VolumeKind::Virtual] {
        assert_eq!(reach(kind, true, false), Way::Device, "{kind:?}");
    }
    for kind in [VolumeKind::Local, VolumeKind::Smb, VolumeKind::Mtp] {
        assert_eq!(reach(kind, false, true), Way::Offline, "{kind:?}");
    }
}

/// A path whose volume said local, reached through a symlink into a share: the
/// probe answered, so identity stands, and nothing else reads through the link.
#[test]
fn a_local_looking_path_on_a_network_mount_runs_no_local_job() {
    let dir = TestDir::new("about-through-a-link");
    let file = dir.as_ref().join("on-the-share.txt");
    std::fs::write(&file, "x").expect("write the fixture");
    let mut probed = probe(&file.to_string_lossy(), None).expect("the fixture is there");
    probed.on_local_disk = false;
    let gathered = local_gathered(
        &file.to_string_lossy(),
        probed,
        ItemText::new("Macintosh HD"),
        macos(),
        Sources::none(),
    );
    assert!(gathered.jobs.is_empty());
    assert_eq!(gathered.plan.kinds(), [FactKind::Identity]);
}

#[test]
fn an_online_only_file_reads_none_of_its_bytes() {
    let probed = Probe {
        target: AboutTarget::File,
        extension: Some("dmg".to_string()),
        is_executable: false,
        contents_local: false,
        on_local_disk: true,
        identity: Fact::unknown(UnknownWhy::NoRecord),
        node: None,
        repo: super::git::RepoAround::None,
    };
    let gathered = local_gathered(
        "/Users/someone/Library/CloudStorage/x.dmg",
        probed,
        ItemText::new("Macintosh HD"),
        macos(),
        Sources::none(),
    );
    let jobs: Vec<FactKind> = gathered.jobs.iter().map(|(kind, _)| *kind).collect();
    assert!(!jobs.contains(&FactKind::RealType));
    assert!(!jobs.contains(&FactKind::Signature));
    assert!(jobs.contains(&FactKind::Download));
}

// ── Cancel and budgets ──────────────────────────────────────────────────────

/// A job that says when it started and answers only once released.
fn held(kind: FactKind, answer: AboutFact) -> (Job<AboutFact>, mpsc::Receiver<()>, mpsc::Sender<()>) {
    let (started_tx, started_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let job: Job<AboutFact> = Box::new(move || {
        let _ = started_tx.send(());
        let _ = release_rx.recv_timeout(LONG);
        let _ = kind;
        answer
    });
    (job, started_rx, release_tx)
}

fn file_card_with(jobs: Vec<(FactKind, Job<AboutFact>)>) -> Gathered {
    let dir = TestDir::new("about-held");
    let file = dir.as_ref().join("notes.txt");
    std::fs::write(&file, "x").expect("write the fixture");
    let mut gathered = local(&file);
    gathered.jobs = jobs;
    gathered
}

// Two workers: the test blocks one waiting for the held job to start, and the
// session needs the other to submit it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_cancel_mid_way_stops_queued_jobs_and_leaves_no_late_events() {
    let (slow, started, release) = held(
        FactKind::Download,
        AboutFact::Download(Fact::unknown(UnknownWhy::NoRecord)),
    );
    let ran_late = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let ran = Arc::clone(&ran_late);
    let queued: Job<AboutFact> = Box::new(move || {
        ran.store(true, std::sync::atomic::Ordering::SeqCst);
        AboutFact::LastOpened(Fact::unknown(UnknownWhy::NoRecord))
    });
    // The queued job waits in a one-at-a-time queue behind the held one.
    let (queue_tx, queue_rx) = mpsc::channel::<Task>();
    std::thread::spawn(move || {
        while let Ok(task) = queue_rx.recv() {
            task();
        }
    });
    let submit: Submit = Arc::new(move |task| {
        let _ = queue_tx.send(task);
    });
    let gathered = file_card_with(vec![(FactKind::Download, slow), (FactKind::LastOpened, queued)]);

    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn AboutSink> = Arc::clone(&recorder) as Arc<dyn AboutSink>;
    let cancel = CancellationToken::new();
    let running = tokio::spawn(run(gathered, sink, cancel.clone(), submit, quick()));
    started.recv_timeout(LONG).expect("the slow job starts");
    cancel.cancel();
    running.await.expect("the session ends");
    let before_release = recorder.updates().len();
    release.send(()).expect("the slow job waits");
    // The queued job's closure holds the other handle; once the queue ran (or
    // skipped) it, the closure is gone.
    wait_until(LONG, "the queue drains", || Arc::strong_count(&ran_late) == 1);
    assert_eq!(recorder.updates().len(), before_release, "no event after a cancel");
    assert!(!matches!(recorder.updates().last(), Some(AboutUpdate::FactsSettled)));
    assert!(
        !ran_late.load(std::sync::atomic::Ordering::SeqCst),
        "a job queued behind the cancel never runs"
    );
}

#[tokio::test]
async fn a_row_past_its_budget_lands_as_timed_out_and_its_late_answer_drops() {
    let (slow, started, release) = held(
        FactKind::Download,
        AboutFact::Download(Fact::unknown(UnknownWhy::NoRecord)),
    );
    let mut gathered = file_card_with(vec![(FactKind::Download, slow)]);
    // Only the held row is planned besides identity.
    gathered
        .plan
        .retain_kinds(|kind| matches!(kind, FactKind::Identity | FactKind::Download));
    let timing = Timing {
        grace: Duration::from_millis(10),
        row_budget: Duration::from_millis(50),
        volume_budget: LONG,
    };
    let recorder = gather(gathered, timing).await;
    started.recv_timeout(LONG).expect("the slow job started");
    let updates = recorder.updates();
    assert_well_formed(&updates);
    assert!(recorder.facts().iter().any(|fact| matches!(
        fact,
        AboutFact::Download(Fact::Unknown(unknown)) if unknown.why() == UnknownWhy::TimedOut
    )));
    release.send(()).expect("the slow job waits");
    assert_eq!(recorder.updates().len(), updates.len(), "the late answer drops");
}

/// The card closed before its start arrived: the start finds the tombstone and
/// sends nothing at all.
#[tokio::test]
async fn a_cancel_that_beats_its_start_makes_the_start_send_nothing() {
    let dir = TestDir::new("about-tombstone");
    let file = dir.as_ref().join("notes.txt");
    std::fs::write(&file, "x").expect("write the fixture");
    let sent = Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = Arc::clone(&sent);
    let channel = tauri::ipc::Channel::new(move |_body| {
        counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Ok(())
    });
    let request_id = "about-test-tombstone".to_string();
    super::commands::about_cancel(request_id.clone());
    super::commands::start(
        super::event::AboutRequest {
            request_id,
            path: file.to_string_lossy().into_owned(),
            volume_id: None,
            is_directory: Some(false),
        },
        channel,
        || panic!("a start its cancel beat asks no gate"),
        || panic!("a start its cancel beat reads no switch"),
    );
    assert_eq!(sent.load(std::sync::atomic::Ordering::SeqCst), 0);
}

/// ❗ The plan and every fact go out while the verdict's gate is still resolving (its
/// slot reads `main.db`, which a writer can hold): no fact row waits for the verdict.
#[tokio::test]
async fn the_facts_dont_wait_for_the_verdicts_gate() {
    use crate::ignore_poison::IgnorePoison;
    crate::file_system::volume::manager::get_volume_manager().register_if_absent(
        "root",
        Arc::new(cmdr_local::LocalPosixVolume::local_folder(
            "Test root",
            "/",
            crate::volume_host::host(),
        )),
    );
    let dir = TestDir::new("about-gate-held");
    let file = dir.as_ref().join("notes.txt");
    std::fs::write(&file, "x").expect("write the fixture");
    let sent = Arc::new(Mutex::new(Vec::<String>::new()));
    let kept = Arc::clone(&sent);
    let channel = tauri::ipc::Channel::new(move |body| {
        if let tauri::ipc::InvokeResponseBody::Json(json) = body {
            kept.lock_ignore_poison().push(json);
        }
        Ok(())
    });
    let saw = |what: &str| {
        let wanted = format!("\"type\":\"{what}\"");
        sent.lock_ignore_poison().iter().any(|json| json.contains(&wanted))
    };
    let (release, held) = mpsc::channel::<()>();
    super::commands::start(
        super::event::AboutRequest {
            request_id: "about-test-gate-held".to_string(),
            path: file.to_string_lossy().into_owned(),
            volume_id: None,
            is_directory: Some(false),
        },
        channel,
        move || {
            let _ = held.recv_timeout(LONG);
            Err(super::event::VerdictUnavailable::AiOff)
        },
        || false,
    );
    // A local card settles in milliseconds; the gate stays held far longer.
    wait_until_async(Duration::from_secs(3), "the facts to settle with the gate held", || {
        saw("factsSettled")
    })
    .await;
    assert!(saw("planned"));
    assert!(!saw("verdictUnavailable"), "the gate is still held");
    release.send(()).expect("the gate waits");
    wait_until_async(LONG, "the verdict's answer once the gate lets go", || {
        saw("verdictUnavailable")
    })
    .await;
}

#[tokio::test]
async fn a_cancel_before_the_plan_sends_nothing() {
    let dir = TestDir::new("about-cancelled-first");
    let file = dir.as_ref().join("notes.txt");
    std::fs::write(&file, "x").expect("write the fixture");
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn AboutSink> = Arc::clone(&recorder) as Arc<dyn AboutSink>;
    let cancel = CancellationToken::new();
    cancel.cancel();
    run(local(&file), sink, cancel, on_threads(), quick()).await;
    assert!(recorder.updates().is_empty());
}

// ── Cmdr's own history ──────────────────────────────────────────────────────

/// Copies `source` into `into` through the engine's real copy path, journaling
/// into `log`, and waits until the copy settled and its rows are on disk.
#[cfg(target_os = "macos")]
async fn copy_with_cmdr(
    log: &cmdr_operations::testing::TestOperationLog,
    root: &std::path::Path,
    source: &std::path::Path,
    into: &std::path::Path,
) {
    use cmdr_fs::ignore_poison::IgnorePoison;
    use cmdr_fs::volume::Volume;
    use cmdr_operations::testing::CollectorEventSink;

    let engine = crate::operations_host::test_engine_journaling_into(log);
    let disk = || -> Arc<dyn Volume> {
        Arc::new(cmdr_local::LocalPosixVolume::local_folder(
            "Disk",
            root,
            crate::volume_host::host(),
        ))
    };
    let events = Arc::new(CollectorEventSink::new());
    cmdr_operations::testing::copy_between_volumes(
        &engine,
        Arc::clone(&events) as Arc<dyn cmdr_operations::OperationEventSink>,
        "about-history-source".to_string(),
        disk(),
        vec![source.to_path_buf()],
        "about-history-dest".to_string(),
        disk(),
        into.to_path_buf(),
        cmdr_operations::VolumeCopyConfig::default(),
        cmdr_operation_log::types::Initiator::User,
        None,
    )
    .await
    .expect("the copy starts");
    wait_until_async(LONG, "the copy to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(events.errors.lock_ignore_poison().is_empty(), "the copy ran clean");
    log.writer().flush_blocking().expect("the journal flushes");
}

#[cfg(target_os = "macos")]
fn with_history(path: &std::path::Path, log: &cmdr_operations::testing::TestOperationLog) -> Gathered {
    let path = path.to_string_lossy().into_owned();
    let probed = probe(&path, None).expect("the fixture is there");
    local_gathered(
        &path,
        probed,
        ItemText::new("Macintosh HD"),
        super::platform::os_caps(),
        Sources {
            history: Some(super::history::History {
                db: log.db().to_path_buf(),
                volume_id: cmdr_fs::volume::DEFAULT_VOLUME_ID.to_string(),
            }),
            ..Sources::none()
        },
    )
}

#[cfg(target_os = "macos")]
fn cmdr_operation_of(recorder: &Recorder) -> Fact<cmdr_about::provenance::CmdrOperation> {
    recorder
        .facts()
        .into_iter()
        .find_map(|fact| match fact {
            AboutFact::CmdrOperation(fact) => Some(fact),
            _ => None,
        })
        .expect("the history row lands")
}

/// A file Cmdr copied says where from and when; the same name holding another
/// file afterwards says it was replaced, never that Cmdr copied it.
#[cfg(target_os = "macos")]
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_file_cmdr_copied_says_where_from_until_it_is_replaced() {
    use cmdr_about::provenance::{LandedBy, StartedBy};

    let dir = TestDir::new("about-history");
    let source = dir.as_ref().join("from/report.pdf");
    let into = dir.as_ref().join("to");
    std::fs::create_dir_all(source.parent().expect("a folder")).expect("mk from");
    std::fs::create_dir_all(&into).expect("mk to");
    std::fs::write(&source, b"the report").expect("write the fixture");
    let log = cmdr_operations::testing::TestOperationLog::new("about-history-log");
    let before = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("after 1970")
        .as_secs() as i64;

    copy_with_cmdr(&log, dir.as_ref(), &source, &into).await;
    let copy = into.join("report.pdf");

    let recorder = gather(with_history(&copy, &log), quick()).await;
    assert_well_formed(&recorder.updates());
    let Fact::Known(operation) = cmdr_operation_of(&recorder) else {
        panic!("Cmdr copied this very file: {:?}", recorder.facts());
    };
    assert_eq!(operation.kind, LandedBy::Copy);
    assert_eq!(operation.initiator, StartedBy::You);
    assert_eq!(operation.source_path, Some(ItemText::new(&source.to_string_lossy())));
    assert_eq!(operation.source_volume, None, "the same disk");
    assert!(operation.at >= before, "{} is when the copy ran", operation.at);
    // The original was never landed by Cmdr.
    let original = gather(with_history(&source, &log), quick()).await;
    assert!(matches!(
        cmdr_operation_of(&original),
        Fact::Unknown(unknown) if unknown.why() == UnknownWhy::NoRecord
    ));

    // An app saves over the copy the way most do: a new file renamed into place.
    let fresh = into.join("report.pdf.tmp");
    std::fs::write(&fresh, b"the report").expect("write the replacement");
    std::fs::rename(&fresh, &copy).expect("replace the copy");

    let replaced = gather(with_history(&copy, &log), quick()).await;
    assert_well_formed(&replaced.updates());
    assert!(
        matches!(
            cmdr_operation_of(&replaced),
            Fact::Unknown(unknown) if unknown.why() == UnknownWhy::NodeChangedSince
        ),
        "{:?}",
        replaced.facts()
    );
}

// ── Where it sits: the owner and the repo ───────────────────────────────────

fn fake_installed_apps() -> Vec<cmdr_about::owner::KnownApp> {
    let app = |name: &str, id: &str| cmdr_about::owner::KnownApp {
        name: name.to_string(),
        id: Some(id.to_string()),
        other_names: Vec::new(),
        path: None,
    };
    vec![
        app("Commander One", "com.eltima.cmd1"),
        app("Slack", "com.tinyspeck.slackmacgap"),
        app("Google Chrome", "com.google.Chrome"),
        app("Google Drive", "com.google.drivefs"),
    ]
}

/// A card for `path` whose owner is asked against `home`, a fixture home, with a
/// fake list of installed apps: ❌ never the real `~/Library`.
fn in_a_fixture_home(path: &std::path::Path, home: &std::path::Path) -> Gathered {
    let path = path.to_string_lossy().into_owned();
    let probed = probe(&path, Some(&home.to_string_lossy())).expect("the fixture is there");
    local_gathered(
        &path,
        probed,
        ItemText::new("Macintosh HD"),
        macos(),
        Sources {
            owner: cmdr_about::owner::OwnerQuestion::of(&path, &home.to_string_lossy()),
            apps: super::owner::AppSource {
                installed: fake_installed_apps,
                with_id: |_| None,
                receipts: |_| Ok(Vec::new()),
            },
            ..Sources::none()
        },
    )
}

fn owner_of(recorder: &Recorder) -> Fact<cmdr_about::owner::OwnerClaim> {
    recorder
        .facts()
        .into_iter()
        .find_map(|fact| match fact {
            AboutFact::Owner(owner) => Some(owner),
            _ => None,
        })
        .expect("the owner row lands")
}

#[tokio::test]
async fn library_folders_answer_their_owner_through_the_card() {
    use cmdr_about::owner::{InstalledApp, OwnerClaim, OwnerPlace};
    let home = TestDir::new("about-owner-home");
    let support = home.join("Library/Application Support");
    for name in ["com.eltima.cmd1", "Slack", "Google"] {
        std::fs::create_dir_all(support.join(name)).expect("make the fixture");
    }

    let by_id = gather(in_a_fixture_home(&support.join("com.eltima.cmd1"), &home), quick()).await;
    assert_well_formed(&by_id.updates());
    assert_eq!(
        owner_of(&by_id),
        Fact::Known(OwnerClaim::ByBundleId {
            bundle_id: ItemText::new("com.eltima.cmd1"),
            place: OwnerPlace::ApplicationSupport,
            app: InstalledApp::Installed {
                name: ItemText::new("Commander One"),
                path: None,
            },
        })
    );
    let by_name = gather(in_a_fixture_home(&support.join("Slack"), &home), quick()).await;
    assert_eq!(
        owner_of(&by_name),
        Fact::Known(OwnerClaim::ByName {
            app: ItemText::new("Slack"),
            place: OwnerPlace::ApplicationSupport,
            app_path: None,
        })
    );
    let vendor = gather(in_a_fixture_home(&support.join("Google"), &home), quick()).await;
    assert_eq!(owner_of(&vendor), Fact::unknown(UnknownWhy::Ambiguous));
}

/// ❗ A card on an app's container, or on anything inside one, never looks inside it:
/// a container that holds a repo plans no git row (git's discovery would `stat`
/// `<container>/.git`), and its `com.vendor.App` name isn't asked about as a package.
/// That access is what can raise macOS's "would like to access data from other apps".
#[test]
fn a_container_is_never_looked_inside() {
    let home = TestDir::new("about-container-home");
    let containers = home.join("Library/Containers");
    let groups = home.join("Library/Group Containers");
    for container in [containers.join("com.vendor.App"), groups.join("ABCDE12345.com.vendor")] {
        let data = container.join("Data");
        std::fs::create_dir_all(&data).expect("make the fixture");
        std::fs::write(data.join("notes.txt"), "x").expect("write the fixture");
        let _repo = cmdr_git::test_fixtures::Fixture::init(container.clone());
        assert!(
            container.join(".git").exists(),
            "the fixture holds a repo the probe would find"
        );
        for path in [container.clone(), data.clone(), data.join("notes.txt")] {
            let gathered = in_a_fixture_home(&path, &home);
            assert!(!gathered.plan.kinds().contains(&FactKind::Git), "{path:?}");
            assert_ne!(gathered.plan.target, AboutTarget::Package, "{path:?}");
        }
    }
    // The same repo outside a container is found, so the test isn't vacuous.
    let outside = home.join("Projects/app");
    std::fs::create_dir_all(&outside).expect("make the fixture");
    let _repo = cmdr_git::test_fixtures::Fixture::init(outside.clone());
    assert!(in_a_fixture_home(&outside, &home).plan.kinds().contains(&FactKind::Git));
}

/// A document no path rule claims and no repo holds has neither row: the card
/// stays short.
#[test]
fn a_plain_document_plans_no_owner_or_git_row() {
    let home = TestDir::new("about-plain-home");
    let documents = home.join("Documents");
    std::fs::create_dir_all(&documents).expect("make the fixture");
    let file = documents.join("notes.txt");
    std::fs::write(&file, "plain words").expect("write the fixture");
    let gathered = in_a_fixture_home(&file, &home);
    assert!(!gathered.plan.has(FactKind::Owner));
    assert!(!gathered.plan.has(FactKind::Git));
}

#[tokio::test]
async fn a_modified_repo_file_answers_uncommitted_work_with_its_branch() {
    use cmdr_about::payload::{GitStatus, TrackedState};
    let dir = TestDir::new("about-git-card");
    let _repo = cmdr_git::test_fixtures::Fixture::init(dir.to_path_buf());
    let file = dir.join("notes.txt");
    std::fs::write(&file, "before").expect("write the fixture");
    cmdr_git::test_fixtures::git_cli(&dir, &["add", "."]);
    cmdr_git::test_fixtures::git_cli(&dir, &["commit", "-q", "-m", "first"]);
    std::fs::write(&file, "after").expect("edit the fixture");

    let recorder = gather(local(&file), quick()).await;
    assert_well_formed(&recorder.updates());
    let git = recorder
        .facts()
        .into_iter()
        .find_map(|fact| match fact {
            AboutFact::Git(Fact::Known(git)) => Some(git),
            _ => None,
        })
        .expect("the git row lands known");
    assert_eq!(git.branch, Some(ItemText::new("main")));
    assert_eq!(git.status, Fact::Known(GitStatus::Tracked(TrackedState::Modified)));
}

/// Without a log to read, the card plans no history row: a row with no job
/// would never settle.
#[test]
fn a_card_with_no_log_to_read_plans_no_history_row() {
    let dir = TestDir::new("about-no-history");
    let file = dir.as_ref().join("notes.txt");
    std::fs::write(&file, "plain words").expect("write the fixture");
    let gathered = local(&file);
    assert!(!gathered.plan.has(FactKind::CmdrOperation));
    assert!(gathered.jobs.iter().all(|(kind, _)| *kind != FactKind::CmdrOperation));
}

/// ❗ One card's slow lookups can't starve the next card: with the card pool's real
/// sizing, every worker it grows to by default held inside a call that hasn't answered,
/// the next card's rows still get threads and land inside their budgets, never
/// `TimedOut`.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_slow_card_cant_starve_the_next_cards_rows() {
    use crate::file_system::framework_pool::Pool;
    let pool = Arc::new(Pool::new(super::pool::POOL));
    let (release_tx, release_rx) = mpsc::channel::<()>();
    let release_rx = Arc::new(Mutex::new(release_rx));
    // One at a time, so each lands on a thread of its own: the first card's slow calls
    // hold every worker the pool grows to by default.
    for held in 1..=super::pool::POOL.target_workers {
        let release_rx = Arc::clone(&release_rx);
        pool.submit(Box::new(move || {
            let _ = release_rx
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .recv();
        }));
        wait_until(LONG, "the slow call started", || pool.busy_count() == held);
    }
    let held_since = std::time::Instant::now();
    let spare_after = super::pool::POOL.spare_after.expect("the card pool frees slots");
    wait_until(LONG, "the slow calls are past spare_after", || {
        held_since.elapsed() >= spare_after
    });

    // The next card: a real plan whose every row answers at once, so only waiting for a
    // thread could make one miss its budget.
    let dir = TestDir::new("about-unstarved");
    let file = dir.as_ref().join("notes.txt");
    std::fs::write(&file, "plain words").expect("write the fixture");
    let mut gathered = local(&file);
    let mut answers = cmdr_about::card::AboutCard::new(&gathered.plan);
    for ready in &gathered.ready {
        let _ = answers.apply(ready.clone());
    }
    gathered.jobs = answers
        .settle_pending(UnknownWhy::NoRecord)
        .into_iter()
        .map(|answer| {
            let kind = answer.kind();
            let job: Job<AboutFact> = Box::new(move || answer);
            (kind, job)
        })
        .collect();
    // Linux plans no Spotlight, signature, or default-app row, so its burst is smaller;
    // one row waiting on a thread is already what this guards.
    let burst = if cfg!(target_os = "macos") { 3 } else { 1 };
    assert!(gathered.jobs.len() >= burst, "a card's burst of rows");
    let next = Arc::clone(&pool);
    let submit: Submit = Arc::new(move |task: Task| next.submit(task));
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn AboutSink> = Arc::clone(&recorder) as Arc<dyn AboutSink>;
    run(gathered, sink, CancellationToken::new(), submit, Timing::CARD).await;

    assert_well_formed(&recorder.updates());
    let facts = serde_json::to_string(&recorder.facts()).expect("facts serialize");
    assert!(
        !facts.contains("timedOut"),
        "a row starved behind the slow card: {facts}"
    );
    drop(release_tx);
}
