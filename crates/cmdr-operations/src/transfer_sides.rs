//! The two volumes a transfer runs between, captured when it starts, and what an
//! operation says when one of them leaves the mount table mid-flight.
//!
//! A drive that vanishes is gone from the volume list AND from the mount table
//! by the time an error is worded, so nothing can look its name up then. Both
//! sides are therefore captured at START (`transfer::volume::copy` /
//! `::r#move`, where both volumes are already resolved) and carried on the
//! operation's state.
//!
//! ❌ Never work a side out from a path prefix: the transfer is HANDED both
//! volumes, and a prefix match can't tell two mounts of the same tree apart.
//! Paths are used here for one thing only, picking between two sides that have
//! BOTH left.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::state::WriteOperationState;
use super::types::{
    DisconnectedSide, ProgressAtStop, TransferRole, WriteErrorEvent, WriteOperationError, WriteOperationType,
};

/// One side of a transfer: the volume it runs on, named and rooted as it was
/// when the operation started.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct TransferSide {
    /// The volume id the operation was started with.
    pub(crate) volume_id: String,
    /// The display name as it read at start. ❗ The one chance to capture it.
    pub(crate) volume_name: String,
    /// The volume's root, which is what the mount table is asked about.
    pub(crate) root: PathBuf,
    /// Whether the root is a mount that CAN leave the table. `false` for a
    /// local volume rooted at a plain folder (a cloud-sync folder like
    /// `~/Dropbox`), which the table never listed: asking about it would read
    /// every failure as that folder's drive having left.
    watched: bool,
}

impl TransferSide {
    /// A side that is a mount, as a test states it before pulling it.
    /// Production captures with [`Self::at_start`].
    pub(crate) fn new(volume_id: String, volume_name: String, root: PathBuf) -> Self {
        Self {
            volume_id,
            volume_name,
            root,
            watched: true,
        }
    }

    /// A side as the transfer finds it when it starts: watched only if the
    /// mount table lists its root then, or can't be read (no guess either way).
    pub(crate) fn at_start(volume_id: String, volume_name: String, root: PathBuf) -> Self {
        let mut side = Self::new(volume_id, volume_name, root);
        side.watched = side.is_listed() != Some(false);
        side
    }

    /// Whether this side's root is still in the mount table: `None` when the
    /// table couldn't be read, which is never read as "gone".
    pub(crate) fn is_listed(&self) -> Option<bool> {
        root_is_listed(&self.root)
    }

    /// Whether the mount table SAYS this side is gone. An unreadable table
    /// answers `false`: a move that can't prove the destination left must keep
    /// behaving as though it's there, and an error must not claim a disconnect
    /// nobody observed. A side that was never a mount never leaves.
    pub(crate) fn has_left(&self) -> bool {
        self.watched && self.is_listed() == Some(false)
    }
}

/// Both volumes of one transfer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TransferSides {
    pub(crate) source: TransferSide,
    pub(crate) destination: TransferSide,
}

impl TransferSides {
    pub(crate) fn new(source: TransferSide, destination: TransferSide) -> Self {
        Self { source, destination }
    }

    pub(crate) fn side(&self, role: TransferRole) -> &TransferSide {
        match role {
            TransferRole::Source => &self.source,
            TransferRole::Destination => &self.destination,
        }
    }

    /// The wire fact for one side, with the other side's name alongside it: the
    /// copy needs both (where the files that made it are, or where the ones that
    /// didn't still sit).
    pub(crate) fn disconnected(&self, role: TransferRole) -> DisconnectedSide {
        let side = self.side(role);
        let counterpart = match role {
            TransferRole::Source => &self.destination,
            TransferRole::Destination => &self.source,
        };
        DisconnectedSide {
            role,
            volume_id: side.volume_id.clone(),
            volume_name: side.volume_name.clone(),
            counterpart_name: counterpart.volume_name.clone(),
        }
    }

    /// Which side has left the mount table, if either.
    ///
    /// With both gone (one hub pulled, say), `path` decides: the failing path
    /// names the volume the operation was actually touching. With no path, or
    /// one under neither root, the destination answers — it's the half a
    /// transfer is writing to, and the half whose loss the user acts on.
    pub(crate) fn vanished_side(&self, path: Option<&str>) -> Option<TransferRole> {
        let source_left = self.source.has_left();
        let destination_left = self.destination.has_left();
        match (source_left, destination_left) {
            (true, false) => Some(TransferRole::Source),
            (false, true) => Some(TransferRole::Destination),
            (true, true) => Some(self.role_of_path(path).unwrap_or(TransferRole::Destination)),
            (false, false) => None,
        }
    }

    /// Which side a path sits under, for the both-gone tie-break alone. The
    /// deeper root wins, so a drive mounted inside another's tree still answers
    /// for its own paths.
    fn role_of_path(&self, path: Option<&str>) -> Option<TransferRole> {
        let path = Path::new(path?);
        let under_source = path.starts_with(&self.source.root);
        let under_destination = path.starts_with(&self.destination.root);
        match (under_source, under_destination) {
            (true, false) => Some(TransferRole::Source),
            (false, true) => Some(TransferRole::Destination),
            (true, true) => Some(
                if self.destination.root.components().count() >= self.source.root.components().count() {
                    TransferRole::Destination
                } else {
                    TransferRole::Source
                },
            ),
            (false, false) => None,
        }
    }
}

/// A cross-filesystem move's source tally at the moment it stopped, in
/// top-level items — the same unit the sweep's own progress bar counts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct MoveSourceCounts {
    /// Originals the sweep had already removed.
    pub(crate) removed: u32,
    /// Originals still standing where the user left them.
    pub(crate) left: u32,
}

/// The `write-error` event for a transfer that stopped: the typed error with the
/// vanished drive named, plus how far the operation had got. Marks the failed
/// row in the transfer tree on the way.
///
/// ❗ Called while the operation is still registered, so the status cache still
/// holds its counters. After `on_settled` there is nothing left to read.
pub(crate) fn transfer_stop_event(
    operation_id: &str,
    operation_type: WriteOperationType,
    state: &Arc<WriteOperationState>,
    error: WriteOperationError,
    sources: Option<MoveSourceCounts>,
) -> WriteErrorEvent {
    let error = name_the_vanished_drive(error, state.sides.as_ref());
    // The row the failure names shows it, while the error is on screen.
    state.transfer_tree.note_failure(&error);
    WriteErrorEvent::new(operation_id.to_string(), operation_type, error).with_progress_at_stop(progress_at_stop(
        state,
        operation_id,
        sources,
    ))
}

/// Rewrites what a transfer answered into `DeviceDisconnected` when one of its
/// volumes has left the mount table, and names that volume.
///
/// ❗ The mount table decides, whatever the errno: a drive pulled mid-write
/// answers `ENOENT`, `EIO`, `EBADF`, or `ENXIO` depending on which call was in
/// flight, and every one of them would otherwise read to the user as a missing
/// file or a broken disk. A `Cancelled` is never rewritten: the user stopped it,
/// and saying a drive left would be a lie about their own click.
pub(crate) fn name_the_vanished_drive(
    error: WriteOperationError,
    sides: Option<&TransferSides>,
) -> WriteOperationError {
    if matches!(error, WriteOperationError::Cancelled { .. }) {
        return error;
    }
    let Some(sides) = sides else { return error };
    // The copy landed, and that stays the headline whatever stopped the
    // original's removal: a vanished source drive is named inside `cause`, never
    // in place of the whole outcome.
    if let WriteOperationError::SourceNotRemoved { path, landed_at, cause } = error {
        return WriteOperationError::SourceNotRemoved {
            path,
            landed_at,
            cause: Box::new(name_the_vanished_drive(*cause, Some(sides))),
        };
    }
    let path = failing_path(&error);
    let Some(role) = sides
        .vanished_side(path)
        // A typed disconnect (`ENODEV`/`ENXIO`) still deserves a named side even
        // when the mount table hasn't caught up with it yet.
        .or_else(|| {
            matches!(error, WriteOperationError::DeviceDisconnected { .. }).then(|| sides.role_of_path(path))?
        })
    else {
        return error;
    };
    WriteOperationError::DeviceDisconnected {
        path: path
            .map(str::to_string)
            .unwrap_or_else(|| sides.side(role).root.display().to_string()),
        side: Some(sides.disconnected(role)),
    }
}

/// The path an in-flight failure names, for the both-drives-gone tie-break and
/// to keep the technical details pointing at the file the operation was on.
/// `None` for the refusals raised before any I/O (`WriteOperationError::item_path`).
fn failing_path(error: &WriteOperationError) -> Option<&str> {
    error.item_path()
}

/// How far the operation had got, from its live status row in its own engine's
/// status cache. A move's source
/// tally rides along when the sweep is what stopped, and answers even for an
/// operation with no status row (every engine test drives the engine directly).
fn progress_at_stop(
    state: &WriteOperationState,
    operation_id: &str,
    sources: Option<MoveSourceCounts>,
) -> Option<ProgressAtStop> {
    let status = state.scope().status().snapshot(operation_id, None);
    if status.is_none() && sources.is_none() {
        return None;
    }
    let status = status.as_ref();
    Some(ProgressAtStop {
        files_done: status.map_or(0, |s| s.files_done as u32),
        files_total: status.map_or(0, |s| s.files_total as u32),
        bytes_done: status.map_or(0, |s| s.bytes_done),
        bytes_total: status.map_or(0, |s| s.bytes_total),
        sources_removed: sources.map(|counts| counts.removed),
        sources_left: sources.map(|counts| counts.left),
    })
}

/// Whether `root` is still in the OS mount table, read without touching the
/// mount itself (the same non-blocking snapshot discovery and eject read).
///
/// `None` when the table couldn't be read at all, which every caller treats as
/// "still there": the destination-listed gate that guards a move's source delete
/// must never act on a guess.
///
/// Shared with the leftover sweep (`in_flight_sweep.rs`), which asks the same
/// question about a drive the registry says is back, and gets the test hook
/// below along with it.
pub(crate) fn root_is_listed(root: &Path) -> Option<bool> {
    #[cfg(test)]
    if let Some(answer) = test_hook::intercept(root) {
        return answer;
    }
    crate::local_disk::is_mount_point(root)
}

/// Test seam: answers the mount-table question for chosen roots on this thread,
/// so a test can pull a drive out from under an engine without one. Production
/// never installs a hook. Thread-local, which is how every local-engine test
/// drives the engine.
#[cfg(test)]
pub(crate) mod test_hook {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};

    type Answer = Box<dyn FnMut(&Path) -> Option<Option<bool>>>;

    thread_local! {
        static HOOK: RefCell<Option<Answer>> = const { RefCell::new(None) };
    }

    /// Uninstalls the hook on drop, so one test's pulled drive can't reach the
    /// next.
    pub(crate) struct MountTableHook;

    impl Drop for MountTableHook {
        fn drop(&mut self) {
            HOOK.with(|h| *h.borrow_mut() = None);
        }
    }

    /// Installs `answer` for this thread: `None` reads the real mount table,
    /// `Some(listed)` answers instead.
    pub(crate) fn answer(answer: impl FnMut(&Path) -> Option<Option<bool>> + 'static) -> MountTableHook {
        HOOK.with(|h| *h.borrow_mut() = Some(Box::new(answer)));
        MountTableHook
    }

    /// Answers "gone" for exactly `root`, and the truth for everything else.
    pub(crate) fn pull(root: &Path) -> MountTableHook {
        let root = root.to_path_buf();
        answer(move |asked| (asked == root).then_some(Some(false)))
    }

    /// Answers for a whole set of roots at once. A tempdir is not a mount point,
    /// so a test whose sides must look PRESENT says so here.
    pub(crate) fn answer_each(roots: Vec<(PathBuf, Option<bool>)>) -> MountTableHook {
        answer(move |asked| roots.iter().find(|(root, _)| root == asked).map(|(_, listed)| *listed))
    }

    pub(super) fn intercept(root: &Path) -> Option<Option<bool>> {
        HOOK.with(|h| h.borrow_mut().as_mut().and_then(|hook| hook(root)))
    }
}

#[cfg(test)]
#[path = "transfer_sides_tests.rs"]
mod tests;
