//! Helpers that populate Modified + Size columns for virtual git entries.
//!
//! ## Modified column
//!
//! Every virtual entry gets a real timestamp into `modified_at` so the
//! Modified column never reads as blank. Inside a snapshot (commit / tag /
//! stash tree walk) every file and subdir borrows the snapshot's commit
//! date. That's a frozen point in time, so the same date everywhere is
//! semantically correct.
//!
//! ## Size column (loose semantics)
//!
//! `display_size` overrides the byte-formatted Size cell with a short
//! string per row (`+12 / -3`, `5 files`, `12 items`, `on main`, short
//! SHA). The numeric `size` keeps a within-category sort key so the user
//! can still sort by Size and get a useful order. Cross-category Size
//! sorting is meaningless on purpose. Each cell is self-explaining via
//! tooltip + aria-label.

use std::path::Path;

use gix::ObjectId;
use gix::object::tree::EntryKind;

use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::repo::{RepoHandle, count_ahead_behind};

/// Picks a fallback comparison branch for ahead/behind when the branch has
/// no configured upstream. Tries `main`, then `master`. Returns the
/// resolved tip commit id of the fallback so callers can pass it straight
/// to `count_ahead_behind`. Returns `None` when neither branch exists.
pub fn fallback_default_branch_tip(repo: &gix::Repository) -> Option<(String, ObjectId)> {
    use gix::refs::PartialName;
    for name in ["main", "master"] {
        let full = format!("refs/heads/{}", name);
        let Ok(partial) = PartialName::try_from(full.as_str()) else {
            continue;
        };
        let Ok(mut reference) = repo.find_reference(&partial) else {
            continue;
        };
        let Ok(id) = reference.peel_to_id() else {
            continue;
        };
        return Some((name.to_string(), id.detach()));
    }
    None
}

/// Resolves the configured upstream tracking ref (`origin/main` etc.) for
/// a local branch name.
///
/// Returns `(short_name, commit_id)`. `short_name` is the
/// `<remote>/<branch>` form for tooltips.
pub fn upstream_tip(repo: &gix::Repository, local_branch: &str) -> Option<(String, ObjectId)> {
    use gix::refs::PartialName;
    let local_full = format!("refs/heads/{}", local_branch);
    let local_partial = PartialName::try_from(local_full.as_str()).ok()?;
    let local_ref = repo.find_reference(&local_partial).ok()?;
    let tracking = repo
        .branch_remote_tracking_ref_name(local_ref.name(), gix::remote::Direction::Fetch)?
        .ok()?;
    let upstream_ref = repo.find_reference(tracking.as_ref()).ok()?;
    let id = upstream_ref.into_fully_peeled_id().ok()?.detach();
    let raw = tracking.as_ref().as_bstr().to_string();
    let short = raw.strip_prefix("refs/remotes/").unwrap_or(&raw).to_string();
    Some((short, id))
}

/// Result of computing ahead/behind for a single branch.
pub struct AheadBehind {
    pub ahead: u32,
    pub behind: u32,
    /// Display label of the comparison branch: `origin/main`, `main`, etc.
    pub vs: String,
}

/// Computes ahead/behind for `local_branch` against its upstream, falling
/// back to `main` / `master` when no upstream is configured.
///
/// Returns `None` when no comparison branch can be found (the branch
/// itself is the default and there's no upstream. In that case the
/// caller leaves the cell blank).
pub fn ahead_behind_for_branch(repo: &gix::Repository, local_branch: &str, local_tip: ObjectId) -> Option<AheadBehind> {
    if let Some((vs, upstream_id)) = upstream_tip(repo, local_branch) {
        let (ahead, behind) = count_ahead_behind(repo, local_tip, upstream_id)?;
        return Some(AheadBehind { ahead, behind, vs });
    }
    let (vs, fallback_id) = fallback_default_branch_tip(repo)?;
    if vs == local_branch {
        // Comparing main against itself produces (0, 0); the cell would
        // mislead the user into thinking the branch is up to date with
        // some upstream that doesn't exist.
        return None;
    }
    let (ahead, behind) = count_ahead_behind(repo, local_tip, fallback_id)?;
    Some(AheadBehind { ahead, behind, vs })
}

/// Decoded commit metadata used for column population: the committer
/// timestamp (in seconds).
pub struct CommitMeta {
    pub committer_secs: i64,
}

/// Reads `id`'s committer time. Cheap (one cached object lookup).
pub fn commit_meta(repo: &gix::Repository, id: ObjectId) -> Result<CommitMeta, FriendlyGitError> {
    let commit = repo
        .find_commit(id)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let committer = commit
        .committer()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let time = committer
        .time()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    Ok(CommitMeta {
        committer_secs: time.seconds,
    })
}

/// Counts files changed between `commit_id` and its first parent. For an
/// initial commit (no parent) returns the total number of entries reachable
/// from the commit's tree.
///
/// Uses gix's tree-diff. We also collect the recursive byte total of the
/// commit tree as a side effect because the tree walk is identical; the
/// caller can drop it if not needed.
pub fn files_changed_count(repo: &gix::Repository, commit_id: ObjectId) -> Option<u64> {
    let commit = repo.find_commit(commit_id).ok()?;
    let tree = commit.tree().ok()?;
    let parents: Vec<ObjectId> = commit.parent_ids().map(|p| p.detach()).collect();

    if parents.is_empty() {
        // Initial commit: count every blob in the tree.
        return Some(count_blobs_recursive(repo, &tree));
    }
    // Diff against first parent (the conventional "main" parent for merge
    // commits, same as `git show`).
    let parent = repo.find_commit(parents[0]).ok()?;
    let parent_tree = parent.tree().ok()?;

    let mut count: u64 = 0;
    let mut platform = parent_tree.changes().ok()?;
    let _ = platform.for_each_to_obtain_tree(&tree, |_change| {
        count = count.saturating_add(1);
        Ok::<_, std::convert::Infallible>(std::ops::ControlFlow::Continue(()))
    });
    Some(count)
}

fn count_blobs_recursive(_repo: &gix::Repository, tree: &gix::Tree<'_>) -> u64 {
    let mut count: u64 = 0;
    let Ok(iter) = tree.iter().collect::<Result<Vec<_>, _>>() else {
        return 0;
    };
    for entry in iter {
        match entry.mode().kind() {
            EntryKind::Tree => {
                if let Ok(obj) = entry.object()
                    && obj.kind == gix::object::Kind::Tree
                {
                    let sub = obj.into_tree();
                    count = count.saturating_add(count_blobs_recursive(_repo, &sub));
                }
            }
            EntryKind::Blob | EntryKind::BlobExecutable | EntryKind::Link => {
                count = count.saturating_add(1);
            }
            EntryKind::Commit => {
                // Submodule pointer: count as one entry.
                count = count.saturating_add(1);
            }
        }
    }
    count
}

/// Recursive byte total for a tree at `sub_path` inside `commit_id`.
/// Used to populate `size` on directory entries inside snapshots.
pub fn recursive_tree_size(repo: &gix::Repository, commit_id: ObjectId, sub_path: &str) -> Option<u64> {
    let commit = repo.find_commit(commit_id).ok()?;
    let mut tree = commit.tree().ok()?;
    let tree = if sub_path.is_empty() {
        tree
    } else {
        let entry = tree.peel_to_entry_by_path(Path::new(sub_path)).ok()??;
        if !matches!(entry.mode().kind(), EntryKind::Tree) {
            return None;
        }
        let obj = entry.object().ok()?;
        obj.into_tree()
    };
    Some(sum_tree_bytes(repo, &tree))
}

fn sum_tree_bytes(repo: &gix::Repository, tree: &gix::Tree<'_>) -> u64 {
    let mut total: u64 = 0;
    let Ok(entries) = tree.iter().collect::<Result<Vec<_>, _>>() else {
        return 0;
    };
    for entry in entries {
        match entry.mode().kind() {
            EntryKind::Tree => {
                if let Ok(obj) = entry.object()
                    && obj.kind == gix::object::Kind::Tree
                {
                    let sub = obj.into_tree();
                    total = total.saturating_add(sum_tree_bytes(repo, &sub));
                }
            }
            EntryKind::Blob | EntryKind::BlobExecutable | EntryKind::Link => {
                if let Ok(header) = repo.find_header(entry.oid().to_owned()) {
                    total = total.saturating_add(header.size());
                }
            }
            EntryKind::Commit => {} // submodule pointer, no bytes
        }
    }
    total
}

/// Newest commit timestamp across the listed branches' tips. Returns
/// `None` for an empty branch list.
pub fn newest_branch_tip_secs(handle: &RepoHandle) -> Option<u64> {
    let repo = handle.to_thread_local();
    let mut newest: Option<i64> = None;
    let platform = repo.references().ok()?;
    let iter = platform.local_branches().ok()?;
    for r in iter.flatten() {
        let mut r = r;
        let Ok(id) = r.peel_to_id() else { continue };
        let Ok(commit) = repo.find_commit(id.detach()) else {
            continue;
        };
        let Ok(committer) = commit.committer() else { continue };
        let Ok(time) = committer.time() else { continue };
        newest = Some(newest.map_or(time.seconds, |n| n.max(time.seconds)));
    }
    newest.and_then(|s| u64::try_from(s).ok())
}

/// Newest tag date (annotated tag date for annotated tags, commit date for
/// lightweight). Returns `None` when there are no tags.
pub fn newest_tag_secs(handle: &RepoHandle) -> Option<u64> {
    let repo = handle.to_thread_local();
    let mut newest: Option<i64> = None;
    let platform = repo.references().ok()?;
    let iter = platform.tags().ok()?;
    for r in iter.flatten() {
        let mut r = r;
        let Ok(id) = r.peel_to_id() else { continue };
        if let Some(secs) = tag_or_commit_secs(&repo, id.detach()) {
            newest = Some(newest.map_or(secs, |n| n.max(secs)));
        }
    }
    newest.and_then(|s| u64::try_from(s).ok())
}

/// Returns the annotated-tag time (when `id` points to a tag object) or
/// the underlying commit's committer time. Used for both per-tag and
/// per-category Modified columns.
pub fn tag_or_commit_secs(repo: &gix::Repository, id: ObjectId) -> Option<i64> {
    let obj = repo.find_object(id).ok()?;
    if obj.kind == gix::object::Kind::Tag {
        let tag = obj.into_tag();
        if let Ok(Some(tagger)) = tag.tagger()
            && let Ok(time) = tagger.time()
        {
            return Some(time.seconds);
        }
        // Fall through to the wrapped commit on missing tagger.
        let target_id = tag.target_id().ok()?.detach();
        let commit = repo.find_commit(target_id).ok()?;
        let committer = commit.committer().ok()?;
        let time = committer.time().ok()?;
        return Some(time.seconds);
    }
    if obj.kind == gix::object::Kind::Commit {
        let commit = obj.into_commit();
        let committer = commit.committer().ok()?;
        let time = committer.time().ok()?;
        return Some(time.seconds);
    }
    None
}

/// HEAD commit's committer time, in seconds. Used for `commits/`
/// category Modified.
pub fn head_commit_secs(handle: &RepoHandle) -> Option<u64> {
    let repo = handle.to_thread_local();
    let id = repo.head_id().ok()?.detach();
    let commit = repo.find_commit(id).ok()?;
    let committer = commit.committer().ok()?;
    let time = committer.time().ok()?;
    u64::try_from(time.seconds).ok()
}
