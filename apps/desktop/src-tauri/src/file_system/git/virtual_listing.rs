//! Virtual `.git/` listings.
//!
//! - `list_root` — the portal root (M2 ships `branches/`, `tags/`, `raw/`)
//! - `list_branches` / `list_tags` — refs as virtual dirs
//! - `list_raw` — passthrough into the real on-disk `.git/<sub>` contents
//!
//! These return `Vec<FileEntry>` because the existing `Volume::list_directory`
//! contract is single-shot. The underlying gix iterators are fast enough
//! (< 50 ms even on 10k branches) that streaming inside this layer doesn't
//! add value yet — cancellation for the surrounding listing pipeline still
//! works because the volume hook runs inside the listing's `spawn_blocking`
//! task, which the listing module aborts on cancel.

use std::path::{Path, PathBuf};

use gix::refs::PartialName;

use crate::file_system::listing::FileEntry;
use crate::file_system::listing::reading::get_single_entry;

use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::path::{Cat, strip_ref_prefix};
use super::repo::RepoHandle;

/// Lists the categories visible at the portal root.
///
/// All seven categories are listed: M2 shipped `branches/`, `tags/`,
/// `raw/`; M3 added `commits/`, `stash/`, `worktrees/`, `submodules/`.
/// Empty categories (no commits, no stashes) still show up — opening
/// them shows an empty listing, which is more honest than hiding the
/// concept altogether.
pub fn list_root(repo_root: &Path) -> Vec<FileEntry> {
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

    categories
        .into_iter()
        .map(|(cat, icon)| {
            let segment = cat.as_segment();
            let path = dot_git.join(segment).to_string_lossy().into_owned();
            let mut fe = FileEntry::new(segment.to_string(), path, true, false);
            fe.permissions = 0o755;
            fe.icon_id = icon.to_string();
            fe
        })
        .collect()
}

/// Lists local branches as virtual directory entries.
pub fn list_branches(handle: &RepoHandle, repo_root: &Path) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let parent = repo_root.join(".git").join(Cat::Branches.as_segment());
    list_refs_under(handle, &parent, Cat::Branches, "git:branch")
}

/// Lists tags as virtual directory entries.
///
/// Annotated tags resolve through their tag object to the underlying
/// commit at navigation time (in `tree::resolve_tree_at`), so this
/// listing only carries the ref names themselves.
pub fn list_tags(handle: &RepoHandle, repo_root: &Path) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let parent = repo_root.join(".git").join(Cat::Tags.as_segment());
    list_refs_under(handle, &parent, Cat::Tags, "git:tag")
}

fn list_refs_under(
    handle: &RepoHandle,
    parent: &Path,
    cat: Cat,
    icon: &str,
) -> Result<Vec<FileEntry>, FriendlyGitError> {
    let repo = handle.to_thread_local();
    let platform = repo
        .references()
        .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?;
    let iter = match cat {
        Cat::Branches => platform
            .local_branches()
            .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?,
        Cat::Tags => platform
            .tags()
            .map_err(|e| FriendlyGitError::with_source(FriendlyGitErrorKind::CorruptRepo, e.to_string(), e))?,
        _ => return Ok(Vec::new()),
    };

    let mut out = Vec::new();
    for r in iter.flatten() {
        let full = r.name().as_bstr().to_string();
        let short = strip_ref_prefix(&full, cat);
        if short.is_empty() {
            continue;
        }
        let path = parent.join(&short).to_string_lossy().into_owned();
        let mut fe = FileEntry::new(short, path, true, false);
        fe.permissions = 0o755;
        fe.icon_id = icon.to_string();
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
/// `<main>/.git/worktrees/<name>` — this helper follows that.
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
