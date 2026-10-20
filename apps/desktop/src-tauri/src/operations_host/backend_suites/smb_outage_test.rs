//! A copy onto a real SMB share whose network goes away mid-copy waits for it
//! to come back and finishes (`#398`).
//!
//! The outage is a proxy of the test's own between the share and the Docker
//! fixture (`outage_proxy_test_support.rs`), so the shared container stack is
//! never touched. The wait itself is pinned without a server in
//! `transfer/volume_outage_tests.rs`; this cell is the part only a live share
//! can answer: that the real backend reads `Disconnected` when its socket
//! closes, refuses to reconnect while the network is gone, and reconnects once
//! it's back, so the copy lands every byte.
//!
//! The close-out soak rides the `flaky` fixture instead (a Samba that comes
//! and goes on its own every 5 s): an Overwrite copy through several of its
//! cycles lands every file and leaves nothing of ours behind.
//!
//! A Cancel during the outage has a cell here too: it's reported at once, and
//! the temp it left on the share goes once the network is back
//! (`transfer/leftover_cleanup.rs`, pinned without a server in
//! `transfer/volume/copy_outage_tests.rs`).
//!
//! `#[ignore]`d like every `smb_integration_` test; the
//! `desktop-rust-integration-tests` check boots the Docker Samba stack and runs
//! it. Locally: `./apps/desktop/test/smb-servers/start.sh`, then
//! `cargo nextest run smb_outage_test --run-ignored all`.

use super::outage_proxy_test_support::OutageProxy;
use super::smb_test_support::*;

use crate::test_support::{wait_until_async, wait_until_async_while_moving};
use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::ConnectionState;
use cmdr_fs::volume::host::VolumeHost;
use cmdr_operations::testing::new_operation_state;
use cmdr_operations::testing::{
    CleanupPolicy, CollectorEventSink, OutagePolicy, TestOperations, copy_volumes_with_progress,
};
use cmdr_operations::{
    ConflictInfo, ConflictResolution, DryRunResult, OperationEventSink, ScanProgressEvent, VolumeCopyConfig,
    WriteCancelledEvent, WriteCompleteEvent, WriteConflictResolvedEvent, WriteErrorEvent, WriteOperationPhase,
    WriteProgressEvent, WriteSettledEvent, WriteSourceItemDoneEvent,
};
use cmdr_smb::volume::{MountAnchor, SmbConnectionParams, SmbVolume, connect_smb_volume};
use std::time::Instant;

/// Files in the folder. Enough that the outage lands in the middle, with files
/// on both sides of it: more than [`PAUSE_AFTER_FILES`] plus a whole concurrent
/// window (SMB's default is 10 slots), so the walk still has files it hasn't
/// started when the network goes. The walk parks per entry while paused, but a
/// file already in flight runs to its end, and with 12 files the whole rest of
/// the folder sometimes fit in the window and landed before the cut.
const FILE_COUNT: usize = 32;

/// Each file's size. Small enough that a file can go out as ONE compound write
/// at its final name (single-shot, no temp) and big enough that one can take the
/// staged writer instead, depending on the uplink smb2 has measured by then: a
/// drop can swallow the reply of either, and the copy has to settle both
/// (`transfer/interrupted_landing.rs`).
const FILE_BYTES: usize = 256 * 1024;

/// The copy pauses once this many files have landed, which is where the test
/// takes the network away.
const PAUSE_AFTER_FILES: usize = 3;

/// How long a copy may go without moving (no new file or byte done) before a
/// wait on it reads as a stall.
///
/// ❗ The waits here are paced by the copy, ❌ never a stopwatch: how long these
/// cells take is the shared guest share's throughput. A healthy run on a
/// starved Samba (one CPU of the stack, which the whole suite shares) took past
/// the general 30 s cap on a CI runner (run 39321102740, 2026-10-19). On a
/// private copy of the guest at `docker update --cpus 0.01` the copy cell
/// took 87 s and finished, its longest stretch without progress 13 s (one
/// window of ten writes awaiting the server; M3 Max, 2026-10-20). A wait that
/// sees nothing move for this long is a hang, whatever the load.
const QUIET: Duration = Duration::from_secs(20);

/// Every wait in a cell together. Under the cells' 120 s nextest cap
/// (`.config/nextest.toml`), so the wait that starved names itself rather than
/// the cap killing the cell mute.
const CELL_BUDGET: Duration = Duration::from_secs(90);

/// What's left of a cell's [`CELL_BUDGET`].
struct CellClock(tokio::time::Instant);

impl CellClock {
    fn start() -> Self {
        Self(tokio::time::Instant::now() + CELL_BUDGET)
    }

    fn left(&self) -> Duration {
        self.0.saturating_duration_since(tokio::time::Instant::now())
    }
}

/// How far the copy has got: the newest tick's files and bytes. A re-sent tick
/// (the outage wait's announcement) repeats it, so it doesn't count as moving.
fn progress_of(events: &CollectorEventSink) -> Option<(usize, u64)> {
    events
        .progress
        .lock_ignore_poison()
        .last()
        .map(|tick| (tick.files_done, tick.bytes_done))
}

/// Waits for `condition` while the copy keeps moving ([`QUIET`]), within what's
/// left of the cell's budget.
#[track_caller]
fn while_copying<'a>(
    clock: &CellClock,
    events: &'a CollectorEventSink,
    description: &'a str,
    condition: impl FnMut() -> bool + 'a,
) -> impl Future<Output = ()> + 'a {
    wait_until_async_while_moving(QUIET, clock.left(), description, move || progress_of(events), condition)
}

/// Reconnect quickly and often: what's under test is that the copy waits and
/// then carries on, not the production cadence.
const CADENCE: &[Duration] = &[Duration::from_millis(100), Duration::from_millis(200)];

fn content(index: usize) -> Vec<u8> {
    let seed = format!("cmdr-398-outage-{index}");
    let block = *blake3::hash(seed.as_bytes()).as_bytes();
    block.iter().copied().cycle().take(FILE_BYTES).collect()
}

/// Forwards everything to a collector, and shows `tap` every progress event
/// first: the copy's own emit decides the moment a cell acts (pauses the copy,
/// cuts the network), so the copy can't finish before the cell gets to.
struct ProgressTap {
    inner: Arc<CollectorEventSink>,
    tap: Box<dyn Fn(&WriteProgressEvent) + Send + Sync>,
}

impl OperationEventSink for ProgressTap {
    fn emit_progress(&self, event: WriteProgressEvent) {
        (self.tap)(&event);
        self.inner.emit_progress(event);
    }
    fn emit_complete(&self, event: WriteCompleteEvent) {
        self.inner.emit_complete(event);
    }
    fn emit_cancelled(&self, event: WriteCancelledEvent) {
        self.inner.emit_cancelled(event);
    }
    fn emit_error(&self, event: WriteErrorEvent) {
        self.inner.emit_error(event);
    }
    fn emit_conflict(&self, event: cmdr_operations::ArmedConflict) {
        self.inner.emit_conflict(event);
    }
    fn emit_conflict_resolved(&self, event: WriteConflictResolvedEvent) {
        self.inner.emit_conflict_resolved(event);
    }
    fn emit_source_item_done(&self, event: WriteSourceItemDoneEvent) {
        self.inner.emit_source_item_done(event);
    }
    fn emit_scan_progress(&self, event: ScanProgressEvent) {
        self.inner.emit_scan_progress(event);
    }
    fn emit_scan_conflict(&self, conflict: ConflictInfo) {
        self.inner.emit_scan_conflict(conflict);
    }
    fn emit_dry_run_complete(&self, result: DryRunResult) {
        self.inner.emit_dry_run_complete(result);
    }
    fn emit_settled(&self, event: WriteSettledEvent) {
        self.inner.emit_settled(event);
    }
}

/// THE scenario from `#398`, against a real share: a folder copy loses the
/// network part-way, waits (named, not failed), and finishes once the network is
/// back, with every file byte-exact.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_copy_waits_out_a_dropped_network_and_finishes() {
    copy_through_a_cut(ConflictResolution::Stop).await;
}

/// The incident's `.DS_Store` shape: every file the copy writes REPLACES one
/// already on the share, so each lands by a swap (delete the original, rename
/// the new bytes onto its name), and the cut can fall inside one. A swap the
/// drop interrupted settles once the network is back, and every original ends
/// up replaced, with nothing parked under ` (recovered)` and no temp left.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_an_overwrite_waits_out_a_dropped_network_and_replaces_every_original() {
    copy_through_a_cut(ConflictResolution::Overwrite).await;
}

/// What the share holds at `index` before an Overwrite: a different size and
/// different bytes, so a file the copy left alone shows.
fn original(index: usize) -> Vec<u8> {
    let mut bytes = content(index);
    bytes.truncate(FILE_BYTES / 2);
    bytes.reverse();
    bytes
}

/// Copies the folder onto the share through a proxy cut part-way, and checks
/// the copy waited and every file landed byte-exact. Under
/// `ConflictResolution::Overwrite` the share starts with an [`original`] at
/// every name, so every file lands over one.
async fn copy_through_a_cut(conflict_resolution: ConflictResolution) {
    // A direct connection seeds and verifies; the copy runs through the proxy.
    let direct = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&direct, &base).await;
    direct
        .create_directory(Path::new(&base))
        .await
        .expect("the scratch dir");
    if conflict_resolution == ConflictResolution::Overwrite {
        let seeded = PathBuf::from(&base).join("batch");
        direct.create_directory(&seeded).await.expect("the seeded folder");
        for index in 0..FILE_COUNT {
            direct
                .create_file(&seeded.join(format!("f-{index:02}.bin")), &original(index))
                .await
                .expect("an original on the share");
        }
    }

    let proxy = OutageProxy::start(guest_port()).await;
    let through_proxy: Arc<dyn Volume> =
        Arc::new(make_docker_volume_through(proxy.port(), crate::volume_host::host()).await);

    let local = tempfile::TempDir::new().expect("a local source dir");
    let folder = local.path().join("batch");
    std::fs::create_dir(&folder).expect("the source folder");
    for index in 0..FILE_COUNT {
        std::fs::write(folder.join(format!("f-{index:02}.bin")), content(index)).expect("a source file");
    }
    let source: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
        "src",
        local.path().to_path_buf(),
        crate::volume_host::host(),
    ));

    // An engine of the cell's own: the copy runs on a state registered there.
    let engine = TestOperations::new();
    let state = Arc::new(
        new_operation_state(&engine, Duration::from_millis(50)).with_outage_policy(OutagePolicy {
            budget: Duration::from_secs(60),
            cadence: CADENCE,
            poll_every: Duration::from_millis(20),
            // Well inside `QUIET`, so one handshake that never returns can't
            // outlast the network coming back (`#406`).
            attempt_timeout: Duration::from_secs(5),
        }),
    );
    let op = engine.register_state("smb-outage", Arc::clone(&state));
    let events = Arc::new(CollectorEventSink::new());
    // Pauses the copy once `PAUSE_AFTER_FILES` have landed, once only: after
    // the resume, the copy has to run on into the outage.
    let pausing = Arc::clone(&state);
    let paused = std::sync::atomic::AtomicBool::new(false);
    let sink: Arc<dyn OperationEventSink> = Arc::new(ProgressTap {
        inner: Arc::clone(&events),
        tap: Box::new(move |event| {
            let pause = matches!(event.phase, WriteOperationPhase::Copying) && event.files_done >= PAUSE_AFTER_FILES;
            if pause && !paused.swap(true, Ordering::SeqCst) {
                pausing.pause_gate.pause();
            }
        }),
    });

    let copy_state = Arc::clone(&state);
    let op_id = op.id().to_owned();
    let dest_path = PathBuf::from(&base);
    let copy = tokio::spawn(async move {
        copy_volumes_with_progress(
            sink,
            op_id.as_str(),
            &copy_state,
            source,
            &[PathBuf::from("batch")],
            through_proxy,
            &dest_path,
            &VolumeCopyConfig {
                conflict_resolution,
                progress_interval_ms: 0,
                ..VolumeCopyConfig::default()
            },
        )
        .await
        .map_err(|failure| format!("{:?}", failure.error))
    });

    // Mid-copy: the network goes away while the copy is paused, then the copy
    // carries on into it.
    let clock = CellClock::start();
    while_copying(&clock, &events, "the copy to pause part-way", || {
        state.pause_gate.is_paused()
    })
    .await;
    proxy.cut();
    state.pause_gate.resume();
    while_copying(&clock, &events, "the copy to wait for the share", || {
        state.outage.waiting_for().is_some()
    })
    .await;

    // While the network is gone, the copy keeps waiting rather than failing.
    // allowed-test-sleep: the subject IS that time passing doesn't end the wait
    // while the network is down; there's no event for "still waiting".
    tokio::time::sleep(Duration::from_secs(1)).await;
    assert!(!copy.is_finished(), "a copy whose share is gone waits; it doesn't fail");
    assert!(state.outage.waiting_for().is_some(), "and says what it's waiting for");

    proxy.restore();
    while_copying(&clock, &events, "the copy to finish once the network is back", || {
        copy.is_finished()
    })
    .await;
    let result = copy.await.expect("the copy task itself must not panic");
    result.expect("a share that comes back within the budget must not fail the copy");

    for index in 0..FILE_COUNT {
        let path = PathBuf::from(&base).join("batch").join(format!("f-{index:02}.bin"));
        assert_eq!(
            hash_volume_file(&*direct, &path).await,
            hash_bytes(&content(index)),
            "{} must land byte-exact",
            path.display()
        );
    }
    let leftovers: Vec<String> = direct
        .list_directory(&PathBuf::from(&base).join("batch"), None)
        .await
        .expect("the destination folder")
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| name.contains(".cmdr-tmp-") || name.contains("(recovered)"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "no staged partial survives the outage, and nothing is parked aside: {leftovers:?}"
    );

    ensure_clean(&direct, &base).await;
}

/// The big file's size: far past any single-shot write, so it always goes
/// through the staged writer and a temp of ours sits on the share when the
/// network goes.
const BIG_FILE_BYTES: usize = 64 * 1024 * 1024;

/// The copy loses the network once this much of the big file is through.
const CUT_AFTER_BYTES: u64 = 2 * 1024 * 1024;

/// How long the background cleanup waits for a share that's out. The cancel
/// must be reported well inside it, which is what tells "reported at once" from
/// "reported after the cleanup gave up".
const CLEANUP_WAIT: Duration = Duration::from_secs(20);

/// The most the cancel may take to be reported while the share is out.
const PROMPTLY: Duration = Duration::from_secs(5);

/// A Cancel while the share is out is reported at once, with the temp it left
/// named as "goes once the share is back", and the temp IS gone once the
/// network returns, without a launch or another transfer into that folder.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_a_cancel_while_the_share_is_out_reports_at_once_and_removes_the_temp_once_it_is_back() {
    let direct = Arc::new(make_docker_volume().await);
    let base = test_dir_name();
    ensure_clean(&direct, &base).await;
    direct
        .create_directory(Path::new(&base))
        .await
        .expect("the scratch dir");

    let proxy = Arc::new(OutageProxy::start(guest_port()).await);
    let through_proxy: Arc<dyn Volume> =
        Arc::new(make_docker_volume_through(proxy.port(), crate::volume_host::host()).await);

    let local = tempfile::TempDir::new().expect("a local source dir");
    let big = content(0).into_iter().cycle().take(BIG_FILE_BYTES).collect::<Vec<u8>>();
    std::fs::write(local.path().join("big.bin"), &big).expect("the source file");
    let source: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
        "src",
        local.path().to_path_buf(),
        crate::volume_host::host(),
    ));

    let engine = TestOperations::new();
    let state = Arc::new(
        new_operation_state(&engine, Duration::from_millis(50))
            .with_outage_policy(OutagePolicy {
                budget: Duration::from_secs(60),
                cadence: CADENCE,
                poll_every: Duration::from_millis(20),
                attempt_timeout: Duration::from_secs(5),
            })
            .with_cleanup_policy(CleanupPolicy {
                wait: CLEANUP_WAIT,
                ..CleanupPolicy::default()
            }),
    );
    let op = engine.register_state("smb-outage-cancel", Arc::clone(&state));
    let events = Arc::new(CollectorEventSink::new());
    // Cuts the network part-way through the big file, from the copy's own
    // emit, so the temp is mid-write when it goes.
    let cutting = Arc::clone(&proxy);
    let cut = std::sync::atomic::AtomicBool::new(false);
    let sink: Arc<dyn OperationEventSink> = Arc::new(ProgressTap {
        inner: Arc::clone(&events),
        tap: Box::new(move |event| {
            let due = matches!(event.phase, WriteOperationPhase::Copying) && event.bytes_done >= CUT_AFTER_BYTES;
            if due && !cut.swap(true, Ordering::SeqCst) {
                cutting.cut();
            }
        }),
    });

    let copy_state = Arc::clone(&state);
    let op_id = op.id().to_owned();
    let dest = Arc::clone(&through_proxy);
    let dest_path = PathBuf::from(&base);
    let mut copy = tokio::spawn(async move {
        copy_volumes_with_progress(
            sink,
            op_id.as_str(),
            &copy_state,
            source,
            &[PathBuf::from("big.bin")],
            dest,
            &dest_path,
            &VolumeCopyConfig {
                progress_interval_ms: 0,
                ..VolumeCopyConfig::default()
            },
        )
        .await
        .map_err(|failure| failure.error)
    });

    let clock = CellClock::start();
    while_copying(&clock, &events, "the copy to wait for the share", || {
        state.outage.waiting_for().is_some()
    })
    .await;
    // The drop really happened: the share's own session reads it.
    assert_eq!(
        through_proxy.connection_state(),
        Some(ConnectionState::Disconnected),
        "the cut reached the share's session"
    );
    assert!(
        temps_in(&direct, &base).await.len() == 1,
        "the staged temp sits on the share while the network is out"
    );

    let cancelled_at = Instant::now();
    engine.cancel_write_operation(op.id(), false);
    let result = match tokio::time::timeout(PROMPTLY, &mut copy).await {
        Ok(joined) => joined.expect("the copy task itself must not panic"),
        Err(_) => {
            copy.abort();
            panic!("a Cancel while the share is out wasn't reported within {PROMPTLY:?}");
        }
    };
    let reported_after = cancelled_at.elapsed();
    assert!(
        matches!(result, Err(cmdr_operations::WriteOperationError::Cancelled { .. })),
        "a cancelled copy reports the cancel: {result:?}"
    );
    assert!(
        reported_after < CLEANUP_WAIT / 2,
        "the cancel doesn't wait for the cleanup ({reported_after:?})"
    );
    let cancelled = events.cancelled.lock_ignore_poison().clone();
    let leftovers = &cancelled
        .first()
        .expect("one write-cancelled")
        .rollback
        .staged_leftovers;
    // Read off the wire: the fate is what the frontend words the line by.
    let fates: Vec<serde_json::Value> = leftovers
        .iter()
        .map(|group| serde_json::to_value(group.fate).expect("a fate serializes"))
        .collect();
    assert_eq!(
        fates,
        vec![serde_json::json!("removedWhenBack")],
        "the temp goes once the share is back: {leftovers:?}"
    );
    assert_eq!(
        temps_in(&direct, &base).await.len(),
        1,
        "still there while the share is out"
    );

    // The network returns, and the share's session comes back the way a pane's
    // backoff brings it back: someone else's reconnect, which the background
    // pass is waiting for.
    proxy.restore();
    through_proxy
        .attempt_reconnect()
        .await
        .expect("the share reconnects once the network is back");
    tokio::time::timeout(clock.left(), engine.leftover_cleanups_settled())
        .await
        .expect("the background cleanup settles once the share is back");

    let temps = temps_in(&direct, &base).await;
    assert!(temps.is_empty(), "no temp is left once the share is back: {temps:?}");

    ensure_clean(&direct, &base).await;
}

/// The `flaky` fixture's host port: a Samba that serves for 5 s, then is gone
/// for 5 s, forever (`smb-servers/.compose/smb-consumer-flaky/cycle.sh`). The
/// check runner exports the stack's port table (`scripts/check/checks/smb_ports.go`).
fn flaky_port() -> u16 {
    std::env::var("SMB_CONSUMER_FLAKY_PORT")
        .ok()
        .and_then(|port| port.parse().ok())
        .unwrap_or(10492)
}

/// Connects to the `flaky` fixture's `public` share, or `None` while it's down.
async fn connect_flaky(host: VolumeHost) -> Option<SmbVolume> {
    let port = flaky_port();
    connect_smb_volume(
        "public",
        MountAnchor::at_share_root(TEST_MOUNT_ROOT),
        &cmdr_fs::volume::smb_volume_id("127.0.0.1", port, "public"),
        SmbConnectionParams::new("127.0.0.1", "public", port, None, None),
        host,
    )
    .await
    .ok()
}

/// Watches the `flaky` share from a connection of the cell's own (a detached
/// host, so the app never sees it): whether it answers right now, and how many
/// times it has gone away. The cell seeds and verifies through [`Self::volume`],
/// inside a window the watch says is up.
struct FlakyShare {
    volume: Arc<std::sync::OnceLock<Arc<SmbVolume>>>,
    up: Arc<std::sync::atomic::AtomicBool>,
    downs: Arc<std::sync::atomic::AtomicU64>,
    watch: tokio::task::JoinHandle<()>,
}

/// How often the watch asks the share whether it's there. Well inside the
/// fixture's 5 s halves, so no down half goes unseen.
const PROBE_EVERY: Duration = Duration::from_millis(100);

/// The longest the fixture takes to go away and come back: one whole cycle,
/// plus slack for a loaded runner's Samba start.
const ONE_CYCLE: Duration = Duration::from_secs(15);

impl FlakyShare {
    fn watch() -> Self {
        let volume = Arc::new(std::sync::OnceLock::<Arc<SmbVolume>>::new());
        let up = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let downs = Arc::new(std::sync::atomic::AtomicU64::new(0));
        let (cell, is_up, went_down) = (Arc::clone(&volume), Arc::clone(&up), Arc::clone(&downs));
        let watch = tokio::spawn(async move {
            loop {
                let answers = match cell.get() {
                    Some(share) => Self::answers(share).await,
                    None => match connect_flaky(VolumeHost::detached()).await {
                        Some(share) => {
                            let _ = cell.set(Arc::new(share));
                            true
                        }
                        None => false,
                    },
                };
                if is_up.swap(answers, Ordering::SeqCst) && !answers {
                    went_down.fetch_add(1, Ordering::SeqCst);
                }
                // allowed-test-sleep: the watch's probe cadence; the fixture's
                // up and down halves have no event to wait on.
                tokio::time::sleep(PROBE_EVERY).await;
            }
        });
        Self {
            volume,
            up,
            downs,
            watch,
        }
    }

    /// One look: the share's session is live (rebuilt if it had to be) and a
    /// file the image bakes in reads.
    async fn answers(share: &SmbVolume) -> bool {
        if !share.connection_state().is_some_and(ConnectionState::is_live) && share.attempt_reconnect().await.is_err() {
            return false;
        }
        share.get_metadata(Path::new("sample.txt")).await.is_ok()
    }

    fn is_up(&self) -> bool {
        self.up.load(Ordering::SeqCst) && self.volume.get().is_some()
    }

    fn downs(&self) -> u64 {
        self.downs.load(Ordering::SeqCst)
    }

    /// The watch's connection, once it has one.
    fn volume(&self) -> &SmbVolume {
        self.volume.get().expect("the watch connects before the share reads up")
    }

    /// Waits for the share to go away (if it hasn't since `downs_before`) and
    /// come back: the start of a whole up half, about 5 s to work in.
    async fn fresh_window_after(&self, downs_before: u64, description: &str) {
        wait_until_async(ONE_CYCLE, description, || self.downs() > downs_before).await;
        wait_until_async(ONE_CYCLE, description, || self.is_up()).await;
    }
}

impl Drop for FlakyShare {
    fn drop(&mut self) {
        self.watch.abort();
    }
}

/// Files in the soak's folder: enough that the copy is still going after
/// every one of [`SOAK_PAUSES`].
const SOAK_FILES: usize = 48;

/// The copy parks at each of these files-done counts and stays parked until
/// the share has gone away under it, so it meets a dead session that many times
/// at least, on top of any cut the cycle lands mid-write.
const SOAK_PAUSES: [usize; 3] = [8, 20, 32];

/// The soak's waits together: a fresh window to start in, one cycle per pause,
/// and the copy. Under the cells' 120 s nextest cap.
const SOAK_BUDGET: Duration = Duration::from_secs(105);

/// The production reconnect attempt's cap (`OutagePolicy::attempt_timeout`).
/// The fixture's 5 s halves are shorter than smb2's own 10 s handshake
/// deadline, so a shorter cap here would drop handshakes production keeps.
const SOAK_ATTEMPT_TIMEOUT: Duration = Duration::from_secs(15);

/// How long the soak's copy may go without moving: one abandoned reconnect
/// attempt ([`SOAK_ATTEMPT_TIMEOUT`]) plus a whole cycle to come back in. An
/// attempt that sits out its whole cap is the outage wait's documented worst
/// case (`transfer/DETAILS.md` § "Waiting out a volume that went away"), ❌ not
/// a hang; nothing moving for longer is one.
const SOAK_QUIET: Duration = Duration::from_secs(30);

/// Waits for `condition` while the soak's copy keeps moving ([`SOAK_QUIET`]),
/// until `deadline`.
#[track_caller]
fn while_soaking<'a>(
    deadline: tokio::time::Instant,
    events: &'a CollectorEventSink,
    description: &'a str,
    condition: impl FnMut() -> bool + 'a,
) -> impl Future<Output = ()> + 'a {
    let left = deadline.saturating_duration_since(tokio::time::Instant::now());
    wait_until_async_while_moving(SOAK_QUIET, left, description, move || progress_of(events), condition)
}

/// What the soak's share holds at an even `index` before the copy, for the
/// Overwrite to replace. Odd indexes start free.
fn seeded(index: usize) -> bool {
    index.is_multiple_of(2)
}

/// The close-out soak: a multi-file copy with Overwrite onto a share that keeps
/// going away (the `flaky` fixture, 5 s up, 5 s down), part of it landing over
/// originals already there. The copy parks at [`SOAK_PAUSES`] and the share
/// dies under it each time, so in-flight writes, single-shot creates, and swap
/// landings all meet a dead session more than once. It ends with every file
/// landed byte-exact, every original replaced, and nothing of ours left on the
/// share: no `.cmdr-tmp-*`, no ` (recovered)`.
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "Requires Docker SMB containers (./apps/desktop/test/smb-servers/start.sh)"]
async fn smb_integration_an_overwrite_copy_rides_out_a_share_that_keeps_going_away() {
    // A failure here is a story across reconnects; `RUST_LOG=copy=debug` tells it.
    let _ = env_logger::builder().is_test(true).try_init();
    let share = FlakyShare::watch();
    let base = test_dir_name();
    let batch = PathBuf::from(&base).join("batch");

    // Seed, connect the copy's volume, and start, all at the top of an up
    // half: the pre-flight scan doesn't wait out a drop.
    share
        .fresh_window_after(share.downs(), "an up half of the flaky share to seed in")
        .await;
    let direct = share.volume();
    ensure_clean(direct, &base).await;
    direct
        .create_directory(Path::new(&base))
        .await
        .expect("the scratch dir");
    direct.create_directory(&batch).await.expect("the seeded folder");
    for index in (0..SOAK_FILES).filter(|index| seeded(*index)) {
        direct
            .create_file(&batch.join(format!("f-{index:02}.bin")), &original(index))
            .await
            .expect("an original on the share, inside an up half");
    }
    // The app's own wiring, so the copy runs at the app's SMB concurrency (a
    // window of ten): the deadlock this cell found needed a wide window.
    let through: Arc<dyn Volume> = Arc::new(
        connect_flaky(crate::volume_host::host())
            .await
            .expect("the copy's own connection, inside an up half"),
    );
    let generation_at_start = through.connection_generation().expect("SMB answers a generation");

    let local = tempfile::TempDir::new().expect("a local source dir");
    let folder = local.path().join("batch");
    std::fs::create_dir(&folder).expect("the source folder");
    for index in 0..SOAK_FILES {
        std::fs::write(folder.join(format!("f-{index:02}.bin")), content(index)).expect("a source file");
    }
    let source: Arc<dyn Volume> = Arc::new(cmdr_local::LocalPosixVolume::local_folder(
        "src",
        local.path().to_path_buf(),
        crate::volume_host::host(),
    ));

    let engine = TestOperations::new();
    let state = Arc::new(
        new_operation_state(&engine, Duration::from_millis(50)).with_outage_policy(OutagePolicy {
            budget: Duration::from_secs(60),
            cadence: CADENCE,
            poll_every: Duration::from_millis(20),
            attempt_timeout: SOAK_ATTEMPT_TIMEOUT,
        }),
    );
    let op = engine.register_state("smb-flaky-soak", Arc::clone(&state));
    let events = Arc::new(CollectorEventSink::new());
    // Parks the copy at each pause point, from its own emit, once each.
    let pausing = Arc::clone(&state);
    let next_pause = std::sync::atomic::AtomicUsize::new(0);
    let sink: Arc<dyn OperationEventSink> = Arc::new(ProgressTap {
        inner: Arc::clone(&events),
        tap: Box::new(move |event| {
            if !matches!(event.phase, WriteOperationPhase::Copying) {
                return;
            }
            let due = next_pause.load(Ordering::SeqCst);
            if SOAK_PAUSES.get(due).is_some_and(|at| event.files_done >= *at)
                && next_pause
                    .compare_exchange(due, due + 1, Ordering::SeqCst, Ordering::SeqCst)
                    .is_ok()
            {
                pausing.pause_gate.pause();
            }
        }),
    });

    let copy_state = Arc::clone(&state);
    let op_id = op.id().to_owned();
    let dest = Arc::clone(&through);
    let dest_path = PathBuf::from(&base);
    let copy = tokio::spawn(async move {
        copy_volumes_with_progress(
            sink,
            op_id.as_str(),
            &copy_state,
            source,
            &[PathBuf::from("batch")],
            dest,
            &dest_path,
            &VolumeCopyConfig {
                conflict_resolution: ConflictResolution::Overwrite,
                progress_interval_ms: 0,
                ..VolumeCopyConfig::default()
            },
        )
        .await
        .map_err(|failure| format!("{:?}", failure.error))
    });

    let deadline = tokio::time::Instant::now() + SOAK_BUDGET;
    for pause in SOAK_PAUSES {
        while_soaking(deadline, &events, "the copy to park at its next pause point", || {
            state.pause_gate.is_paused() || copy.is_finished()
        })
        .await;
        assert!(
            !copy.is_finished(),
            "the copy finished before its pause point at {pause}: the folder is too small for the soak"
        );
        let downs = share.downs();
        wait_until_async(ONE_CYCLE, "the share to go away under the parked copy", || {
            share.downs() > downs
        })
        .await;
        state.pause_gate.resume();
    }
    while_soaking(deadline, &events, "the copy to finish through the cycles", || {
        copy.is_finished()
    })
    .await;
    let result = copy.await.expect("the copy task itself must not panic");
    result.expect("a share that keeps coming back within the budget must not fail the copy");
    let reconnects = through.connection_generation().expect("SMB answers a generation") - generation_at_start;
    assert!(
        reconnects >= SOAK_PAUSES.len() as u64,
        "the copy met the share gone and reconnected at every pause at least (generation moved by {reconnects})"
    );

    // Read back inside an up half, a fresh one if the first look gets cut.
    let mut landed = None;
    for _ in 0..2 {
        let downs = share.downs();
        wait_until_async(ONE_CYCLE, "the share to be up to read back", || share.is_up()).await;
        match read_back(share.volume(), &batch).await {
            Ok(read) => {
                landed = Some(read);
                break;
            }
            Err(_) => share.fresh_window_after(downs, "an up half to read back in").await,
        }
    }
    let (hashes, names) = landed.expect("the read-back fits in an up half");
    for (index, hash) in hashes.iter().enumerate() {
        assert_eq!(
            *hash,
            hash_bytes(&content(index)),
            "f-{index:02}.bin must land byte-exact{}",
            if seeded(index) { ", replacing its original" } else { "" }
        );
    }
    let leftovers: Vec<&String> = names
        .iter()
        .filter(|name| name.contains(".cmdr-tmp-") || name.contains("(recovered)"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "no temp survives the cycles, and nothing is parked aside: {leftovers:?}"
    );
    assert_eq!(names.len(), SOAK_FILES, "exactly the copied files: {names:?}");

    wait_until_async(ONE_CYCLE, "the share to be up to clean up", || share.is_up()).await;
    ensure_clean(share.volume(), &base).await;
}

/// Every soak file's hash in index order, and every name in the folder.
async fn read_back(share: &SmbVolume, batch: &Path) -> Result<(Vec<[u8; 32]>, Vec<String>), VolumeError> {
    let names = share
        .list_directory(batch, None)
        .await?
        .into_iter()
        .map(|entry| entry.name)
        .collect();
    let mut hashes = Vec::with_capacity(SOAK_FILES);
    for index in 0..SOAK_FILES {
        let mut stream = share.open_read_stream(&batch.join(format!("f-{index:02}.bin"))).await?;
        let mut hasher = blake3::Hasher::new();
        while let Some(chunk) = stream.next_chunk().await {
            hasher.update(&chunk?);
        }
        hashes.push(*hasher.finalize().as_bytes());
    }
    Ok((hashes, names))
}

/// The `.cmdr-tmp-*` names in `dir` on the share, read through `volume`.
async fn temps_in(volume: &SmbVolume, dir: &str) -> Vec<String> {
    volume
        .list_directory(Path::new(dir), None)
        .await
        .expect("the destination folder")
        .into_iter()
        .map(|entry| entry.name)
        .filter(|name| name.contains(".cmdr-tmp-"))
        .collect()
}
