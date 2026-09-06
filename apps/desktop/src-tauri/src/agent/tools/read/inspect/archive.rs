//! The archive kind: a `.zip` (or tar, 7z) named in `paths`, a directory inside one, or a
//! file inside one, each answered the way the pane browses an archive.
//!
//! Detection and routing are the volume manager's (`VolumeManager::resolve`: the shared
//! boundary detector, confirmed by magic bytes, plus the on-demand `ArchiveVolume` and its
//! LRU), so the agent and the pane see one tree, and a mislabeled `.zip` falls through to
//! the plain pipeline as text or binary. A directory (the archive root, or one inside it)
//! is listed from the archive's cached index, which is where `encrypted` lives (`FileEntry`
//! can't carry it). A file is streamed to the viewer's bounded temp
//! (`file_viewer::routed_extract`, the same 256 MiB refuse-before-extract cap) and read by
//! the normal per-kind pipeline, and [`TempCleanup`] removes the temp however the read ends.
//! An encrypted file is refused before any byte is extracted: the tool has no password path.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use serde::Serialize;

use super::{Content, FileRow, ReadFailure, TextAsk, UnreadableReason, ok_row, spoken_modified, status_for};
use crate::file_system::volume::VolumeError;
use crate::file_system::volume::manager::RoutedKind;
use crate::file_system::volume::manager::get_volume_manager;
use crate::file_viewer::ViewerError;
use crate::file_viewer::routed_extract::ExtractedEntry;
use crate::search::format_size;
use cmdr_archive::ArchiveNode;
use cmdr_archive::{ArchiveVolume, archive_boundary_candidate, format_for_path};

/// Immediate children one archive row lists. Past it the row says `truncated` and the
/// model asks about a subfolder; paging an archive's children is a follow-up.
pub(crate) const MAX_ARCHIVE_ENTRIES: usize = 200;

/// The extract step, as a value so a test can shrink the cap and point the temp at its own
/// dir. Production passes `routed_extract::extract_if_routed`.
pub(crate) type ExtractFn<'a> = &'a (dyn Fn(&Path, &str) -> Result<Option<ExtractedEntry>, ViewerError> + Sync);

// ── Result DTOs ─────────────────────────────────────────────────────────────

/// One immediate child of the listed directory.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveEntry {
    pub name: String,
    pub is_dir: bool,
    /// Uncompressed bytes. Absent for a directory.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size_human: Option<String>,
    /// RFC 3339 UTC seconds, when the archive recorded one (a directory implied by a
    /// deeper entry's path has none).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified_human: Option<String>,
    /// `true` when the entry needs a password to extract; the tool has none, so such a
    /// path answers `unreadable { encrypted }`.
    #[serde(skip_serializing_if = "super::super::is_false")]
    pub encrypted: bool,
}

/// An archive, or a directory inside one: its immediate children.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ArchiveContent {
    /// `ArchiveFormat::label()`: `zip`, `tar.gz`, `7z`, ...
    pub format: String,
    /// `""` for the archive root, else the inner directory listed (`/`-separated).
    pub inner: String,
    /// Directories first, then names case-insensitively, as the pane lists them.
    pub entries: Vec<ArchiveEntry>,
    /// Immediate children in the directory.
    pub total: usize,
    /// Of them, how many `entries` holds.
    pub returned: usize,
    /// `returned < total`.
    pub truncated: bool,
    /// Whether any entry anywhere in the archive is encrypted.
    pub has_encrypted_entries: bool,
}

// ── Routing ─────────────────────────────────────────────────────────────────

/// Where a requested path sits relative to the archives the volume manager knows.
pub(super) enum Routed {
    /// Not an archive path (no archive-named component, or one that isn't a real
    /// archive): read it with `std::fs`.
    Plain,
    /// The row for a path that crosses into a confirmed archive.
    Row(FileRow),
}

/// Route `path` through the volume manager. A pure string check gates everything: a path
/// with no archive-named component costs no I/O here.
pub(super) fn route_archive_path(
    path: &str,
    volume_id: &str,
    ask: &TextAsk,
    cancel: &AtomicBool,
    extract: ExtractFn,
) -> Routed {
    let p = Path::new(path);
    // The one inner-path parser (`boundary.rs`): the archive file, and the path inside it
    // (empty for the archive itself), which is also the index key.
    let Some((archive_path, inner)) = archive_boundary_candidate(p) else {
        return Routed::Plain;
    };
    let resolved = tauri::async_runtime::block_on(get_volume_manager().resolve(volume_id, p));
    if resolved.routed != Some(RoutedKind::Archive) {
        return Routed::Plain;
    }
    // `None` is an unmount race after the boundary confirmed; the plain pipeline's stat
    // then answers honestly (`missing`).
    let Some(volume) = resolved.volume else {
        return Routed::Plain;
    };
    let Some(archive) = volume.as_any().downcast_ref::<ArchiveVolume>() else {
        return Routed::Plain;
    };
    let inner = inner.to_string_lossy().into_owned();
    Routed::Row(inspect_archive_path(
        Located {
            path,
            archive_path: &archive_path,
            inner: &inner,
            volume_id,
        },
        archive,
        ask,
        cancel,
        extract,
    ))
}

/// Where a requested path sits: the archive file, and the inner path (the index key).
struct Located<'a> {
    path: &'a str,
    archive_path: &'a Path,
    /// `""` for the archive itself.
    inner: &'a str,
    volume_id: &'a str,
}

/// The row for a path inside (or naming) a confirmed archive.
fn inspect_archive_path(
    at: Located<'_>,
    archive: &ArchiveVolume,
    ask: &TextAsk,
    cancel: &AtomicBool,
    extract: ExtractFn,
) -> FileRow {
    let Located {
        path,
        archive_path,
        inner,
        volume_id,
    } = at;
    let owned = path.to_string();
    let p = Path::new(path);
    let index = match tauri::async_runtime::block_on(archive.index()) {
        Ok(index) => index,
        Err(e) => return status_for_volume(owned, e),
    };
    let Some(node) = index.get(inner) else {
        return FileRow::Missing { path: owned };
    };

    if node.is_dir {
        // The archive root's metadata is the `.zip` file's own (size on disk); an inner
        // directory has none to report.
        let (size, modified) = if inner.is_empty() {
            match std::fs::metadata(archive_path) {
                Ok(meta) => (Some(meta.len()), super::modified_secs(&meta)),
                Err(_) => (None, None),
            }
        } else {
            (None, unix_secs(node.modified))
        };
        let format = format_for_path(archive_path).map_or("archive", |f| f.label());
        let children = index.list(inner).unwrap_or_default();
        let content = listing(
            format,
            inner,
            &children,
            index.has_encrypted_entries(),
            MAX_ARCHIVE_ENTRIES,
        );
        return ok_row(owned, p, size, modified, Content::Archive(content));
    }

    if node.encrypted {
        return FileRow::Unreadable {
            path: owned,
            reason: UnreadableReason::Encrypted,
        };
    }
    let extracted = match extract(p, volume_id) {
        Ok(Some(extracted)) => extracted,
        // The boundary confirmed a moment ago and is gone now: an unmount race.
        Ok(None) => return FileRow::Missing { path: owned },
        Err(e) => return status_for(owned, ReadFailure::Viewer(e)),
    };
    let _cleanup = TempCleanup(extracted.cleanup_dir);
    let size = node
        .size
        .or_else(|| std::fs::metadata(&extracted.temp_file).ok().map(|m| m.len()))
        .unwrap_or(0);
    match super::read_content(&extracted.temp_file, size, ask, cancel) {
        Ok(content) => ok_row(owned, p, Some(size), unix_secs(node.modified), content),
        Err(failure) => status_for(owned, failure),
    }
}

/// Removes an extraction's temp subdir when dropped, so an early return or a panic in
/// the per-kind pipeline can't leak it. The viewer ties the same dir to its session
/// instead (`file_viewer/DETAILS.md` § Preview of a routed file).
pub(crate) struct TempCleanup(pub(crate) PathBuf);

impl Drop for TempCleanup {
    fn drop(&mut self) {
        if let Err(e) = std::fs::remove_dir_all(&self.0) {
            log::debug!(
                target: "agent::tools::inspect",
                "inspect_file: could not remove the extraction temp {}: {e}",
                self.0.display()
            );
        }
    }
}

/// The typed status for an archive that couldn't be read. The archive layer collapses a
/// failed parse to two kinds: `IoError` for a structurally damaged file (`corrupt`), and
/// `NotSupported` for what it can't or won't serve (an unsupported codec, a non-archive,
/// a tree past its DoS cap: `unsupported`). An unsupported codec is not a damaged file,
/// and the model relays the word it gets.
pub(super) fn status_for_volume(path: String, err: VolumeError) -> FileRow {
    let reason = match err {
        VolumeError::NotFound(_) => return FileRow::Missing { path },
        VolumeError::PermissionDenied(_) => UnreadableReason::Permission,
        VolumeError::NeedsPassword { .. } => UnreadableReason::Encrypted,
        VolumeError::IoError { .. } => UnreadableReason::Corrupt,
        VolumeError::NotSupported => UnreadableReason::Unsupported,
        _ => UnreadableReason::Io,
    };
    FileRow::Unreadable { path, reason }
}

/// A node's mtime as Unix seconds; a pre-1970 stamp is dropped rather than wrapped, as
/// the pane's `FileEntry` does.
fn unix_secs(modified: Option<i64>) -> Option<u64> {
    modified.and_then(|secs| u64::try_from(secs).ok())
}

/// Shape a directory's children into the model-facing listing, cut at `max`. Pure.
pub(crate) fn listing(
    format: &str,
    inner: &str,
    children: &[ArchiveNode],
    has_encrypted_entries: bool,
    max: usize,
) -> ArchiveContent {
    let entries: Vec<ArchiveEntry> = children.iter().take(max).map(entry_of).collect();
    ArchiveContent {
        format: format.to_string(),
        inner: inner.to_string(),
        total: children.len(),
        returned: entries.len(),
        truncated: entries.len() < children.len(),
        has_encrypted_entries,
        entries,
    }
}

fn entry_of(node: &ArchiveNode) -> ArchiveEntry {
    let size = if node.is_dir { None } else { node.size };
    let (modified, modified_human) = spoken_modified(unix_secs(node.modified));
    ArchiveEntry {
        name: node.name.clone(),
        is_dir: node.is_dir,
        size,
        size_human: size.map(format_size),
        modified,
        modified_human,
        encrypted: node.encrypted,
    }
}
