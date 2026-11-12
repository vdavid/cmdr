//! The events fake: every report the engine makes, recorded.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use cmdr_fs::ignore_poison::IgnorePoison;
use cmdr_fs::volume::{ListingPhase, ListingProgress};

use crate::DirectoryDiffBatch;
use crate::host::events::StalledOn;
use crate::host::{ListingEnd, ListingEvents};
use crate::listing_id::ListingId;
use cmdr_sort::SortColumn;

/// Records every report, per kind and in one combined order.
// DEFAULT-OK: a recorder that has heard nothing yet holds empty lists.
#[derive(Default)]
pub struct RecordingEvents {
    /// Every listing that reported it's opening.
    pub opening: Mutex<Vec<String>>,
    /// Per stalled listing, what it stalled on.
    pub stalled: Mutex<Vec<(String, StalledOn)>>,
    /// Per progress report, the rows loaded so far.
    pub progress: Mutex<Vec<(String, usize)>>,
    /// Per progress report that carried a phase, that phase.
    pub phases: Mutex<Vec<(String, ListingPhase)>>,
    /// Per finished read, the rows it read.
    pub read_complete: Mutex<Vec<(String, usize)>>,
    /// Per completed listing, its row count.
    pub complete: Mutex<Vec<(String, usize)>>,
    /// Per completed listing, the hidden items it reported its empty pane hides.
    pub hidden_counts: Mutex<Vec<(String, usize)>>,
    /// Per completed listing, the stored spelling it reported (`None` = as asked).
    pub stored_paths: Mutex<Vec<(String, Option<PathBuf>)>>,
    /// Per completed listing, whether it came back empty at its volume's root.
    pub empty_at_volume_root: Mutex<Vec<(String, bool)>>,
    /// Per completed listing, the column its rows are ordered by.
    pub effective_sorts: Mutex<Vec<(String, SortColumn)>>,
    /// Every listing that ended cancelled.
    pub cancelled: Mutex<Vec<String>>,
    /// Per failed listing, the error's message.
    pub failed: Mutex<Vec<(String, String)>>,
    /// Per flush, the batches it sent.
    pub diffs: Mutex<Vec<(String, Vec<DirectoryDiffBatch>)>>,
    /// Per respelled listing, the stored spelling it took.
    pub respelled: Mutex<Vec<(String, String)>>,
    /// Every report's kind (`"opening"`, `"progress"`, `"complete"`, …), in order.
    pub sequence: Mutex<Vec<&'static str>>,
}

impl RecordingEvents {
    /// A recorder that has heard nothing yet.
    pub fn new() -> Self {
        Self::default()
    }

    /// The kinds of every report so far, in order.
    pub fn sequence(&self) -> Vec<&'static str> {
        self.sequence.lock_ignore_poison().clone()
    }

    fn note(&self, kind: &'static str) {
        self.sequence.lock_ignore_poison().push(kind);
    }
}

impl ListingEvents for RecordingEvents {
    fn opening(&self, id: &ListingId) {
        self.note("opening");
        self.opening.lock_ignore_poison().push(id.to_string());
    }

    fn stalled(&self, id: &ListingId, on: StalledOn) {
        self.note("stalled");
        self.stalled.lock_ignore_poison().push((id.to_string(), on));
    }

    fn progress(&self, id: &ListingId, progress: ListingProgress) {
        self.note("progress");
        self.progress
            .lock_ignore_poison()
            .push((id.to_string(), progress.entries()));
        if let Some(phase) = progress.phase {
            self.phases.lock_ignore_poison().push((id.to_string(), phase));
        }
    }

    fn read_complete(&self, id: &ListingId, total: usize) {
        self.note("read_complete");
        self.read_complete.lock_ignore_poison().push((id.to_string(), total));
    }

    fn finished(&self, id: &ListingId, end: ListingEnd) {
        let id = id.to_string();
        match end {
            ListingEnd::Complete {
                total_count,
                hidden_count,
                stored_path,
                empty_at_volume_root,
                effective_sort_by,
                ..
            } => {
                self.note("complete");
                self.complete.lock_ignore_poison().push((id.clone(), total_count));
                self.hidden_counts.lock_ignore_poison().push((id.clone(), hidden_count));
                self.effective_sorts
                    .lock_ignore_poison()
                    .push((id.clone(), effective_sort_by));
                self.stored_paths.lock_ignore_poison().push((id.clone(), stored_path));
                self.empty_at_volume_root
                    .lock_ignore_poison()
                    .push((id, empty_at_volume_root));
            }
            ListingEnd::Cancelled { .. } => {
                self.note("cancelled");
                self.cancelled.lock_ignore_poison().push(id);
            }
            ListingEnd::Failed { error, .. } => {
                self.note("failed");
                self.failed.lock_ignore_poison().push((id, error.to_string()));
            }
        }
    }

    fn diff(&self, id: &ListingId, batches: Vec<DirectoryDiffBatch>) {
        self.note("diff");
        self.diffs.lock_ignore_poison().push((id.to_string(), batches));
    }

    fn respelled(&self, id: &ListingId, path: &Path) {
        self.note("respelled");
        self.respelled
            .lock_ignore_poison()
            .push((id.to_string(), path.to_string_lossy().into_owned()));
    }
}
