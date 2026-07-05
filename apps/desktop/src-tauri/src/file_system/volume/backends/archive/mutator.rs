//! `ArchiveMutator`: applies a changeset to a zip via temp+rename safe-overwrite.
//!
//! This is the data-safety heart of zip editing. It builds the FULL new archive
//! into a same-directory sibling temp, then atomically renames it over the
//! original — so the original is byte-for-byte intact until the final rename, and
//! a cancel or crash at ANY earlier point leaves the original fully readable and
//! the temp abandoned (reaped on the next edit). It is deliberately decoupled
//! from the `Volume` trait and the operation manager: it takes a plain
//! [`Changeset`] plus a [`MutationHooks`] control seam, so it's fully
//! unit-testable without Tauri or the write-ops machinery. The `ArchiveEditOperation`
//! driver wraps it with the real event sink, pause gate, and cancel intent.
//!
//! ## Why temp+rename, not append-in-place
//!
//! `zip`'s `ZipWriter::new_append` overwrites the old central directory, so a
//! cancel mid-edit corrupts the archive (the original does NOT survive). Building
//! a fresh archive to a temp and renaming is the app's mandated safe-overwrite
//! (AGENTS.md principle 4) and the only shape where cancel is genuinely free
//! (abandon the temp, no rollback ledger). Retained entries copy verbatim via
//! `raw_copy_file_rename` (no decompress/recompress — byte-for-byte, including
//! encrypted entries); only newly added files are compressed.

use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// Same-directory temp infix: `foo.zip` builds into `foo.zip.cmdr-tmp-<uuid>`.
/// The `.cmdr-` prefix marks a crash-recoverable temp (the app-wide convention);
/// the sibling placement keeps the final rename atomic on one filesystem.
const TEMP_INFIX: &str = ".cmdr-tmp-";

/// Chunk size for streaming an added file's bytes into the compressor: bounds
/// peak in-flight memory regardless of the file's size (principle 5 — never
/// buffer a whole file), and is the pause/cancel granularity mid-file.
const ADD_CHUNK_BYTES: usize = 128 * 1024;

/// Where an added entry's bytes come from.
pub enum AddSource {
    /// Stream from a local file (a copy/move INTO the archive). Never whole-buffered.
    LocalPath(PathBuf),
    /// In-memory bytes: a mkfile (empty), or a small resident payload / test input.
    Bytes(Vec<u8>),
}

/// One file to add to the archive at `inner_path` (`/`-separated, archive-root-relative).
pub struct AddEntry {
    pub inner_path: String,
    pub source: AddSource,
}

/// A batch of changes applied to a zip in a single rewrite pass. Built by the
/// driver from a resolved transfer/mutation (conflicts already resolved), then
/// handed to [`apply`] which is deterministic and conflict-free.
///
/// Ordering of application: retained entries (verbatim) first, then `mkdirs`,
/// then `adds`. `deletes` and `renames` reshape the retained set (they never add
/// bytes). Inner paths are trimmed of surrounding slashes internally, so a
/// caller may pass `dir` or `/dir/` interchangeably.
#[derive(Default)]
pub struct Changeset {
    /// Files to add (compressed from their source). A mkfile is an add of empty `Bytes`.
    pub adds: Vec<AddEntry>,
    /// Explicit directory entries to create (`add_directory`).
    pub mkdirs: Vec<String>,
    /// Inner paths to drop. A directory target drops its whole subtree.
    pub deletes: Vec<String>,
    /// `(from, to)` inner-path renames. A directory target rewrites every descendant's prefix.
    pub renames: Vec<(String, String)>,
}

/// Progress snapshot passed to [`MutationHooks::on_progress`]. Two axes: entries
/// written (retained + mkdir + add) and bytes processed (retained compressed
/// bytes raw-copied + added bytes compressed), so a delete of one file from a
/// big zip shows a real, moving progress bar rather than an instant flash.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MutationProgress {
    pub entries_done: usize,
    pub entries_total: usize,
    pub bytes_done: u64,
    pub bytes_total: u64,
}

/// The control seam the driver injects: cancel, pause, and progress. Every method
/// has a no-op default so tests and uncontrolled edits use [`NoHooks`].
pub trait MutationHooks: Sync {
    /// Checked between entries and between chunks. `true` abandons the temp and
    /// returns [`MutationError::Cancelled`] with the original intact.
    fn is_cancelled(&self) -> bool {
        false
    }
    /// Parks while paused, returning on resume or cancel. Called between entries
    /// and between chunks, AFTER the cancel check (cancel-before-park ordering).
    fn wait_if_paused(&self) {}
    /// Reports cumulative progress; called after every entry and every chunk.
    fn on_progress(&self, _progress: MutationProgress) {}
}

/// No-op hooks for tests and uncontrolled edits.
pub struct NoHooks;
impl MutationHooks for NoHooks {}

/// Why an archive edit failed. Cancellation is a first-class, non-error outcome
/// (the temp is abandoned, the original is untouched); the rest are genuine
/// faults. Typed so the driver classifies without message matching.
#[derive(Debug)]
pub enum MutationError {
    /// The user cancelled: the temp was removed, the original is fully intact.
    Cancelled,
    /// Couldn't open the original archive for reading.
    OpenOriginal(io::Error),
    /// The `zip` crate rejected the original or a write into the temp.
    Zip(zip::result::ZipError),
    /// The archive holds an encrypted entry the edit would have to keep, and the
    /// `zip` crate's raw copy can't preserve the traditional-PKWARE encryption
    /// flag — so retaining it would silently corrupt the entry (ciphertext under
    /// a header that no longer says "encrypted"). We refuse the whole edit rather
    /// than risk that. Deleting the encrypted entry instead is allowed (it isn't
    /// retained). Editing encrypted archives is out of scope in v1.
    EncryptedEntryRetained { name: String },
    /// An added entry's source bytes couldn't be read.
    ReadSource { inner_path: String, source: io::Error },
    /// Filesystem I/O on the temp (create, flush, fsync, rename).
    Io(io::Error),
}

impl std::fmt::Display for MutationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MutationError::Cancelled => write!(f, "the archive edit was cancelled"),
            MutationError::OpenOriginal(e) => write!(f, "couldn't open the archive: {e}"),
            MutationError::Zip(e) => write!(f, "the archive couldn't be rewritten: {e}"),
            MutationError::EncryptedEntryRetained { name } => {
                write!(f, "the archive can't be edited because it contains an encrypted entry ('{name}')")
            }
            MutationError::ReadSource { inner_path, source } => {
                write!(f, "couldn't read the source for '{inner_path}': {source}")
            }
            MutationError::Io(e) => write!(f, "couldn't write the new archive: {e}"),
        }
    }
}

impl std::error::Error for MutationError {}

/// Applies `changeset` to the zip at `archive_path` via temp+rename safe-overwrite.
///
/// Blocking (opens files, decompresses/compresses, fsyncs) — call it from
/// `spawn_blocking`, never the async executor. The original archive is never
/// touched until the final atomic rename; any earlier failure or cancel removes
/// the temp and returns with the original intact.
pub fn apply(archive_path: &Path, changeset: &Changeset, hooks: &dyn MutationHooks) -> Result<(), MutationError> {
    // Reap any abandoned build from a prior crashed edit of THIS archive before
    // we start (there is no startup reaper). Our own temp uses a fresh uuid, so
    // this can't touch the build we're about to make. A leftover is always an
    // abandoned build — the original is intact — so reaping here is safe.
    reap_sibling_temps(archive_path);

    let src_file = File::open(archive_path).map_err(MutationError::OpenOriginal)?;
    let mut src = ZipArchive::new(src_file).map_err(MutationError::Zip)?;

    // Plan the retained set: for each original entry, compute its post-edit name
    // (dropped by a delete, prefix-rewritten by a rename, or unchanged). One raw
    // lookup per entry reads the name + compressed size together.
    let mut retained: Vec<RetainedEntry> = Vec::new();
    let mut retained_bytes: u64 = 0;
    for index in 0..src.len() {
        let file = src.by_index_raw(index).map_err(MutationError::Zip)?;
        let original_name = file.name().to_string();
        let compressed_size = file.compressed_size();
        let encrypted = file.encrypted();
        drop(file);
        if let Some(new_name) = plan_new_name(&original_name, &changeset.deletes, &changeset.renames) {
            // Refuse before creating any temp: a retained encrypted entry can't
            // be copied verbatim (the flag is lost), so touching this archive at
            // all would corrupt it. Bailing here keeps the original untouched.
            if encrypted {
                return Err(MutationError::EncryptedEntryRetained { name: original_name });
            }
            retained.push(RetainedEntry {
                index,
                new_name,
                compressed_size,
            });
            retained_bytes += compressed_size;
        }
    }

    let added_bytes: u64 = changeset.adds.iter().map(add_source_len).sum();
    let entries_total = retained.len() + changeset.mkdirs.len() + changeset.adds.len();
    let bytes_total = retained_bytes + added_bytes;
    let mut progress = MutationProgress {
        entries_done: 0,
        entries_total,
        bytes_done: 0,
        bytes_total,
    };
    hooks.on_progress(progress);

    // Build the new archive into a same-directory sibling temp. The guard removes
    // it on any early return (error or cancel) so the original stays alone.
    let temp_path = temp_sibling_path(archive_path);
    let temp_file = File::create(&temp_path).map_err(MutationError::Io)?;
    let mut guard = TempGuard {
        path: temp_path.clone(),
        armed: true,
    };
    let mut writer = ZipWriter::new(temp_file);

    // Retained entries, copied verbatim (no decompress/recompress; encrypted
    // entries survive byte-for-byte). Cancel/pause between entries.
    for entry in &retained {
        checkpoint(hooks)?;
        let file = src.by_index_raw(entry.index).map_err(MutationError::Zip)?;
        writer
            .raw_copy_file_rename(file, entry.new_name.clone())
            .map_err(MutationError::Zip)?;
        progress.entries_done += 1;
        progress.bytes_done += entry.compressed_size;
        hooks.on_progress(progress);
    }

    // Explicit directory entries.
    for dir in &changeset.mkdirs {
        checkpoint(hooks)?;
        writer
            .add_directory(dir.trim_matches('/'), SimpleFileOptions::default())
            .map_err(MutationError::Zip)?;
        progress.entries_done += 1;
        hooks.on_progress(progress);
    }

    // Added files, stream-compressed chunk-by-chunk (never whole-buffered).
    for add in &changeset.adds {
        checkpoint(hooks)?;
        writer
            .start_file(
                add.inner_path.trim_matches('/'),
                SimpleFileOptions::default().compression_method(CompressionMethod::Deflated),
            )
            .map_err(MutationError::Zip)?;
        stream_added_entry(&mut writer, add, hooks, &mut progress)?;
        progress.entries_done += 1;
        hooks.on_progress(progress);
    }

    // Flush the central directory and durably persist the temp before the swap.
    let temp_file = writer.finish().map_err(MutationError::Zip)?;
    temp_file.sync_all().map_err(MutationError::Io)?;
    drop(temp_file);

    // A rewrite yields a fresh inode; carry the original's mode, times, and
    // xattrs (macOS Finder tags, quarantine, creation date) onto the temp so an
    // edit doesn't strip what a plain copy keeps. Best-effort: metadata loss
    // never fails a data-safe edit.
    preserve_archive_metadata(archive_path, &temp_path);

    // The atomic swap: on one filesystem this is the single instant the archive
    // changes. A concurrent reader sees either the old-complete or new-complete
    // inode, never a torn read.
    std::fs::rename(&temp_path, archive_path).map_err(MutationError::Io)?;
    guard.disarm();

    // Best-effort: fsync the parent dir so the rename itself is durable across a
    // power loss (the bytes are already fsynced above).
    fsync_parent_dir(archive_path);

    Ok(())
}

/// One original entry kept in the rewrite, with its post-edit name.
struct RetainedEntry {
    index: usize,
    new_name: String,
    compressed_size: u64,
}

/// Cancel/pause boundary between entries and chunks. Checks cancel first (so a
/// cancel while paused still bails), parks if paused, then re-checks cancel.
fn checkpoint(hooks: &dyn MutationHooks) -> Result<(), MutationError> {
    if hooks.is_cancelled() {
        return Err(MutationError::Cancelled);
    }
    hooks.wait_if_paused();
    if hooks.is_cancelled() {
        return Err(MutationError::Cancelled);
    }
    Ok(())
}

/// Streams one added entry's bytes into the (already-opened) writer entry,
/// chunk-by-chunk, gating cancel/pause between chunks and reporting byte
/// progress. Never holds the whole file in memory.
fn stream_added_entry(
    writer: &mut ZipWriter<File>,
    add: &AddEntry,
    hooks: &dyn MutationHooks,
    progress: &mut MutationProgress,
) -> Result<(), MutationError> {
    match &add.source {
        AddSource::Bytes(bytes) => {
            for chunk in bytes.chunks(ADD_CHUNK_BYTES) {
                checkpoint(hooks)?;
                writer.write_all(chunk).map_err(MutationError::Io)?;
                progress.bytes_done += chunk.len() as u64;
                hooks.on_progress(*progress);
            }
        }
        AddSource::LocalPath(path) => {
            let mut file = File::open(path).map_err(|source| MutationError::ReadSource {
                inner_path: add.inner_path.clone(),
                source,
            })?;
            let mut buf = vec![0u8; ADD_CHUNK_BYTES];
            loop {
                checkpoint(hooks)?;
                let read = file.read(&mut buf).map_err(|source| MutationError::ReadSource {
                    inner_path: add.inner_path.clone(),
                    source,
                })?;
                if read == 0 {
                    break;
                }
                writer.write_all(&buf[..read]).map_err(MutationError::Io)?;
                progress.bytes_done += read as u64;
                hooks.on_progress(*progress);
            }
        }
    }
    Ok(())
}

/// The uncompressed byte length of an add source, for the progress total. A
/// missing local source counts as 0 (the read later surfaces the real error).
fn add_source_len(add: &AddEntry) -> u64 {
    match &add.source {
        AddSource::Bytes(bytes) => bytes.len() as u64,
        AddSource::LocalPath(path) => std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
    }
}

/// Computes an original entry's post-edit name, or `None` if a delete drops it.
///
/// A trailing slash (an explicit directory entry) is preserved. Deletes are
/// checked before renames (a deleted entry is gone regardless of any rename).
/// Both deletes and renames match a whole path component: `foo` deletes/renames
/// `foo` and everything under `foo/`, but never a sibling `foobar`.
fn plan_new_name(original_name: &str, deletes: &[String], renames: &[(String, String)]) -> Option<String> {
    let is_dir = original_name.ends_with('/');
    let logical = original_name.trim_end_matches('/');

    for delete in deletes {
        if path_matches_or_under(logical, delete.trim_matches('/')) {
            return None;
        }
    }

    let mut new_logical = logical.to_string();
    for (from, to) in renames {
        let from = from.trim_matches('/');
        let to = to.trim_matches('/');
        if logical == from {
            new_logical = to.to_string();
            break;
        }
        if let Some(rest) = logical.strip_prefix(&format!("{from}/")) {
            new_logical = format!("{to}/{rest}");
            break;
        }
    }

    Some(if is_dir { format!("{new_logical}/") } else { new_logical })
}

/// Whether `path` is `target` itself or a descendant of it (component-wise).
fn path_matches_or_under(path: &str, target: &str) -> bool {
    path == target || path.strip_prefix(target).is_some_and(|rest| rest.starts_with('/'))
}

/// A fresh same-directory temp path: `foo.zip` -> `foo.zip.cmdr-tmp-<uuid>`.
fn temp_sibling_path(archive_path: &Path) -> PathBuf {
    let mut name = archive_path
        .file_name()
        .map(|s| s.to_os_string())
        .unwrap_or_default();
    name.push(TEMP_INFIX);
    name.push(Uuid::new_v4().to_string());
    archive_path.with_file_name(name)
}

/// Removes any `foo.zip.cmdr-tmp-*` sibling of the archive (an abandoned build
/// from a crashed edit). Best-effort — a failure here never blocks the edit.
fn reap_sibling_temps(archive_path: &Path) {
    let (Some(parent), Some(file_name)) = (archive_path.parent(), archive_path.file_name()) else {
        return;
    };
    let prefix = format!("{}{}", file_name.to_string_lossy(), TEMP_INFIX);
    let Ok(entries) = std::fs::read_dir(parent) else {
        return;
    };
    for entry in entries.flatten() {
        if entry.file_name().to_string_lossy().starts_with(&prefix) {
            let _ = std::fs::remove_file(entry.path());
        }
    }
}

/// fsyncs the archive's parent directory so a just-completed rename is durable.
/// Best-effort (opening a dir read-only can fail on some filesystems).
fn fsync_parent_dir(archive_path: &Path) {
    if let Some(parent) = archive_path.parent()
        && let Ok(dir) = File::open(parent)
    {
        let _ = dir.sync_all();
    }
}

/// Carries the original archive's identity metadata onto the freshly built temp,
/// so an edit-in-place preserves what a plain file copy would (mode, timestamps,
/// extended attributes). Best-effort; never fails the edit.
#[cfg(target_os = "macos")]
fn preserve_archive_metadata(from: &Path, to: &Path) {
    use std::ffi::CString;
    use std::os::unix::ffi::OsStrExt;

    // copyfile flags (from <copyfile.h>): metadata only, never the data — the
    // temp already holds the rebuilt archive bytes. STAT (mode + times) | ACL |
    // XATTR (Finder tags, quarantine, creation date, and `com.apple.FinderInfo`
    // copied VERBATIM — a faithful copy that preserves the custom-icon flag,
    // never zeroing it, so the `tags.rs` FinderInfo gotcha doesn't apply here).
    const COPYFILE_ACL: u32 = 1 << 0;
    const COPYFILE_STAT: u32 = 1 << 1;
    const COPYFILE_XATTR: u32 = 1 << 2;
    const COPYFILE_METADATA: u32 = COPYFILE_ACL | COPYFILE_STAT | COPYFILE_XATTR;

    #[link(name = "System", kind = "dylib")]
    unsafe extern "C" {
        fn copyfile(from: *const i8, to: *const i8, state: *mut std::ffi::c_void, flags: u32) -> std::ffi::c_int;
    }

    let (Ok(from_c), Ok(to_c)) = (
        CString::new(from.as_os_str().as_bytes()),
        CString::new(to.as_os_str().as_bytes()),
    ) else {
        return;
    };
    // SAFETY: both C strings are valid null-terminated paths that outlive the
    // call; a null state pointer is the documented "no progress state" form; the
    // flag set omits COPYFILE_DATA so only metadata is written onto the existing
    // temp. The return value is best-effort (metadata loss never fails the edit).
    let result = unsafe { copyfile(from_c.as_ptr(), to_c.as_ptr(), std::ptr::null_mut(), COPYFILE_METADATA) };
    if result != 0 {
        log::warn!(target: "archive_mutator", "couldn't preserve archive metadata onto {}", to.display());
    }
}

/// Non-macOS metadata carry-over: mode, mtime, and extended attributes. There are
/// no Finder tags to worry about off macOS.
#[cfg(not(target_os = "macos"))]
fn preserve_archive_metadata(from: &Path, to: &Path) {
    if let Ok(meta) = std::fs::metadata(from) {
        let _ = std::fs::set_permissions(to, meta.permissions());
        if let Ok(mtime) = meta.modified() {
            let _ = filetime::set_file_mtime(to, filetime::FileTime::from_system_time(mtime));
        }
    }
    if let Ok(names) = xattr::list(from) {
        for name in names {
            if let Ok(Some(value)) = xattr::get(from, &name) {
                let _ = xattr::set(to, &name, &value);
            }
        }
    }
}

/// Removes the in-progress temp on any early return (error or cancel), so a
/// failed edit never leaves the original with a half-built sibling around. The
/// happy path disarms it right after the atomic rename (the temp no longer
/// exists under `path`).
struct TempGuard {
    path: PathBuf,
    armed: bool,
}

impl TempGuard {
    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for TempGuard {
    fn drop(&mut self) {
        if self.armed {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

#[cfg(test)]
#[path = "mutator_test.rs"]
mod mutator_test;
