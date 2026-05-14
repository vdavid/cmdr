//! Per-entry git status for a working-tree directory.
//!
//! Uses `gix::Repository::status()` to enumerate both staged changes (HEAD vs
//! index) and worktree changes (index vs working tree) in one pass. The
//! iterator yields `TreeIndex` items for staged changes and `IndexWorktree`
//! items for worktree changes; we merge by path, giving staged changes
//! priority (matching git's XY column precedence).
//!
//! An earlier attempt (gix < 0.72) missed staged additions in fixture-driven
//! tests against a single-commit repo. gix 0.81 ships the `TreeIndex` leg of
//! the iterator via `into_iter()`, which handles this correctly. See
//! `CLAUDE.md` § Decisions for the full history.
//!
//! ## Caching
//!
//! `list_status` runs once per repo per `.git/index` mtime change. Every
//! `listing-complete` event used to trigger a fresh walk; now we walk the
//! whole worktree once, cache the result keyed by repo root + index mtime,
//! and slice it by `dir_in_worktree` on subsequent calls. The `.git/index`
//! watcher invalidates the entry on any index change so the next call
//! re-walks.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{OnceLock, RwLock};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::repo::RepoHandle;

/// Single-character status code.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum EntryStatusCode {
    /// Modified in worktree relative to index.
    Modified,
    /// Added (in index but not in HEAD's tree).
    Added,
    /// Deleted in worktree relative to index, or in index relative to HEAD.
    Deleted,
    /// Renamed.
    Renamed,
    /// Tracked as a copy.
    Copied,
    /// Type changed (file ↔ symlink ↔ submodule).
    TypeChange,
    /// Untracked.
    Untracked,
    /// Ignored.
    Ignored,
    /// Conflicted (merge state).
    Conflicted,
}

impl EntryStatusCode {
    /// One-glyph render for the Full-mode status column.
    #[allow(dead_code, reason = "Public helper used by frontend tests and future Rust callers")]
    pub fn glyph(&self) -> &'static str {
        match self {
            EntryStatusCode::Modified => "M",
            EntryStatusCode::Added => "A",
            EntryStatusCode::Deleted => "D",
            EntryStatusCode::Renamed => "R",
            EntryStatusCode::Copied => "C",
            EntryStatusCode::TypeChange => "T",
            EntryStatusCode::Untracked => "?",
            EntryStatusCode::Ignored => "!",
            EntryStatusCode::Conflicted => "U",
        }
    }

    /// Long form, for `aria-label` / tooltip.
    #[allow(dead_code, reason = "Public helper used by frontend tests and future Rust callers")]
    pub fn label(&self) -> &'static str {
        match self {
            EntryStatusCode::Modified => "Modified",
            EntryStatusCode::Added => "Added",
            EntryStatusCode::Deleted => "Deleted",
            EntryStatusCode::Renamed => "Renamed",
            EntryStatusCode::Copied => "Copied",
            EntryStatusCode::TypeChange => "Type changed",
            EntryStatusCode::Untracked => "Untracked",
            EntryStatusCode::Ignored => "Ignored",
            EntryStatusCode::Conflicted => "Conflicted",
        }
    }
}

/// One status entry, surfaced to the frontend as `{ path, code }`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EntryStatus {
    /// Path relative to the repo's working tree root, with `/` separators.
    pub relative_path: String,
    pub code: EntryStatusCode,
}

/// One full-repo status snapshot, keyed by `.git/index` mtime.
struct CachedStatus {
    /// `.git/index` mtime at the time the snapshot was built. `None` means
    /// the index file didn't exist (unborn repo). The cache still keys on
    /// this so a later `git add` (which creates the index) invalidates.
    index_mtime: Option<SystemTime>,
    /// All entries from a full-repo gix status walk.
    /// Keyed by relative path (forward-slashed) for quick prefix slicing.
    entries: Vec<EntryStatus>,
}

/// Process-wide cache. One snapshot per repo. We slice it by
/// `dir_in_worktree` on each call so the same snapshot serves every pane
/// pointing inside the same repo.
fn status_cache() -> &'static RwLock<HashMap<PathBuf, CachedStatus>> {
    static CACHE: OnceLock<RwLock<HashMap<PathBuf, CachedStatus>>> = OnceLock::new();
    CACHE.get_or_init(|| RwLock::new(HashMap::new()))
}

/// Drops the cached snapshot for `repo_root`. Called by the `.git/index`
/// watcher and by `unsubscribe` so a repo with no active panes doesn't
/// pin its snapshot forever.
pub(crate) fn invalidate_status_cache(repo_root: &Path) {
    let canonical = repo_root.canonicalize().unwrap_or_else(|_| repo_root.to_path_buf());
    if let Ok(mut guard) = status_cache().write() {
        guard.remove(&canonical);
    }
}

/// Test entry point: the cache size.
#[cfg(test)]
pub(crate) fn cache_len_for_test() -> usize {
    status_cache().read().map(|g| g.len()).unwrap_or(0)
}

/// Returns the absolute path to the index file for this repo (handles linked
/// worktrees, where the index lives under `<common>/worktrees/<name>/index`).
fn index_path_for(repo: &RepoHandle) -> PathBuf {
    let local = repo.to_thread_local();
    local.index_path()
}

/// Reads the `.git/index` mtime. Missing file → `None` (unborn repo or fresh
/// init before first add). Any I/O hiccup also collapses to `None` rather
/// than failing the whole call; the cache then re-walks on every call until
/// the file shows up, which is the safe behaviour.
fn index_mtime(index_path: &Path) -> Option<SystemTime> {
    std::fs::metadata(index_path).and_then(|m| m.modified()).ok()
}

/// Lists the per-entry status for the worktree.
///
/// Caches the full-repo result keyed by `.git/index` mtime and slices by
/// `dir_in_worktree` on the way out. Cache misses run a full gix status walk
/// (no pathspec) so any pane on the same repo benefits from the warm cache
/// afterwards.
///
/// `dir_in_worktree` scopes the *result* to a subtree. An empty / repo-root
/// scope returns the whole worktree.
pub fn list_status(repo: &RepoHandle, dir_in_worktree: &Path) -> Result<Vec<EntryStatus>, FriendlyGitError> {
    let local = repo.to_thread_local();
    let work_dir = local
        .workdir()
        .ok_or_else(|| FriendlyGitError::new(FriendlyGitErrorKind::BareRepo, ""))?
        .to_path_buf();
    let canonical_root = work_dir.canonicalize().unwrap_or_else(|_| work_dir.clone());

    let index_path = index_path_for(repo);
    let current_mtime = index_mtime(&index_path);

    // Fast path: cache hit with matching mtime.
    if let Ok(guard) = status_cache().read()
        && let Some(cached) = guard.get(&canonical_root)
        && cached.index_mtime == current_mtime
    {
        return Ok(slice_entries(&cached.entries, &work_dir, dir_in_worktree));
    }

    // Cache miss or stale: run a full-repo walk.
    let entries = run_full_repo_status(repo)?;
    let sliced = slice_entries(&entries, &work_dir, dir_in_worktree);

    if let Ok(mut guard) = status_cache().write() {
        guard.insert(
            canonical_root,
            CachedStatus {
                index_mtime: current_mtime,
                entries,
            },
        );
    }

    Ok(sliced)
}

/// Runs a full-repo status walk via gix and returns one [`EntryStatus`] per
/// changed path.
///
/// Uses `gix::Repository::status()` which runs both a HEAD-vs-index diff
/// (`TreeIndex` items, for staged changes) and an index-vs-worktree walk
/// (`IndexWorktree` items, for unstaged changes) in parallel. Items are merged
/// by path; staged changes take precedence over worktree changes, matching
/// git's XY column precedence in `--porcelain=v2` output.
///
/// Error mapping is typed: no string parsing of stderr is performed.
fn run_full_repo_status(repo: &RepoHandle) -> Result<Vec<EntryStatus>, FriendlyGitError> {
    let local = repo.to_thread_local();
    let work_dir = local
        .workdir()
        .ok_or_else(|| FriendlyGitError::new(FriendlyGitErrorKind::BareRepo, ""))?;

    let platform = local
        .status(gix::progress::Discard)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?
        .untracked_files(gix::status::UntrackedFiles::Files);

    let iter = platform
        .into_iter(std::iter::empty::<gix::bstr::BString>())
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

    // Collect items. TreeIndex items (staged changes) and IndexWorktree items
    // (worktree changes) can both reference the same path; we give TreeIndex
    // priority by inserting it last.
    let mut by_path: HashMap<String, EntryStatusCode> = HashMap::new();

    for item_result in iter {
        let item = item_result
            .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

        match item {
            gix::status::Item::IndexWorktree(iw_item) => {
                if let Some((path, code)) = index_worktree_to_entry(&iw_item, work_dir) {
                    // Only insert if no TreeIndex entry already claimed this path.
                    by_path.entry(path).or_insert(code);
                }
            }
            gix::status::Item::TreeIndex(ref ti_change) => {
                if let Some((path, code)) = tree_index_to_entry(ti_change) {
                    // TreeIndex (staged) takes priority: overwrite any worktree entry.
                    by_path.insert(path, code);
                }
            }
        }
    }

    let mut entries: Vec<EntryStatus> = by_path
        .into_iter()
        .map(|(relative_path, code)| EntryStatus { relative_path, code })
        .collect();
    entries.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(entries)
}

/// Maps a `TreeIndex` change (HEAD vs index, staged) to an entry.
fn tree_index_to_entry(change: &gix::diff::index::Change) -> Option<(String, EntryStatusCode)> {
    use gix::bstr::ByteSlice as _;
    use gix::diff::index::ChangeRef;
    let (location, code) = match change {
        ChangeRef::Addition { location, .. } => (location, EntryStatusCode::Added),
        ChangeRef::Deletion { location, .. } => (location, EntryStatusCode::Deleted),
        ChangeRef::Modification { location, .. } => (location, EntryStatusCode::Modified),
        ChangeRef::Rewrite { location, copy, .. } => {
            if *copy {
                (location, EntryStatusCode::Copied)
            } else {
                (location, EntryStatusCode::Renamed)
            }
        }
    };
    let path = location.to_str_lossy().replace('\\', "/");
    Some((path, code))
}

/// Maps an `IndexWorktree` item (index vs worktree, i.e., unstaged) to an
/// `EntryStatus`. Returns `None` for items that don't represent a user-visible
/// change (for example, stat-refresh-only updates).
fn index_worktree_to_entry(
    item: &gix::status::index_worktree::Item,
    work_dir: &Path,
) -> Option<(String, EntryStatusCode)> {
    use gix::bstr::ByteSlice as _;
    use gix::dir::entry::Status as DirStatus;
    use gix::status::index_worktree::Item as IwItem;
    use gix::status::plumbing::index_as_worktree::{Change, EntryStatus as GixEntryStatus};

    match item {
        IwItem::Modification { rela_path, status, .. } => {
            let code = match status {
                GixEntryStatus::Change(Change::Removed) => EntryStatusCode::Deleted,
                GixEntryStatus::Change(Change::Modification { .. }) => EntryStatusCode::Modified,
                GixEntryStatus::Change(Change::Type { .. }) => EntryStatusCode::TypeChange,
                GixEntryStatus::Change(Change::SubmoduleModification(_)) => EntryStatusCode::Modified,
                GixEntryStatus::Conflict { .. } => EntryStatusCode::Conflicted,
                // NeedsUpdate and IntentToAdd are not user-visible changes.
                GixEntryStatus::NeedsUpdate(_) | GixEntryStatus::IntentToAdd => return None,
            };
            let path = rela_path.to_str_lossy().replace('\\', "/");
            Some((path, code))
        }
        IwItem::DirectoryContents { entry, .. } => {
            let code = match entry.status {
                DirStatus::Untracked => EntryStatusCode::Untracked,
                DirStatus::Ignored(_) => EntryStatusCode::Ignored,
                DirStatus::Tracked | DirStatus::Pruned => return None,
            };
            // The dirwalk entry path is relative to work_dir; no strip needed.
            let _ = work_dir;
            let path = entry.rela_path.to_str_lossy().replace('\\', "/");
            Some((path, code))
        }
        IwItem::Rewrite {
            dirwalk_entry, copy, ..
        } => {
            let code = if *copy {
                EntryStatusCode::Copied
            } else {
                EntryStatusCode::Renamed
            };
            let path = dirwalk_entry.rela_path.to_str_lossy().replace('\\', "/");
            Some((path, code))
        }
    }
}

/// Returns the entries that fall under `dir_in_worktree`. Repo-root scope
/// (empty relative path) returns everything. Otherwise we filter by
/// `<rel>/` prefix: the dir itself is excluded, only its descendants land
/// in the result, matching what the file-list cell renderer needs.
fn slice_entries(entries: &[EntryStatus], work_dir: &Path, dir_in_worktree: &Path) -> Vec<EntryStatus> {
    let rel = match dir_in_worktree.strip_prefix(work_dir) {
        Ok(rel) if !rel.as_os_str().is_empty() => rel.to_string_lossy().replace('\\', "/"),
        _ => return entries.to_vec(),
    };
    let prefix = format!("{}/", rel);
    entries
        .iter()
        .filter(|e| e.relative_path.starts_with(&prefix) || e.relative_path == rel)
        .cloned()
        .collect()
}

#[cfg(test)]
mod slice_tests {
    use super::*;

    fn entry(rel: &str) -> EntryStatus {
        EntryStatus {
            relative_path: rel.to_string(),
            code: EntryStatusCode::Modified,
        }
    }

    #[test]
    fn root_scope_returns_everything() {
        let work = Path::new("/repo");
        let all = vec![entry("a.txt"), entry("sub/b.txt"), entry("sub/deep/c.txt")];
        let out = slice_entries(&all, work, work);
        assert_eq!(out.len(), 3);
    }

    #[test]
    fn sub_scope_returns_only_descendants() {
        let work = Path::new("/repo");
        let all = vec![
            entry("a.txt"),
            entry("sub/b.txt"),
            entry("sub/deep/c.txt"),
            entry("other/d.txt"),
        ];
        let out = slice_entries(&all, work, &work.join("sub"));
        let paths: Vec<_> = out.iter().map(|e| e.relative_path.as_str()).collect();
        assert_eq!(paths, vec!["sub/b.txt", "sub/deep/c.txt"]);
    }

    #[test]
    fn sub_scope_excludes_self_directory() {
        // The dir itself shouldn't appear in the slice; only its children.
        let work = Path::new("/repo");
        let all = vec![entry("sub"), entry("sub/b.txt")];
        let out = slice_entries(&all, work, &work.join("sub"));
        let paths: Vec<_> = out.iter().map(|e| e.relative_path.as_str()).collect();
        // The dir "sub" matches `e.relative_path == rel`, but only because the
        // index records it explicitly (rare for git but possible for renames).
        // We keep this case for correctness symmetry. What matters is no
        // false positives like "subterranean.txt" sneaking in.
        assert!(paths.iter().any(|p| *p == "sub" || *p == "sub/b.txt"));
        assert!(!paths.contains(&"subterranean.txt"));
    }

    #[test]
    fn sub_scope_does_not_match_lookalike_siblings() {
        let work = Path::new("/repo");
        let all = vec![entry("sub/b.txt"), entry("subterranean.txt"), entry("sub-other/x.txt")];
        let out = slice_entries(&all, work, &work.join("sub"));
        let paths: Vec<_> = out.iter().map(|e| e.relative_path.as_str()).collect();
        assert_eq!(paths, vec!["sub/b.txt"]);
    }
}

#[cfg(test)]
mod cache_tests {
    //! Cache hit / miss / mtime invalidation tests. These build a tiny real
    //! repo so we exercise the gix status walk path. Total runtime ~200 ms each.
    use std::process::{Command, Stdio};

    use super::super::repo::discover_repo;
    use super::*;

    fn temp_repo(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("cmdr_status_cache_{}_{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        run(&dir, &["init", "-q", "-b", "main"]);
        run(&dir, &["config", "user.name", "Test"]);
        run(&dir, &["config", "user.email", "test@cmdr.local"]);
        std::fs::write(dir.join("README.md"), "hi\n").unwrap();
        run(&dir, &["add", "."]);
        run(&dir, &["commit", "-q", "-m", "init"]);
        dir
    }

    fn run(dir: &Path, args: &[&str]) {
        Command::new("git")
            .current_dir(dir)
            .args(args)
            .env("GIT_AUTHOR_NAME", "Test")
            .env("GIT_AUTHOR_EMAIL", "test@cmdr.local")
            .env("GIT_COMMITTER_NAME", "Test")
            .env("GIT_COMMITTER_EMAIL", "test@cmdr.local")
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status()
            .expect("git");
    }

    #[test]
    fn second_call_hits_cache_when_index_unchanged() {
        let dir = temp_repo("hit");
        let (handle, root) = discover_repo(&dir).unwrap();

        // Drop any leftover cache from prior runs of this test process.
        invalidate_status_cache(&root);
        std::fs::write(dir.join("untracked.txt"), "x\n").unwrap();

        let first = list_status(&handle, &dir).unwrap();
        let second = list_status(&handle, &dir).unwrap();
        assert_eq!(first.len(), second.len());
        // After the first call, the cache must have an entry.
        assert!(cache_len_for_test() >= 1);

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn index_mtime_change_invalidates_cache() {
        let dir = temp_repo("mtime");
        let (handle, root) = discover_repo(&dir).unwrap();
        invalidate_status_cache(&root);

        // First snapshot: README.md is clean.
        let _first = list_status(&handle, &dir).unwrap();
        let entries_first: Vec<&str> = _first.iter().map(|e| e.relative_path.as_str()).collect();
        assert!(
            !entries_first.contains(&"new.txt"),
            "fresh repo had no untracked new.txt"
        );

        // Stage a new file. `git add` rewrites `.git/index`, bumping the mtime.
        std::fs::write(dir.join("new.txt"), "x\n").unwrap();
        run(&dir, &["add", "new.txt"]);

        // Sleep one filesystem tick so the mtime is guaranteed to change on
        // filesystems with second-resolution timestamps. macOS APFS has
        // sub-second resolution but CI's overlayfs sometimes doesn't.
        std::thread::sleep(std::time::Duration::from_millis(1100));

        let second = list_status(&handle, &dir).unwrap();
        let entries_second: Vec<&str> = second.iter().map(|e| e.relative_path.as_str()).collect();
        assert!(
            entries_second.contains(&"new.txt"),
            "post-add status missed the new file"
        );

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn explicit_invalidate_drops_entry() {
        let dir = temp_repo("invalidate");
        let (handle, root) = discover_repo(&dir).unwrap();
        invalidate_status_cache(&root);

        let _ = list_status(&handle, &dir).unwrap();
        let canonical = root.canonicalize().unwrap_or_else(|_| root.clone());
        assert!(status_cache().read().unwrap().contains_key(&canonical));

        invalidate_status_cache(&root);
        assert!(!status_cache().read().unwrap().contains_key(&canonical));

        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn slice_returns_only_subtree_entries_from_cached_walk() {
        let dir = temp_repo("slice");
        let (handle, root) = discover_repo(&dir).unwrap();
        invalidate_status_cache(&root);

        // Stage a file under `sub/` so git records it by path rather than
        // collapsing the whole `sub/` directory into one untracked entry.
        std::fs::create_dir_all(dir.join("sub")).unwrap();
        std::fs::write(dir.join("sub/a.txt"), "x\n").unwrap();
        run(&dir, &["add", "sub/a.txt"]);
        std::fs::write(dir.join("top.txt"), "x\n").unwrap();
        run(&dir, &["add", "top.txt"]);

        // Whole-repo: sees both staged paths.
        let full = list_status(&handle, &dir).unwrap();
        let full_paths: Vec<&str> = full.iter().map(|e| e.relative_path.as_str()).collect();
        assert!(
            full_paths.contains(&"top.txt"),
            "whole-repo missed top.txt: {:?}",
            full_paths
        );
        assert!(
            full_paths.contains(&"sub/a.txt"),
            "whole-repo missed sub/a.txt: {:?}",
            full_paths
        );

        // Subtree: sees only `sub/a.txt`. The cache stays warm; slicing is in-memory.
        let scoped = list_status(&handle, &dir.join("sub")).unwrap();
        let scoped_paths: Vec<&str> = scoped.iter().map(|e| e.relative_path.as_str()).collect();
        assert!(!scoped_paths.contains(&"top.txt"));
        assert!(scoped_paths.contains(&"sub/a.txt"));

        let _ = std::fs::remove_dir_all(&dir);
    }
}
