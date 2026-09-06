//! Submodules under `.git/submodules/`.
//!
//! Each configured submodule becomes a virtual entry whose
//! `redirectToPath` points at the submodule's working directory. The
//! submodule itself is a git repo, so opening it lands the user in
//! another portal automatically.
//!
//! ## Decision: gix `Repository::submodules()`
//!
//! gix reads `.gitmodules` and exposes one `Submodule` per entry. The
//! API gives us name, path, and an `open()` for full repo access. We
//! only need name + working-tree path here.

use std::path::Path;

use gix::bstr::ByteSlice;

use cmdr_fs::entry::FileEntry;

use crate::repo::RepoHandle;
use cmdr_fs::git_meta::GitEntryMeta;
use cmdr_fs::volume::friendly_error::git::{FriendlyGitError, FriendlyGitErrorKind};

/// Lists submodules as virtual entries with `redirectToPath` set.
pub fn list_submodules(handle: &RepoHandle, repo_root: &Path) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let parent = repo_root.join(".git").join("submodules");
    let repo = handle.to_thread_local();
    let modules = match repo
        .submodules()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?
    {
        Some(iter) => iter,
        None => return Ok(Vec::new()),
    };

    let mut out = Vec::new();
    for sm in modules {
        let name = sm.name().to_str_lossy().into_owned();
        let rel_path = match sm.path() {
            Ok(p) => p.to_string(),
            Err(_) => continue,
        };
        let work_dir = repo_root.join(&rel_path);
        let display_path = parent.join(&name);
        let mut fe = FileEntry::new(name, display_path.to_string_lossy().into_owned(), true, false);
        fe.icon_id = "git:fork".into();
        fe.permissions = 0o755;
        fe.redirect_to_path = Some(work_dir.display().to_string());

        // Pinned commit: open the submodule's repo at its working dir
        // and read HEAD. If the submodule isn't initialized yet, leave
        // the cell blank rather than guess.
        if let Ok(opened) = gix::open(&work_dir)
            && let Ok(id) = opened.head_id()
        {
            let target_id = id.detach();
            fe.git_meta = Some(GitEntryMeta::PinnedCommit {
                id: target_id.to_string(),
            });
            if let Ok(commit) = opened.find_commit(target_id)
                && let Ok(committer) = commit.committer()
                && let Ok(time) = committer.time()
            {
                fe.modified_at = u64::try_from(time.seconds).ok();
                fe.created_at = fe.modified_at;
                fe.added_at = fe.modified_at;
            }
        }
        out.push(fe);
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}
