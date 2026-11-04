//! The ONE step every read runs between the volume's answer and the cache: the
//! drive index's enrich, then the overlays (`host/overlays.rs`).
//!
//! What it hands back is a [`Prepared`], which nothing outside this file can
//! build, and the only things that store a read's entries
//! (`CachedListing::new` and `Inner::update_listing_entries`) take nothing else.
//! So a read path that skips the overlays doesn't compile: it has no
//! `Prepared` to store. Without that, a refresh that forgot them would strip a
//! repo's six portal rows out of an open `.git/` pane.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use crate::rt::Runtime;
use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::{SupportedDates, Volume};

use crate::Inner;
use crate::host::ListingOverlay;

/// What a cache update knows about the [`ListingOverlay`] rows inside the
/// entries it is about to write.
///
/// The two travel together because they must be written under ONE lock
/// acquisition: a walker asking the fresh-listing oracle in between would see
/// decorated entries described by the previous count, and six rows with no inode
/// behind them would go to a delete walker.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum OverlayRows {
    /// The overlays ran again for this write, and this is what they contributed.
    Recounted(usize),
    /// The overlays did not run: this write replaces entries a previous read
    /// already decorated, so the stored count still describes them.
    Unchanged,
}

/// Which dates the volume that read a [`Prepared`] records, so the listing that
/// stores it sorts the way that volume can (`sorting::EffectiveSort`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ListedDates {
    /// The volume answered `Volume::supported_dates` for this read's directory.
    /// Asked on every volume read, so a re-read after a backend swap (an smbfs
    /// mount handing over to direct SMB) refreshes them.
    Claimed(SupportedDates),
    /// No volume read these (`std::fs`, or a backend's handed-over entries), so
    /// the listing keeps the dates it had.
    Unchanged,
}

/// A read's entries, enriched and decorated: the only form a listing stores.
#[derive(Clone)]
pub(crate) struct Prepared {
    entries: Vec<FileEntry>,
    overlay_rows: OverlayRows,
    dates: ListedDates,
    /// Whether the drive index holds these rows, so the enrich ran. A routed
    /// volume's (an archive's, a git snapshot's) paths aren't on disk.
    indexed: bool,
}

impl Prepared {
    /// How many entries, contributed rows included.
    pub(crate) fn len(&self) -> usize {
        self.entries.len()
    }

    /// Whether the drive index holds these rows (`false` for a routed volume).
    pub(crate) fn indexed(&self) -> bool {
        self.indexed
    }

    /// The entries, what the overlays contributed to them, and the dates the
    /// reading volume records, for the cache write that stores them.
    pub(crate) fn into_parts(self) -> (Vec<FileEntry>, OverlayRows, ListedDates) {
        (self.entries, self.overlay_rows, self.dates)
    }

    /// Entries a test hands the cache as if a read prepared them, from a volume
    /// that records every date. [`Self::claiming`] plays one that doesn't.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn for_test(entries: Vec<FileEntry>, overlay_rows: OverlayRows) -> Self {
        Self {
            entries,
            overlay_rows,
            dates: ListedDates::Claimed(SupportedDates::ALL),
            indexed: true,
        }
    }

    /// The same entries, read by a volume that records `dates`.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn claiming(self, dates: ListedDates) -> Self {
        Self { dates, ..self }
    }
}

/// Who read the entries a [`Prepared`] starts from.
pub(crate) enum ReadBy<'a> {
    /// A volume. `routed` when it's an archive or a git snapshot, whose paths
    /// aren't real filesystem paths, so no drive index holds them.
    Volume { volume: &'a Arc<dyn Volume>, routed: bool },
    /// `std::fs`, because nothing serves the listing's volume id any more (the
    /// watcher's fallback). Nothing to decorate through, so the listing keeps
    /// the overlay count it had.
    StdFs,
}

impl Inner {
    /// Enriches `entries` with what the drive index knows (skipped for a routed
    /// volume), then folds in every applicable overlay's rows: AFTER the enrich,
    /// since a contributed row has no index entry, and before the caller sorts.
    /// A volume read also asks the volume which dates it records there: the
    /// RESOLVED volume, so a routed archive answers for itself.
    pub(crate) async fn prepare(
        &self,
        read_by: ReadBy<'_>,
        volume_id: &str,
        path: &Path,
        mut entries: Vec<FileEntry>,
    ) -> Prepared {
        match read_by {
            ReadBy::Volume { volume, routed } => {
                if !routed {
                    self.index.enrich(volume_id, &mut entries);
                }
                let overlay_rows = OverlayRows::Recounted(self.decorate(volume_id, volume, path, &mut entries).await);
                let dates = ListedDates::Claimed(volume.supported_dates(path).await);
                Prepared {
                    entries,
                    overlay_rows,
                    dates,
                    indexed: !routed,
                }
            }
            ReadBy::StdFs => {
                self.index.enrich(volume_id, &mut entries);
                Prepared {
                    entries,
                    overlay_rows: OverlayRows::Unchanged,
                    dates: ListedDates::Unchanged,
                    indexed: true,
                }
            }
        }
    }

    /// A device backend's own re-read of `path` (`DirectoryChange::Replaced`),
    /// enriched, or `None` when an overlay applies there.
    ///
    /// Synchronous: the arm runs on the backend's thread, and an `Added` the
    /// backend sends right after must land after it, so it can't wait on the
    /// overlays' blocking gather. It asks only their zero-I/O `applies_to`:
    /// none applies (every device backend today) and the entries go in with zero
    /// contributed rows; one applies and the caller re-reads the directory
    /// instead, dropping these entries rather than half-applying them.
    pub(crate) fn prepare_handed_over(
        &self,
        volume_id: &str,
        path: &Path,
        mut entries: Vec<FileEntry>,
    ) -> Option<Prepared> {
        // Asked outside every engine lock: the volume seam reads the host's
        // registry. No volume served means no overlay can apply.
        if let Some(volume) = self.volumes.get(volume_id)
            && !self.applicable_overlays(volume.as_ref(), path, &entries).is_empty()
        {
            return None;
        }
        self.index.enrich(volume_id, &mut entries);
        Some(Prepared {
            entries,
            overlay_rows: OverlayRows::Recounted(0),
            dates: ListedDates::Unchanged,
            indexed: true,
        })
    }

    /// The overlays that have something to say about `path` on `volume`, whose
    /// listing holds `entries`, in registration order. Cheap: `applies_to` is
    /// zero-I/O by contract.
    fn applicable_overlays(
        &self,
        volume: &dyn Volume,
        path: &Path,
        entries: &[FileEntry],
    ) -> Vec<Arc<dyn ListingOverlay>> {
        self.overlays()
            .iter()
            .filter(|overlay| overlay.applies_to(volume, path, entries))
            .cloned()
            .collect()
    }

    /// Folds every applicable contributor's rows into `entries`, and reports how
    /// many rows they added.
    ///
    /// **A contributed row SHADOWS a real one of the same name.** A repo created by
    /// an older git carries a real (deprecated) `.git/branches/` directory, and the
    /// portal's `branches` row is the one the user came for; showing two rows called
    /// `branches` would be worse than either.
    ///
    /// Sorting happens after this returns, so a contributor hands rows over in any
    /// order.
    pub(super) async fn decorate(
        &self,
        volume_id: &str,
        volume: &Arc<dyn Volume>,
        path: &Path,
        entries: &mut Vec<FileEntry>,
    ) -> usize {
        let applicable = self.applicable_overlays(volume.as_ref(), path, entries);
        if applicable.is_empty() {
            return 0;
        }

        let contributed = gather(
            &self.runtime,
            applicable,
            volume_id.to_string(),
            Arc::clone(volume),
            path.to_path_buf(),
        )
        .await;
        if contributed.is_empty() {
            return 0;
        }

        let names: HashSet<&str> = contributed.iter().map(|entry| entry.name.as_str()).collect();
        entries.retain(|entry| !names.contains(entry.name.as_str()));
        let added = contributed.len();
        entries.extend(contributed);
        added
    }
}

/// Runs the applicable contributors on the host runtime's blocking pool: one of
/// them opens a repository and counts refs, which is not work for an async
/// worker.
///
/// A contributor that panics costs its rows and nothing else. The volume's own
/// entries are already in hand by the time this runs, and a pane showing a
/// repo's real `.git/` contents beats a pane showing a listing error because
/// `gix` tripped over a damaged object.
async fn gather(
    runtime: &Runtime,
    overlays: Vec<Arc<dyn ListingOverlay>>,
    volume_id: String,
    volume: Arc<dyn Volume>,
    path: PathBuf,
) -> Vec<FileEntry> {
    runtime
        .spawn_blocking(move || {
            overlays
                .iter()
                .flat_map(|overlay| overlay.extra_entries(&volume_id, volume.as_ref(), &path))
                .collect()
        })
        .await
        .unwrap_or_else(|e| {
            log::warn!(target: "listing", "a listing overlay panicked, listing without its rows: {e}");
            Vec::new()
        })
}
