//! Tree walks for ref tips and commits.
//!
//! `list_tree` enumerates a commit's tree at a path; `get_tree_entry`
//! returns a single `FileEntry`. Both surface the executable bit through
//! `FileEntry.permissions` so cross-volume copy preserves it.

use std::path::Path;

use gix::object::tree::EntryKind;

use crate::file_system::listing::FileEntry;

use super::column_meta::recursive_tree_size;
use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::repo::RepoHandle;
use super::snapshot_dates;

/// Per-blob byte cap for cross-volume copies and previews. Above this we
/// surface a friendly "blob too big" rather than allocate the whole thing.
pub const MAX_BLOB_BYTES: u64 = 256 * 1024 * 1024;

/// Lists tree entries at `sub_path` inside the commit pointed to by `commit_id`.
///
/// `sub_path` uses forward slashes; an empty string means the commit's
/// root tree. `display_parent` is the absolute virtual path the entries
/// should appear under (used to build each entry's `path`).
pub fn list_tree(
    handle: &RepoHandle,
    commit_id: gix::ObjectId,
    sub_path: &str,
    display_parent: &Path,
) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let repo = handle.to_thread_local();
    let tree = resolve_tree_at(&repo, commit_id, sub_path)?;

    // Per-file Modified dates: each entry's date reflects the most recent
    // commit that touched it (or any file underneath, for subdirs). Falls
    // back to the snapshot date when an entry hasn't been touched within
    // the walk cap. See `snapshot_dates` for the algorithm.
    let snapshot_secs = commit_committer_secs(&repo, commit_id);
    let per_file_dates = snapshot_dates::decode_per_file_dates(handle, commit_id, sub_path).unwrap_or_default();

    let mut out = Vec::new();
    for entry in tree.iter() {
        let entry =
            entry.map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
        let name = entry.filename().to_string();
        let kind = entry.mode().kind();
        let display_path = display_parent.join(&name).to_string_lossy().into_owned();
        let mut fe = FileEntry::new(
            name.clone(),
            display_path,
            matches!(kind, EntryKind::Tree),
            matches!(kind, EntryKind::Link),
        );
        apply_kind(&mut fe, kind, &repo, entry.oid())?;
        fe.icon_id = pick_icon_id(&fe);
        if matches!(kind, EntryKind::Tree) {
            // Recursive byte total so dir sizes inside snapshots aren't
            // blank. Cheap because we already have the entry's tree id.
            let sub = if sub_path.is_empty() {
                name.clone()
            } else {
                format!("{}/{}", sub_path, name)
            };
            if let Some(bytes) = recursive_tree_size(&repo, commit_id, &sub) {
                fe.size = Some(bytes);
                fe.recursive_size = Some(bytes);
            }
        }
        // Per-file date if we found one in the walk; otherwise fall back to
        // the snapshot's commit date so the cell never reads as blank.
        let entry_secs = per_file_dates.get(&name).copied().or(snapshot_secs);
        if let Some(s) = entry_secs {
            fe.modified_at = Some(s);
            fe.created_at = Some(s);
            fe.added_at = Some(s);
        }
        out.push(fe);
    }

    // Stable order: dirs first, then case-insensitive name, matching what
    // local listings do.
    out.sort_by(|a, b| match (a.is_directory, b.is_directory) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    Ok(out)
}

/// Returns the `FileEntry` for a single tree entry at `sub_path` inside `commit_id`.
pub fn get_tree_entry(
    handle: &RepoHandle,
    commit_id: gix::ObjectId,
    sub_path: &str,
    display_path: &Path,
) -> Result<FileEntry, FriendlyGitError> {
    let repo = handle.to_thread_local();
    if sub_path.is_empty() {
        let mut fe = FileEntry::new(
            display_path
                .file_name()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_default(),
            display_path.to_string_lossy().into_owned(),
            true,
            false,
        );
        fe.permissions = 0o755;
        fe.icon_id = "dir".to_string();
        return Ok(fe);
    }

    let commit = repo
        .find_commit(commit_id)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let mut tree = commit
        .tree()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

    // Use peel_to_entry_by_path so any intermediate trees walk via the same gix path.
    let entry = tree
        .peel_to_entry_by_path(Path::new(sub_path))
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?
        .ok_or_else(|| FriendlyGitError::new(FriendlyGitErrorKind::CorruptRepo, display_path.display().to_string()))?;

    let kind = entry.mode().kind();
    let name = display_path
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| sub_path.split('/').next_back().unwrap_or("").to_string());
    let mut fe = FileEntry::new(
        name,
        display_path.to_string_lossy().into_owned(),
        matches!(kind, EntryKind::Tree),
        matches!(kind, EntryKind::Link),
    );
    apply_kind(&mut fe, kind, &repo, entry.oid())?;
    fe.icon_id = pick_icon_id(&fe);
    // Per-file date for the Modified cell. We compute the parent dir's
    // date map (cheap because the cache is shared with `list_tree`) and
    // pull the entry's name out. Falls back to the snapshot date.
    let (parent_path, leaf_name) = split_parent_and_leaf(sub_path);
    let per_file_dates = snapshot_dates::decode_per_file_dates(handle, commit_id, parent_path).unwrap_or_default();
    let entry_secs = per_file_dates
        .get(leaf_name)
        .copied()
        .or_else(|| commit_committer_secs(&repo, commit_id));
    if let Some(s) = entry_secs {
        fe.modified_at = Some(s);
        fe.created_at = Some(s);
        fe.added_at = Some(s);
    }
    if matches!(kind, EntryKind::Tree)
        && let Some(bytes) = recursive_tree_size(&repo, commit_id, sub_path)
    {
        fe.size = Some(bytes);
        fe.recursive_size = Some(bytes);
    }
    Ok(fe)
}

/// Splits `sub_path` into `(parent_dir, leaf_name)` using forward slashes.
///
/// `"src/lib/mod.rs"` → `("src/lib", "mod.rs")`. `"foo"` → `("", "foo")`.
fn split_parent_and_leaf(sub_path: &str) -> (&str, &str) {
    match sub_path.rfind('/') {
        Some(idx) => (&sub_path[..idx], &sub_path[idx + 1..]),
        None => ("", sub_path),
    }
}

fn commit_committer_secs(repo: &gix::Repository, commit_id: gix::ObjectId) -> Option<u64> {
    let commit = repo.find_commit(commit_id).ok()?;
    let committer = commit.committer().ok()?;
    let time = committer.time().ok()?;
    u64::try_from(time.seconds).ok()
}

/// Resolves the commit's tree at `sub_path`, descending into nested trees.
pub(crate) fn resolve_tree_at<'r>(
    repo: &'r gix::Repository,
    commit_id: gix::ObjectId,
    sub_path: &str,
) -> Result<gix::Tree<'r>, FriendlyGitError> {
    let commit = repo
        .find_commit(commit_id)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let mut tree = commit
        .tree()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    if sub_path.is_empty() {
        return Ok(tree);
    }
    let entry = tree
        .peel_to_entry_by_path(Path::new(sub_path))
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?
        .ok_or_else(|| FriendlyGitError::new(FriendlyGitErrorKind::CorruptRepo, sub_path.to_string()))?;
    let kind = entry.mode().kind();
    if !matches!(kind, EntryKind::Tree) {
        return Err(FriendlyGitError::new(
            FriendlyGitErrorKind::CorruptRepo,
            sub_path.to_string(),
        ));
    }
    let obj = entry
        .object()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    Ok(obj.into_tree())
}

/// Returns the blob's bytes (or a friendly error for too-large blobs).
pub fn read_blob(handle: &RepoHandle, blob_id: gix::ObjectId) -> Result<Vec<u8>, FriendlyGitError> {
    let repo = handle.to_thread_local();
    let header = repo
        .find_header(blob_id)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    if header.size() > MAX_BLOB_BYTES {
        return Err(FriendlyGitError::new(
            FriendlyGitErrorKind::BlobTooLarge,
            blob_id.to_string(),
        ));
    }
    let blob = repo
        .find_blob(blob_id)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    Ok(blob.data.clone())
}

/// Resolves a blob `ObjectId` for a path inside a commit's tree.
pub fn lookup_blob_id(
    handle: &RepoHandle,
    commit_id: gix::ObjectId,
    sub_path: &str,
) -> Result<gix::ObjectId, FriendlyGitError> {
    let repo = handle.to_thread_local();
    let commit = repo
        .find_commit(commit_id)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let mut tree = commit
        .tree()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let entry = tree
        .peel_to_entry_by_path(Path::new(sub_path))
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?
        .ok_or_else(|| FriendlyGitError::new(FriendlyGitErrorKind::CorruptRepo, sub_path.to_string()))?;
    if !matches!(
        entry.mode().kind(),
        EntryKind::Blob | EntryKind::BlobExecutable | EntryKind::Link
    ) {
        return Err(FriendlyGitError::new(
            FriendlyGitErrorKind::CorruptRepo,
            sub_path.to_string(),
        ));
    }
    Ok(entry.object_id())
}

fn apply_kind(
    fe: &mut FileEntry,
    kind: EntryKind,
    repo: &gix::Repository,
    oid: &gix::hash::oid,
) -> Result<(), FriendlyGitError> {
    match kind {
        EntryKind::Tree => {
            fe.permissions = 0o755;
        }
        EntryKind::Blob => {
            fe.permissions = 0o644;
            fe.size = blob_size(repo, oid);
        }
        EntryKind::BlobExecutable => {
            fe.permissions = 0o755;
            fe.size = blob_size(repo, oid);
        }
        EntryKind::Link => {
            fe.permissions = 0o777;
            fe.size = blob_size(repo, oid);
        }
        EntryKind::Commit => {
            // Submodule pointer. M3 will redirect; for M2 surface as a dir.
            fe.is_directory = true;
            fe.permissions = 0o755;
        }
    }
    Ok(())
}

fn blob_size(repo: &gix::Repository, oid: &gix::hash::oid) -> Option<u64> {
    repo.find_header(oid.to_owned()).ok().map(|h| h.size())
}

fn pick_icon_id(fe: &FileEntry) -> String {
    if fe.is_symlink {
        return if fe.is_directory {
            "symlink-dir".into()
        } else {
            "symlink-file".into()
        };
    }
    if fe.is_directory {
        return "dir".into();
    }
    if let Some(ext) = Path::new(&fe.name).extension() {
        return format!("ext:{}", ext.to_string_lossy().to_lowercase());
    }
    "file".into()
}
