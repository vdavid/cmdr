//! The writer thread's buffered ANN mutations (plan M6).

use std::path::PathBuf;

use crate::media_index::ann;

/// The writer thread's buffered ANN mutations (plan M6): ops accumulate beside the
/// DB writes they mirror and land on the `.usearch` file at flush seams. The dirty
/// marker goes on disk BEFORE the first tracked commit, so a crash with unflushed
/// ops is detectable next session (`ann::wipe_if_crashed`).
pub(super) struct AnnPending {
    pub(super) db_path: PathBuf,
    ops: Vec<ann::AnnOp>,
    dirty_marked: bool,
}

impl AnnPending {
    pub(super) fn new(db_path: PathBuf) -> Self {
        Self {
            db_path,
            ops: Vec::new(),
            dirty_marked: false,
        }
    }

    /// Put the dirty marker on disk (once per batch). MUST run before the DB write
    /// it tracks commits — that ordering is what makes a crash between the commit
    /// and the flush detectable rather than a silently-lagging index.
    pub(super) fn mark_dirty(&mut self) {
        if !self.dirty_marked {
            ann::mark_dirty(&self.db_path, ann::AnnSpace::Clip);
            self.dirty_marked = true;
        }
    }

    /// Buffer one op; auto-flush past the bound so a long pass can't hold an
    /// unbounded vector buffer.
    pub(super) fn push(&mut self, op: ann::AnnOp) {
        self.ops.push(op);
        if self.ops.len() >= ann::ANN_PENDING_FLUSH_LIMIT {
            self.flush();
        }
    }

    /// Apply the buffered ops to the on-disk index (best-effort; an unusable index
    /// is wiped for rebuild). Clears the dirty marker via `ann::flush_ops`.
    ///
    /// While a rebuild is IN FLIGHT the buffer is RETAINED instead (ops kept, dirty
    /// marker kept): a flush landing mid-rebuild would lose the ops — applied to a
    /// file the install is about to overwrite, or dropped against a missing/stale
    /// file whose replacement was snapshotted BEFORE these rows committed. The next
    /// seam flush replays the retained batch idempotently on top of the installed
    /// index. The `is_in_flight` → `kick` race is benign in the other direction
    /// too: if a rebuild starts right after this check returns false, its snapshot
    /// includes the rows this flush just applied (their DB writes committed before
    /// the rebuild opens its read connection). The buffer may exceed
    /// [`ann::ANN_PENDING_FLUSH_LIMIT`] during the window — accepted, bounded by
    /// the rebuild's duration (minutes at worst).
    pub(super) fn flush(&mut self) {
        let space = ann::AnnSpace::Clip;
        if ann::rebuild::is_in_flight(&self.db_path, space) {
            log::debug!(
                target: "media_index",
                "ann flush deferred for {} (rebuild in flight; {} ops retained)",
                self.db_path.display(),
                self.ops.len()
            );
            return;
        }
        let ops = std::mem::take(&mut self.ops);
        let outcome = ann::flush_ops(&self.db_path, space, space.current_model_id(), ops);
        log::debug!(target: "media_index", "ann flush for {}: {outcome:?}", self.db_path.display());
        self.dirty_marked = false;
    }

    /// The volume's ANN index files are being deleted wholesale (purge /
    /// delete-CLIP-model): drop the buffered ops with them.
    pub(super) fn clear_after_delete(&mut self) {
        self.ops.clear();
        self.dirty_marked = false;
    }
}
