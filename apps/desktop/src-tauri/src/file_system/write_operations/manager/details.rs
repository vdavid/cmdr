//! What an EXPANDED queue row shows: the full paths an operation acts on and
//! when it was registered and admitted. Fetched on demand by
//! `get_operation_details(id)`, ❌ never carried on `operations-changed`.
//!
//! Why on demand: the snapshot is rebuilt and re-sent to every window each time
//! anything in the registry moves, and a selection can hold thousands of
//! sources. Most rows are never expanded, so paying for every path on every
//! broadcast would fatten the one event that must stay cheap for nothing.
//! DETAILS § "Row details".

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::file_system::volume::Volume;
use crate::ignore_poison::IgnorePoison;

use super::{OperationManager, OperationSnapshot};

/// How many top-level source paths an operation keeps for its details. Past
/// this the list stops growing and `source_count` carries the rest, so a
/// 100 000-item selection costs a bounded copy here and a bounded IPC answer,
/// and the row says "and N more" instead of laying out a list nobody scrolls.
pub const DETAILS_SOURCE_CAP: usize = 200;

/// The full paths behind an operation's summary names, captured when it's
/// built. Best-effort like the names: an op whose builder has no paths to hand
/// (a synthetic test op, the content of a new file) leaves it empty, and the
/// row shows nothing rather than a guess.
// DEFAULT-OK: empty means "no paths known", which the row renders as absence.
#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationPaths {
    /// The first [`DETAILS_SOURCE_CAP`] top-level sources, in selection order.
    sources: Vec<String>,
    /// How many top-level sources there really are.
    source_count: usize,
    /// Where the operation writes: the destination folder of a transfer, the
    /// new name of a rename, the archive of a compress. `None` for a delete.
    destination: Option<String>,
    /// The display name of the volume `sources` live on, when those paths
    /// can't name a place by themselves ([`Self::volume_label`]).
    source_volume: Option<String>,
    /// The same for `destination`.
    destination_volume: Option<String>,
}

impl OperationPaths {
    /// From filesystem paths, which is what nearly every builder holds.
    pub(crate) fn from_paths(sources: &[impl AsRef<Path>], destination: Option<&Path>) -> Self {
        Self::from_strings(
            sources.len(),
            sources.iter().map(|path| path.as_ref().to_string_lossy().into_owned()),
            destination.map(|path| path.to_string_lossy().into_owned()),
        )
    }

    /// From paths already rendered as text (a journal row, an archive's
    /// `archive/inner` path). `count` is the real total; only the first
    /// [`DETAILS_SOURCE_CAP`] of `sources` are kept.
    pub(crate) fn from_strings(
        count: usize,
        sources: impl IntoIterator<Item = String>,
        destination: Option<String>,
    ) -> Self {
        Self {
            sources: sources.into_iter().take(DETAILS_SOURCE_CAP).collect(),
            source_count: count,
            destination,
            source_volume: None,
            destination_volume: None,
        }
    }

    /// Names the volumes the paths live on, each from [`Self::volume_label`].
    pub(crate) fn on_volumes(self, source: Option<String>, destination: Option<String>) -> Self {
        Self {
            source_volume: source,
            destination_volume: destination,
            ..self
        }
    }

    /// The name to put in front of a path on `volume`, or `None` when the path
    /// already names a place on this Mac.
    ///
    /// The rule: a path the OS can resolve (`paths_are_os_visible`: the local
    /// disk, a mounted drive, an OS-mounted share, all `/…` or `/Volumes/…`)
    /// says where it is on its own. A path on an MTP phone, an S3 bucket, an
    /// SFTP or WebDAV server is relative to that volume, so `/DCIM/a.jpg`
    /// alone could be anywhere: it gets the volume's `name()`, the same name
    /// the queue row's summary shows. How the two are joined on screen is the
    /// frontend's call (`formatOperationPath`).
    pub(crate) fn volume_label(volume: &dyn Volume) -> Option<String> {
        (!volume.paths_are_os_visible()).then(|| volume.name().to_string())
    }

    /// The IPC answer for this op, given its timing.
    pub(crate) fn details_for(&self, operation_id: &str, queued_at: u64, started_at: Option<u64>) -> OperationDetails {
        OperationDetails {
            operation_id: operation_id.to_string(),
            source_paths: self.sources.clone(),
            source_count: self.source_count.max(self.sources.len()),
            destination_path: self.destination.clone(),
            source_volume_name: self.source_volume.clone(),
            destination_volume_name: self.destination_volume.clone(),
            queued_at,
            started_at,
        }
    }
}

/// One expanded queue row: the full source and destination paths, and when the
/// operation was registered and admitted. Times are Unix seconds, the unit
/// `formatDateTime` on the frontend takes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OperationDetails {
    pub operation_id: String,
    /// The first [`DETAILS_SOURCE_CAP`] top-level source paths, in selection
    /// order. Empty when the operation's builder had none to give.
    pub source_paths: Vec<String>,
    /// How many top-level sources there are; above `source_paths.len()` when
    /// the list was capped.
    pub source_count: usize,
    /// Where it writes (see [`OperationPaths`]), `None` for a delete or trash.
    pub destination_path: Option<String>,
    /// The display name of the volume the source paths live on, set only when
    /// those paths can't name a place by themselves (an MTP phone, a cloud or
    /// server volume). `None` for a plain local path. See
    /// `OperationPaths::volume_label`.
    pub source_volume_name: Option<String>,
    /// The same for `destination_path`.
    pub destination_volume_name: Option<String>,
    /// When the operation was registered, which is when its row appeared.
    pub queued_at: u64,
    /// When it was admitted to run. `None` while it waits for a lane.
    pub started_at: Option<u64>,
}

/// Why `get_operation_details` has no answer.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum OperationDetailsError {
    /// The manager tracks no operation, live or retained, under this id: it
    /// settled (or its failure was dismissed) between the row being drawn and
    /// the question arriving. The row is about to leave anyway, so the frontend
    /// drops the answer rather than reporting anything.
    NotFound {
        /// The id asked about.
        operation_id: String,
    },
}

/// A failed operation kept past its record: the snapshot row it surfaces as, and
/// the details an expanded row asks for. One entry, so the two can't be evicted
/// or dismissed apart.
pub(super) struct RetainedFailure {
    pub(super) snapshot: OperationSnapshot,
    pub(super) details: OperationDetails,
}

impl OperationManager {
    /// One row's details, for the queue window's expanded row: a live record
    /// first, then a retained failure (the same join `snapshot()` makes).
    pub(crate) fn details(&self, operation_id: &str) -> Result<OperationDetails, OperationDetailsError> {
        let inner = self.inner.lock_ignore_poison();
        if let Some(rec) = inner.records.get(operation_id) {
            return Ok(rec
                .descriptor
                .summary
                .paths
                .details_for(operation_id, rec.queued_at, rec.started_at));
        }
        inner
            .failures
            .iter()
            .find(|failure| failure.snapshot.operation_id == operation_id)
            .map(|failure| failure.details.clone())
            .ok_or_else(|| OperationDetailsError::NotFound {
                operation_id: operation_id.to_string(),
            })
    }
}

/// The wall clock as Unix seconds. A clock set before 1970 reads as 0, which
/// the frontend renders as "no time" rather than a date in 1970.
pub(super) fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}
