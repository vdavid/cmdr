//! The fresh-listing oracle: a cached listing a walker may trust instead of
//! reading the volume again.

use crate::rt::Instant;
use std::path::Path;
use std::sync::atomic::Ordering;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use cmdr_fs::volume::WatchCoverage;

use crate::Inner;
use crate::cached_listing::CachedListing;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::RoutedKind;

impl Inner {
    /// Returns cached entries for `(volume_id, path)` when the volume reports
    /// [`WatchCoverage::EveryWriter`] for this listing and no listing overlay
    /// decorated it. Otherwise `None`.
    ///
    /// **Freshness contract (read carefully)**: a `Some(_)` result means the volume has
    /// a change-notification channel that every writer's changes reach, and the cache
    /// reflects the volume's most recently observed state. It does NOT mean the cache is
    /// byte-perfect with the device right now: every backend has a debounce or settling
    /// window between a real change and the cache reflecting it.
    ///
    /// - Local FS: FSEvents coalesce window (~10 ms). An OS-mounted network share is
    ///   served by the same backend but never qualifies: FSEvents can't see other
    ///   clients there, so it reports `ThisMachineOnly` and this returns `None`.
    /// - SMB: 200 ms watcher debounce; > 50 events per directory triggers a `FullRefresh` which arrives
    ///   via a real re-read.
    /// - MTP: 500 ms event debouncer plus per-device polling. Many MTP devices (cameras especially)
    ///   never emit per-object events, so "watched" there means only "the device is reachable and would
    ///   forward changes if it sent any."
    /// - Archive (`.zip`): 200 ms debounce on a watch of the backing file's parent directory, plus a
    ///   re-parse of the central directory. A mid-write (truncated) archive keeps the previous listing
    ///   until a clean re-read, so "watched" here means "the last clean parse," never a half-written one.
    ///
    /// Callers must treat the result as "fresh as our most recent observation," which
    /// is the same guarantee a `list_directory` call gives: it sees the device's state
    /// at the moment the call returned, not at the moment the caller reads its result.
    /// The contract intentionally accepts this window; a tighter one would force us to
    /// re-validate every walk, defeating the whole point of the oracle.
    ///
    /// When multiple cached listings exist for the same `(volume_id, path)` pair (two
    /// panes browsing the same directory), the picker is deterministic: highest
    /// `sequence`, ties broken by the latest `created_at`. Both listings receive watcher
    /// events, so they're equally fresh; the tiebreaker is just to keep the result
    /// stable across calls.
    pub(crate) fn try_get_authoritative_listing(&self, volume_id: &str, path: &Path) -> Option<Vec<FileEntry>> {
        // Step 1: find all listings on this (volume_id, path) and pick the most-recently-updated
        // one (highest sequence, ties broken by latest created_at). Read the entries out
        // under the cache lock and drop the lock before crossing any async / volume boundary.
        let key = self.listing_path(volume_id, path);
        let entries: Vec<FileEntry> = {
            let cache = self.cache.read_ignore_poison();
            let mut best: Option<(&String, &CachedListing, u64, Instant)> = None;
            for (id, listing) in cache.iter() {
                if listing.volume_id != volume_id || listing.path != key {
                    continue;
                }
                let seq = listing.sequence.load(Ordering::Relaxed);
                let created = listing.created_at;
                best = match best {
                    None => Some((id, listing, seq, created)),
                    Some((_, _, best_seq, best_created))
                        if seq > best_seq || (seq == best_seq && created > best_created) =>
                    {
                        Some((id, listing, seq, created))
                    }
                    Some(other) => Some(other),
                };
            }
            let (_, listing, ..) = best?;
            // A listing carrying overlay rows is what a PANE sees, not what the
            // directory holds: handing it to a delete walker or a copy scan would
            // send one at a path with no inode behind it. See
            // `host/overlays.rs`.
            if listing.has_overlay_rows() {
                return None;
            }
            listing.entries().to_vec()
        };

        // Step 2: ask the volume what a watch on this listing actually covers.
        // `resolve_local_only` (the sync sibling of `resolve`) routes a LOCAL `.zip`
        // listing to its ArchiveVolume, whose live content watch answers
        // `listing_watch_coverage` honestly (covered once established, `None` if it
        // couldn't start). This oracle runs on sync recursive scan walkers, so it can't
        // `.await` the async remote confirm — hence the local-only variant.
        let resolved = self.volumes.resolve_local_only(volume_id, path);
        let volume = resolved.volume?;

        // Honesty guard for a REMOTE archive-inner path. `resolve_local_only` can't
        // confirm a remote boundary, so such a path stays a passthrough to its parent
        // (a direct-SMB / MTP volume). That parent's `listing_watch_coverage` is
        // VOLUME-level ("device reachable"), which would falsely claim freshness for an
        // archive whose content watch is local-only and never established. Decline, so
        // the write-op pre-flight rescans through the ArchiveVolume rather than reusing
        // a possibly-stale cached inner listing. A local dir merely NAMED `foo.zip`
        // isn't affected (its parent is local), nor is a genuine local archive (routed
        // to `RoutedKind::Archive`). Matching the ARCHIVE kind rather than "routed at
        // all" is deliberate: the doubt this guards is specifically an unconfirmed
        // archive boundary.
        if resolved.routed != Some(RoutedKind::Archive)
            && !volume.supports_local_fs_access()
            && cmdr_fs::archive_format::archive_boundary_candidate(path).is_some()
        {
            return None;
        }

        // The same doubt for a restic view. `resolve_local_only` routes one only on
        // the hub's memo, so a path under a `Browse snapshots` component that came
        // back unrouted is a view listing its parent's coverage says nothing about
        // (or a real folder of that name, where declining costs one re-read).
        if resolved.routed != Some(RoutedKind::Restic) && cmdr_fs::restic_view::restic_route(path).is_some() {
            return None;
        }

        // Only full coverage substitutes for a read. `ThisMachineOnly` means a live
        // watch that can't see other writers (an OS-mounted share), so the cache may
        // be missing entries nobody told us about — exactly the case where handing
        // these to a delete walker or a copy scan does damage. Pay the re-read.
        match volume.listing_watch_coverage(path) {
            WatchCoverage::EveryWriter => Some(entries),
            WatchCoverage::ThisMachineOnly | WatchCoverage::None => None,
        }
    }
}
