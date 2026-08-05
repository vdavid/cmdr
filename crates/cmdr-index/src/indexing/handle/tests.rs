//! The handle's own tests: the single-instance contract, and the acceptance scan
//! that proves the index runs with nothing app-side in the room.

use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::testing::wait_until_async;
use cmdr_fs::volume::{InMemoryVolume, SmbConnectionState, Volume};

use crate::indexing::events::{EventSink, IndexEventKind, RecordingSink};
use crate::indexing::handle::{Index, IndexBuildError, StartOutcome};
use crate::indexing::host::volumes::FakeVolumeProvider;
use crate::indexing::read::coverage::{CoverageDimension, CoverageToken};

/// Building twice is not two indexes. The statics behind the seams are
/// process-wide, so a second `build()` reports [`IndexBuildError::AlreadyBuilt`]
/// rather than handing back a handle that would quietly share the first one's
/// state. The variant goes away when the statics move into the handle.
#[test]
fn a_second_build_reports_already_built_rather_than_a_second_index() {
    let _serialized = crate::indexing::handle::test_lock();
    let _released = Index::release_build_claim_for_test();
    let data = tempfile::tempdir().expect("index data dir");
    // `build()` is the real thing, so it installs the config PERMANENTLY —
    // `install_for_test`'s restore-on-drop is exactly what it doesn't do. Without
    // this guard the data dir stays set for the rest of the binary, and
    // `host::config::tests::an_unset_data_dir_is_an_error_not_an_empty_path` fails
    // for anyone unlucky enough to run after it.
    let _config = crate::indexing::host::config::install_data_dir_for_test(data.path());

    Index::builder()
        .data_dir(data.path())
        .build()
        .expect("the first build claims the process's index");

    match Index::builder().data_dir(data.path()).build() {
        Err(IndexBuildError::AlreadyBuilt) => {}
        Ok(_) => panic!("a second build must not hand back a second index"),
    }
}

/// Three files and a subdirectory on a volume that exists only in memory.
fn in_memory_share(root: &str) -> Arc<dyn Volume> {
    let entries = vec![
        FileEntry::new("sub".into(), format!("{root}/sub"), true, false),
        FileEntry {
            size: Some(11),
            ..FileEntry::new("a.txt".into(), format!("{root}/a.txt"), false, false)
        },
        FileEntry {
            size: Some(22),
            ..FileEntry::new("b.txt".into(), format!("{root}/b.txt"), false, false)
        },
        FileEntry {
            size: Some(33),
            ..FileEntry::new("c.txt".into(), format!("{root}/sub/c.txt"), false, false)
        },
    ];
    Arc::new(
        InMemoryVolume::with_entries("Acceptance", entries)
            .with_root(root)
            .with_smb_connection_state(SmbConnectionState::Direct),
    )
}

/// The acceptance test for the whole extraction: a full scan, start to finish,
/// driven through the public handle over an `InMemoryVolume`, reporting into a
/// `RecordingSink`. Nothing app-side is in the room — no `AppHandle`, no
/// `VolumeManager`, no Tauri event, no real disk beneath the volume.
///
/// If a hidden global ever starts carrying something the handle should, this is
/// what fails: the scan would either not start or report into the wrong sink.
#[tokio::test(flavor = "multi_thread")]
#[allow(
    clippy::await_holding_lock,
    reason = "the lock serializes the process-wide seams for the whole scan; holding it across the awaits IS the point"
)]
async fn a_handle_scans_an_in_memory_volume_and_reports_to_its_own_sink() {
    let _serialized = crate::indexing::handle::test_lock();
    let data = tempfile::tempdir().expect("index data dir");
    // A platform-appropriate mount root. Read routing sends a path to a per-mount
    // index only when it sits under an external-mount prefix, and those differ per
    // OS (`/Volumes/` on macOS, `/mnt/` and `/media/` on Linux). A hardcoded
    // `/Volumes/…` routes back to `root`'s index on Linux, where nothing scanned it.
    #[cfg(target_os = "macos")]
    let root = "/Volumes/acceptance";
    #[cfg(not(target_os = "macos"))]
    let root = "/media/acceptance";
    let volume_id = "smb-acceptance-test";

    let volumes = FakeVolumeProvider::shared();
    volumes.register(volume_id, in_memory_share(root)).mark_network(root);

    let events = Arc::new(RecordingSink::new());
    let (index, _installed) = Index::builder()
        .data_dir(data.path())
        .volumes(Arc::clone(&volumes) as Arc<_>)
        .events(Arc::clone(&events) as Arc<dyn EventSink>)
        .install_for_test();

    assert_eq!(
        index.start_volume(volume_id).await.expect("the share starts indexing"),
        StartOutcome::Started
    );

    wait_until_async(
        Duration::from_secs(20),
        "the in-memory share's scan to complete",
        || events.kinds_for(volume_id).contains(&IndexEventKind::ScanComplete),
    )
    .await;

    let kinds = events.kinds_for(volume_id);
    let started_at = kinds
        .iter()
        .position(|k| *k == IndexEventKind::ScanStarted)
        .expect("a scan announces itself");
    let complete_at = kinds
        .iter()
        .position(|k| *k == IndexEventKind::ScanComplete)
        .expect("a completed scan reports ScanComplete");
    assert!(
        started_at < complete_at,
        "ScanComplete must follow ScanStarted, not lead it: {kinds:?}"
    );
    for event in events.events() {
        if let Some(vid) = event.volume_id() {
            assert_eq!(vid, volume_id, "the scan reported under the wrong volume: {event:?}");
        }
    }

    // And the index can answer for what it just walked, through the handle only.
    let children = index
        .list_children(root)
        .expect("the scanned root is readable")
        .expect("the scanned root is in the index");
    let mut names: Vec<&str> = children.iter().map(|row| row.name.as_str()).collect();
    names.sort_unstable();
    assert_eq!(names, ["a.txt", "b.txt", "sub"], "the walk found the share's entries");

    // And it knows there is nothing left to walk. This is the end-to-end proof for
    // coverage: a real scan-start sequence stamped the exclusion policy (without it
    // NOTHING reads as covered), a real walk marked its directories listed, and the
    // descent rule reads both back. A frontier here would mean a search over this
    // freshly indexed share would re-walk it.
    let coverage = index
        .coverage(volume_id, root, CoverageDimension::Listing)
        .expect("the scanned share answers for its own coverage");
    assert!(
        coverage.frontier.is_empty() && coverage.unreadable.is_empty(),
        "a completed walk leaves nothing to cover: {coverage:?}"
    );
    assert_eq!(
        coverage.token,
        index.coverage_token(volume_id),
        "the token a coverage answer carries is the one the volume reports"
    );
    assert_ne!(
        coverage.token,
        CoverageToken::UNINDEXED,
        "an indexed volume must not report the no-index token"
    );

    index.forget_volume(volume_id).expect("tear the test index down");
}
