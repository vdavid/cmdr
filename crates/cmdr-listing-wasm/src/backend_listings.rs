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
use std::sync::OnceLock;

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::host::listings::ListingHost;
use cmdr_fs::volume::{DirectoryChange, SupportedDates};

use cmdr_listing::{DirectorySortMode, EffectiveSort, Listings, SortColumn, SortOrder, sort_entries};

/// The listing engine as the [`ListingHost`] a backend reports its writes to.
/// Late-bound: the volume needs its listing host before the engine that lists it
/// exists, so the demo builds this first and [`attach`](Self::attach)es the engine
/// once it does. Before that (and on a host that never attaches one) every
/// question answers "no pane", every change goes nowhere.
#[derive(Default)]
pub struct BackendListings {
    listings: OnceLock<Listings>,
}

impl BackendListings {
    /// Starts answering for `listings`. A second call is ignored: one engine per page.
    pub fn attach(&self, listings: &Listings) {
        let _ = self.listings.set(listings.clone());
    }
}

impl ListingHost for BackendListings {
    fn directory_changed(&self, volume_id: &str, parent_path: &Path, change: DirectoryChange) {
        if let Some(listings) = self.listings.get() {
            listings.directory_changed(volume_id, parent_path, change);
        }
    }

    fn authoritative_listing(&self, volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
        self.listings.get()?.authoritative_listing(volume_id, path)
    }

    fn volumes_with_open_listings(&self, volume_id_prefix: &str) -> Vec<String> {
        self.listings
            .get()
            .map(|l| l.volume_ids_with_listings(volume_id_prefix))
            .unwrap_or_default()
    }

    fn open_listing_paths(&self, volume_id: &str) -> Vec<PathBuf> {
        self.listings
            .get()
            .map(|l| l.open_listing_paths(volume_id))
            .unwrap_or_default()
    }

    fn refresh_archive_listings<'a>(
        &'a self,
        volume_id: &'a str,
        archive_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async move {
            if let Some(listings) = self.listings.get() {
                listings.refresh_archive_listings(volume_id, archive_path).await;
            }
        })
    }

    fn sort_listing(&self, entries: &mut [FileEntry]) {
        let Some(listings) = self.listings.get() else {
            return;
        };
        sort_entries(
            &listings.collator(),
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
