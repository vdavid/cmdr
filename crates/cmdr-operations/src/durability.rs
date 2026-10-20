//! Targeted durability, so "complete" means "durable on disk", not "buffered in
//! the OS page cache". Two passes, one per kind of write: an operation that
//! produced BYTES `fdatasync`s each created destination and `fsync`s every
//! directory that gained an entry (`flush_created_destinations`), while one that
//! only moved directory ENTRIES flushes the directories themselves
//! (`flush_touched_directories`). Both announce the `Flushing` phase first. Also
//! holds the drive-index size lookup used to render directory sizes in conflict
//! UI.

use cmdr_fs::log_error::{ErrorLogText, ErrorText, LogError, write_plain};
use std::collections::HashSet;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use super::event_sinks::OperationEventSink;
use super::state::WriteOperationState;
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
/// durable: `fdatasync`s every created file whose data isn't already on disk,
/// and `fsync`s every directory that gained an entry. Blocks until it's done.
///
/// - **File data.** `already_synced` holds the files the copy strategy already
///   made durable (chunked copy's inline `sync_data`) or for which a flush is
///   moot (APFS clonefile / reflink, which share copy-on-write extents with the
///   source). Their data sync is skipped, so a long chunked batch doesn't pay a
///   second `fcntl(F_FULLFSYNC)` per file. A symlink has no data of its own.
/// - **Directory entries.** A synced file is still only reachable once the entry
///   naming it is on disk, and FAT/exFAT keep no journal to replay a lost one.
///   So the parent of every created file AND of every created directory is
///   fsynced, each distinct directory once, whatever `already_synced` says. A
///   directory that only received a `mkdir` is covered by its parent's fsync.
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
    already_synced: &HashSet<PathBuf>,
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

    for file in created_files.iter().filter(|file| !already_synced.contains(*file)) {
        if let Err(e) = sync_file_data(file) {
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
    let mut warned_volumes: HashSet<Option<u64>> = HashSet::new();
    for dir in directories_gaining_entries(created_files, created_dirs) {
        match fsync_dir(&dir) {
            Ok(()) => {}
            Err(e) if refuses_directory_fsync(&e) => {
                if warned_volumes.insert(volume_of(&dir)) {
                    log::warn!(
                        target: "write_durability",
                        "flush: the filesystem holding {} doesn't support directory fsync, so its entries count as done for this operation: {e}",
                        LogPrivate::dir_path(&dir)
                    );
                    #[cfg(test)]
                    test_hook::note_unsupported_warning(&dir);
                }
            }
            Err(e) => {
                log::warn!(
                    target: "write_durability",
                    "flush: couldn't fsync the directory {}: {e}",
                    LogPrivate::dir_path(&dir)
                );
                note_failure(&dir, &e);
            }
        }
    }

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

/// The errnos a filesystem answers for a directory fsync it doesn't support at
/// all. Raw `libc` values, which differ by platform: Linux's `ENOTSUP` and
/// `EOPNOTSUPP` are one number, macOS keeps them apart, and `ENOSYS` is 78 on
/// macOS and 38 on Linux (`durability_tests.rs` pins each one).
const DIRECTORY_FSYNC_UNSUPPORTED: [i32; 5] = [
    libc::ENOTSUP,
    libc::EOPNOTSUPP,
    libc::EINVAL,
    libc::ENOSYS,
    libc::ENOTTY,
];

/// Whether a directory fsync failed because the filesystem doesn't do directory
/// fsync at all. Network and FUSE mounts are the likely ones, and treating it as
/// a failure would turn every move onto them into a failed move with duplicated
/// files. Every other errno (`EIO`, `ENOSPC`, `EROFS`, `ENXIO`, `ENOENT`,
/// `EACCES`, …), and an error carrying none, stays a failure that keeps sources.
fn refuses_directory_fsync(e: &io::Error) -> bool {
    e.raw_os_error()
        .is_some_and(|errno| DIRECTORY_FSYNC_UNSUPPORTED.contains(&errno))
}

/// The device a directory lives on, to group the "doesn't support directory
/// fsync" warning by volume. `None` when it can't be read, which groups those
/// directories together rather than warning for each.
fn volume_of(dir: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    fs::metadata(dir).ok().map(|meta| meta.dev())
}

/// `fdatasync`s one created file. A symlink carries no data of its own, and
/// opening one would follow it, so it's done without opening anything.
fn sync_file_data(file: &Path) -> io::Result<()> {
    if fs::symlink_metadata(file)?.file_type().is_symlink() {
        return Ok(());
    }
    fs::File::open(file)?.sync_data()
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
/// this pass — `transfer/DETAILS.md` § Durability carries the measurements.
///
/// Best-effort: nothing is deleted after a same-FS move's flush, so a directory
/// that rejects `fsync` is logged, never propagated.
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
        if let Err(e) = fsync_dir(dir) {
            log::debug!(
                target: "write_durability",
                "flush: dir fsync skipped for {}: {e}",
                LogPrivate::dir_path(dir)
            );
        }
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

/// Opens a directory and `fsync`s it so the entry changes inside it are durable.
fn fsync_dir(dir: &Path) -> io::Result<()> {
    #[cfg(test)]
    if let Some(answer) = test_hook::intercept(dir) {
        return answer;
    }
    let f = fs::File::open(dir)?;
    f.sync_all()
}

/// Looks up a directory's recursive size from the drive index. Returns `None`
/// when the index doesn't cover the path (network mount, MTP, outside-scope
/// path, indexer not yet initialised). The BE intentionally never *walks*
/// the tree to compute this — `(unknown)` on the FE is the legitimate
/// fallback when the cached value isn't available.
pub(super) fn lookup_indexed_size(host: &super::host::EngineHost, path: &Path) -> Option<super::types::KnownSize> {
    host.index().recursive_size(path)
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
