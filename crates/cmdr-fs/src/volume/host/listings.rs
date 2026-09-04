//! What the open panes are showing, and how to tell them something changed.
//!
//! The single busiest seam: all four of Cmdr's backends use it, and a new one
//! will too. A pane is a live view of one directory on one volume, so when a
//! backend learns that a file appeared, vanished, or changed — from its own
//! mutation, or from a watcher event the server pushed — this is how the view
//! catches up without a re-read.
//!
//! ❌ **Call it per mutation, never per entry.** See `DETAILS.md` § "The dispatch
//! rule". A listing of 250 000 entries produces ONE
//! [`FullRefresh`](DirectoryChange::FullRefresh), not 250 000 calls.

use std::future::Future;
use std::path::Path;
use std::pin::Pin;

use crate::entry::FileEntry;
use crate::volume::DirectoryChange;

/// The pane listings, from a backend's point of view.
///
/// Cmdr answers all three from `file_system::listing::caching`; a test or a tool
/// answers none (`NoListings`).
pub trait ListingHost: Send + Sync {
    /// A directory's contents changed on `volume_id`.
    ///
    /// Fire-and-forget, and cheap when nothing is watching that path. One call
    /// covers three host concerns at once — the panes showing the directory, the
    /// file index, and the cloud-badge cache — so a backend never has to know
    /// which of them care.
    ///
    /// **Every mutation a backend performs must report itself here**, including
    /// writes that came in through `Volume::write_from_stream`. Watchers on
    /// network and device protocols are lossy under load, so a pane that waited
    /// for the watcher event would show a stale directory after a copy.
    fn directory_changed(&self, volume_id: &str, parent_path: &Path, change: DirectoryChange);

    /// The entries of `path`, but ONLY if a pane is showing it and a live watch
    /// is reporting every writer's changes to that view.
    ///
    /// This is an oracle for bulk work: a pre-flight scan that's about to
    /// enumerate a directory the user already has open can take the answer from
    /// here instead of paying a network round trip. `None` means "ask the
    /// protocol", never "the directory is empty".
    ///
    /// The guarantee is the same one a `list_directory` call gives: the state as
    /// of the most recent observation, not as of this instant. A backend whose
    /// watch can silently miss changes says so by reporting anything other than
    /// [`WatchCoverage::EveryWriter`](crate::volume::WatchCoverage::EveryWriter)
    /// from `Volume::listing_watch_coverage`, and the oracle then declines for it.
    fn authoritative_listing(&self, volume_id: &str, path: &Path) -> Option<Vec<FileEntry>>;

    /// The volume ids under `volume_id_prefix` that a pane is currently showing,
    /// each listed once.
    ///
    /// For a backend whose events name an object the protocol alone can't place.
    /// One MTP device carries several storages, each its own volume and its own
    /// path namespace, and a PTP handle says nothing about which; resolving it
    /// costs a device round trip per storage. Asking here narrows the search to
    /// the storages a pane is actually showing, which are the only ones where a
    /// targeted refresh could change anything on screen.
    ///
    /// A prefix rather than an exact id because the asker is the DEVICE, one
    /// level above the volumes it serves. Order is unspecified. An empty answer
    /// means nothing is open, so there is nothing to aim a targeted refresh at —
    /// ❌ never read it as "the device is gone".
    fn volumes_with_open_listings(&self, volume_id_prefix: &str) -> Vec<String>;

    /// The archive file at `archive_path` on `volume_id` changed, so re-read
    /// every open pane at or INSIDE it.
    ///
    /// A pane browsing `/share/photos.zip/2026/` has no filesystem path to watch,
    /// so nothing else can notice. Only a backend that can observe writes to a
    /// file it doesn't itself serve needs this: the local archive watch and the
    /// SMB share watcher, both watching the drive that HOLDS the archive.
    ///
    /// Deliberately separate from [`directory_changed`](Self::directory_changed):
    /// a path inside an archive isn't a real filesystem path, so feeding it to
    /// the file index would be meaningless. This refreshes views and nothing else.
    #[allow(
        clippy::type_complexity,
        reason = "async trait method returns a pinned boxed future, matching the `Volume` trait"
    )]
    fn refresh_archive_listings<'a>(
        &'a self,
        volume_id: &'a str,
        archive_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>>;
}

/// Nothing is showing anything: changes go nowhere and the oracle always misses.
///
/// The right answer for a bench or a CLI tool, and it's why a backend never needs
/// an `Option<ListingHost>`.
pub(super) struct NoListings;

impl ListingHost for NoListings {
    fn directory_changed(&self, _volume_id: &str, _parent_path: &Path, _change: DirectoryChange) {}

    fn authoritative_listing(&self, _volume_id: &str, _path: &Path) -> Option<Vec<FileEntry>> {
        None
    }

    fn volumes_with_open_listings(&self, _volume_id_prefix: &str) -> Vec<String> {
        Vec::new()
    }

    fn refresh_archive_listings<'a>(
        &'a self,
        _volume_id: &'a str,
        _archive_path: &'a Path,
    ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
        Box::pin(async {})
    }
}

#[cfg(any(test, feature = "testing"))]
pub use recording::RecordingListings;

#[cfg(any(test, feature = "testing"))]
mod recording {
    use std::path::PathBuf;
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::{DirectoryChange, FileEntry, Future, ListingHost, Path, Pin};
    use crate::ignore_poison::IgnorePoison;

    /// A [`ListingHost`] that remembers what it was told, so
    /// a backend's test can assert on the pane updates it produced.
    ///
    /// The call COUNT is the point as much as the contents: it's the instrument
    /// for the per-mutation dispatch rule. A walk over four directories of 250
    /// files should report a handful of changes; if
    /// [`change_count`](Self::change_count) comes back in the thousands, a seam
    /// call landed in a per-entry loop.
    #[derive(Default)]
    pub struct RecordingListings {
        changes: Mutex<Vec<(String, PathBuf, DirectoryChange)>>,
        watched: Mutex<Vec<(String, PathBuf, Vec<FileEntry>)>>,
        open_volumes: Mutex<Vec<String>>,
        authoritative_lookups: AtomicUsize,
        archive_refreshes: Mutex<Vec<(String, PathBuf)>>,
    }

    impl RecordingListings {
        /// A recorder with no canned listings: every oracle lookup misses.
        pub fn new() -> Self {
            Self::default()
        }

        /// Makes `entries` the answer for one
        /// [`authoritative_listing`](super::ListingHost::authoritative_listing) lookup, as if
        /// a pane were showing that directory with a live watcher on it.
        /// A listing the oracle can answer for is one a pane is showing, so this
        /// registers the volume as open too.
        pub fn with_authoritative_listing(
            self,
            volume_id: &str,
            path: impl Into<PathBuf>,
            entries: Vec<FileEntry>,
        ) -> Self {
            self.watched
                .lock_ignore_poison()
                .push((volume_id.to_string(), path.into(), entries));
            self.with_open_listing(volume_id)
        }

        /// Makes `volume_id` one of the answers to
        /// [`volumes_with_open_listings`](super::ListingHost::volumes_with_open_listings),
        /// as if a pane were showing something on it. For an open pane whose
        /// contents the oracle would decline to serve; a watched one comes with
        /// [`with_authoritative_listing`](Self::with_authoritative_listing).
        pub fn with_open_listing(self, volume_id: &str) -> Self {
            let mut open = self.open_volumes.lock_ignore_poison();
            if !open.iter().any(|id| id == volume_id) {
                open.push(volume_id.to_string());
            }
            drop(open);
            self
        }

        /// Every change reported so far, in order.
        pub fn changes(&self) -> Vec<(String, PathBuf, DirectoryChange)> {
            self.changes.lock_ignore_poison().clone()
        }

        /// How many changes were reported. The dispatch-rule guard asserts on
        /// this.
        pub fn change_count(&self) -> usize {
            self.changes.lock_ignore_poison().len()
        }

        /// How many times the fresh-listing oracle was consulted.
        pub fn authoritative_lookup_count(&self) -> usize {
            self.authoritative_lookups.load(Ordering::Relaxed)
        }

        /// Every `(volume_id, archive_path)` an archive refresh was asked for.
        pub fn archive_refreshes(&self) -> Vec<(String, PathBuf)> {
            self.archive_refreshes.lock_ignore_poison().clone()
        }
    }

    impl ListingHost for RecordingListings {
        fn directory_changed(&self, volume_id: &str, parent_path: &Path, change: DirectoryChange) {
            self.changes
                .lock_ignore_poison()
                .push((volume_id.to_string(), parent_path.to_path_buf(), change));
        }

        fn authoritative_listing(&self, volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
            self.authoritative_lookups.fetch_add(1, Ordering::Relaxed);
            self.watched
                .lock_ignore_poison()
                .iter()
                .find(|(vid, dir, _)| vid == volume_id && dir == path)
                .map(|(_, _, entries)| entries.clone())
        }

        fn volumes_with_open_listings(&self, volume_id_prefix: &str) -> Vec<String> {
            self.open_volumes
                .lock_ignore_poison()
                .iter()
                .filter(|id| id.starts_with(volume_id_prefix))
                .cloned()
                .collect()
        }

        fn refresh_archive_listings<'a>(
            &'a self,
            volume_id: &'a str,
            archive_path: &'a Path,
        ) -> Pin<Box<dyn Future<Output = ()> + Send + 'a>> {
            self.archive_refreshes
                .lock_ignore_poison()
                .push((volume_id.to_string(), archive_path.to_path_buf()));
            Box::pin(async {})
        }
    }
}
