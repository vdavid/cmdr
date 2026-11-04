//! Directory listing cache for on-demand virtual scrolling.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;

use crate::Inner;
use crate::listing_id::ListingId;
use cmdr_fs::entry::FileEntry;
pub(crate) use cmdr_fs::volume::DirectoryChange;

/// One open listing at a glance ([`Listings::summaries`](super::Listings::summaries)).
#[derive(Debug, Clone)]
pub struct ListingSummary {
    /// The listing.
    pub listing_id: ListingId,
    /// The volume it reads through.
    pub volume_id: String,
    /// The directory, in the volume's spelling.
    pub path: PathBuf,
    /// How many entries it holds, hidden ones included.
    pub entry_count: usize,
    /// How long ago it was opened.
    pub age_ms: u128,
}

impl Inner {
    /// Returns a snapshot of every active listing in the cache. Used by `cmdr://state`
    /// so triagers can spot orphan listings (started but never bound to a pane,
    /// for example when a volume dropdown commits a navigation that the user then
    /// abandons or that surfaces an error).
    pub(crate) fn snapshot_listings(&self) -> Vec<ListingSummary> {
        let cache = self.cache.read_ignore_poison();
        let now = crate::rt::now();
        let mut out: Vec<ListingSummary> = cache
            .iter()
            .map(|(id, listing)| ListingSummary {
                listing_id: ListingId::new(id.clone()),
                volume_id: listing.volume_id.clone(),
                path: listing.path.as_path().to_path_buf(),
                entry_count: listing.entries().len(),
                age_ms: now.saturating_duration_since(listing.created_at).as_millis(),
            })
            .collect();
        out.sort_by_key(|a| a.age_ms);
        out
    }

    /// Finds all cached listings whose directory is `parent_path` on `volume_id`,
    /// matching `parent_path` in that volume's one spelling
    /// ([`ListingPath`](super::cached_listing::ListingPath)).
    ///
    /// With `volume_id` `None` the path compares verbatim across EVERY volume, so a
    /// remote pane showing the same path string matches too. Only a local caller
    /// that doesn't know its own volume id may rely on that.
    ///
    /// Returns each match's listing id: typically none (no pane showing that
    /// dir), one, or two (both panes showing the same dir). Ids only, since the
    /// listing is the one holder of its sort.
    pub(crate) fn find_listings_for_path_on_volume(&self, volume_id: Option<&str>, parent_path: &Path) -> Vec<String> {
        let key = volume_id.map(|vid| (vid, self.listing_path(vid, parent_path)));
        let cache = self.cache.read_ignore_poison();

        cache
            .iter()
            .filter(|(_, listing)| match &key {
                Some((vid, key)) => listing.volume_id == *vid && listing.path == *key,
                None => listing.path.as_path() == parent_path,
            })
            .map(|(id, _)| id.clone())
            .collect()
    }

    /// Returns the newest cached pane listing for a `(volume_id, path)` pair.
    ///
    /// Unlike `try_get_authoritative_listing`, this doesn't require a watcher: SMB, MTP, and
    /// other virtual panes still have a UI listing cache. No filesystem call happens here.
    pub(crate) fn get_cached_listing(&self, volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
        let key = self.listing_path(volume_id, path);
        let cache = self.cache.read_ignore_poison();
        let listing = cache
            .values()
            .filter(|listing| listing.volume_id == volume_id && listing.path == key)
            .max_by_key(|listing| (listing.sequence.load(Ordering::Relaxed), listing.created_at))?;
        listing.touch();
        Some(listing.entries().to_vec())
    }

    /// Finds all cached listings belonging to a volume, regardless of path.
    ///
    /// Used by `FullRefresh` when the SMB watcher emits `STATUS_NOTIFY_ENUM_DIR` for
    /// the share root but no listing matches that exact path (the user may be browsing
    /// a subdirectory).
    pub(crate) fn find_listings_on_volume(&self, volume_id: &str) -> Vec<(String, PathBuf)> {
        let cache = self.cache.read_ignore_poison();

        cache
            .iter()
            .filter(|(_, listing)| listing.volume_id == volume_id)
            .map(|(id, listing)| (id.clone(), listing.path.as_path().to_path_buf()))
            .collect()
    }

    /// Returns `(volume_id, path)` for a cached listing in one read-lock acquisition.
    ///
    /// Used by `refresh_listing` so the short-circuit check can ask the volume
    /// `listing_watch_coverage(path)` without two separate cache reads.
    pub(crate) fn get_listing_volume_id_and_path(&self, listing_id: &str) -> Option<(String, PathBuf)> {
        let cache = self.cache.read_ignore_poison();
        cache
            .get(listing_id)
            .map(|listing| (listing.volume_id.clone(), listing.path.as_path().to_path_buf()))
    }

    /// Applies a directory change on a volume to the panes showing it.
    ///
    /// Finds all active listings matching `volume_id` and `parent_path`, applies the
    /// change to the cache, and emits `directory-diff` events to the frontend.
    ///
    /// For single-entry changes (add/remove/modify/rename), patches the cache in-place.
    /// For `FullRefresh`, re-reads the directory via the Volume trait and computes a diff.
    ///
    /// Panes only: the host applies the change to everything else that keeps a copy
    /// of the directory (the drive index, the cloud badge) BEFORE this, so the
    /// enrich below sees fresh sizes. Cmdr's order lives in the app's
    /// `listing_host::directory_changed`; ❌ never call the index's own change intake
    /// from here.
    pub(crate) fn notify_directory_changed(
        self: &Arc<Self>,
        volume_id: &str,
        parent_path: &Path,
        change: DirectoryChange,
    ) {
        let listings = self.find_listings_for_path_on_volume(Some(volume_id), parent_path);

        // For non-FullRefresh changes, bail early if no listing matches this path.
        // FullRefresh has a volume-wide fallback below (for STATUS_NOTIFY_ENUM_DIR).
        if listings.is_empty() && !matches!(change, DirectoryChange::FullRefresh) {
            return;
        }

        match change {
            DirectoryChange::Added(entry) => {
                let mut entry = entry;
                self.index.enrich(volume_id, std::slice::from_mut(&mut entry));
                for listing_id in &listings {
                    self.notify_added(listing_id, entry.clone());
                }
            }
            DirectoryChange::Removed(name) => {
                let full_path = parent_path.join(&name);
                for listing_id in &listings {
                    self.notify_removed(listing_id, &full_path);
                }
            }
            DirectoryChange::Modified(entry) => {
                let mut entry = entry;
                self.index.enrich(volume_id, std::slice::from_mut(&mut entry));
                for listing_id in &listings {
                    self.notify_modified(listing_id, entry.clone());
                }
            }
            DirectoryChange::Renamed { old_name, new_entry } => {
                let mut new_entry = new_entry;
                self.index.enrich(volume_id, std::slice::from_mut(&mut new_entry));
                let old_path = parent_path.join(&old_name);
                for listing_id in &listings {
                    self.notify_removed(listing_id, &old_path);
                    self.notify_added(listing_id, new_entry.clone());
                }
            }
            DirectoryChange::Replaced(entries) => {
                // The backend already re-read the directory, so there's nothing to ask
                // it for: prepare once for the volume, then publish per listing. This is
                // where the diffing a device backend used to do for itself lives now.
                // Dated on arrival: the backend serializes its own re-reads.
                let generation = self.next_read_generation();
                match self.prepare_handed_over(volume_id, parent_path, entries) {
                    Some(prepared) => {
                        for listing_id in &listings {
                            self.publish_replacement(listing_id, prepared.clone(), generation);
                        }
                    }
                    // An overlay applies here, and this arm can't wait on it: re-read
                    // the directory the way a `FullRefresh` does, which decorates.
                    None => self.spawn_full_refresh(volume_id.to_string(), parent_path.to_path_buf(), listings),
                }
            }
            DirectoryChange::FullRefresh => {
                if listings.is_empty() {
                    // No listing matches this exact path. For STATUS_NOTIFY_ENUM_DIR the
                    // path is the share root, but the user may be browsing a subdirectory.
                    // Refresh all listings on this volume instead.
                    let volume_listings = self.find_listings_on_volume(volume_id);
                    for (lid, path) in volume_listings {
                        self.spawn_full_refresh(volume_id.to_string(), path, vec![lid]);
                    }
                } else {
                    self.spawn_full_refresh(volume_id.to_string(), parent_path.to_path_buf(), listings);
                }
            }
        }
    }

    /// Inserts an entry into the cache and queues a single-add change for the next
    /// coalesced `directory-diff` flush.
    ///
    /// **Upsert semantics**: if a cached entry with the same path already exists,
    /// delegates to `notify_modified` so the cache reflects the latest observation
    /// instead of dropping it. This matters when the SMB / MTP watcher fires an
    /// Add event mid-write (the watcher's stat catches a partial file size), then
    /// `Volume::write_from_stream` fires its own Add post-close with the final
    /// size. Without upsert, the partial size from the watcher sticks and the FE
    /// shows a wrong size until the next manual refresh. Concretely seen on
    /// MTP→SMB copies: 9 files copied, 3 stuck at half size (watcher stat'd
    /// mid-write, self-notify lost the race against `insert_entry_sorted`'s
    /// duplicate guard).
    pub(crate) fn notify_added(&self, listing_id: &str, entry: FileEntry) {
        if self.has_entry(listing_id, &entry.path) {
            self.notify_modified(listing_id, entry);
            return;
        }

        let _ = self.insert_entry_sorted(listing_id, entry);
    }

    /// Removes an entry from the cache and queues a single-remove change.
    ///
    /// Matches by file name within the listing (its directory), NOT the full path, so
    /// it works when the listing's stored entry paths use a different path space than
    /// the notifier's resolved parent (MTP: inner `/Dir/file` entries vs `mtp://…`
    /// parent). See `remove_entry_by_name`.
    pub(crate) fn notify_removed(&self, listing_id: &str, full_path: &Path) {
        let Some(name) = full_path.file_name() else {
            return;
        };
        let _ = self.remove_entry_by_name(listing_id, name);
    }

    /// Updates an entry in the cache and queues a modify (or, when its sort key changed, a move) change.
    fn notify_modified(&self, listing_id: &str, mut entry: FileEntry) {
        // Preserve already-loaded Finder tags across this re-stat (see `carry_forward_deferred`).
        self.carry_forward_deferred(listing_id, &mut entry);

        let _ = self.update_entry_sorted(listing_id, entry);
    }
}
