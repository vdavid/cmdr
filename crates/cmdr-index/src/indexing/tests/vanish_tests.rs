//! What a scan parked on a real drive does, pinned on a real HFS+ disk image (macOS):
//! what the index does TODAY when the drive vanishes mid-scan, and that a parked
//! walker worker still holds the drive once its manager is gone.
//!
//! A real `IndexManager` scans a tree on the image, the way `event_stream_tests.rs`
//! drives one over a temp dir. The walker's test-only park point
//! (`scanner/walker/park.rs`) holds the walk between directories after a few reads,
//! with no directory handle open on the image. The pin force-detaches the image there
//! and lets the walk go on, so the drive vanishes BETWEEN reads, never under an open
//! fd; the control releases the same park without detaching, so the pin's outcome is
//! the vanish's doing and not the park's.
//!
//! `#[ignore]`d: they attach real disk images. Hand-run with
//! `cargo nextest run -p cmdr-index --run-ignored only -E 'test(indexing::tests::vanish_tests::)'`.
//! Serialized in the `disk-image` nextest group, and machine-wide by the harness's
//! session lock. HFS+ only: the kernel `hfs` driver takes a forced detach under a live
//! FSEvents stream, where FSKit `msdos` is the kernel-panic surface.

use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cmdr_fs::testing::disk_images::{DiskImage, DiskImageSession, ImageSpec};
use cmdr_fs::testing::wait_until_async;

use crate::indexing::events::{ActivityPhase, IndexEvent, IndexEventKind, RecordingSink};
use crate::indexing::hold::{self, HoldKind, VolumeWork};
use crate::indexing::host::volumes::{FakeVolumeProvider, MountIdentity, VolumeProvider, install_for_test};
use crate::indexing::lifecycle::manager::IndexManager;
use crate::indexing::lifecycle::signals::VolumeSignals;
use crate::indexing::scanner::park::ParkHandle;
use crate::indexing::store::{IndexStore, UnreadableCause};
use crate::indexing::volume::IndexVolumeKind;

const VOLUME_ID: &str = "vanish-pin";

/// The filesystem the image is, as far as the host's mount table is concerned.
const IMAGE_DRIVE: MountIdentity = MountIdentity::from_raw(0x0100_00A1);

/// The tree: the root plus 80 folders of 50 empty files each. Its size doesn't decide
/// when the drive leaves (the park point does); it only has to leave most folders
/// unread when the walk parks.
const FOLDERS: usize = 80;
const FILES_PER_FOLDER: usize = 50;

/// The walk parks once it has read this many directories. Workers already past the
/// park point finish their task first, so it holds after at most this plus one per
/// worker, still far short of the 81.
const PARK_AFTER_DIRS: u64 = 5;

/// What the test does while the walk is parked.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AtThePark {
    /// Force-detach the image, then release the walk.
    DetachTheImage,
    /// Release the walk with the image still attached.
    LeaveTheImage,
}

/// Everything a run looked at, dumped by the assertions so a changed outcome says
/// what it changed to.
#[derive(Debug)]
struct Observed {
    files_created: usize,
    complete_at_the_park: bool,
    aborted: bool,
    reached_live: bool,
    scan_completed_at: Option<String>,
    rows: i64,
    abandoned_marks: usize,
    kinds: Vec<IndexEventKind>,
}

fn populate(root: &Path) -> usize {
    for folder in 0..FOLDERS {
        let dir = root.join(format!("folder-{folder:03}"));
        std::fs::create_dir(&dir).expect("create a folder on the image");
        for file in 0..FILES_PER_FOLDER {
            std::fs::File::create(dir.join(format!("file-{file:04}.txt"))).expect("create a file on the image");
        }
    }
    FOLDERS * FILES_PER_FOLDER
}

/// Scans a fresh tree on an HFS+ image through the park point, does `at_the_park`
/// while the walk is held, and reads back what the index wrote once the scan went
/// live (or aborted) and the manager shut down.
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the process-wide volume-provider slot for the whole run, and the scan reads that slot while we await its outcome: holding it across the await IS the point"
)]
async fn scan_through_the_park(at_the_park: AtThePark) -> Observed {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
    let root = image.volumes()[0].mount_point.clone();
    let files_created = populate(&root);

    // The host's mount table, which is what every presence gate asks — ❗ never the
    // filesystem. The real one lives in the app (`file_system::index_provider`), so
    // this pin plays that part: the image is listed while it is attached, and taken
    // out of the table at the moment `hdiutil` really detaches it. Everything else
    // here is real — the walk, the park, the detach, and the gates' own decisions.
    let _serialized = crate::indexing::handle::test_lock();
    let volumes = FakeVolumeProvider::shared();
    volumes.mount(&root, IMAGE_DRIVE);
    let _installed = install_for_test(Arc::clone(&volumes) as Arc<dyn VolumeProvider>);

    let data = tempfile::tempdir().expect("index data dir");
    let db_path = data.path().join(format!("index-{VOLUME_ID}.db"));
    let events = Arc::new(RecordingSink::new());
    let mut manager = IndexManager::new_for_kind(
        VOLUME_ID.to_string(),
        root.clone(),
        db_path.clone(),
        IndexVolumeKind::LocalExternal,
        true,
        VolumeSignals::new(
            Arc::new(std::sync::Mutex::new(None)),
            Arc::clone(&events) as Arc<dyn crate::EventSink>,
        ),
        // The generation carries the image's mount identity, the way a real
        // `LocalExternal` start's reservation captures it.
        VolumeWork::for_test_on(VOLUME_ID, IMAGE_DRIVE),
    )
    .expect("build the index manager");

    let park = ParkHandle::arm(&root, PARK_AFTER_DIRS);
    let scan_started = Instant::now();
    manager.start_scan("vanish pin").expect("start the scan");
    let parked = tokio::task::block_in_place(|| park.wait_until_parked(Duration::from_secs(20)));
    assert!(
        parked,
        // allowed-pluralize-noun: PARK_AFTER_DIRS is a compile-time constant of 5, never 1.
        "the walk of {} never parked after {PARK_AFTER_DIRS} directories ({:?} after the scan started; events {:?})",
        root.display(),
        scan_started.elapsed(),
        events.kinds_for(VOLUME_ID),
    );
    let complete_at_the_park = events.kinds_for(VOLUME_ID).contains(&IndexEventKind::ScanComplete);

    if at_the_park == AtThePark::DetachTheImage {
        image
            .force_detach()
            .expect("force-detach the image while the walk is parked");
        // The filesystem is gone, so the mount table stops listing it — the one
        // fact the index gets to learn about a drive that left.
        volumes.mark_unmounted(&root);
    }
    park.release();

    let ended = |event: &IndexEvent| match event {
        IndexEvent::ScanAborted { volume_id } => volume_id == VOLUME_ID,
        IndexEvent::PhaseChanged { volume_id, phase } => volume_id == VOLUME_ID && matches!(phase, ActivityPhase::Live),
        _ => false,
    };
    wait_until_async(
        Duration::from_secs(20),
        "the scan to go live or abort after the park",
        || events.events().iter().any(ended),
    )
    .await;
    // Drains the writer, so whatever the completion path queued has landed.
    manager.shutdown();

    let conn = IndexStore::open_read_connection(&db_path).expect("open a read connection");
    let rows: i64 = conn
        .query_row("SELECT COUNT(*) FROM entries", [], |row| row.get(0))
        .expect("count the rows");
    let abandoned_marks = conn
        .prepare("SELECT unreadable_cause FROM entries WHERE unreadable_cause != 0")
        .expect("prepare the marks query")
        .query_map([], |row| row.get::<_, i64>(0))
        .expect("read the marks")
        .filter_map(Result::ok)
        .filter(|stored| matches!(UnreadableCause::from_stored(*stored), Some(UnreadableCause::Abandoned)))
        .count();
    let recorded = events.events();
    Observed {
        files_created,
        complete_at_the_park,
        aborted: recorded
            .iter()
            .any(|event| matches!(event, IndexEvent::ScanAborted { volume_id } if volume_id == VOLUME_ID)),
        reached_live: recorded.iter().any(|event| {
            matches!(event, IndexEvent::PhaseChanged { volume_id, phase: ActivityPhase::Live } if volume_id == VOLUME_ID)
        }),
        scan_completed_at: IndexStore::get_meta(&conn, "scan_completed_at").expect("read the completion stamp"),
        rows,
        abandoned_marks,
        kinds: events.kinds_for(VOLUME_ID),
    }
}

/// Asserts the park held the walk mid-scan, which both tests need to mean anything.
fn assert_the_park_held_the_walk(observed: &Observed) {
    assert!(
        !observed.complete_at_the_park,
        "the scan finished before the park held it, so this pins nothing (events {:?})",
        observed.kinds
    );
}

/// The control: the same scan through the same park, released without a detach,
/// indexes every row and stamps completion honestly.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
async fn a_walk_released_from_the_park_on_a_drive_that_stays_indexes_every_row() {
    let observed = scan_through_the_park(AtThePark::LeaveTheImage).await;

    assert_the_park_held_the_walk(&observed);
    assert!(observed.reached_live && !observed.aborted, "{observed:#?}");
    assert!(observed.scan_completed_at.is_some(), "{observed:#?}");
    assert!(
        observed.rows >= (FOLDERS + observed.files_created) as i64,
        "every folder and file is indexed: {observed:#?}"
    );
    assert_eq!(observed.abandoned_marks, 0, "{observed:#?}");
}

/// ❗ The pin this whole milestone exists for. The walk parks after a few
/// directories, the image is force-detached, and the walk goes on over a drive that
/// is no longer there.
///
/// Before the gates it went LIVE and stamped `scan_completed_at` over ground nobody
/// walked, leaving an index with no rows that every later launch trusted (verified
/// on macOS 26.6.2, hand runs, 2026-09-14). Now the reads that fail because the
/// drive left condemn nothing, the scan reports the vanish, and nothing claims the
/// volume is done — so the next mount walks it again.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "attaches a real HFS+ disk image via hdiutil and force-detaches it mid-scan; run with --run-ignored"]
async fn a_drive_that_vanishes_mid_scan_claims_no_completion() {
    let observed = scan_through_the_park(AtThePark::DetachTheImage).await;

    assert_the_park_held_the_walk(&observed);
    assert!(
        observed.aborted && !observed.reached_live,
        "a vanish is reported as one, rather than going live over a drive that isn't there: {observed:#?}"
    );
    assert!(
        observed.scan_completed_at.is_none(),
        "❌ no completion stamp may land over ground nobody walked: {observed:#?}"
    );
    assert_eq!(
        observed.abandoned_marks, 0,
        "and nothing is condemned on the word of reads that failed with the drive: {observed:#?}"
    );
}

/// A walker worker parked on a real drive keeps it held after the manager has drained
/// and gone: the drain joins none of the walk's threads, and a stop that answered
/// "released" here would unmount under the read that worker is about to make. It lets
/// go once it gets past that read.
#[tokio::test(flavor = "multi_thread")]
#[ignore = "attaches a real HFS+ disk image via hdiutil; run with --run-ignored"]
async fn a_walker_worker_parked_on_a_real_drive_holds_it_after_the_manager_is_gone() {
    let session = DiskImageSession::acquire();
    let image = DiskImage::attach(&session, ImageSpec::Hfs).expect("attach the image");
    let root = image.volumes()[0].mount_point.clone();
    populate(&root);

    let data = tempfile::tempdir().expect("index data dir");
    let events = Arc::new(RecordingSink::new());
    let mut manager = IndexManager::new_for_kind(
        VOLUME_ID.to_string(),
        root.clone(),
        data.path().join(format!("index-{VOLUME_ID}.db")),
        IndexVolumeKind::LocalExternal,
        true,
        VolumeSignals::new(
            Arc::new(std::sync::Mutex::new(None)),
            Arc::clone(&events) as Arc<dyn crate::EventSink>,
        ),
        VolumeWork::for_test(VOLUME_ID),
    )
    .expect("build the index manager");

    let park = ParkHandle::arm(&root, PARK_AFTER_DIRS);
    manager.start_scan("hold pin").expect("start the scan");
    assert!(
        tokio::task::block_in_place(|| park.wait_until_parked(Duration::from_secs(20))),
        "the walk of {} never parked (events {:?})",
        root.display(),
        events.kinds_for(VOLUME_ID),
    );

    manager.shutdown();
    drop(manager);
    let held = match hold::wait_until_released(VOLUME_ID, Duration::ZERO) {
        hold::Release::Released => Vec::new(),
        hold::Release::StillHeld(holders) => holders,
    };
    assert!(
        held.contains(&(HoldKind::WalkerWorker, 1)),
        "the parked worker still holds the drive once the manager is gone: {held:?}"
    );

    drop(park);
    assert_eq!(
        tokio::task::block_in_place(|| hold::wait_until_released(VOLUME_ID, Duration::from_secs(20))),
        hold::Release::Released,
        "and lets go once it gets past its next read"
    );
}
