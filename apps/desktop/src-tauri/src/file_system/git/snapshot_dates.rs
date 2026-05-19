//! Per-file Modified dates inside snapshot listings.
//!
//! Inside `.git/branches/main/src/`, `.git/commits/<sha>/`, etc., each entry's
//! Modified column should reflect the most recent commit that touched THAT
//! file (or any file under that subdir), not the snapshot's commit date. The
//! latter is correct as a "frozen point in time", but it's useless as a
//! "when did I last work on this?" hint.
//!
//! ## Algorithm: walk-once batching
//!
//! For a tree-listing call at `(commit_id, dir_path)` returning N entries:
//!
//! 1. Collect the entry set: top-level relative names visible under `dir_path`.
//! 2. Walk commits backwards from `commit_id` by commit time, newest first. For each commit, diff
//!    against its first parent. For every changed path starting with `dir_path/`, find which
//!    top-level entry it falls under and, if we don't have a date yet, record this commit's
//!    committer time.
//! 3. Stop early when every entry is dated, or after `MAX_COMMITS_PER_WALK`, or when the rev-walk
//!    runs out of commits.
//! 4. Entries that didn't get a date within the cap fall back to the snapshot's commit date
//!    (`tree::list_tree` handles that).
//!
//! For initial commits (no parent), every entry gets the initial commit's
//! date, short-circuited up front.
//!
//! ## Cache
//!
//! Tree listings at a specific commit are immutable, so `(commit_id,
//! dir_path)` is a content-addressable key that never goes stale. Cmdr uses
//! a tiny FIFO-bounded cache (`MAX_CACHE_ENTRIES` keys), far smaller than
//! the heap impact of a full listing, and big enough to cover repeated
//! navigation between sibling snapshot dirs without re-walking.
//!
//! The cache is process-global. There's no invalidation hook because
//! `(commit_id, dir_path)` is content-addressable: a commit's tree never
//! changes, so a hit is always correct. `clear_cache()` exists for tests
//! and for any future "free memory at idle" path.
//!
//! ## Why gix, not shell-out
//!
//! gix exposes everything we need: `rev_walk()` for the commit walk,
//! `Tree::changes()::for_each_to_obtain_tree()` for tree-diff vs first
//! parent, and `Change::location()` for the changed path. Each `Change`
//! variant (Addition, Deletion, Modification, Rewrite) carries its
//! `location` as a `BStr`, and that's all we need. The shell-out fallback
//! discussed in the spec isn't needed.

use std::collections::HashMap;
use std::path::Path;
use std::sync::Mutex;

use gix::ObjectId;
use gix::bstr::ByteSlice;
use gix::revision::walk::Sorting;
use gix::traverse::commit::simple::CommitTimeOrder;

use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::repo::RepoHandle;

/// How many commits we'll walk back per listing before falling back to the
/// snapshot date. Bigger than the typical churn window for a 50k-commit
/// monorepo, small enough to stay inside the perf budget.
pub const MAX_COMMITS_PER_WALK: usize = 1000;

/// FIFO cap on the result cache. Each entry is a `HashMap<String, u64>`
/// bounded by directory size, so this caps total memory at around
/// (avg-dir-size × 50). For a 100-entry directory, ~5000 string + u64
/// pairs sit in the cache (a few hundred KB at most).
const MAX_CACHE_ENTRIES: usize = 50;

/// Result map: top-level entry name (relative to `dir_path`) → committer
/// time in seconds-since-epoch.
pub type DateMap = HashMap<String, u64>;

/// Cache key. `dir_path` uses forward slashes and never starts/ends with `/`
/// (same shape `tree::list_tree` accepts). Empty string means the root tree.
type CacheKey = (ObjectId, String);

/// FIFO bounded cache. We picked FIFO over true LRU to keep the
/// implementation tiny: for snapshot navigation the access pattern is
/// "open a folder, look around, move on", so eviction order rarely matters.
struct DateCache {
    entries: Vec<(CacheKey, DateMap)>,
}

impl DateCache {
    const fn new() -> Self {
        Self { entries: Vec::new() }
    }

    fn get(&mut self, key: &CacheKey) -> Option<DateMap> {
        // Linear scan; with cap=50 this is trivial.
        for (k, v) in &self.entries {
            if k == key {
                return Some(v.clone());
            }
        }
        None
    }

    fn insert(&mut self, key: CacheKey, value: DateMap) {
        if self.entries.len() >= MAX_CACHE_ENTRIES {
            self.entries.remove(0);
        }
        self.entries.push((key, value));
    }

    fn clear(&mut self) {
        self.entries.clear();
    }
}

fn cache() -> &'static Mutex<DateCache> {
    static CACHE: Mutex<DateCache> = Mutex::new(DateCache::new());
    &CACHE
}

/// Drops every cached `(commit_id, dir_path)` result. Test-only today;
/// the runtime cache is content-addressable and never needs invalidation.
#[allow(
    dead_code,
    reason = "Test-only escape hatch; the cache never needs runtime invalidation"
)]
pub fn clear_cache() {
    if let Ok(mut c) = cache().lock() {
        c.clear();
    }
}

/// Decodes the per-file Modified date for every visible top-level entry
/// under `dir_path` inside the tree at `commit_id`.
///
/// `dir_path` uses forward slashes, no leading/trailing slash, empty for
/// the root tree. Returns a map keyed by top-level entry name. Entries
/// that didn't surface within `MAX_COMMITS_PER_WALK` are simply absent;
/// the caller falls back to the snapshot date.
pub fn decode_per_file_dates(
    handle: &RepoHandle,
    commit_id: ObjectId,
    dir_path: &str,
) -> Result<DateMap, FriendlyGitError> {
    let normalized = dir_path.trim_matches('/').to_string();
    let key: CacheKey = (commit_id, normalized.clone());

    if let Ok(mut c) = cache().lock()
        && let Some(hit) = c.get(&key)
    {
        return Ok(hit);
    }

    let computed = compute_dates(handle, commit_id, &normalized)?;
    if let Ok(mut c) = cache().lock() {
        c.insert(key, computed.clone());
    }
    Ok(computed)
}

fn compute_dates(handle: &RepoHandle, commit_id: ObjectId, dir_path: &str) -> Result<DateMap, FriendlyGitError> {
    let repo = handle.to_thread_local();

    // The entry set is the top-level names under `dir_path` for this
    // commit's tree. We track which ones still need a date.
    let pending = collect_top_level_names(&repo, commit_id, dir_path)?;
    let mut dates: DateMap = HashMap::with_capacity(pending.len());
    if pending.is_empty() {
        return Ok(dates);
    }

    // For an initial commit, every entry's "most recent change" IS the
    // initial commit. Short-circuit so we don't walk a one-commit history.
    let initial_commit = commit_has_no_parent(&repo, commit_id);
    let snapshot_secs = committer_secs(&repo, commit_id);
    if initial_commit {
        if let Some(secs) = snapshot_secs {
            for name in &pending {
                dates.insert(name.clone(), secs);
            }
        }
        return Ok(dates);
    }

    // Walk backwards. For each commit, diff against its first parent and
    // attribute the committer time to any pending entry that the diff
    // touches.
    let walk = repo
        .rev_walk([commit_id])
        .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
        .first_parent_only()
        .all()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

    let prefix = if dir_path.is_empty() {
        String::new()
    } else {
        format!("{}/", dir_path)
    };
    for (walked, info) in walk.enumerate() {
        if dates.len() == pending.len() {
            break;
        }
        if walked >= MAX_COMMITS_PER_WALK {
            break;
        }

        let info = match info {
            Ok(i) => i,
            // Skip unreadable commits rather than abort: an early-history
            // shallow boundary shouldn't poison every entry's date.
            Err(_) => continue,
        };
        let id = info.id;

        let secs = match committer_secs(&repo, id) {
            Some(s) => s,
            None => continue,
        };

        let parent_id = match first_parent(&repo, id) {
            Some(p) => p,
            // Walk reached the root (initial commit). Treat every still-
            // pending entry that exists in this tree as touched by it.
            None => {
                for name in &pending {
                    dates.entry(name.clone()).or_insert(secs);
                }
                break;
            }
        };

        diff_into_dates(&repo, parent_id, id, &prefix, &pending, &mut dates, secs);
    }

    Ok(dates)
}

/// Diffs `parent_id` against `id` and attributes `secs` to any pending
/// top-level entry that the diff touches inside `prefix` (which already
/// ends with `/`, or is empty for the root). Only fills in entries we
/// haven't dated yet.
fn diff_into_dates(
    repo: &gix::Repository,
    parent_id: ObjectId,
    id: ObjectId,
    prefix: &str,
    pending: &[String],
    dates: &mut DateMap,
    secs: u64,
) {
    let Some(parent_tree) = repo.find_commit(parent_id).ok().and_then(|c| c.tree().ok()) else {
        return;
    };
    let Some(commit_tree) = repo.find_commit(id).ok().and_then(|c| c.tree().ok()) else {
        return;
    };
    let Ok(mut platform) = parent_tree.changes() else {
        return;
    };

    let _ = platform.for_each_to_obtain_tree(&commit_tree, |change| {
        let location = change_location(&change);
        // gix doesn't include a leading slash on `location`. We rebuilt the
        // dir prefix to include the trailing slash up front.
        let path_str = match location.to_str() {
            Ok(s) => s,
            Err(_) => return Ok::<_, std::convert::Infallible>(std::ops::ControlFlow::Continue(())),
        };

        if !prefix.is_empty() && !path_str.starts_with(prefix) {
            return Ok(std::ops::ControlFlow::Continue(()));
        }
        let rel = &path_str[prefix.len()..];
        // Top-level segment is everything up to the first `/`.
        let top = match rel.find('/') {
            Some(idx) => &rel[..idx],
            None => rel,
        };
        if top.is_empty() {
            return Ok(std::ops::ControlFlow::Continue(()));
        }
        // Cheap membership test (`pending` is small, single-directory
        // listings are <=200 entries in practice).
        if pending.iter().any(|p| p == top) {
            dates.entry(top.to_string()).or_insert(secs);
        }
        if dates.len() == pending.len() {
            return Ok(std::ops::ControlFlow::Break(()));
        }
        Ok(std::ops::ControlFlow::Continue(()))
    });
}

/// Pulls the `location` BStr out of any `Change` variant. Renames carry
/// both `source_location` and `location`; we attribute the change to the
/// destination since that's the path the user is browsing.
fn change_location<'a>(change: &gix::object::tree::diff::Change<'a, '_, '_>) -> &'a gix::bstr::BStr {
    use gix::object::tree::diff::Change::*;
    match *change {
        Addition { location, .. }
        | Deletion { location, .. }
        | Modification { location, .. }
        | Rewrite { location, .. } => location,
    }
}

fn collect_top_level_names(
    repo: &gix::Repository,
    commit_id: ObjectId,
    dir_path: &str,
) -> Result<Vec<String>, FriendlyGitError> {
    let tree = super::tree::resolve_tree_at(repo, commit_id, dir_path)?;
    let mut out = Vec::new();
    for entry in tree.iter() {
        let entry =
            entry.map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
        out.push(entry.filename().to_string());
    }
    Ok(out)
}

fn commit_has_no_parent(repo: &gix::Repository, id: ObjectId) -> bool {
    let Ok(commit) = repo.find_commit(id) else {
        return false;
    };
    commit.parent_ids().next().is_none()
}

fn first_parent(repo: &gix::Repository, id: ObjectId) -> Option<ObjectId> {
    let commit = repo.find_commit(id).ok()?;
    commit.parent_ids().next().map(|p| p.detach())
}

fn committer_secs(repo: &gix::Repository, id: ObjectId) -> Option<u64> {
    let commit = repo.find_commit(id).ok()?;
    let committer = commit.committer().ok()?;
    let time = committer.time().ok()?;
    u64::try_from(time.seconds).ok()
}

/// Backstop helper: `Path` form of the keying convention. Not used by the
/// hot path (`dir_path` is already a `&str`) but exposed for callers that
/// hold a `Path` and want to query the cache without rebuilding the slash
/// shape themselves.
#[allow(
    dead_code,
    reason = "Public helper; the hot path uses &str, but this exists for future callers passing a Path"
)]
pub fn dir_path_from_subpath(sub: &Path) -> String {
    sub.to_string_lossy().replace('\\', "/").trim_matches('/').to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cache_evicts_oldest_when_full() {
        let mut c = DateCache::new();
        for i in 0..MAX_CACHE_ENTRIES + 5 {
            let oid = mock_oid(i as u8);
            let mut m = DateMap::new();
            m.insert(format!("f{}", i), i as u64);
            c.insert((oid, format!("dir/{}", i)), m);
        }
        assert_eq!(c.entries.len(), MAX_CACHE_ENTRIES);
        // The first 5 keys should have been evicted.
        for i in 0..5 {
            let oid = mock_oid(i as u8);
            assert!(
                c.get(&(oid, format!("dir/{}", i))).is_none(),
                "entry {} should be gone",
                i
            );
        }
        // Key 5 should still be present.
        let oid = mock_oid(5);
        assert!(c.get(&(oid, "dir/5".to_string())).is_some());
    }

    #[test]
    fn dir_path_normalizer_strips_slashes_and_backslashes() {
        assert_eq!(dir_path_from_subpath(Path::new("/src/lib/")), "src/lib");
        assert_eq!(dir_path_from_subpath(Path::new("")), "");
        assert_eq!(dir_path_from_subpath(Path::new("a/b")), "a/b");
    }

    fn mock_oid(byte: u8) -> ObjectId {
        // Build a SHA-1 OID with a single varying byte so test keys are
        // distinguishable. gix accepts the raw 20-byte form via `from_bytes`.
        let mut buf = [0u8; 20];
        buf[0] = byte;
        ObjectId::try_from(buf.as_slice()).expect("valid oid")
    }
}
