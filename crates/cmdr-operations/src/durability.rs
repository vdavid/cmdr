//! Targeted durability, so "complete" means "durable on disk", not "buffered in
//! the OS page cache or the drive's cache". Two passes, one per kind of write:
//! an operation that produced BYTES syncs each created destination the copy
//! didn't already sync and `fsync`s every directory that gained an entry
//! (`flush_created_destinations`), while one that only moved directory ENTRIES
//! flushes the directories themselves (`flush_touched_directories`). Both
//! announce the `Flushing` phase first, and both end with the operation's
//! closing pass (`state.barriers`, `cmdr_fs::durable::DeviceBarriers`), which
//! syncs the written files still waiting and then barriers: every sync before
//! the barrier is a plain `fsync`, so one barrier covers them all.

use cmdr_fs::durable::DeviceBarriers;
use cmdr_fs::log_error::{ErrorLogText, ErrorText, LogError, write_plain};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::event_sinks::OperationEventSink;
use super::state::WriteOperationState;
use cmdr_fs::file_identity::{DeviceId, device_of};
use cmdr_fs::log_private::LogPrivate;

/// What a flush couldn't make durable: the path it was syncing and the OS error
/// number (`None` when the failure carried none).
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct FlushFailure {
    pub path: PathBuf,
    pub errno: Option<i32>,
}

impl std::fmt::Display for FlushFailure {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write_plain(self, f)
    }
}

/// The one text behind `Display` and `LogError`: the path a flush synced is marked.
impl ErrorLogText for FlushFailure {
    fn write_log_text(&self, out: &mut ErrorText<'_>) -> std::fmt::Result {
        use std::fmt::Write as _;
        out.value(LogPrivate::path(&self.path))?;
        match self.errno {
            Some(errno) => write!(out, ": {}", io::Error::from_raw_os_error(errno)),
            None => out.write_str(": no OS error code"),
        }
    }
}

/// Emits a `Flushing`-phase progress event, then makes a transfer's output
/// durable: syncs every created file whose data isn't already with its drive,
/// `fsync`s every directory that gained an entry, then runs the operation's
/// closing device barrier. Blocks until it's done.
///
/// - **File data.** `data_covered` holds the files whose sync `state.barriers`
///   carries (chunked copy's `note_written`: synced inline on a network mount,
///   or waiting for the pass, which `state.barriers.flush()` below runs) or for
///   which a flush is moot (APFS clonefile / reflink, which share copy-on-write
///   extents with the source). This loop skips them. A symlink has no data of
///   its own.
/// - **Directory entries.** A synced file is still only reachable once the entry
///   naming it is on disk, and FAT/exFAT keep no journal to replay a lost one.
///   So the parent of every created file AND of every created directory is
///   fsynced, each distinct directory once, whatever `data_covered` says. A
///   directory that only received a `mkdir` is covered by its parent's fsync.
/// - **The closing pass.** On macOS every sync above is a plain `fsync`, which
///   stops at the drive's cache, so the pass ends with `state.barriers.flush()`:
///   it syncs the written files still waiting, then runs one `F_FULLFSYNC` per
///   device the operation wrote to (nothing on Linux, whose `fdatasync` already
///   flushed). A file sync it (or an earlier interval pass) couldn't confirm, or
///   a barrier the drive refuses, is a failure, named after the first directory
///   that gained an entry: the user's own destination, never a temp name. The
///   log names the file.
///
/// Every path is tried even after a failure, so the output ends as durable as it
/// can, and the first failure is the answer. A move deletes its sources only on
/// `Ok`; a copy deletes nothing and only logs an `Err`.
#[allow(
    clippy::too_many_arguments,
    reason = "These are the natural operation-wide values a progress emit needs; bundling them into a struct adds ceremony without cleaning anything up, matching WriteProgressEvent::new."
)]
pub(super) fn flush_created_destinations(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: super::types::WriteOperationType,
    state: &Arc<WriteOperationState>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
    created_files: &[PathBuf],
    created_dirs: &[PathBuf],
    data_covered: &HashSet<PathBuf>,
) -> Result<(), FlushFailure> {
    emit_flushing_phase(
        events,
        operation_id,
        operation_type,
        state,
        files_done,
        files_total,
        bytes_done,
        bytes_total,
    );

    let mut first_failure: Option<FlushFailure> = None;
    let mut note_failure = |path: &Path, e: &io::Error| {
        first_failure.get_or_insert_with(|| FlushFailure {
            path: path.to_path_buf(),
            errno: e.raw_os_error(),
        });
    };

    for file in created_files.iter().filter(|file| !data_covered.contains(*file)) {
        if let Err(e) = sync_file_data(file, &state.barriers) {
            log::warn!(
                target: "write_durability",
                "flush: couldn't sync the data of {}: {e}",
                LogPrivate::file_path(file)
            );
            note_failure(file, &e);
        }
    }

    // A filesystem that doesn't do directory fsync answers that for every
    // directory on it, so it's said once per destination volume per operation.
    let mut warned_volumes: HashSet<Option<DeviceId>> = HashSet::new();
    let dirs = directories_gaining_entries(created_files, created_dirs);
    for dir in &dirs {
        match fsync_dir(dir, &state.barriers) {
            Ok(()) => {}
            Err(e) if refuses_directory_fsync(&e) => {
                if warned_volumes.insert(volume_of(dir)) {
                    log::warn!(
                        target: "write_durability",
                        "flush: the filesystem holding {} doesn't support directory fsync, so its entries count as done for this operation: {e}",
                        LogPrivate::dir_path(dir)
                    );
                    #[cfg(test)]
                    test_hook::note_unsupported_warning(dir);
                }
            }
            Err(e) => {
                log::warn!(
                    target: "write_durability",
                    "flush: couldn't fsync the directory {}: {e}",
                    LogPrivate::dir_path(dir)
                );
                note_failure(dir, &e);
            }
        }
    }

    // The closing pass, after every sync above: the written files still
    // waiting, then one barrier per device, covering what the copy handed over
    // file by file too.
    if let Err(failure) = state.barriers.flush() {
        log::warn!(
            target: "write_durability",
            "flush: the drive didn't confirm the closing pass (at {}): {}",
            LogPrivate::path(&failure.path),
            // allowed-log-private: an errno from `fsync` or `fcntl` on an open fd, which names no path
            failure.error
        );
        let named = dirs.first().map_or(failure.path.as_path(), PathBuf::as_path);
        note_failure(named, &failure.error);
    }
    log::debug!(
        target: "write_durability",
        "flush: op={operation_id} issued {} device barriers",
        state.barriers.barriers_issued()
    );

    first_failure.map_or(Ok(()), Err)
}

/// What a copy's completion says about its flush: nothing when it confirmed
/// everything, else the typed warning, with the path and errno logged here. A
/// copy deletes nothing, so a failed flush doesn't stop it (a move, which would
/// delete its sources, stops with `MoveNotConfirmed` instead).
pub(super) fn unconfirmed_copy(
    flushed: Result<(), FlushFailure>,
    operation_id: &str,
    state: &WriteOperationState,
) -> Option<super::types::DurabilityUnconfirmed> {
    let failure = flushed.err()?;
    log::warn!(
        target: "write_durability",
        "copy_files_with_progress: op={} completes without proof its output is durable ({failure})",
        operation_id, failure = LogError(&failure)
    );
    Some(super::types::DurabilityUnconfirmed {
        volume_name: state.sides.as_ref().map(|sides| sides.destination.volume_name.clone()),
    })
}

/// Whether a directory fsync failed because the filesystem doesn't do directory
/// fsync at all. Network and FUSE mounts are the likely ones, and treating it as
/// a failure would turn every move onto them into a failed move with duplicated
/// files. Every other errno (`EIO`, `ENOSPC`, `EROFS`, `ENXIO`, `ENOENT`,
/// `EACCES`, …), and an error carrying none, stays a failure that keeps sources.
/// The class list is `cmdr_fs::durable::SYNC_UNSUPPORTED`, shared with the
/// device barrier; `durability_tests.rs` pins each errno on both platforms.
fn refuses_directory_fsync(e: &io::Error) -> bool {
    cmdr_fs::durable::is_sync_unsupported(e)
}

/// The device a directory lives on, to group the "doesn't support directory
/// fsync" warning by volume. `None` when it can't be read, which groups those
/// directories together rather than warning for each.
fn volume_of(dir: &Path) -> Option<DeviceId> {
    fs::metadata(dir).ok().as_ref().and_then(device_of)
}

/// Hands one created file to its drive (`durable::sync_to_drive`) and notes it
/// for the closing barrier. A symlink carries no data of its own, and opening
/// one would follow it, so it's done without opening anything.
fn sync_file_data(file: &Path, barriers: &DeviceBarriers) -> io::Result<()> {
    if fs::symlink_metadata(file)?.file_type().is_symlink() {
        return Ok(());
    }
    let handle = fs::File::open(file)?;
    cmdr_fs::durable::sync_to_drive(&handle)?;
    barriers.note_synced(&handle, file);
    Ok(())
}

/// The distinct directories whose entries a transfer added, in first-seen
/// order: the parent of every file and of every directory it created.
fn directories_gaining_entries(created_files: &[PathBuf], created_dirs: &[PathBuf]) -> Vec<PathBuf> {
    let mut seen: HashSet<&Path> = HashSet::new();
    let mut dirs: Vec<PathBuf> = Vec::new();
    for parent in created_files.iter().chain(created_dirs).filter_map(|p| p.parent()) {
        if seen.insert(parent) {
            dirs.push(parent.to_path_buf());
        }
    }
    dirs
}

/// Emits a `Flushing`-phase progress event, then `fsync`s each of `directories`
/// once so the entry changes inside them are durable. Blocks until they're done.
///
/// This is what a `rename(2)`-based move flushes: the rename moved directory
/// ENTRIES, and the moved files' own data blocks and inodes were already durable
/// before it (see `MoveTransaction::touched_directories`, which picks the
/// directories). Syncing the files here would buy nothing and cost one
/// `fcntl(F_FULLFSYNC)` per file on macOS, a device-level barrier that dwarfs
/// this pass (`transfer/DETAILS.md` § Durability carries the measurements). The
/// directories get a plain `fsync` each, then one closing barrier.
///
/// Best-effort: nothing is deleted after a same-FS move's flush, so a directory
/// that rejects `fsync`, or a barrier the drive refuses, is logged, never
/// propagated.
#[allow(
    clippy::too_many_arguments,
    reason = "The natural operation-wide values a progress emit needs, matching flush_created_destinations."
)]
pub(super) fn flush_touched_directories(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: super::types::WriteOperationType,
    state: &Arc<WriteOperationState>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
    directories: &[PathBuf],
) {
    emit_flushing_phase(
        events,
        operation_id,
        operation_type,
        state,
        files_done,
        files_total,
        bytes_done,
        bytes_total,
    );

    for dir in directories {
        if let Err(e) = fsync_dir(dir, &state.barriers) {
            log::debug!(
                target: "write_durability",
                "flush: dir fsync skipped for {}: {e}",
                LogPrivate::dir_path(dir)
            );
        }
    }
    if let Err(failure) = state.barriers.flush() {
        log::debug!(
            target: "write_durability",
            "flush: the closing barrier went unconfirmed (issued through {}): {}",
            LogPrivate::path(&failure.path),
            // allowed-log-private: an errno from `fcntl` on an open fd, which names no path
            failure.error
        );
    }
}

/// Announces the closing flush so the FE can show "Writing the last piece…"
/// instead of a bar frozen at 100% on slow media.
#[allow(
    clippy::too_many_arguments,
    reason = "The natural operation-wide values a progress emit needs, matching WriteProgressEvent::new."
)]
fn emit_flushing_phase(
    events: &dyn OperationEventSink,
    operation_id: &str,
    operation_type: super::types::WriteOperationType,
    state: &Arc<WriteOperationState>,
    files_done: usize,
    files_total: usize,
    bytes_done: u64,
    bytes_total: u64,
) {
    use super::types::{WriteOperationPhase, WriteProgressEvent};

    state.emit_progress_via_sink(
        events,
        WriteProgressEvent::new(
            operation_id.to_string(),
            operation_type,
            WriteOperationPhase::Flushing,
            None,
            files_done,
            files_total,
            bytes_done,
            bytes_total,
        ),
    );
}

/// Opens a directory and hands the entry changes inside it to the drive
/// (`durable::sync_dir_to_drive`), noting it for the closing barrier, which is
/// what makes them durable on macOS.
fn fsync_dir(dir: &Path, barriers: &DeviceBarriers) -> io::Result<()> {
    #[cfg(test)]
    if let Some(answer) = test_hook::intercept(dir) {
        return answer;
    }
    let handle = fs::File::open(dir)?;
    cmdr_fs::durable::sync_dir_to_drive(&handle)?;
    barriers.note_synced(&handle, dir);
    Ok(())
}

/// Test seam: records every directory `fsync_dir` is asked to sync on this
/// thread, and optionally answers for it, so a test can see which directories a
/// flush reached and make one fail with a chosen errno. Production never installs
/// a hook. Thread-local, so it only sees engines the test drives on its own
/// thread, which is how every local-move and local-copy test calls them.
#[cfg(test)]
pub(super) mod test_hook {
    use std::cell::RefCell;
    use std::path::{Path, PathBuf};
    use std::rc::Rc;

    type Answer = Box<dyn FnMut(&Path) -> Option<std::io::Result<()>>>;

    struct Hook {
        dirs: Rc<RefCell<Vec<PathBuf>>>,
        unsupported_warnings: Rc<RefCell<Vec<PathBuf>>>,
        answer: Answer,
    }

    thread_local! {
        static HOOK: RefCell<Option<Hook>> = const { RefCell::new(None) };
    }

    /// The directories a flush asked to `fsync` while this log was installed.
    /// Uninstalls the hook on drop.
    pub(crate) struct DirSyncLog {
        dirs: Rc<RefCell<Vec<PathBuf>>>,
        unsupported_warnings: Rc<RefCell<Vec<PathBuf>>>,
    }

    impl DirSyncLog {
        /// Every directory `fsync_dir` was called with, in call order,
        /// duplicates included.
        pub(crate) fn dirs(&self) -> Vec<PathBuf> {
            self.dirs.borrow().clone()
        }

        /// The directory each "doesn't support directory fsync" `warn` was
        /// logged for, in order.
        pub(crate) fn unsupported_warnings(&self) -> Vec<PathBuf> {
            self.unsupported_warnings.borrow().clone()
        }
    }

    impl Drop for DirSyncLog {
        fn drop(&mut self) {
            HOOK.with(|h| *h.borrow_mut() = None);
        }
    }

    /// Records every directory sync on this thread. `answer` decides each one:
    /// `None` runs the real `fsync`, `Some(result)` returns `result` instead.
    pub(crate) fn record(answer: impl FnMut(&Path) -> Option<std::io::Result<()>> + 'static) -> DirSyncLog {
        let dirs = Rc::new(RefCell::new(Vec::new()));
        let unsupported_warnings = Rc::new(RefCell::new(Vec::new()));
        HOOK.with(|h| {
            *h.borrow_mut() = Some(Hook {
                dirs: Rc::clone(&dirs),
                unsupported_warnings: Rc::clone(&unsupported_warnings),
                answer: Box::new(answer),
            });
        });
        DirSyncLog {
            dirs,
            unsupported_warnings,
        }
    }

    pub(super) fn note_unsupported_warning(dir: &Path) {
        HOOK.with(|h| {
            if let Some(hook) = h.borrow().as_ref() {
                hook.unsupported_warnings.borrow_mut().push(dir.to_path_buf());
            }
        });
    }

    pub(super) fn intercept(dir: &Path) -> Option<std::io::Result<()>> {
        HOOK.with(|h| {
            h.borrow_mut().as_mut().and_then(|hook| {
                hook.dirs.borrow_mut().push(dir.to_path_buf());
                (hook.answer)(dir)
            })
        })
    }
}

#[cfg(test)]
#[path = "durability_tests.rs"]
mod tests;
