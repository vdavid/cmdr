//! Virtual `.git/` listings.
//!
//! - `list_root` – the portal root: real `.git/*` entries (HEAD, config, hooks/, objects/, refs/,
//!   etc.) followed by the six virtual category entries (`branches/`, `tags/`, `commits/`,
//!   `stash/`, `worktrees/`, `submodules/`).
//! - `list_branches` / `list_tags` – refs as virtual dirs
//!
//! Real `.git/*` entries that aren't the root listing fall through to the
//! real-FS code path via the volume hook returning `None` for non-virtual
//! paths.
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

use super::Lookup;
use super::column_meta::{
    ahead_behind_for_branch, commit_meta, files_changed_count, head_commit_secs, newest_branch_tip_secs,
    newest_tag_secs, tag_or_commit_secs,
};
use super::friendly::{FriendlyGitError, FriendlyGitErrorKind};
use super::path::{Cat, strip_ref_prefix};
use super::repo::RepoHandle;
use cmdr_fs::git_meta::{GitCountKind, GitEntryMeta};

/// Lists the portal root: real `.git/*` entries first, virtual category
/// entries after.
///
/// Real entries come from a direct `std::fs::read_dir` on the resolved
/// gitdir (handles linked-worktree gitlinks). They sort dirs-first,
/// alphabetical, matching the listing pipeline's default. Then the six
/// virtual categories (`branches/`, `tags/`, `commits/`, `stash/`,
/// `worktrees/`, `submodules/`) append in fixed order.
///
/// Real entries whose name collides with a virtual category get filtered
/// out – the virtual entry wins. In practice this hides the deprecated
/// real `.git/branches/` directory (git itself stopped using it long ago)
/// and the `.git/worktrees/` directory in linked-worktree setups (its
/// internals belong to git, not to the user). Power users who really
/// want the raw bytes can open the gitdir from the terminal.
///
/// Modified + Size columns are populated per category. See `column_meta`
/// for the rules. Empty categories still show up – opening them lists
/// nothing, which is more honest than hiding the concept altogether.
pub fn list_root(handle: &RepoHandle, repo_root: &Path) -> Vec<FileEntry> {
    let virtual_names: std::collections::HashSet<&'static str> = Cat::ALL.iter().map(|c| c.as_segment()).collect();

    let mut out = read_real_dot_git(repo_root);
    out.retain(|fe| !virtual_names.contains(fe.name.as_str()));
    out.sort_by(|a, b| match (a.is_directory, b.is_directory) {
        (true, false) => std::cmp::Ordering::Less,
        (false, true) => std::cmp::Ordering::Greater,
        _ => a.name.to_lowercase().cmp(&b.name.to_lowercase()),
    });

    out.extend(list_categories(handle, repo_root));
    out
}

/// The six virtual category rows on their own, in display order, with their
/// Modified and Size cells filled in.
///
/// This is what a [`GitPortalVolume`](super::volume::GitPortalVolume) lists at
/// its own root: the volume serves the virtual namespace and nothing else, so
/// the real `.git/*` entries are the parent volume's to list.
pub fn list_categories(handle: &RepoHandle, repo_root: &Path) -> Vec<FileEntry> {
    let dot_git = repo_root.join(".git");
    Cat::ALL
        .into_iter()
        .map(|cat| {
            let segment = cat.as_segment();
            let path = dot_git.join(segment).to_string_lossy().into_owned();
            let mut fe = FileEntry::new(segment.to_string(), path, true, false);
            fe.permissions = 0o755;
            fe.icon_id = icon_for_category(cat).to_string();
            populate_root_category(&mut fe, cat, handle, repo_root);
            fe
        })
        .collect()
}

/// The icon every row for `cat` carries, wherever it's built.
fn icon_for_category(cat: Cat) -> &'static str {
    match cat {
        Cat::Branches => "git:branch",
        Cat::Tags => "git:tag",
        Cat::Commits => "git:commit",
        Cat::Stash | Cat::Worktrees | Cat::Submodules => "git:fork",
    }
}

/// Reads the real on-disk gitdir for the portal root listing. Bypasses
/// the volume hook (`std::fs` directly) to avoid recursing back into
/// `git::try_route_listing`. Returns an empty Vec on any I/O hiccup;
/// the virtual entries below carry the conceptual structure regardless.
fn read_real_dot_git(repo_root: &Path) -> Vec<FileEntry> {
    let gitdir = real_gitdir_path(repo_root);
    let Ok(read) = std::fs::read_dir(&gitdir) else {
        return Vec::new();
    };
    let dot_git = repo_root.join(".git");
    let mut out = Vec::new();
    for entry in read.flatten() {
        let abs = entry.path();
        let Ok(mut fe) = get_single_entry(&abs) else {
            continue;
        };
        // Display under `<repo>/.git/<name>` so URLs stay anchored at the
        // worktree (and so navigation into a linked-worktree gitdir's
        // `.git/HEAD` keeps the worktree-rooted form). For non-gitlink
        // worktrees, this is identical to `abs`.
        fe.path = dot_git.join(&fe.name).to_string_lossy().into_owned();
        out.push(fe);
    }
    out
}

fn populate_root_category(fe: &mut FileEntry, cat: Cat, handle: &RepoHandle, repo_root: &Path) {
    let repo = handle.to_thread_local();
    let (counted, count) = match cat {
        Cat::Branches => {
            fe.modified_at = newest_branch_tip_secs(handle);
            (GitCountKind::Branches, count_local_branches(&repo))
        }
        Cat::Tags => {
            fe.modified_at = newest_tag_secs(handle);
            (GitCountKind::Tags, count_tags(&repo))
        }
        Cat::Commits => {
            fe.modified_at = head_commit_secs(handle);
            (GitCountKind::Commits, count_commits_capped(&repo))
        }
        Cat::Stash => {
            fe.modified_at = newest_stash_secs(repo_root);
            let count = super::stash::list_stashes(repo_root)
                .map(|v| v.len() as u64)
                .unwrap_or(0);
            (GitCountKind::StashEntries, count)
        }
        Cat::Worktrees => {
            fe.modified_at = newest_worktree_head_secs(&repo);
            let count = super::worktrees::list_worktrees(handle, repo_root)
                .map(|v| v.len() as u64)
                .unwrap_or(0);
            (GitCountKind::LinkedWorktrees, count)
        }
        Cat::Submodules => {
            fe.modified_at = newest_submodule_secs(&repo, handle, repo_root);
            let count = super::submodules::list_submodules(handle, repo_root)
                .map(|v| v.len() as u64)
                .unwrap_or(0);
            (GitCountKind::Submodules, count)
        }
    };
    fe.size = Some(count);
    fe.git_meta = Some(GitEntryMeta::Count { counted, n: count });
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
            if let Ok(Some(id)) = resolve_ref_commit(handle, Cat::Branches, name) {
                if let Ok(meta) = commit_meta(&repo, id) {
                    fe.modified_at = u64::try_from(meta.committer_secs).ok();
                    fe.created_at = fe.modified_at;
                    fe.added_at = fe.modified_at;
                }
                if let Some(ab) = ahead_behind_for_branch(&repo, name, id) {
                    fe.size = Some(u64::from(ab.ahead));
                    fe.git_meta = Some(GitEntryMeta::AheadBehind {
                        ahead: ab.ahead,
                        behind: ab.behind,
                        vs: ab.vs,
                    });
                }
            }
        }
        Cat::Tags => {
            if let Ok(Some(id)) = resolve_ref_commit(handle, Cat::Tags, name) {
                if let Some(secs) = tag_or_commit_secs(&repo, id) {
                    fe.modified_at = u64::try_from(secs).ok();
                    fe.created_at = fe.modified_at;
                    fe.added_at = fe.modified_at;
                }
                fe.git_meta = Some(GitEntryMeta::TaggedCommit { id: id.to_string() });
            }
        }
        Cat::Commits => {
            if let Ok(id) = super::log::resolve_commit_id(handle, name) {
                if let Ok(meta) = commit_meta(&repo, id) {
                    fe.modified_at = u64::try_from(meta.committer_secs).ok();
                    fe.created_at = fe.modified_at;
                    fe.added_at = fe.modified_at;
                }
                if let Some(n) = files_changed_count(&repo, id) {
                    fe.size = Some(n);
                    fe.git_meta = Some(GitEntryMeta::Count {
                        counted: GitCountKind::FilesChanged,
                        n,
                    });
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
                fe.git_meta = found.git_meta;
            }
        }
        Cat::Worktrees => {
            if let Ok(entries) = super::worktrees::list_worktrees(handle, repo_root)
                && let Some(found) = entries.into_iter().find(|e| e.name == name)
            {
                fe.modified_at = found.modified_at;
                fe.created_at = found.created_at;
                fe.added_at = found.added_at;
                fe.git_meta = found.git_meta;
            }
        }
        Cat::Submodules => {
            if let Ok(entries) = super::submodules::list_submodules(handle, repo_root)
                && let Some(found) = entries.into_iter().find(|e| e.name == name)
            {
                fe.modified_at = found.modified_at;
                fe.created_at = found.created_at;
                fe.added_at = found.added_at;
                fe.git_meta = found.git_meta;
            }
        }
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
/// and a `git_meta` stating ahead/behind relative to the branch's upstream,
/// falling back to `main`/`master` for branches without a configured
/// upstream. The numeric `size` field carries the
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
                fe.git_meta = Some(GitEntryMeta::AheadBehind {
                    ahead: ab.ahead,
                    behind: ab.behind,
                    vs: ab.vs,
                });
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
            // The wrapped commit, whose short form the cell shows. Annotated
            // tags peel to their commit through gix's `peel_to_id` chain when
            // reading via `references()`, so `target_id` is the commit.
            fe.git_meta = Some(GitEntryMeta::TaggedCommit {
                id: target_id.to_string(),
            });
        }
        out.push(fe);
    }
    out.sort_by_key(|a| a.name.to_lowercase());
    Ok(out)
}

/// Resolves the actual on-disk gitdir for a worktree.
///
/// For a normal worktree the gitdir is `<root>/.git`. For a linked
/// worktree (gitlink), `<root>/.git` is a file pointing into
/// `<main>/.git/worktrees/<name>` – this helper follows that.
pub fn real_gitdir_path(repo_root: &Path) -> PathBuf {
    let dot_git = repo_root.join(".git");
    if dot_git.is_file()
        && let Ok(content) = std::fs::read_to_string(&dot_git)
        && let Some(stripped) = content.trim().strip_prefix("gitdir:")
    {
        let p = stripped.trim();
        if Path::new(p).is_absolute() {
            return PathBuf::from(p);
        }
        return repo_root.join(p);
    }
    dot_git
}

/// Returns metadata for a single virtual entry. Used by `try_route_metadata`.
pub fn get_metadata_for(
    repo_root: &Path,
    virt: &super::path::VirtualGitPath,
    handle: &RepoHandle,
) -> Lookup<FileEntry> {
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
            Ok(Some(fe))
        }
        Category(cat) => {
            let segment = cat.as_segment();
            let path = repo_root.join(".git").join(segment).to_string_lossy().into_owned();
            let mut fe = FileEntry::new(segment.into(), path, true, false);
            fe.permissions = 0o755;
            fe.icon_id = icon_for_category(*cat).to_string();
            populate_root_category(&mut fe, *cat, handle, repo_root);
            Ok(Some(fe))
        }
        Ref(cat, name) => {
            // A branch or tag that isn't in the repo is a miss, not a row: the
            // ref lookup is the cheap authoritative answer, and without this
            // gate `.git/branches/<typo>` stats as an existing directory.
            if matches!(cat, Cat::Branches | Cat::Tags) && resolve_ref_commit(handle, *cat, name)?.is_none() {
                return Ok(None);
            }
            let path = repo_root
                .join(".git")
                .join(cat.as_segment())
                .join(name)
                .to_string_lossy()
                .into_owned();
            let mut fe = FileEntry::new(name.clone(), path, true, false);
            fe.permissions = 0o755;
            fe.icon_id = icon_for_category(*cat).to_string();
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
            Ok(Some(fe))
        }
        RefTree(cat, name, sub) => {
            let Some(commit_id) = super::resolve_commit_for_cat(handle, *cat, name)? else {
                return Ok(None);
            };
            let display_path = repo_root
                .join(".git")
                .join(cat.as_segment())
                .join(name)
                .join(sub.replace('/', std::path::MAIN_SEPARATOR_STR));
            super::tree::get_tree_entry(handle, commit_id, sub, &display_path)
        }
    }
}

/// Resolves a ref name to its tip commit for `branches/` and `tags/`.
///
/// Annotated tags peel through to the commit they wrap.
pub fn resolve_ref_commit(handle: &RepoHandle, cat: Cat, name: &str) -> Lookup<gix::ObjectId> {
    let repo = handle.to_thread_local();
    let full = match cat {
        Cat::Branches => format!("refs/heads/{}", name),
        Cat::Tags => format!("refs/tags/{}", name),
        // No other category names a ref, so there is nothing here to find.
        Cat::Commits | Cat::Stash | Cat::Worktrees | Cat::Submodules => return Ok(None),
    };
    // A name git itself would reject (a stray `..`, a trailing `.lock`) names
    // no ref, so it's a miss rather than a damaged repo.
    let Ok(partial) = PartialName::try_from(full.as_str()) else {
        return Ok(None);
    };
    let mut reference = match repo.find_reference(&partial) {
        Ok(reference) => reference,
        // Typed, per gix: the ref simply isn't in this repo (a typo in the path
        // bar, a branch that only exists on a remote). Anything else is the odb
        // failing to answer, which stays an error.
        Err(gix::reference::find::existing::Error::NotFound { .. }) => return Ok(None),
        Err(e) => {
            return Err(FriendlyGitError::with_source(
                FriendlyGitErrorKind::CorruptRepo,
                e.to_string(),
                e,
            ));
        }
    };
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
                return Ok(Some(cur_id));
            }
        }
    }
    Ok(Some(id))
}
