//! What happened to a listing, as facts the host turns into whatever its user
//! sees.
//!
//! The engine never names a wire event, a word of copy, or an analytics
//! category: it says "this listing finished, here's how", and the host decides
//! that a refused read becomes a `listing-error` with a typed classification, a
//! TCC bookkeeping call, and an entry in the MCP ring. Cmdr's answer is
//! `TauriListingEvents`, in the app's `listing_host::events`.

use std::path::{Path, PathBuf};

use cmdr_fs::volume::{ListingProgress, VolumeError};
use serde::{Deserialize, Serialize};

use crate::DirectoryDiffBatch;
use crate::listing_id::ListingId;
use cmdr_sort::SortColumn;

/// How one streaming listing ended. Exactly one per listing the engine starts.
#[derive(Debug)]
pub enum ListingEnd {
    /// The listing is in the cache and the pane can fetch its rows.
    Complete {
        /// The volume the pane asked for, which keys the listing.
        volume_id: String,
        /// The directory as its volume spells it: the path the listing is keyed on.
        path: PathBuf,
        /// How many rows the pane shows, from the same filter every later fetch uses.
        total_count: usize,
        /// The serving volume's root. For a path inside an archive, the archive file.
        volume_root: PathBuf,
        /// The volume's own spelling of the directory, when it differs from what the
        /// pane asked for; `None` for a path that listed as given. The real bytes:
        /// how the pane is told is the host's.
        stored_path: Option<PathBuf>,
        /// The listing came back empty AT its volume's root, which on some volumes
        /// means the OS hid the contents (iCloud Drive without Full Disk Access).
        empty_at_volume_root: bool,
        /// The column the rows are ordered by: the requested one, or Modified when
        /// it's a date this volume doesn't record (`EffectiveSort`).
        effective_sort_by: SortColumn,
    },
    /// The pane moved on before the listing landed. Nothing was cached.
    Cancelled {
        /// The volume the pane asked for.
        volume_id: String,
        /// The directory as the pane asked for it.
        path: PathBuf,
    },
    /// The read failed. Nothing was cached.
    Failed {
        /// The volume the pane asked for.
        volume_id: String,
        /// The directory as the pane asked for it.
        path: PathBuf,
        /// What the volume answered.
        error: VolumeError,
    },
}

impl ListingEnd {
    /// The volume the pane asked for.
    pub fn volume_id(&self) -> &str {
        match self {
            Self::Complete { volume_id, .. } | Self::Cancelled { volume_id, .. } | Self::Failed { volume_id, .. } => {
                volume_id
            }
        }
    }

    /// The directory the listing was for.
    pub fn path(&self) -> &Path {
        match self {
            Self::Complete { path, .. } | Self::Cancelled { path, .. } | Self::Failed { path, .. } => path,
        }
    }
}

/// What a stalled listing's folder lives on, as far as the mount proves it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum StalledOn {
    /// A network share or a direct server connection (SMB, NFS, AFP, WebDAV, SFTP, S3, ...).
    Server,
    /// A known local disk: a block device, or a local filesystem type.
    Drive,
    /// Anything else: a phone, a FUSE or cloud mount, or a mount we couldn't read.
    Unknown,
}

/// Where the engine reports what happened to a listing.
///
/// Every call carries the listing it's about, and none of them blocks: an
/// implementation forwards and returns. Called from the read's own task and
/// from the diff flush, never under an engine lock.
pub trait ListingEvents: Send + Sync {
    /// The read is about to start: the slow part on a network folder.
    fn opening(&self, id: &ListingId);

    /// The read has been quiet for the stall window, and the folder lives on
    /// `on`. The listing keeps waiting; a later event supersedes this one.
    fn stalled(&self, id: &ListingId, on: StalledOn);

    /// The read's running tally: entries arrived so far (`progress.entries()`),
    /// and the phase it's in when it's doing work before its first entry (a
    /// backup's index load).
    fn progress(&self, id: &ListingId, progress: ListingProgress);

    /// The volume answered with `total` entries, before the sort and the cache.
    fn read_complete(&self, id: &ListingId, total: usize);

    /// The listing ended, one way or another. The last call for `id`'s read.
    fn finished(&self, id: &ListingId, end: ListingEnd);

    /// Committed changes to the rows a pane shows, in revision order.
    fn diff(&self, id: &ListingId, batches: Vec<DirectoryDiffBatch>);

    /// The listing now shows its directory under the volume's own spelling,
    /// `path`, which the pane adopts.
    fn respelled(&self, id: &ListingId, path: &Path);
}

/// Drops every report: the events of an engine nobody watches.
pub struct NoEvents;

impl ListingEvents for NoEvents {
    fn opening(&self, _id: &ListingId) {}
    fn stalled(&self, _id: &ListingId, _on: StalledOn) {}
    fn progress(&self, _id: &ListingId, _progress: ListingProgress) {}
    fn read_complete(&self, _id: &ListingId, _total: usize) {}
    fn finished(&self, _id: &ListingId, _end: ListingEnd) {}
    fn diff(&self, _id: &ListingId, _batches: Vec<DirectoryDiffBatch>) {}
    fn respelled(&self, _id: &ListingId, _path: &Path) {}
}
