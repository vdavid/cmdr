//! The listing engine as a backend's [`ListingHost`], for the demo, whose panes ARE
//! the engine and nothing else. A backend patches a pane
//! after its own write through it, and the copy engine's fresh-listing oracle
//! asks it.
//!
//! ❗ The desktop does NOT use this: its `listing_host::backends::AppListings`
//! sends every change through a composer (the drive index, the cloud badges,
//! then the panes) and answers local watch coverage from FSEvents, neither of
//! which this engine knows about.

use std::future::Future;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::sync::Arc;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::{DirectoryChange, SupportedDates};

use cmdr_listing::{DirectorySortMode, EffectiveSort, Listings, SortColumn, SortOrder, sort_entries};

/// `listings` as the [`ListingHost`] a backend reports its writes to.
pub fn backend_listings(listings: &Listings) -> Arc<dyn ListingHost> {
    Arc::new(BackendListings {
        listings: listings.clone(),
    })
}

struct BackendListings {
    listings: Listings,
}

impl ListingHost for BackendListings {
    fn directory_changed(&self, volume_id: &str, parent_path: &Path, change: DirectoryChange) {
        self.listings.directory_changed(volume_id, parent_path, change);
    }

    fn authoritative_listing(&self, volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
        self.listings.authoritative_listing(volume_id, path)
    }

    fn volumes_with_open_listings(&self, volume_id_prefix: &str) -> Vec<String> {
        self.listings.volume_ids_with_listings(volume_id_prefix)
    }

    fn open_listing_paths(&self, volume_id: &str) -> Vec<PathBuf> {
        self.listings.open_listing_paths(volume_id)
    }

    fn refresh_archive_listings<'a>(
        &'a self,
        volume_id: &'a str,
        archive_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move { self.listings.refresh_archive_listings(volume_id, archive_path).await })
    }

    fn sort_listing(&self, entries: &mut [FileEntry]) {
        sort_entries(
            &self.listings.collator(),
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
#[path = "backend_listings_test.rs"]
mod tests;
