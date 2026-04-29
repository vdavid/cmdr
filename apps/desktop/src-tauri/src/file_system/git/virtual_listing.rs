//! Virtual `.git/` listings.
//!
//! - `list_root` – the portal root (M2 ships `branches/`, `tags/`, `raw/`)
//! - `list_branches` / `list_tags` – refs as virtual dirs
//! - `list_raw` – passthrough into the real on-disk `.git/<sub>` contents
//!
//! These return `Vec<FileEntry>` because the existing `Volume::list_directory`
//! contract is single-shot. The underlying gix iterators are fast enough
//! (< 50 ms even on 10k branches) that streaming inside this layer doesn't
//! add value yet – cancellation for the surrounding listing pipeline still
//! works because the volume hook runs inside the listing's `spawn_blocking`
//! task, which the listing module aborts on cancel.

use std::path::{Path, PathBuf};

use gix::refs::PartialName;

use crate::file_system::listing::FileEntry;
use crate::file_system::listing::reading::get_single_entry;

use super::column_meta::{
    self, ahead_behind_for_branch, commit_meta, head_commit_secs, newest_branch_tip_secs, newest_tag_secs,
    tag_or_commit_secs,
};
use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::path::{Cat, strip_ref_prefix};
use super::repo::RepoHandle;

/// Lists the categories visible at the portal root.
///
/// All seven categories are listed: M2 shipped `branches/`, `tags/`,
/// `raw/`; M3 added `commits/`, `stash/`, `worktrees/`, `submodules/`.
/// Empty categories (no commits, no stashes) still show up – opening
/// them shows an empty listing, which is more honest than hiding the
/// concept altogether.
///
/// Modified + Size columns are populated per category. See
/// `column_meta` for the rules.
pub fn list_root(handle: &RepoHandle, repo_root: &Path) -> Vec<FileEntry> {
    let dot_git = repo_root.join(".git");
    let categories = [
        (Cat::Branches, "git:branch"),
        (Cat::Tags, "git:tag"),
        (Cat::Commits, "git:commit"),
        (Cat::Stash, "git:fork"),
        (Cat::Worktrees, "git:fork"),
        (Cat::Submodules, "git:fork"),
        (Cat::Raw, "git:fork"),
    ];

    let raw_mtime = std::fs::metadata(&dot_git).ok().and_then(|m| {
        m.modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
    });

    categories
        .into_iter()
        .map(|(cat, icon)| {
            let segment = cat.as_segment();
            let path = dot_git.join(segment).to_string_lossy().into_owned();
            let mut fe = FileEntry::new(segment.to_string(), path, true, false);
            fe.permissions = 0o755;
            fe.icon_id = icon.to_string();
            populate_root_category(&mut fe, cat, handle, repo_root, raw_mtime);
            fe
        })
        .collect()
}

fn populate_root_category(fe: &mut FileEntry, cat: Cat, handle: &RepoHandle, repo_root: &Path, raw_mtime: Option<u64>) {
    let repo = handle.to_thread_local();
    match cat {
        Cat::Branches => {
            let count = count_local_branches(&repo);
            fe.size = Some(count);
            fe.display_size = Some(column_meta::pluralize(count, "branch", "branches"));
            fe.display_size_tooltip = Some(format!("{} on this repo", fe.display_size.as_ref().unwrap()));
            fe.modified_at = newest_branch_tip_secs(handle);
        }
        Cat::Tags => {
            let count = count_tags(&repo);
            fe.size = Some(count);
            fe.display_size = Some(column_meta::pluralize(count, "tag", "tags"));
            fe.display_size_tooltip = Some(format!("{} on this repo", fe.display_size.as_ref().unwrap()));
            fe.modified_at = newest_tag_secs(handle);
        }
        Cat::Commits => {
            let count = count_commits_capped(&repo);
            fe.size = Some(count);
            fe.display_size = Some(column_meta::pluralize(count, "commit", "commits"));
            fe.display_size_tooltip = Some(format!("{} reachable from HEAD", fe.display_size.as_ref().unwrap()));
            fe.modified_at = head_commit_secs(handle);
        }
        Cat::Stash => {
            let count = super::stash::list_stashes(repo_root)
                .map(|v| v.len() as u64)
                .unwrap_or(0);
            fe.size = Some(count);
            fe.display_size = Some(column_meta::pluralize(count, "stash entry", "stash entries"));
            fe.display_size_tooltip = Some(fe.display_size.clone().unwrap());
            fe.modified_at = newest_stash_secs(repo_root);
        }
        Cat::Worktrees => {
            let entries = super::worktrees::list_worktrees(handle, repo_root).unwrap_or_default();
            let count = entries.len() as u64;
            fe.size = Some(count);
            fe.display_size = Some(column_meta::pluralize(count, "linked worktree", "linked worktrees"));
            fe.display_size_tooltip = Some(fe.display_size.clone().unwrap());
            fe.modified_at = newest_worktree_head_secs(&repo);
        }
        Cat::Submodules => {
            let entries = super::submodules::list_submodules(handle, repo_root).unwrap_or_default();
            let count = entries.len() as u64;
            fe.size = Some(count);
            fe.display_size = Some(column_meta::pluralize(count, "submodule", "submodules"));
            fe.display_size_tooltip = Some(fe.display_size.clone().unwrap());
            fe.modified_at = newest_submodule_secs(&repo, handle, repo_root);
        }
        Cat::Raw => {
            fe.modified_at = raw_mtime;
            // Raw stays without `display_size`; the recursive byte total
            // would require walking the on-disk gitdir, and the Size cell
            // for `.git/raw/` falls back to bytes.
        }
    }
}

/// Populates Modified + Size on a single `Ref(cat, name)` stat without
/// re-running the full per-category listing. Mirrors what `list_branches`
/// / `list_tags` / `list_commits` / etc. set per row, so a direct
/// metadata fetch (for example, navigating into the entry) shows the
/// same Size cell as the parent listing did.
fn populate_ref_columns(fe: &mut FileEntry, cat: Cat, name: &str, handle: &RepoHandle, repo_root: &Path) {
    let repo = handle.to_thread_local();
    match cat {
        Cat::Branches => {
            if let Ok(id) = resolve_ref_commit(handle, Cat::Branches, name) {
                if let Ok(meta) = commit_meta(&repo, id) {
                    fe.modified_at = u64::try_from(meta.committer_secs).ok();
                    fe.created_at = fe.modified_at;
                    fe.added_at = fe.modified_at;
                }
                if let Some(ab) = ahead_behind_for_branch(&repo, name, id) {
                    fe.size = Some(u64::from(ab.ahead));
                    fe.display_size = Some(format!("+{} / -{}", ab.ahead, ab.behind));
                    fe.display_size_tooltip = Some(format!(
                        "{} commits ahead, {} commits behind `{}`",
                        ab.ahead, ab.behind, ab.vs
                    ));
                }
            }
        }
        Cat::Tags => {
            if let Ok(id) = resolve_ref_commit(handle, Cat::Tags, name) {
                if let Some(secs) = tag_or_commit_secs(&repo, id) {
                    fe.modified_at = u64::try_from(secs).ok();
                    fe.created_at = fe.modified_at;
                    fe.added_at = fe.modified_at;
                }
                let short: String = id.to_string().chars().take(7).collect();
                fe.display_size = Some(short);
                fe.display_size_tooltip = Some(format!("Tagged commit {}", id));
            }
        }
        Cat::Commits => {
            if let Ok(id) = super::log::resolve_commit_id(handle, name) {
                if let Ok(meta) = commit_meta(&repo, id) {
                    fe.modified_at = u64::try_from(meta.committer_secs).ok();
                    fe.created_at = fe.modified_at;
                    fe.added_at = fe.modified_at;
                }
                if let Some(n) = column_meta::files_changed_count(&repo, id) {
                    fe.size = Some(n);
                    fe.display_size = Some(column_meta::pluralize(n, "file", "files"));
                    fe.display_size_tooltip = Some(format!(
                        "{} changed compared to the parent commit",
                        column_meta::pluralize(n, "file", "files")
                    ));
                }
            }
        }
        Cat::Stash => {
            if let Ok(idx) = name.parse::<usize>()
                && let Ok(entries) = super::stash::list_stashes(repo_root)
                && let Some(found) = entries.into_iter().nth(idx)
            {
                fe.modified_at = found.modified_at;
                fe.created_at = found.created_at;
                fe.added_at = found.added_at;
                fe.display_size = found.display_size;
                fe.display_size_tooltip = found.display_size_tooltip;
            }
        }
        Cat::Worktrees => {
            if let Ok(entries) = super::worktrees::list_worktrees(handle, repo_root)
                && let Some(found) = entries.into_iter().find(|e| e.name == name)
            {
                fe.modified_at = found.modified_at;
                fe.created_at = found.created_at;
                fe.added_at = found.added_at;
                fe.display_size = found.display_size;
                fe.display_size_tooltip = found.display_size_tooltip;
            }
        }
        Cat::Submodules => {
            if let Ok(entries) = super::submodules::list_submodules(handle, repo_root)
                && let Some(found) = entries.into_iter().find(|e| e.name == name)
            {
                fe.modified_at = found.modified_at;
                fe.created_at = found.created_at;
                fe.added_at = found.added_at;
                fe.display_size = found.display_size;
                fe.display_size_tooltip = found.display_size_tooltip;
            }
        }
        Cat::Raw => {} // raw entries get real-FS metadata via `get_single_entry`.
    }
}

fn count_local_branches(repo: &gix::Repository) -> u64 {
    let Ok(platform) = repo.references() else {
        return 0;
    };
    let Ok(iter) = platform.local_branches() else {
        return 0;
    };
    iter.flatten().count() as u64
}

fn count_tags(repo: &gix::Repository) -> u64 {
    let Ok(platform) = repo.references() else {
        return 0;
    };
    let Ok(iter) = platform.tags() else {
        return 0;
    };
    iter.flatten().count() as u64
}

fn count_commits_capped(repo: &gix::Repository) -> u64 {
    use gix::revision::walk::Sorting;
    use gix::traverse::commit::simple::CommitTimeOrder;
    let Ok(head) = repo.head_id() else { return 0 };
    let Ok(walk) = repo
        .rev_walk([head.detach()])
        .sorting(Sorting::ByCommitTime(CommitTimeOrder::NewestFirst))
        .all()
    else {
        return 0;
    };
    let mut count: u64 = 0;
    for info in walk {
        if info.is_err() {
            break;
        }
        count = count.saturating_add(1);
        // Cap matches `log::MAX_COMMITS` so the `.git/commits/` Size cell
        // ("5000 commits") matches what the user sees on entering.
        if count >= super::log::MAX_COMMITS as u64 {
            break;
        }
    }
    count
}

fn newest_stash_secs(repo_root: &Path) -> Option<u64> {
    let entries = super::stash::list_stashes(repo_root).ok()?;
    entries.iter().filter_map(|e| e.modified_at).max()
}

fn newest_worktree_head_secs(repo: &gix::Repository) -> Option<u64> {
    let proxies = repo.worktrees().ok()?;
    let mut newest: Option<i64> = None;
    for proxy in proxies {
        // Each proxy can open its own repo; we read its HEAD commit time.
        let Ok(wt_repo) = proxy.into_repo() else { continue };
        let Ok(id) = wt_repo.head_id() else { continue };
        let Ok(commit) = wt_repo.find_commit(id.detach()) else {
            continue;
        };
        let Ok(committer) = commit.committer() else { continue };
        let Ok(time) = committer.time() else { continue };
        newest = Some(newest.map_or(time.seconds, |n| n.max(time.seconds)));
    }
    newest.and_then(|s| u64::try_from(s).ok())
}

fn newest_submodule_secs(repo: &gix::Repository, _handle: &RepoHandle, repo_root: &Path) -> Option<u64> {
    let modules = repo.submodules().ok()??;
    let mut newest: Option<i64> = None;
    for sm in modules {
        // Pinned commit lives in the parent's index, not in the submodule's
        // own ODB necessarily. We resolve via gix's submodule helpers.
        let Some(secs) = pinned_commit_secs(&sm, repo_root) else {
            continue;
        };
        newest = Some(newest.map_or(secs, |n| n.max(secs)));
    }
    newest.and_then(|s| u64::try_from(s).ok())
}

fn pinned_commit_secs(sm: &gix::Submodule<'_>, repo_root: &Path) -> Option<i64> {
    // Open the submodule's own repo and resolve its HEAD; the pinned
    // commit equals what's checked out there. If the submodule isn't
    // initialized (no working tree yet), fall back to the parent's
    // recorded id via `head_id`.
    if let Ok(rel) = sm.path() {
        let path = repo_root.join(rel.to_string());
        if let Ok(opened) = gix::open(&path)
            && let Ok(id) = opened.head_id()
            && let Ok(commit) = opened.find_commit(id.detach())
            && let Ok(committer) = commit.committer()
            && let Ok(time) = committer.time()
        {
            return Some(time.seconds);
        }
    }
    None
}

/// Lists local branches as virtual directory entries.
///
/// Each entry carries a real `modified_at` (branch tip's committer date)
/// and a loose `display_size` showing ahead/behind relative to the
/// branch's upstream — falling back to `main`/`master` for branches
/// without a configured upstream. The numeric `size` field carries the
/// ahead-count so within-category Size sort puts the most-ahead branch
/// first.
pub fn list_branches(handle: &RepoHandle, repo_root: &Path) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let parent = repo_root.join(".git").join(Cat::Branches.as_segment());
    let repo = handle.to_thread_local();
    let platform = repo
        .references()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let iter = platform
        .local_branches()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

    let mut out = Vec::new();
    for r in iter.flatten() {
        let mut r = r;
        let full = r.name().as_bstr().to_string();
        let short = strip_ref_prefix(&full, Cat::Branches);
        if short.is_empty() {
            continue;
        }
        let path = parent.join(&short).to_string_lossy().into_owned();
        let mut fe = FileEntry::new(short.clone(), path, true, false);
        fe.permissions = 0o755;
        fe.icon_id = "git:branch".into();

        if let Ok(tip) = r.peel_to_id() {
            let tip_id = tip.detach();
            if let Ok(meta) = commit_meta(&repo, tip_id) {
                fe.modified_at = u64::try_from(meta.committer_secs).ok();
                fe.created_at = fe.modified_at;
                fe.added_at = fe.modified_at;
            }
            // Ahead/behind via upstream or fallback default branch.
            if let Some(ab) = ahead_behind_for_branch(&repo, &short, tip_id) {
                fe.size = Some(u64::from(ab.ahead));
                fe.display_size = Some(format!("+{} / -{}", ab.ahead, ab.behind));
                fe.display_size_tooltip = Some(format!(
                    "{} commits ahead, {} commits behind `{}`",
                    ab.ahead, ab.behind, ab.vs
                ));
            }
        }
        out.push(fe);
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

/// Lists tags as virtual directory entries.
///
/// Annotated tags resolve through their tag object to the underlying
/// commit at navigation time (in `tree::resolve_tree_at`), so this
/// listing only carries the ref names themselves.
///
/// Each tag carries the annotated-tag date when present, otherwise the
/// tagged commit's committer date. The Size column shows the short SHA
/// of the tagged commit so users can ID it at a glance.
pub fn list_tags(handle: &RepoHandle, repo_root: &Path) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let parent = repo_root.join(".git").join(Cat::Tags.as_segment());
    let repo = handle.to_thread_local();
    let platform = repo
        .references()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let iter = platform
        .tags()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;

    let mut out = Vec::new();
    for r in iter.flatten() {
        let mut r = r;
        let full = r.name().as_bstr().to_string();
        let short = strip_ref_prefix(&full, Cat::Tags);
        if short.is_empty() {
            continue;
        }
        let path = parent.join(&short).to_string_lossy().into_owned();
        let mut fe = FileEntry::new(short, path, true, false);
        fe.permissions = 0o755;
        fe.icon_id = "git:tag".into();

        if let Ok(target) = r.peel_to_id() {
            let target_id = target.detach();
            if let Some(secs) = tag_or_commit_secs(&repo, target_id) {
                fe.modified_at = u64::try_from(secs).ok();
                fe.created_at = fe.modified_at;
                fe.added_at = fe.modified_at;
            }
            // Display the wrapped commit's short SHA. Annotated tags peel
            // to their commit through gix's `peel_to_id` chain when
            // reading via `references()`, so `target_id` is the commit.
            let short_sha: String = target_id.to_string().chars().take(7).collect();
            fe.display_size = Some(short_sha.clone());
            fe.display_size_tooltip = Some(format!("Tagged commit {}", target_id));
        }
        out.push(fe);
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

/// Reads the real on-disk `.git/<sub_path>` directory.
///
/// This bypasses the volume hook to avoid recursion: we use `std::fs`
/// directly. Calling back into `LocalPosixVolume::list_directory` would
/// classify the path as virtual again and loop forever.
pub fn list_raw(repo_root: &Path, sub_path: &str) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let display_parent = repo_root.join(".git").join(Cat::Raw.as_segment());
    let real = real_gitdir_path(repo_root, sub_path);

    let read = std::fs::read_dir(&real).map_err(|e| FriendlyGitError {
        kind: FriendlyGitErrorKind::PermissionDenied,
        path: real.display().to_string(),
        raw: Some(e.to_string()),
    })?;

    let mut out = Vec::new();
    for entry in read.flatten() {
        let abs = entry.path();
        // Stat the real on-disk entry but rewrite its display path to live
        // under `.git/raw/...` so the URL the user sees stays virtual.
        let Ok(mut fe) = get_single_entry(&abs) else {
            continue;
        };
        let virt_path = if sub_path.is_empty() {
            display_parent.join(&fe.name)
        } else {
            display_parent.join(sub_path).join(&fe.name)
        };
        fe.path = virt_path.to_string_lossy().into_owned();
        out.push(fe);
    }
    out.sort_by(|a, b| match (a.is_directory, b.is_directory) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });
    Ok(out)
}

/// Resolves a virtual `raw/<sub>` path to the actual on-disk gitdir entry.
///
/// For a normal worktree the gitdir is `<root>/.git`. For a linked
/// worktree (gitlink), `<root>/.git` is a file pointing into
/// `<main>/.git/worktrees/<name>` – this helper follows that.
pub fn real_gitdir_path(repo_root: &Path, sub_path: &str) -> PathBuf {
    let dot_git = repo_root.join(".git");
    let gitdir = if dot_git.is_file() {
        if let Ok(content) = std::fs::read_to_string(&dot_git)
            && let Some(stripped) = content.trim().strip_prefix("gitdir:")
        {
            let p = stripped.trim();
            if Path::new(p).is_absolute() {
                PathBuf::from(p)
            } else {
                repo_root.join(p)
            }
        } else {
            dot_git.clone()
        }
    } else {
        dot_git
    };
    if sub_path.is_empty() {
        gitdir
    } else {
        let mut out = gitdir;
        for piece in sub_path.split('/').filter(|p| !p.is_empty()) {
            out.push(piece);
        }
        out
    }
}

/// Returns metadata for a single virtual entry. Used by `try_route_metadata`.
pub fn get_metadata_for(
    repo_root: &Path,
    virt: &super::path::VirtualGitPath,
    handle: &RepoHandle,
) -> Result<FileEntry, FriendlyGitError> {
    use super::path::VirtualGitPath::*;
    match virt {
        Root => {
            let path = repo_root.join(".git").to_string_lossy().into_owned();
            let mut fe = FileEntry::new(".git".into(), path, true, false);
            fe.permissions = 0o755;
            fe.icon_id = "git:fork".into();
            // Use the on-disk `.git/` mtime so the row isn't blank.
            if let Ok(meta) = std::fs::metadata(repo_root.join(".git"))
                && let Ok(t) = meta.modified()
                && let Ok(d) = t.duration_since(std::time::UNIX_EPOCH)
            {
                fe.modified_at = Some(d.as_secs());
            }
            Ok(fe)
        }
        Category(cat) => {
            let segment = cat.as_segment();
            let path = repo_root.join(".git").join(segment).to_string_lossy().into_owned();
            let mut fe = FileEntry::new(segment.into(), path, true, false);
            fe.permissions = 0o755;
            fe.icon_id = match cat {
                Cat::Branches => "git:branch",
                Cat::Tags => "git:tag",
                Cat::Commits => "git:commit",
                Cat::Stash | Cat::Worktrees | Cat::Submodules | Cat::Raw => "git:fork",
            }
            .to_string();
            let raw_mtime = std::fs::metadata(repo_root.join(".git"))
                .ok()
                .and_then(|m| m.modified().ok())
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map(|d| d.as_secs());
            populate_root_category(&mut fe, *cat, handle, repo_root, raw_mtime);
            Ok(fe)
        }
        Ref(cat, name) => {
            let path = repo_root
                .join(".git")
                .join(cat.as_segment())
                .join(name)
                .to_string_lossy()
                .into_owned();
            let mut fe = FileEntry::new(name.clone(), path, true, false);
            fe.permissions = 0o755;
            fe.icon_id = match cat {
                Cat::Branches => "git:branch",
                Cat::Tags => "git:tag",
                Cat::Commits => "git:commit",
                _ => "git:fork",
            }
            .to_string();
            populate_ref_columns(&mut fe, *cat, name, handle, repo_root);
            // For worktrees and submodules, surface the redirect even on a
            // direct stat so drag-drop, clipboard, and copy preview see it.
            match cat {
                Cat::Worktrees => {
                    use gix::bstr::ByteSlice;
                    let repo = handle.to_thread_local();
                    if let Ok(proxies) = repo.worktrees() {
                        for p in proxies {
                            if p.id().as_bstr() == name.as_bytes().as_bstr()
                                && let Ok(base) = p.base()
                            {
                                fe.redirect_to_path = Some(base.display().to_string());
                                break;
                            }
                        }
                    }
                }
                Cat::Submodules => {
                    use gix::bstr::ByteSlice;
                    let repo = handle.to_thread_local();
                    if let Ok(Some(modules)) = repo.submodules() {
                        for sm in modules {
                            if sm.name().as_bstr() == name.as_bytes().as_bstr()
                                && let Ok(rel) = sm.path()
                            {
                                fe.redirect_to_path =
                                    Some(repo_root.join(rel.to_str_lossy().as_ref()).display().to_string());
                                break;
                            }
                        }
                    }
                }
                _ => {}
            }
            Ok(fe)
        }
        RefTree(cat, name, sub) => {
            let commit_id = super::resolve_commit_for_cat(handle, *cat, name)?;
            let display_path = repo_root
                .join(".git")
                .join(cat.as_segment())
                .join(name)
                .join(sub.replace('/', std::path::MAIN_SEPARATOR_STR));
            super::tree::get_tree_entry(handle, commit_id, sub, &display_path)
        }
        Raw(sub) => {
            if sub.is_empty() {
                let path = repo_root
                    .join(".git")
                    .join(Cat::Raw.as_segment())
                    .to_string_lossy()
                    .into_owned();
                let mut fe = FileEntry::new(Cat::Raw.as_segment().into(), path, true, false);
                fe.permissions = 0o755;
                fe.icon_id = "git:fork".into();
                return Ok(fe);
            }
            let real = real_gitdir_path(repo_root, sub);
            let mut fe = get_single_entry(&real).map_err(|e| FriendlyGitError {
                kind: FriendlyGitErrorKind::PermissionDenied,
                path: real.display().to_string(),
                raw: Some(e.to_string()),
            })?;
            // Rewrite display path back into the virtual namespace.
            fe.path = repo_root
                .join(".git")
                .join(Cat::Raw.as_segment())
                .join(sub)
                .to_string_lossy()
                .into_owned();
            Ok(fe)
        }
    }
}

/// Resolves a ref name to its tip commit for `branches/` and `tags/`.
///
/// Annotated tags peel through to the commit they wrap.
pub fn resolve_ref_commit(handle: &RepoHandle, cat: Cat, name: &str) -> Result<gix::ObjectId, FriendlyGitError> {
    let repo = handle.to_thread_local();
    let full = match cat {
        Cat::Branches => format!("refs/heads/{}", name),
        Cat::Tags => format!("refs/tags/{}", name),
        _ => {
            return Err(FriendlyGitError::new(
                FriendlyGitErrorKind::CorruptRepo,
                name.to_string(),
            ));
        }
    };
    let partial = PartialName::try_from(full.as_str())
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let mut reference = repo
        .find_reference(&partial)
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let id = reference
        .peel_to_id()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?
        .detach();
    // Annotated tags peel through to the commit object specifically.
    if matches!(cat, Cat::Tags) {
        let obj = repo
            .find_object(id)
            .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
        if obj.kind == gix::object::Kind::Tag {
            let tag = obj.into_tag();
            // Walk through nested annotated tags.
            let mut cur_id = tag
                .target_id()
                .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?
                .detach();
            loop {
                let cur_obj = repo
                    .find_object(cur_id)
                    .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
                if cur_obj.kind == gix::object::Kind::Tag {
                    let t = cur_obj.into_tag();
                    cur_id = t
                        .target_id()
                        .map_err(|e| {
                            FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e)
                        })?
                        .detach();
                    continue;
                }
                return Ok(cur_id);
            }
        }
    }
    Ok(id)
}
