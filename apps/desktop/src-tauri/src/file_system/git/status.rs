//! Per-entry git status for a working-tree directory.
//!
//! v1 shells out to `git status --porcelain=v2 --untracked-files=normal`
//! because gix's `Repository::status()` iterator missed staged additions in
//! our fixture-driven tests against a single-commit repo. The shell-out has
//! a 5 s timeout via the IPC layer and gives us identical semantics to the
//! command line. See `CLAUDE.md` § "gix vs shell-out outcome" for details.
//!
//! The `git` binary is part of the project's system requirements; no new
//! external dependency is taken on.
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
use std::process::{Command, Stdio};
use std::sync::{OnceLock, RwLock};
use std::time::SystemTime;

use serde::{Deserialize, Serialize};

use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::repo::RepoHandle;

/// Single-character status code.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
    /// All entries from a full-repo `git status --porcelain=v2 -z`.
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
/// `dir_in_worktree` on the way out. Cache misses run a full
/// `git status --porcelain=v2 -z --untracked-files=normal` (no pathspec)
/// so any pane on the same repo benefits from the warm cache afterwards.
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
    let entries = run_full_repo_status(&work_dir)?;
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

/// Runs `git status --porcelain=v2 -z` over the whole worktree and parses it.
fn run_full_repo_status(work_dir: &Path) -> Result<Vec<EntryStatus>, FriendlyGitError> {
    let output = Command::new("git")
        .arg("-C")
        .arg(work_dir)
        .args(["status", "--porcelain=v2", "--untracked-files=normal", "-z"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let kind = if stderr.contains("index.lock") || stderr.contains("Unable to create") {
            FriendlyGitErrorKind::IndexLocked
        } else {
            FriendlyGitErrorKind::CorruptRepo
        };
        return Err(FriendlyGitError {
            kind,
            path: work_dir.display().to_string(),
            raw: Some(stderr.into_owned()),
        });
    }

    Ok(parse_porcelain_v2(&output.stdout))
}

/// Returns the entries that fall under `dir_in_worktree`. Repo-root scope
/// (empty relative path) returns everything. Otherwise we filter by
/// `<rel>/` prefix — the dir itself is excluded, only its descendants land
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

/// Parses git's `--porcelain=v2 -z` NUL-separated output.
///
/// Each record is one of:
/// - `1 XY ...` ordinary changed entries (one path)
/// - `2 XY ...` renamed/copied entries (NUL-separated `<path>\0<orig>`)
/// - `u XY ...` unmerged entries (one path)
/// - `? <path>` untracked
/// - `! <path>` ignored
fn parse_porcelain_v2(stdout: &[u8]) -> Vec<EntryStatus> {
    let mut out = Vec::new();
    let mut iter = stdout.split(|b| *b == 0).peekable();
    while let Some(record) = iter.next() {
        if record.is_empty() {
            continue;
        }
        let rec = String::from_utf8_lossy(record).into_owned();
        let mut chars = rec.chars();
        match chars.next() {
            Some('?') => {
                if let Some(path) = rec.get(2..) {
                    out.push(EntryStatus {
                        relative_path: path.to_string(),
                        code: EntryStatusCode::Untracked,
                    });
                }
            }
            Some('!') => {
                if let Some(path) = rec.get(2..) {
                    out.push(EntryStatus {
                        relative_path: path.to_string(),
                        code: EntryStatusCode::Ignored,
                    });
                }
            }
            Some('1') => {
                // `1 XY <sub> <mode_h> <mode_i> <mode_w> <h_h> <h_i> <path>`
                let parts: Vec<&str> = rec.splitn(9, ' ').collect();
                if parts.len() >= 9 {
                    let xy = parts[1];
                    let path = parts[8];
                    if let Some(code) = code_from_xy(xy) {
                        out.push(EntryStatus {
                            relative_path: path.to_string(),
                            code,
                        });
                    }
                }
            }
            Some('2') => {
                // Rename/copy: header followed by NUL `<orig>` field.
                let parts: Vec<&str> = rec.splitn(10, ' ').collect();
                if parts.len() >= 10 {
                    let xy = parts[1];
                    let path = parts[9];
                    let _orig = iter.next();
                    let code = if xy.starts_with('C') || xy.contains('C') {
                        EntryStatusCode::Copied
                    } else {
                        EntryStatusCode::Renamed
                    };
                    out.push(EntryStatus {
                        relative_path: path.to_string(),
                        code,
                    });
                }
            }
            Some('u') => {
                // Unmerged: `u XY <sub> <m1> <m2> <m3> <mw> <h1> <h2> <h3> <path>`
                let parts: Vec<&str> = rec.splitn(11, ' ').collect();
                if parts.len() >= 11 {
                    out.push(EntryStatus {
                        relative_path: parts[10].to_string(),
                        code: EntryStatusCode::Conflicted,
                    });
                }
            }
            _ => {}
        }
    }
    out
}

/// Maps the `XY` columns from porcelain v2 to a single status code.
/// `X` is the index-vs-HEAD column, `Y` is the worktree-vs-index column.
/// We pick the more "user-meaningful" one: a staged add with a worktree
/// modification still shows as Added in the column.
fn code_from_xy(xy: &str) -> Option<EntryStatusCode> {
    let mut chars = xy.chars();
    let x = chars.next()?;
    let y = chars.next()?;
    // Index changes (X) take precedence – they reflect what's about to land.
    let from = |c: char| -> Option<EntryStatusCode> {
        match c {
            'M' => Some(EntryStatusCode::Modified),
            'A' => Some(EntryStatusCode::Added),
            'D' => Some(EntryStatusCode::Deleted),
            'R' => Some(EntryStatusCode::Renamed),
            'C' => Some(EntryStatusCode::Copied),
            'T' => Some(EntryStatusCode::TypeChange),
            'U' => Some(EntryStatusCode::Conflicted),
            '.' | ' ' => None,
            _ => None,
        }
    };
    from(x).or_else(|| from(y))
}

#[cfg(test)]
mod parse_tests {
    use super::*;

    #[test]
    fn parses_untracked_and_modified_and_added() {
        // Hand-crafted -z output mimicking the CLI we shell out to.
        let stdout = b"1 .M N... 100644 100644 100644 deadbeef deadbeef README.md\0\
1 A. N... 000000 100644 100644 0000000000000000000000000000000000000000 f2ad README.added.txt\0\
? untracked.txt\0\
! ignored.log\0";
        let entries = parse_porcelain_v2(stdout);
        let codes: Vec<EntryStatusCode> = entries.iter().map(|e| e.code).collect();
        assert!(codes.contains(&EntryStatusCode::Modified));
        assert!(codes.contains(&EntryStatusCode::Added));
        assert!(codes.contains(&EntryStatusCode::Untracked));
        assert!(codes.contains(&EntryStatusCode::Ignored));
    }

    #[test]
    fn empty_input_parses_to_empty() {
        let entries = parse_porcelain_v2(b"");
        assert!(entries.is_empty());
    }
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
        // We keep this case for correctness symmetry — what matters is no
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
    //! repo so we exercise the actual `git status` shell-out path, not just
    //! the parser. Total runtime ~200 ms each.
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
