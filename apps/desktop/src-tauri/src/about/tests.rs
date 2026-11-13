//! The host end to end with plain threads: what the card's channel carries, in
//! what order, and what a cancel or a budget does to it.

use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use cmdr_about::card::{AboutFact, OsCaps};
use cmdr_about::fact::{AboutTarget, Fact, FactKind, UnknownWhy};
use cmdr_about::gather::{Job, Task};
#[cfg(target_os = "macos")]
use cmdr_about::provenance::{WebSite, WhereFrom};
use cmdr_about::text::ItemText;
use tokio_util::sync::CancellationToken;

use super::event::AboutUpdate;
use super::probe::{Probe, probe};
use super::route::{Reach, local_gathered, reach, remote_gathered};
use super::session::{AboutSink, Gathered, Submit, Timing, run};
use crate::test_support::{TestDir, wait_until};
use crate::tools::resources::volumes::VolumeKind;

/// Far longer than any step here takes.
const LONG: Duration = Duration::from_secs(10);

#[derive(Default)]
struct Recorder(Mutex<Vec<AboutUpdate>>);

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
    fn updates(&self) -> Vec<AboutUpdate> {
        self.0.lock().unwrap_or_else(std::sync::PoisonError::into_inner).clone()
    }

    fn facts(&self) -> Vec<AboutFact> {
        self.updates()
            .into_iter()
            .filter_map(|update| match update {
                AboutUpdate::Fact { fact, .. } => Some(fact),
                _ => None,
            })
            .collect()
    }
}

fn on_threads() -> Submit {
    Arc::new(|task: Task| {
        std::thread::spawn(task);
    })
}

fn quick() -> Timing {
    Timing {
        grace: Duration::from_millis(100),
        row_budget: LONG,
    }
}

fn macos() -> OsCaps {
    OsCaps {
        use_history: true,
        code_signing: true,
        default_app: true,
    }
}

fn kinds(facts: &[AboutFact]) -> Vec<FactKind> {
    let mut kinds: Vec<FactKind> = facts.iter().map(AboutFact::kind).collect();
    kinds.sort();
    kinds
}

async fn gather(gathered: Gathered, timing: Timing) -> Arc<Recorder> {
    let recorder = Arc::new(Recorder::default());
    let sink: Arc<dyn AboutSink> = Arc::clone(&recorder) as Arc<dyn AboutSink>;
    run(gathered, sink, CancellationToken::new(), on_threads(), timing).await;
    recorder
}

/// Every stream opens with the plan and closes with `factsSettled`, and every
/// fact in between has a row in that plan, exactly once.
fn assert_well_formed(updates: &[AboutUpdate]) {
    let Some(AboutUpdate::Planned { plan, .. }) = updates.first() else {
        panic!("the first event is the plan: {updates:?}");
    };
    assert!(matches!(updates.last(), Some(AboutUpdate::FactsSettled)), "{updates:?}");
    let mut seen = Vec::new();
    for update in &updates[1..updates.len() - 1] {
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
    let probed = probe(&path).expect("the fixture is there");
    local_gathered(
        &path,
        probed,
        ItemText::new("Macintosh HD"),
        super::platform::os_caps(),
        None,
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
    std::fs::write(&file, b"not really a disk image").expect("write the fixture");
    write_chrome_download(&file);

    let recorder = gather(local(&file), quick()).await;
    let updates = recorder.updates();
    assert_well_formed(&updates);
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
    let probed = probe(&file.to_string_lossy()).expect("the fixture is there");
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
        probe(&missing.to_string_lossy()).map(|_| ()),
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
        let probed = probe(&link.to_string_lossy()).expect("a dangling link is still there");
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
            None,
        );
        assert!(
            gathered.jobs.iter().all(|(kind, _)| *kind == FactKind::Download),
            "a link plans no row that would follow it"
        );
    }
}

// ── Routing ─────────────────────────────────────────────────────────────────

#[test]
fn only_a_live_local_volume_is_read_locally() {
    assert_eq!(reach(VolumeKind::Local, true), Reach::Local);
    assert_eq!(reach(VolumeKind::Local, false), Reach::ThroughTheVolume);
    for kind in [
        VolumeKind::Smb,
        VolumeKind::Sftp,
        VolumeKind::Webdav,
        VolumeKind::S3,
        VolumeKind::Mtp,
        VolumeKind::Adb,
        VolumeKind::Network,
        VolumeKind::Virtual,
    ] {
        assert_eq!(reach(kind, true), Reach::ThroughTheVolume, "{kind:?}");
    }
}

#[test]
fn an_item_on_a_share_answers_identity_alone_and_runs_no_local_job() {
    let mut entry = cmdr_fs::entry::FileEntry::new(
        "report.pdf".to_string(),
        "/Volumes/naspi/report.pdf".to_string(),
        false,
        false,
    );
    entry.size = Some(2_048);
    let gathered = remote_gathered(&entry, ItemText::new("naspi"));
    assert!(gathered.jobs.is_empty());
    assert_eq!(gathered.plan.kinds(), [FactKind::Identity]);
    assert!(
        gathered.plan.volume_note.is_some(),
        "the card says once what the volume can't answer"
    );
}

/// A path whose volume said local, reached through a symlink into a share: the
/// probe answered, so identity stands, and nothing else reads through the link.
#[test]
fn a_local_looking_path_on_a_network_mount_runs_no_local_job() {
    let dir = TestDir::new("about-through-a-link");
    let file = dir.as_ref().join("on-the-share.txt");
    std::fs::write(&file, "x").expect("write the fixture");
    let mut probed = probe(&file.to_string_lossy()).expect("the fixture is there");
    probed.on_local_disk = false;
    let gathered = local_gathered(
        &file.to_string_lossy(),
        probed,
        ItemText::new("Macintosh HD"),
        macos(),
        None,
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
    };
    let gathered = local_gathered(
        "/Users/someone/Library/CloudStorage/x.dmg",
        probed,
        ItemText::new("Macintosh HD"),
        macos(),
        None,
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
    super::commands::about_start(
        super::event::AboutRequest {
            request_id,
            path: file.to_string_lossy().into_owned(),
            volume_id: None,
        },
        channel,
    )
    .await;
    assert_eq!(sent.load(std::sync::atomic::Ordering::SeqCst), 0);
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
    crate::test_support::wait_until_async(LONG, "the copy to settle", || {
        !events.settled.lock_ignore_poison().is_empty()
    })
    .await;
    assert!(events.errors.lock_ignore_poison().is_empty(), "the copy ran clean");
    log.writer().flush_blocking().expect("the journal flushes");
}

#[cfg(target_os = "macos")]
fn with_history(path: &std::path::Path, log: &cmdr_operations::testing::TestOperationLog) -> Gathered {
    let path = path.to_string_lossy().into_owned();
    let probed = probe(&path).expect("the fixture is there");
    local_gathered(
        &path,
        probed,
        ItemText::new("Macintosh HD"),
        super::platform::os_caps(),
        Some(super::history::History {
            db: log.db().to_path_buf(),
            volume_id: cmdr_fs::volume::DEFAULT_VOLUME_ID.to_string(),
        }),
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
