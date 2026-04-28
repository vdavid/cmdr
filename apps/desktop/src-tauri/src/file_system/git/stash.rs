//! Stash entries under `.git/stash/`.
//!
//! v1 lists `stash@{n}` entries as virtual directories, browsing the
//! stash's working-tree state.
//!
//! ## Decision: shell out for the listing
//!
//! gix 0.81 doesn't expose a stash-list API at the public surface. The
//! reflog at `refs/stash` is the canonical source – each entry is a
//! merge commit `W` whose first parent is the original HEAD `B` and
//! whose second parent (or third when `git stash -u`) is the index/
//! untracked-files commit. We could parse the reflog by hand, but
//! shelling out to `git stash list` gives us git's own ordering and the
//! exact `stash@{n}` indices users see in the terminal. The `git` binary
//! is part of the project's system requirements anyway.
//!
//! ## Decision: browse the W (working-tree) commit, not B
//!
//! `git stash` records the dirty worktree state as a *merge commit* (the
//! "W" commit). Its tree is the worktree at stash time. The first parent
//! ("B") is HEAD at stash time – that's the *clean* tree, not the
//! stashed changes. Users typing `.git/stash/0/...` expect to see what
//! they stashed, so we browse W's tree directly.

use std::path::Path;
use std::process::{Command, Stdio};

use crate::file_system::listing::FileEntry;

use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::repo::RepoHandle;

/// Lists stash entries as virtual directory entries `0`, `1`, …
///
/// Each entry's display name is `stash@{n}: <subject>` so the listing
/// reads naturally; the on-disk segment is just the index. We don't take
/// a `RepoHandle` because gix has no public stash API and we shell out to
/// `git -C <repo_root>` directly; the caller resolves `repo_root` from
/// the same `discover_repo` cache so there's no redundant lookup.
pub fn list_stashes(repo_root: &Path) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let parent = repo_root.join(".git").join("stash");
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_root)
        .args(["stash", "list", "-z", "--format=%H%x09%gd%x09%s%x09%ct"])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

    if !output.status.success() {
        // Non-zero exit doesn't mean "stash failed" – it can also mean
        // the repo just doesn't have a stash yet. Treat as empty list.
        return Ok(Vec::new());
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let mut out = Vec::new();
    for record in raw.split('\0').filter(|r| !r.is_empty()) {
        // record: <full-sha>\t<gd>\t<subject>\t<commit-time>
        let mut parts = record.splitn(4, '\t');
        let _sha = parts.next().unwrap_or("");
        let gd = parts.next().unwrap_or("");
        let subject = parts.next().unwrap_or("");
        let time_str = parts.next().unwrap_or("");

        let idx = parse_stash_index(gd);
        let Some(idx) = idx else {
            continue;
        };

        let segment = idx.to_string();
        let entry_path = parent.join(&segment);
        let display = format!("stash@{{{}}}: {}", idx, subject);
        let mut fe = FileEntry::new(display, entry_path.to_string_lossy().into_owned(), true, false);
        fe.icon_id = "git:fork".into();
        fe.permissions = 0o755;
        let secs = time_str.parse::<u64>().ok();
        fe.modified_at = secs;
        fe.created_at = secs;
        fe.added_at = secs;
        out.push(fe);
    }

    Ok(out)
}

/// Resolves a `stash/<n>` segment to the W (working-tree) commit ID.
///
/// We re-shell-out rather than caching: stashes can be added/dropped at
/// any time, and the watcher re-emits anyway. The cost is one process
/// spawn per nav, which is negligible compared to the tree walk that
/// follows.
pub fn resolve_stash_commit(handle: &RepoHandle, n: usize) -> Result<gix::ObjectId, FriendlyGitError> {
    // gix has no `stash@{n}` parser; we shell out to `git rev-parse`.
    // The handle is here only to derive the worktree root for the
    // `git -C` cwd, via the same `work_dir()` path the rest of the
    // module uses.
    use gix::bstr::ByteSlice;
    use std::path::PathBuf;

    let repo_root = repo_root_from_handle(handle)?;
    // git rev-parse stash@{n} prints the W commit's full SHA.
    let spec = format!("stash@{{{}}}", n);
    let output = Command::new("git")
        .arg("-C")
        .arg(&repo_root as &PathBuf)
        .args(["rev-parse", "--verify", spec.as_str()])
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

    if !output.status.success() {
        return Err(FriendlyGitError::new(
            FriendlyGitErrorKind::CorruptRepo,
            format!("stash@{{{}}}", n),
        ));
    }
    let hex = output.stdout.trim_with(|c: char| c.is_ascii_whitespace());
    let hex_str = std::str::from_utf8(hex)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    gix::ObjectId::from_hex(hex_str.as_bytes())
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))
}

fn repo_root_from_handle(handle: &RepoHandle) -> Result<std::path::PathBuf, FriendlyGitError> {
    #[allow(deprecated, reason = "ThreadSafeRepository only exposes work_dir(); see repo.rs")]
    handle
        .work_dir()
        .map(|p| p.to_path_buf())
        .ok_or_else(|| FriendlyGitError::new(FriendlyGitErrorKind::BareRepo, "<unknown>"))
}

fn parse_stash_index(gd: &str) -> Option<usize> {
    // gd looks like `stash@{0}` – pull the digits between `{` and `}`.
    let start = gd.find('{')?;
    let end = gd.find('}')?;
    if end <= start {
        return None;
    }
    gd[start + 1..end].parse().ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_stash_index() {
        assert_eq!(parse_stash_index("stash@{0}"), Some(0));
        assert_eq!(parse_stash_index("stash@{42}"), Some(42));
        assert_eq!(parse_stash_index(""), None);
        assert_eq!(parse_stash_index("notastash"), None);
    }
}
