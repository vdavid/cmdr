//! Linked worktrees under `.git/worktrees/`.
//!
//! Each linked worktree is exposed as a virtual entry whose
//! `redirectToPath` points at the worktree's working directory. The
//! frontend opens that path directly instead of treating the entry as a
//! virtual subtree.
//!
//! ## Decision: gix `Repository::worktrees()`
//!
//! gix exposes `worktrees() -> Vec<worktree::Proxy>` which reads
//! `<common-dir>/worktrees/*/gitdir` and gives us the working-tree base
//! path via `proxy.base()`. That's exactly what we need – no shell-out.
//!
//! ## Real `.git/worktrees/` collision
//!
//! Linked-worktree setups have a real `.git/worktrees/` directory. We
//! shadow it with the virtual one. The real directory contents stay
//! reachable under `.git/raw/worktrees/` (the existing M2 escape hatch).
//! The chip tooltip and the git/CLAUDE.md document this.

use std::path::Path;

use crate::file_system::listing::FileEntry;

use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::repo::RepoHandle;

/// Lists linked worktrees as virtual entries with `redirectToPath` set.
///
/// Each entry's Modified column carries the worktree's HEAD commit date
/// (so the user can spot stale worktrees at a glance). The Size column
/// shows the worktree's checked-out branch (`on feature-x`) or short
/// SHA when detached.
pub fn list_worktrees(handle: &RepoHandle, repo_root: &Path) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let parent = repo_root.join(".git").join("worktrees");
    let repo = handle.to_thread_local();
    let proxies = repo
        .worktrees()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

    let mut out = Vec::with_capacity(proxies.len());
    for proxy in proxies {
        let id = proxy.id().to_string();
        // base() reads `gitdir` and resolves the worktree path. Skip
        // worktrees whose base is missing rather than fail the whole
        // listing – orphaned linked-worktrees are common after manual
        // moves.
        let Ok(base) = proxy.base() else {
            continue;
        };
        let display_path = parent.join(&id);
        let mut fe = FileEntry::new(id, display_path.to_string_lossy().into_owned(), true, false);
        fe.icon_id = "git:fork".into();
        fe.permissions = 0o755;
        // Redirect navigation: opening this entry takes the user to the
        // worktree's working directory, which itself is a git portal.
        fe.redirect_to_path = Some(base.display().to_string());
        // Decode the worktree's HEAD: a symbolic ref points at a branch
        // (`on feature-x`), a detached HEAD shows the short SHA.
        if let Ok(wt_repo) = proxy.into_repo() {
            populate_worktree_columns(&mut fe, &wt_repo);
        }
        out.push(fe);
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

fn populate_worktree_columns(fe: &mut FileEntry, wt_repo: &gix::Repository) {
    if let Ok(head) = wt_repo.head() {
        match &head.kind {
            gix::head::Kind::Symbolic(reference) => {
                let full = reference.name.as_bstr().to_string();
                let branch = full.strip_prefix("refs/heads/").unwrap_or(&full).to_string();
                fe.display_size = Some(format!("on {}", branch));
                fe.display_size_tooltip = Some(format!("Branch `{}` is checked out", branch));
            }
            gix::head::Kind::Detached { target, .. } => {
                let short: String = target.to_string().chars().take(7).collect();
                fe.display_size = Some(short.clone());
                fe.display_size_tooltip = Some(format!("Detached at {}", target));
            }
            gix::head::Kind::Unborn(_) => {
                // Fresh worktree without commits — leave the cell blank.
            }
        }
    }
    if let Ok(id) = wt_repo.head_id()
        && let Ok(commit) = wt_repo.find_commit(id.detach())
        && let Ok(committer) = commit.committer()
        && let Ok(time) = committer.time()
    {
        fe.modified_at = u64::try_from(time.seconds).ok();
        fe.created_at = fe.modified_at;
        fe.added_at = fe.modified_at;
    }
}
