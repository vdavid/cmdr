//! The app's answer to a backend's "what are the panes showing?" question.
//!
//! A storage backend can't reach the listing engine from its own crate, so it
//! asks through `cmdr_fs::volume::host::listings::ListingHost` and this is what
//! the app installs. The oracle, the open-listing questions, the archive refresh,
//! and the name order are the engine's own answers; `directory_changed` goes
//! through the composer (index, badges, then panes) and the local watch coverage
//! through the FSEvents watcher, which is why the app answers this seam itself
//! instead of handing backends `Listings::listing_host`.
//!
//! ❌ The seam is per mutation, never per entry. That rule is the backend's to
//! keep, but it's this module's to know: `Listings::directory_changed` walks every
//! cached listing on the volume, so a per-entry caller would turn one directory
//! into a quadratic sweep.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;

use cmdr_fs::volume::host::listings::ListingHost;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::SupportedDates;
use cmdr_fs::volume::{DirectoryChange, WatchCoverage};
use cmdr_listing::{DirectorySortMode, EffectiveSort, SortColumn, SortOrder, sort_entries};

/// Answers a backend's listing questions from the app's real listing cache.
pub struct AppListings;

impl ListingHost for AppListings {
    fn directory_changed(&self, volume_id: &str, parent_path: &Path, change: DirectoryChange) {
        super::directory_changed(volume_id, parent_path, change);
    }

    fn authoritative_listing(&self, volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
        super::listings().authoritative_listing(volume_id, path)
    }

    fn volumes_with_open_listings(&self, volume_id_prefix: &str) -> Vec<String> {
        super::listings().volume_ids_with_listings(volume_id_prefix)
    }

    fn open_listing_paths(&self, volume_id: &str) -> Vec<PathBuf> {
        super::listings().open_listing_paths(volume_id)
    }

    fn row_xattrs_may_have_changed(&self, volume_id: &str, parent_path: &Path, paths: Vec<PathBuf>) {
        crate::file_system::tags_io::refresh_rows(volume_id, parent_path, paths);
    }

    fn refresh_archive_listings<'a>(
        &'a self,
        volume_id: &'a str,
        archive_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            super::listings()
                .refresh_archive_listings(volume_id, archive_path)
                .await
        })
    }

    fn local_watch_coverage(&self, abs_path: &Path) -> WatchCoverage {
        // Volume-agnostic: the listing cache is keyed by listing id and tagged
        // with a volume id, but a local volume doesn't know its own id (the
        // registry assigns it), so this asks by path.
        let listings = super::listings().listings_for_local_path(abs_path);
        if listings.is_empty() {
            return WatchCoverage::None;
        }
        // A listing exists; report what the FSEvents watch attached to it covers.
        // There's a race window between the listing being populated and the watcher
        // being registered, during which this deliberately answers `None` (the
        // listing exists but isn't being kept fresh yet).
        //
        // The coverage was decided when the watch was armed, so this stays a pure
        // in-memory read: a `statfs` here would land in the middle of every
        // recursive scan walk. `watcher::coverage_for_watched_path` is where the
        // network-mount question is actually asked.
        let listing_ids: Vec<String> = listings.into_iter().map(|id| id.as_str().to_string()).collect();
        crate::file_system::watcher::coverage_for_listings(&listing_ids)
    }

    fn sort_listing(&self, entries: &mut [FileEntry]) {
        sort_entries(
            &super::listings().collator(),
            entries,
            // A name sort reads no date, so no volume's dates change it.
            EffectiveSort::resolve(
                SortColumn::Name,
                SortOrder::Ascending,
                DirectorySortMode::LikeFiles,
                SupportedDates::NONE,
            ),
        );
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::*;
    use crate::file_system::volume::manager::get_volume_manager;
    use cmdr_fs::testing::unique_test_id;
    use cmdr_fs::volume::{InMemoryVolume, Volume, WatchCoverage};
    use cmdr_listing::testing::{TestListing, WatchCoverageVolume};

    fn entry_at(dir: &str, name: &str) -> FileEntry {
        FileEntry {
            ..FileEntry::new(name.to_string(), format!("{dir}/{name}"), false, false)
        }
    }

    /// The oracle only answers when the volume says a live watcher is keeping the
    /// view fresh, and the answer is the cache's real contents.
    #[test]
    fn the_oracle_answers_from_the_real_cache_when_a_watcher_keeps_it_fresh() {
        let volume_id = unique_test_id("listing-host-watched");
        let path = "/host/watched";
        get_volume_manager().register(
            &volume_id,
            Arc::new(WatchCoverageVolume::new("watched-vol", WatchCoverage::EveryWriter)),
        );

        let listing = TestListing::new()
            .volume(&volume_id)
            .path(path)
            .entries(vec![entry_at(path, "a.txt"), entry_at(path, "b.txt")])
            .insert(crate::listing_host::listings(), "listing-host-watched");

        let answer = AppListings
            .authoritative_listing(&volume_id, Path::new(path))
            .expect("a watched listing is the whole point of the oracle");
        assert_eq!(
            answer.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            ["a.txt", "b.txt"]
        );

        drop(listing);
        get_volume_manager().unregister(&volume_id);
    }

    /// A cached listing nobody is keeping fresh must MISS, or a pre-flight scan
    /// would reuse a stale directory instead of asking the protocol.
    #[test]
    fn the_oracle_misses_when_no_watcher_is_keeping_the_listing_fresh() {
        let volume_id = unique_test_id("listing-host-unwatched");
        let path = "/host/unwatched";
        get_volume_manager().register(
            &volume_id,
            Arc::new(WatchCoverageVolume::new("unwatched-vol", WatchCoverage::None)),
        );

        let listing = TestListing::new()
            .volume(&volume_id)
            .path(path)
            .entries(vec![entry_at(path, "a.txt")])
            .insert(crate::listing_host::listings(), "listing-host-unwatched");

        assert!(
            AppListings.authoritative_listing(&volume_id, Path::new(path)).is_none(),
            "an unwatched listing must send the caller to the protocol"
        );

        drop(listing);
        get_volume_manager().unregister(&volume_id);
    }

    /// Reporting a change for a directory nobody is showing is the common case
    /// (a watcher fires for the whole volume), so it has to be silent and cheap
    /// rather than an error a backend has to avoid.
    #[test]
    fn reporting_a_change_nobody_is_showing_leaves_every_other_listing_alone() {
        let volume_id = unique_test_id("listing-host-quiet");
        let shown = "/host/quiet/shown";
        let listing = TestListing::new()
            .volume(&volume_id)
            .path(shown)
            .entries(vec![entry_at(shown, "kept.txt")])
            .insert(crate::listing_host::listings(), "listing-host-quiet");

        AppListings.directory_changed(
            &volume_id,
            Path::new("/host/quiet/elsewhere"),
            DirectoryChange::Added(entry_at("/host/quiet/elsewhere", "new.txt")),
        );

        assert_eq!(listing.entry_names(), ["kept.txt"]);
    }

    /// A device backend narrows a handle resolution to the storages a pane is
    /// showing, and gets each such volume once however many panes are on it. The
    /// prefix is the DEVICE id, one level above the volumes it serves.
    #[test]
    fn open_listings_are_answered_once_per_volume_under_the_device_prefix() {
        let device = unique_test_id("mtp-listing-host");
        let one = format!("{device}:65537");
        let two = format!("{device}:131073");

        let first = TestListing::new()
            .volume(&one)
            .path("/DCIM")
            .entries(vec![entry_at("/DCIM", "a.jpg")])
            .insert(crate::listing_host::listings(), "listing-host-open-1");
        let second = TestListing::new()
            .volume(&one)
            .path("/Download")
            .entries(Vec::new())
            .insert(crate::listing_host::listings(), "listing-host-open-2");
        let third = TestListing::new()
            .volume(&two)
            .path("/")
            .entries(Vec::new())
            .insert(crate::listing_host::listings(), "listing-host-open-3");

        let mut open = AppListings.volumes_with_open_listings(&device);
        open.sort();
        let mut expected = vec![one.clone(), two.clone()];
        expected.sort();
        assert_eq!(
            open, expected,
            "two panes on one storage answer once, and every storage under the device answers"
        );

        drop((first, second, third));
        assert!(
            AppListings.volumes_with_open_listings(&device).is_empty(),
            "a closed pane leaves nothing to aim a targeted refresh at"
        );
    }

    /// A poller asks which directories a volume's panes show, through the real
    /// engine: each once however many panes show it, never another volume's,
    /// and nothing once the last pane closes.
    #[test]
    fn open_listing_paths_answer_each_shown_directory_once_from_the_real_cache() {
        let volume_id = unique_test_id("listing-host-paths");
        let other = unique_test_id("listing-host-paths-other");
        let shown = |vid: &str, path: &str, tag: &str| {
            TestListing::new()
                .volume(vid)
                .path(path)
                .entries(Vec::new())
                .insert(crate::listing_host::listings(), tag)
        };
        let first = shown(&volume_id, "/host/paths/a", "listing-host-paths-1");
        let second = shown(&volume_id, "/host/paths/a", "listing-host-paths-2");
        let third = shown(&volume_id, "/host/paths/b", "listing-host-paths-3");
        let elsewhere = shown(&other, "/host/paths/c", "listing-host-paths-4");

        let mut open = AppListings.open_listing_paths(&volume_id);
        open.sort();
        assert_eq!(
            open,
            [PathBuf::from("/host/paths/a"), PathBuf::from("/host/paths/b")],
            "two panes on one directory answer once, and another volume's pane not at all"
        );

        drop((first, second, third));
        assert!(
            AppListings.open_listing_paths(&volume_id).is_empty(),
            "a closed pane leaves nothing to poll"
        );
        drop(elsewhere);
    }

    /// The archive refresh re-reads every open listing at or inside the path it's
    /// given, which is what a pane browsing INSIDE an archive has instead of a
    /// watchable filesystem path. Which events reach it is the SMB watcher's half,
    /// covered by `crates/cmdr-smb/src/volume/watcher/archive_refresh_test.rs`;
    /// what's pinned here is that the seam reaches the real refresh, not a stub.
    #[test]
    fn the_archive_refresh_re_reads_the_listings_under_its_path() {
        let volume_id = unique_test_id("listing-host-archive");
        let root = "/host/archive/bundle";
        let fresh = vec![entry_at(root, "added-since.txt")];
        get_volume_manager().register(
            &volume_id,
            Arc::new(InMemoryVolume::with_entries("archive-vol", fresh)) as Arc<dyn Volume>,
        );

        let listing = TestListing::new()
            .volume(&volume_id)
            .path(root)
            .entries(vec![entry_at(root, "stale.txt")])
            .insert(crate::listing_host::listings(), "listing-host-archive");

        // The shared fallback runtime, the one the test binary's engine spawns on.
        cmdr_fs::volume::host::VolumeHost::detached()
            .runtime()
            .block_on(AppListings.refresh_archive_listings(&volume_id, &PathBuf::from(root)));

        assert_eq!(
            listing.entry_names(),
            ["added-since.txt"],
            "a stale inner listing must be replaced by what the volume now reports"
        );

        drop(listing);
        get_volume_manager().unregister(&volume_id);
    }
}
