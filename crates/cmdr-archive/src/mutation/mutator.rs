//! `ArchiveMutator`: applies a changeset to a zip via temp+rename safe-overwrite.
//!
//! This is the data-safety heart of zip editing. It builds the FULL new archive
//! into a same-directory sibling temp, then atomically renames it over the
//! original — so the original is byte-for-byte intact until the final rename, and
//! a cancel or crash at ANY earlier point leaves the original fully readable and
//! the temp abandoned (reaped on the next edit). It is deliberately decoupled
//! from the `Volume` trait and the operation manager: it takes a plain
//! [`Changeset`] plus a [`MutationHooks`] control seam, so it's fully
//! unit-testable without a host or its write-ops machinery. The host's archive-edit
//! driver wraps it with the real event sink, pause gate, and cancel intent.
//!
//! ## Why temp+rename, not append-in-place
//!
//! `zip`'s `ZipWriter::new_append` overwrites the old central directory, so a
//! cancel mid-edit corrupts the archive (the original does NOT survive). Building
//! a fresh archive to a temp and renaming is Cmdr's mandated safe-overwrite
//! (AGENTS.md principle 4) and the only shape where cancel is genuinely free
//! (abandon the temp, no rollback ledger). Retained entries copy verbatim via
//! `raw_copy_file` (name bytes and UTF-8 flag as stored) or, when renamed,
//! `raw_copy_file_rename` (no decompress/recompress — byte-for-byte); only newly
//! added files are compressed. An encrypted entry raw-copies with its flags and
//! still decrypts; one in a scheme the copy can't carry refuses the edit (see
//! `MutationError::EncryptedEntryRetained`).

use cmdr_fs::log_error::{ErrorLogText, ErrorText, write_plain};
use std::collections::HashMap;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use uuid::Uuid;
use zip::write::{FullFileOptions, SimpleFileOptions};
use zip::{CompressionMethod, ZipArchive, ZipWriter};

/// `foo.zip` builds into the same-directory sibling `foo.zip.cmdr-tmp-<uuid>`
/// (the app-wide scratch convention, shortened when it wouldn't fit); the
/// sibling placement keeps the final rename atomic on one filesystem.
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::metadata::{CarriedMetadata, LandedMetadata, StagedItem, StagedTimes, apply_landed, apply_staged};
use cmdr_fs::staging::{StagingTemp, temp_sibling_prefix};

pub use super::changeset::{AddEntry, AddSource, Changeset, MutationError};
use super::plan::plan_retained;
use crate::apple_double;

/// Chunk size for streaming an added file's bytes into the compressor: bounds
/// peak in-flight memory regardless of the file's size (principle 5 — never
/// buffer a whole file), and is the pause/cancel granularity mid-file.
const ADD_CHUNK_BYTES: usize = 128 * 1024;

/// Progress snapshot passed to [`MutationHooks::on_progress`]. Two axes: entries
/// written (retained + mkdir + add) and bytes processed (retained compressed
/// bytes raw-copied + added bytes compressed), so a delete of one file from a
/// big zip shows a real, moving progress bar rather than an instant flash.
///
/// `entries_changed` is a separate, constant tally of the entries the edit
/// actually ADDS, REMOVES, or RENAMES — the user-facing "files processed" count.
/// It is deliberately NOT the written-entry total (`entries_total`, which counts
/// RETAINED entries too): deleting one file from a 3-entry zip changes 1, even
/// though 2 retained entries are rewritten. Drivers report this, not
/// `entries_total`, on the terminal event.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MutationProgress {
    /// Entries written into the temp so far, retained ones included.
    pub entries_done: usize,
    /// Entries the rewrite will write in total, retained ones included. ❌ Not the
    /// user-facing count; see the struct doc.
    pub entries_total: usize,
    /// Constant across ticks: the count of entries added, deleted, or renamed
    /// (the user-facing affected-item count). See the struct doc.
    pub entries_changed: usize,
    /// Uncompressed bytes written so far.
    pub bytes_done: u64,
    /// Uncompressed bytes the rewrite will write in total.
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
    /// Announces a path the mutator is about to write, BEFORE the syscall — the
    /// driver registers it with the downloads-watcher ignore set so Cmdr's own
    /// write doesn't fire a "Downloaded …" toast. Called for the temp (before
    /// create) and the final archive path (before the rename). No-op by default.
    fn note_pending(&self, _path: &Path) {}
}

/// No-op hooks for tests and uncontrolled edits.
pub struct NoHooks;
impl MutationHooks for NoHooks {}

/// Applies `changeset` to the zip at `archive_path` via temp+rename safe-overwrite.
///
/// Blocking (opens files, decompresses/compresses, fsyncs) — call it from
/// `spawn_blocking`, never the async executor. The original archive is never
/// touched until the final atomic rename; any earlier failure or cancel removes
/// the temp and returns with the original intact.
pub fn apply(archive_path: &Path, changeset: &Changeset, hooks: &dyn MutationHooks) -> Result<(), MutationError> {
    let archive_path = &link_target(archive_path)?;

    // Reap any abandoned build from a prior crashed edit of THIS archive before
    // we start (there is no startup reaper). Our own temp uses a fresh uuid, so
    // this can't touch the build we're about to make. A leftover is always an
    // abandoned build — the original is intact — so reaping here is safe.
    reap_sibling_temps(archive_path);

    let src_file = File::open(archive_path).map_err(MutationError::OpenOriginal)?;
    let mut src = ZipArchive::new(src_file).map_err(MutationError::Zip)?;

    // Which original entries the edit keeps, and under what name. Refuses (with
    // no temp made yet) an edit that would keep an encrypted entry.
    let plan = plan_retained(&mut src, archive_path, changeset)?;
    let retained = plan.entries;

    let added_bytes: u64 = changeset.adds.iter().map(add_source_len).sum();
    let entries_total = retained.len() + changeset.mkdirs.len() + changeset.adds.len();
    // The user-facing "files processed" count: entries this edit adds, removes, or
    // renames — never the retained entries it merely rewrites in place.
    let entries_changed = plan.deleted_count + plan.renamed_count + changeset.mkdirs.len() + changeset.adds.len();
    let bytes_total = plan.bytes + added_bytes;
    let mut progress = MutationProgress {
        entries_done: 0,
        entries_total,
        entries_changed,
        bytes_done: 0,
        bytes_total,
    };
    hooks.on_progress(progress);

    // Build the new archive into a same-directory sibling temp. The guard removes
    // it on any early return (error or cancel) so the original stays alone.
    // `staged` lives to the end of this function, keeping the temp out of the
    // pane while the new archive is built and until the rename lands it.
    let staged = StagingTemp::mint(archive_path, None);
    let temp_path = staged.path().to_path_buf();
    hooks.note_pending(&temp_path);
    let mut temp_file = File::create(&temp_path).map_err(MutationError::Io)?;
    let mut guard = TempGuard {
        path: temp_path.clone(),
        armed: true,
    };

    // Whatever precedes the zip data (a self-extractor stub) goes first,
    // verbatim. The writer then starts at its end and records every offset
    // from the start of the file, the shape `zip -A` gives a self-extractor.
    if plan.prefix_len > 0 {
        checkpoint(hooks)?;
        copy_prefix(archive_path, plan.prefix_len, &mut temp_file)?;
        progress.bytes_done += plan.prefix_len;
        hooks.on_progress(progress);
    }
    let mut writer = ZipWriter::new(temp_file);
    // The archive comment, as stored. The source's came through the same
    // `u16` length field, so it always fits.
    writer
        .set_raw_comment(Box::from(src.comment()))
        .map_err(MutationError::Zip)?;

    // Retained entries, copied verbatim (no decompress/recompress). One the edit
    // doesn't rename keeps its name bytes and UTF-8 flag as stored; a renamed one
    // takes its new name as flagged UTF-8. Cancel/pause between entries.
    for entry in &retained {
        checkpoint(hooks)?;
        let file = src.by_index_raw(entry.index).map_err(MutationError::Zip)?;
        match &entry.new_name {
            None => writer.raw_copy_file(file),
            Some(new_name) => writer.raw_copy_file_rename(file, new_name),
        }
        .map_err(MutationError::Zip)?;
        progress.entries_done += 1;
        progress.bytes_done += entry.compressed_size;
        hooks.on_progress(progress);
    }

    // Explicit directory entries, each made from a local folder followed by
    // that folder's sidecar when it carries xattrs worth keeping.
    for dir in &changeset.mkdirs {
        checkpoint(hooks)?;
        writer
            .add_directory(
                dir.trim_matches('/'),
                with_entry_mtime(SimpleFileOptions::default(), std::time::SystemTime::now()),
            )
            .map_err(MutationError::Zip)?;
        if let Some(sidecar) = changeset
            .mkdir_sources
            .get(dir)
            .and_then(|source| local_folder_sidecar(source))
        {
            write_sidecar(&mut writer, dir, &sidecar, std::time::SystemTime::now())?;
        }
        progress.entries_done += 1;
        hooks.on_progress(progress);
    }

    // Added files, stream-compressed chunk-by-chunk (never whole-buffered).
    for add in &changeset.adds {
        checkpoint(hooks)?;
        writer
            .start_file(
                add.inner_path.trim_matches('/'),
                add_entry_options(add, changeset.compression_level),
            )
            .map_err(MutationError::Zip)?;
        if let Some(sidecar) = stream_added_entry(&mut writer, add, hooks, &mut progress)? {
            let modified = add_source_mtime(add).unwrap_or_else(std::time::SystemTime::now);
            write_sidecar(&mut writer, &add.inner_path, &sidecar, modified)?;
        }
        progress.entries_done += 1;
        hooks.on_progress(progress);
    }

    // Flush the central directory and make the bytes durable. A rewrite yields
    // a fresh inode, so the original's xattrs (Finder tags, quarantine), mode,
    // and creation date then go onto the temp, so an edit doesn't strip what a
    // plain copy keeps; the modification date becomes the edit's. Best-effort:
    // metadata loss never fails a data-safe edit.
    //
    // ❗ The flush goes first, through `sync_file_data`: on a Finder-mounted
    // SMB share, `F_FULLFSYNC` (std's `sync_all`) answers `ENOTSUP`, which
    // failed every edit of a zip there, and writes still buffered when the
    // dates go on reach the server at the real close, which stamps its own
    // mtime (verified against the Docker Samba guest share mounted with
    // `mount_smbfs`, macOS 27.0, 2026-10-12).
    let temp_file = writer.finish().map_err(MutationError::Zip)?;
    cmdr_fs::durable::sync_file_data(&temp_file).map_err(MutationError::Io)?;
    let landed = carry_archive_metadata(archive_path, &temp_file);
    drop(temp_file);

    // The atomic swap: on one filesystem this is the single instant the archive
    // changes. A concurrent reader sees either the old-complete or new-complete
    // inode, never a torn read.
    hooks.note_pending(archive_path);
    std::fs::rename(&temp_path, archive_path).map_err(MutationError::Io)?;
    guard.disarm();

    // The ACL and flags only now: a `deny delete` ACL on the temp would have
    // refused the swap (`EACCES`).
    if !landed.is_empty() {
        let report = apply_landed(archive_path, &landed);
        if report.any_refused() {
            log::warn!(target: "archive_mutator", "{} lost some of its metadata in the edit: {report:?}", LogPrivate::file_path(archive_path));
        }
    }

    // Best-effort: fsync the parent dir so the rename itself is durable across a
    // power loss (the bytes are already fsynced above).
    fsync_parent_dir(archive_path);

    Ok(())
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
/// progress. Never holds the whole file in memory. Answers the local source's
/// AppleDouble sidecar, read off the fd it already holds, when it carries
/// xattrs worth keeping.
fn stream_added_entry(
    writer: &mut ZipWriter<File>,
    add: &AddEntry,
    hooks: &dyn MutationHooks,
    progress: &mut MutationProgress,
) -> Result<Option<Vec<u8>>, MutationError> {
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
            return Ok(local_file_sidecar(&file));
        }
    }
    Ok(None)
}

/// Writes `sidecar` as the `__MACOSX` entry of the item at `inner`, dated like
/// the item (ditto does the same).
fn write_sidecar(
    writer: &mut ZipWriter<File>,
    inner: &str,
    sidecar: &[u8],
    modified: std::time::SystemTime,
) -> Result<(), MutationError> {
    let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
    writer
        .start_file(sidecar_entry_name(inner), with_entry_mtime(options, modified))
        .map_err(MutationError::Zip)?;
    writer.write_all(sidecar).map_err(MutationError::Io)
}

/// The `__MACOSX` entry that carries the AppleDouble sidecar of the archive
/// item at `inner` (`a/b` → `__MACOSX/a/._b`), the shape Finder's Compress
/// writes and every macOS extractor reads.
pub fn sidecar_entry_name(inner: &str) -> String {
    apple_double::sidecar_entry_name(inner)
}

/// Whether `inner` is the archive-root `__MACOSX` folder or inside it: where a
/// zip's sidecars live, hidden from listings, so a planner never puts an item
/// there (it could never be seen again, and an extract would read it as one).
pub fn is_in_sidecar_folder(inner: &str) -> bool {
    apple_double::is_in_macosx_folder(inner)
}

/// The AppleDouble sidecar for the open local file `file`, or `None` when it
/// carries no xattr worth keeping (David's call: an ordinary zip stays free of
/// `__MACOSX` for Windows users). Provenance and quarantine alone aren't, and
/// neither is an all-zero FinderInfo; they cost one name-only `flistxattr`.
/// macOS only: `__MACOSX` is a Mac convention, and Linux's `user.*` xattrs
/// would mean nothing to a Mac reading it.
pub fn local_file_sidecar(file: &File) -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        use xattr::FileExt as _;
        let names = file.list_xattr().ok()?.collect();
        apple_double::sidecar_for(names, |name| file.get_xattr(name).ok().flatten())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = file;
        None
    }
}

/// [`local_file_sidecar`] for the local folder at `path`, read without following
/// a link.
pub fn local_folder_sidecar(path: &Path) -> Option<Vec<u8>> {
    #[cfg(target_os = "macos")]
    {
        let names = xattr::list(path).ok()?.collect();
        apple_double::sidecar_for(names, |name| xattr::get(path, name).ok().flatten())
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = path;
        None
    }
}

/// The write options for an added entry: Deflated compression at `compression_level`
/// (`None` = the crate default, level 6), plus a modification time: the source
/// file's for a `LocalPath` add (a copy/move INTO the archive), so the archived
/// copy carries the original's mtime, and the write time for an in-memory `Bytes`
/// add (mkfile / resident payload, which has no source file).
///
/// The level is CLAMPED to 1..=9: the `zip` crate hard-errors (`UnsupportedArchive`)
/// on an out-of-range deflate level at the first entry write rather than clamping, so
/// a bad setting value or MCP `set_setting` must not be able to fail the whole edit.
fn add_entry_options(add: &AddEntry, compression_level: Option<i64>) -> FullFileOptions<'static, 'static> {
    let options = SimpleFileOptions::default()
        .compression_method(CompressionMethod::Deflated)
        .compression_level(compression_level.map(|level| level.clamp(1, 9)));
    with_entry_mtime(
        options,
        add_source_mtime(add).unwrap_or_else(std::time::SystemTime::now),
    )
}

/// A `LocalPath` add's source mtime; `None` for in-memory bytes (or a source
/// that won't stat), which the caller dates now.
fn add_source_mtime(add: &AddEntry) -> Option<std::time::SystemTime> {
    match &add.source {
        AddSource::LocalPath(path) => std::fs::metadata(path).and_then(|meta| meta.modified()).ok(),
        AddSource::Bytes(_) => None,
    }
}

/// Info-ZIP's extended-timestamp extra field ("UT"): a flags byte, then the
/// times it names as Unix seconds.
const EXTENDED_TIMESTAMP_ID: u16 = 0x5455;

/// Stamps `modified` on a NEW entry's write options the way ZIP tools read it,
/// twice over:
///
/// - the header's MS-DOS date and time, decomposed in LOCAL time. The format
///   carries no zone and every tool (`unzip -l`, Finder, Explorer) shows the
///   field as local wall-clock time, so a UTC decompose reads hours off. It has
///   2-second granularity and a 1980-2107 range; outside that range it stays at
///   the format's zero, 1980-01-01.
/// - the extended-timestamp extra field, the exact UTC second. Tools prefer it
///   over the DOS field, and so does our own reader (`rc-zip`), which is what
///   keeps a written mtime stable across the write-then-reparse round trip in
///   any time zone. Written when the time fits the field's signed 32 bits
///   (1970 to 2038), which is what Info-ZIP reads it as.
///
/// Every entry Cmdr writes goes through here, the fresh-ZIP producer included.
/// ❗ Without it a `zip` entry says 1980-01-01: the crate's own "now" default
/// needs its `time` feature, which this workspace doesn't enable.
pub fn with_entry_mtime(
    options: SimpleFileOptions,
    modified: std::time::SystemTime,
) -> FullFileOptions<'static, 'static> {
    with_entry_mtime_in(options, modified, &chrono::Local)
}

/// [`with_entry_mtime`] with the zone that stands for "local" spelled out, so a
/// test doesn't depend on the machine's.
fn with_entry_mtime_in<Tz: chrono::TimeZone>(
    options: SimpleFileOptions,
    modified: std::time::SystemTime,
    local_zone: &Tz,
) -> FullFileOptions<'static, 'static> {
    use chrono::{Datelike, Timelike};
    let utc: chrono::DateTime<chrono::Utc> = modified.into();
    let local = utc.with_timezone(local_zone).naive_local();
    let mut options = options.into_full_options();
    let dos_time = u16::try_from(local.year()).ok().and_then(|year| {
        zip::DateTime::from_date_and_time(
            year,
            local.month() as u8,
            local.day() as u8,
            local.hour() as u8,
            local.minute() as u8,
            local.second() as u8,
        )
        .ok()
    });
    if let Some(dos_time) = dos_time {
        options = options.last_modified_time(dos_time);
    }
    if let Ok(seconds) = i32::try_from(utc.timestamp())
        && seconds >= 0
    {
        const MODIFIED_PRESENT: u8 = 1;
        let mut field = [MODIFIED_PRESENT; 5];
        field[1..].copy_from_slice(&seconds.to_le_bytes());
        // `false`: into the local header, which `zip` repeats in the central directory.
        if let Err(error) = options.add_extra_field(EXTENDED_TIMESTAMP_ID, field, false) {
            log::warn!(target: "archive_mutator", "Couldn't attach a ZIP entry's exact timestamp: {error}");
        }
    }
    options
}

/// The uncompressed byte length of an add source, for the progress total. A
/// missing local source counts as 0 (the read later surfaces the real error).
fn add_source_len(add: &AddEntry) -> u64 {
    match &add.source {
        AddSource::Bytes(bytes) => bytes.len() as u64,
        AddSource::LocalPath(path) => std::fs::metadata(path).map(|m| m.len()).unwrap_or(0),
    }
}

/// The file the edit rewrites: `archive_path` itself, or for a symlinked
/// archive the file the link resolves to, so the temp is minted beside it and
/// the rename replaces IT. Renaming over the link swapped the link for a
/// regular file holding the edit and left the real archive unedited. Only the
/// last component matters: through a linked FOLDER the rename already lands in
/// the real one, so that path stays as given (which is also the path the
/// driver's announcements name).
fn link_target(archive_path: &Path) -> Result<PathBuf, MutationError> {
    let meta = std::fs::symlink_metadata(archive_path).map_err(MutationError::OpenOriginal)?;
    if meta.file_type().is_symlink() {
        std::fs::canonicalize(archive_path).map_err(MutationError::OpenOriginal)
    } else {
        Ok(archive_path.to_path_buf())
    }
}

/// Copies the first `len` bytes of the original archive into the temp.
fn copy_prefix(archive_path: &Path, len: u64, temp: &mut File) -> Result<(), MutationError> {
    let original = File::open(archive_path).map_err(MutationError::OpenOriginal)?;
    let copied = io::copy(&mut original.take(len), temp).map_err(MutationError::Io)?;
    if copied != len {
        return Err(MutationError::Io(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "the archive ended before the data ahead of its first entry did",
        )));
    }
    Ok(())
}

/// Removes any `foo.zip.cmdr-tmp-*` sibling of the archive (an abandoned build
/// from a crashed edit). Best-effort — a failure here never blocks the edit.
fn reap_sibling_temps(archive_path: &Path) {
    let (Some(parent), Some(file_name)) = (archive_path.parent(), archive_path.file_name()) else {
        return;
    };
    // The mint's own prefix, so a temp shortened to fit is matched too.
    let prefix = temp_sibling_prefix(&file_name.to_string_lossy());
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

/// Puts the original archive's metadata that may sit on a staged temp onto the
/// rebuilt `temp` (through its fd), and answers what goes on only after the
/// swap: its ACL and user flags. The shared helper (`cmdr_fs::metadata`) owns
/// the order. `com.apple.FinderInfo` is copied VERBATIM, a faithful copy that
/// keeps the custom-icon flag, so the `tags.rs` FinderInfo gotcha doesn't apply.
fn carry_archive_metadata(original: &Path, temp: &File) -> LandedMetadata {
    let source = match File::open(original).and_then(|file| file.metadata().map(|meta| (file, meta))) {
        Ok(source) => source,
        Err(e) => {
            log::warn!(target: "archive_mutator", "couldn't read the metadata of {}: {e}", LogPrivate::file_path(original));
            return LandedMetadata::default();
        }
    };
    let (file, meta) = source;
    let carried = CarriedMetadata::of_local_file(&file, &meta);
    // The birth date is the archive's own, but the modification date is the
    // edit's: the contents just changed, so sorting by date surfaces it and a
    // size + mtime backup check (rsync's default) can't skip it. Set here, in
    // the one dates call, so a Finder-mounted SMB share keeps it too.
    let times = StagedTimes {
        modified: Some(std::time::SystemTime::now()),
        accessed: meta.accessed().ok(),
    };
    let report = apply_staged(StagedItem::File(temp), &carried, times);
    if report.any_refused() {
        log::warn!(target: "archive_mutator", "the rebuilt {} took only some of its metadata: {report:?}", LogPrivate::file_path(original));
    }
    LandedMetadata::of_source_acl(original).with_flags(carried.flags)
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

#[cfg(all(test, target_os = "macos"))]
#[path = "mutator_apple_double_test.rs"]
mod mutator_apple_double_test;

#[cfg(test)]
#[path = "mutator_raw_names_test.rs"]
mod mutator_raw_names_test;

#[cfg(test)]
#[path = "mutator_archive_shape_test.rs"]
mod mutator_archive_shape_test;

#[cfg(test)]
#[path = "mutator_encrypted_test.rs"]
mod mutator_encrypted_test;
