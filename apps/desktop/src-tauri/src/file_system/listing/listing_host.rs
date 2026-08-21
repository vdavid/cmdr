//! The app's answer to a backend's "what are the panes showing?" question.
//!
//! A storage backend can't reach `listing::caching` from its own crate, so it
//! asks through `cmdr_fs::volume::host::listings::ListingHost` and this is what
//! the app installs. A pure adapter: the cache, the diff emitter, the index sync,
//! and the cloud-badge invalidation all stay where they are, and every decision
//! is still made by the functions below it.
//!
//! ❌ The seam is per mutation, never per entry. That rule is the backend's to
//! keep, but it's this module's to know: `notify_directory_changed` walks every
//! cached listing on the volume, so a per-entry caller would turn one directory
//! into a quadratic sweep.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use cmdr_fs::volume::host::listings::ListingHost;

use super::caching;
use super::metadata::FileEntry;
use crate::file_system::volume::DirectoryChange;

/// Answers a backend's listing questions from the app's real listing cache.
pub struct AppListings;

impl ListingHost for AppListings {
    fn directory_changed(&self, volume_id: &str, parent_path: &Path, change: DirectoryChange) {
        caching::notify_directory_changed(volume_id, parent_path, change);
    }

    fn authoritative_listing(&self, volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
        caching::try_get_authoritative_listing(volume_id, path)
    }

    fn refresh_archive_listings<'a>(
        &'a self,
        volume_id: &'a str,
        archive_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(caching::refresh_archive_listings(volume_id, archive_path))
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::sync::Arc;

    use super::*;
    use crate::file_system::listing::caching_test_support::{TestListing, WatchCoverageVolume, unique_test_id};
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::volume::{InMemoryVolume, Volume, WatchCoverage};

    fn entry_at(dir: &str, name: &str) -> FileEntry {
        FileEntry {
            extended_metadata_loaded: true,
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
            .insert("listing-host-watched");

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
            .insert("listing-host-unwatched");

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
            .insert("listing-host-quiet");

        AppListings.directory_changed(
            &volume_id,
            Path::new("/host/quiet/elsewhere"),
            DirectoryChange::Added(entry_at("/host/quiet/elsewhere", "new.txt")),
        );

        assert_eq!(listing.entry_names(), ["kept.txt"]);
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
            .insert("listing-host-archive");

        tauri::async_runtime::block_on(AppListings.refresh_archive_listings(&volume_id, &PathBuf::from(root)));

        assert_eq!(
            listing.entry_names(),
            ["added-since.txt"],
            "a stale inner listing must be replaced by what the volume now reports"
        );

        drop(listing);
        get_volume_manager().unregister(&volume_id);
    }
}
