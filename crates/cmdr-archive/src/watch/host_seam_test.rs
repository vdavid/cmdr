//! The live content watch, driven by real FSEvents and asserted through the host
//! seam instead of the app around it.
//!
//! An external rewrite of the backing `.zip` has to reach the host's
//! archive-refresh seam carrying the PARENT drive id and the archive path. That's
//! the whole contract this backend has with its host here, and a
//! [`RecordingListings`] observes it with no listing cache, no `VolumeManager`,
//! and no Tauri. The app-side companion
//! (`file_system/volume/backends/archive_watch_integration_test.rs`) proves the
//! other half: that a refresh re-reads the open panes.
//!
//! Every rewrite is redone each round until the watch delivers, the way
//! `downloads::watcher::observe_mutation` does. That defeats two failure modes
//! waiting alone can't: `Debouncer::watch` returns before macOS finishes arming
//! the stream, and a rewrite landing inside that window is dropped outright
//! rather than delayed; and FSEvents coalesces or drops a lone event when the
//! host is saturated (a full-suite run pins every core).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use cmdr_fs::volume::host::VolumeHost;
use cmdr_fs::volume::host::listings::{ListingHost, RecordingListings};

use crate::test_fixtures::{FixtureFile, build_zip, stored};
use crate::{ArchiveFormat, ArchiveVolume, active_watch_count};
use cmdr_fs::volume::{InMemoryVolume, Volume, WatchCoverage};

/// The parent drive id the listing cache keys archive listings on. The refresh
/// must carry this, never the archive's own registry id.
const PARENT_DRIVE: &str = "parent-drive";

/// A temp directory holding one `.zip`, plus the archive volume over it and the
/// recorder its host reports to. The zip sits in its own directory so the
/// parent-directory content watch sees nothing else.
struct WatchedArchive {
    _dir: tempfile::TempDir,
    zip_path: PathBuf,
    volume: ArchiveVolume,
    listings: Arc<RecordingListings>,
}

impl WatchedArchive {
    /// Builds the fixture zip and an `ArchiveVolume` over it, hosted by a
    /// recorder. The watch isn't started yet.
    fn new(entries: &[FixtureFile]) -> Self {
        let dir = tempfile::tempdir().expect("tempdir");
        let zip_path = dir.path().join("bundle.zip");
        std::fs::write(&zip_path, build_zip(entries)).expect("write fixture zip");

        let listings = Arc::new(RecordingListings::new());
        let host = VolumeHost::builder()
            .listings(Arc::clone(&listings) as Arc<dyn ListingHost>)
            .build();
        // A local-backed parent, so the archive reads the real temp file and the
        // watch can arm on its directory.
        let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("parent").with_local_fs_access());
        let volume = ArchiveVolume::new(parent, zip_path.clone(), ArchiveFormat::Zip, host);
        Self {
            _dir: dir,
            zip_path,
            volume,
            listings,
        }
    }

    /// Rewrites the archive in place (a `cp` over it, an in-place writer).
    fn rewrite(&self, entries: &[FixtureFile]) {
        std::fs::write(&self.zip_path, build_zip(entries)).expect("rewrite fixture zip");
    }

    /// Rewrites the way editors and safe-overwrites do: build a sibling temp,
    /// then atomically rename it over the archive. This swaps the file's inode,
    /// which a file-pinned watch would miss.
    fn rewrite_via_temp_rename(&self, entries: &[FixtureFile]) {
        let tmp = self.zip_path.with_extension("zip.tmp");
        std::fs::write(&tmp, build_zip(entries)).expect("write temp zip");
        std::fs::rename(&tmp, &self.zip_path).expect("rename temp over zip");
    }

    /// Redoes `rewrite` on a fixed cadence until the host is asked to refresh,
    /// then asserts the refresh named this archive on its parent drive. All of it
    /// sits inside ONE 15 s budget, deliberately under the 20 s nextest cap, so a
    /// merely-slow delivery still lands here instead of racing a SIGTERM. This
    /// self-healing is what lets both callers sit in the `retries = 0` group in
    /// `.config/nextest.toml`.
    async fn drive_until_refreshed(&self, what: &str, mut rewrite: impl FnMut()) {
        const BUDGET: Duration = Duration::from_secs(15);
        // A live stream refreshes within the debounce (a few hundred ms), so
        // re-rewrite on that cadence: a dropped event triggers another rewrite
        // rather than burning the whole budget.
        const ROUND: Duration = Duration::from_millis(750);

        let description = format!("{what} to reach the host's archive-refresh seam (FSEvents watch starved)");
        let mut next_rewrite = Instant::now();
        cmdr_fs::testing::wait_until_async(BUDGET, &description, || {
            if Instant::now() >= next_rewrite {
                rewrite();
                next_rewrite = Instant::now() + ROUND;
            }
            !self.listings.archive_refreshes().is_empty()
        })
        .await;

        assert_eq!(
            self.listings.archive_refreshes()[0],
            (PARENT_DRIVE.to_string(), self.zip_path.clone()),
            "the refresh must name the parent drive and the archive path, not the archive's own volume id"
        );
    }
}

/// An in-place rewrite reaches the refresh seam, and the volume reports the watch
/// as live for as long as it holds it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_rewrite_asks_the_host_to_refresh_the_archive_listings() {
    let fixture = WatchedArchive::new(&[stored("a.txt", b"a".to_vec())]);
    assert_eq!(
        fixture.volume.listing_watch_coverage(&fixture.zip_path),
        WatchCoverage::None,
        "a volume with no watch must not claim its listings are watched"
    );

    fixture.volume.start_content_watch(PARENT_DRIVE, WatchCoverage::EveryWriter);
    assert_eq!(
        fixture.volume.listing_watch_coverage(&fixture.zip_path),
        WatchCoverage::EveryWriter,
        "an archive with an established watch must report coverage"
    );
    // `active_watch_count` is process-wide, so a sibling test's watch can be live
    // at the same time under a threaded runner: assert presence, not an exact
    // total. That eviction releases the handle is pinned app-side, where the
    // registry's `Arc` is observable
    // (`archive_watch_integration_test::lru_eviction_releases_the_archive_and_its_watch`).
    assert!(
        active_watch_count() >= 1,
        "the live watch must count toward the active total"
    );

    fixture
        .drive_until_refreshed("an in-place rewrite", || {
            fixture.rewrite(&[stored("a.txt", b"a".to_vec()), stored("b.txt", b"bb".to_vec())]);
        })
        .await;
}

/// The editor-style inode swap: rewriting via a sibling temp + atomic rename (a
/// NEW inode) must still reach the seam. A file-pinned watch would go silent
/// here; the parent-directory watch catches it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_temp_rename_swap_still_asks_for_a_refresh() {
    let fixture = WatchedArchive::new(&[stored("a.txt", b"a".to_vec())]);
    fixture.volume.start_content_watch(PARENT_DRIVE, WatchCoverage::EveryWriter);

    fixture
        .drive_until_refreshed("a temp+rename inode swap", || {
            fixture.rewrite_via_temp_rename(&[stored("a.txt", b"a".to_vec()), stored("b.txt", b"bb".to_vec())]);
        })
        .await;
}

/// An archive on an OS-mounted network share reports only what the ceiling its
/// caller armed it with allows, even though the watch itself is genuinely live.
///
/// The watch is an FSEvents watch on the archive's parent DIRECTORY, so on a
/// share it sees this machine's writes and nothing another client does. Without
/// the ceiling, a live watch would answer `EveryWriter` and let a pre-flight scan
/// reuse a listing that another client had already invalidated. The app resolves
/// the ceiling in `manager::archive_routing::watch_coverage_for_backing_file`;
/// this pins that the backend actually honors it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn an_archive_on_a_network_mount_reports_only_this_machine() {
    let fixture = WatchedArchive::new(&[stored("a.txt", b"a".to_vec())]);

    fixture
        .volume
        .start_content_watch(PARENT_DRIVE, WatchCoverage::ThisMachineOnly);

    assert_eq!(
        fixture.volume.listing_watch_coverage(&fixture.zip_path),
        WatchCoverage::ThisMachineOnly,
        "a live watch must not report more coverage than its backing storage allows"
    );
}

/// A remote parent has no local path for `notify`, so no watch establishes and
/// the volume keeps reporting its listings as unwatched — a pre-flight scan then
/// re-reads instead of trusting a cache nothing is keeping fresh.
#[tokio::test]
async fn a_remote_parent_establishes_no_watch() {
    let listings = Arc::new(RecordingListings::new());
    let host = VolumeHost::builder()
        .listings(Arc::clone(&listings) as Arc<dyn ListingHost>)
        .build();
    // No `with_local_fs_access`, and the path doesn't exist locally: `notify` has
    // nothing to arm on.
    let parent: Arc<dyn Volume> = Arc::new(InMemoryVolume::new("remote"));
    let path = PathBuf::from("/nowhere-remote/bundle.zip");
    let volume = ArchiveVolume::new(parent, path.clone(), ArchiveFormat::Zip, host);

    volume.start_content_watch(PARENT_DRIVE, WatchCoverage::EveryWriter);

    assert_eq!(
        volume.listing_watch_coverage(&path),
        WatchCoverage::None,
        "a watch that couldn't establish must never claim freshness"
    );
    assert!(listings.archive_refreshes().is_empty(), "no watch, no refresh");
}
