//! Virtual `.git` portal path classification.
//!
//! Maps an on-disk absolute path to a `VirtualGitPath` describing what the
//! user wants to see: the portal root, a category (`branches/`, `tags/`,
//! `commits/`, `stash/`, `worktrees/`, `submodules/`), a specific ref, or
//! a sub-path inside a ref's tree.
//!
//! Real `.git/*` entries (`HEAD`, `config`, `hooks/`, `objects/`, etc.)
//! aren't classified here – they fall through to the real-FS code path
//! via the volume hook returning `None`. The portal root listing
//! (`virtual_listing::list_root`) merges the real entries with the
//! virtual categories so the user sees both.
//!
//! ## Why ref names render flat
//!
//! Ref names like `feature/foo` contain `/`. We render them flat – the ref
//! list shows one entry called `feature/foo`, not nested `feature/` then
//! `foo`. Sub-paths inside a ref's tree (the actual file tree at the tip)
//! still render as a normal hierarchy. The classifier handles this by
//! greedy-matching ref names against the repo's known refs before treating
//! any remainder as a tree sub-path.

use std::path::{Component, Path, PathBuf};

use super::repo::RepoHandle;

/// Top-level categories under `.git/`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Cat {
    Branches,
    Tags,
    Commits,
    Stash,
    Worktrees,
    Submodules,
}

impl Cat {
    /// Path segment used in the URL form (`.git/<segment>/...`).
    pub fn as_segment(&self) -> &'static str {
        match self {
            Cat::Branches => "branches",
            Cat::Tags => "tags",
            Cat::Commits => "commits",
            Cat::Stash => "stash",
            Cat::Worktrees => "worktrees",
            Cat::Submodules => "submodules",
        }
    }

    fn from_segment(s: &str) -> Option<Self> {
        match s {
            "branches" => Some(Cat::Branches),
            "tags" => Some(Cat::Tags),
            "commits" => Some(Cat::Commits),
            "stash" => Some(Cat::Stash),
            "worktrees" => Some(Cat::Worktrees),
            "submodules" => Some(Cat::Submodules),
            _ => None,
        }
    }

    /// All six virtual categories, in the fixed display order used by
    /// `virtual_listing::list_root` and the post-toggle invalidation set.
    pub const ALL: [Cat; 6] = [
        Cat::Branches,
        Cat::Tags,
        Cat::Commits,
        Cat::Stash,
        Cat::Worktrees,
        Cat::Submodules,
    ];

    /// True for categories whose `Ref(_, name)` resolves to a *commit
    /// tree* the user can browse: branches, tags, commits, stash. The
    /// other categories (`worktrees`, `submodules`) emit a redirect
    /// instead of a sub-tree.
    pub fn browses_commit_tree(&self) -> bool {
        matches!(self, Cat::Branches | Cat::Tags | Cat::Commits | Cat::Stash)
    }
}

/// Classified virtual git path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VirtualGitPath {
    /// `.git/` itself – shows the portal entries plus real `.git/*` files.
    Root,
    /// `.git/<category>/` – the category landing page (also a "ref list" for
    /// branches and tags). Kept distinct so categories with their own
    /// shape (`commits/<sha>/`) reuse the same parser entry point.
    Category(Cat),
    /// `.git/<category>/<ref>` – a specific ref / sha / stash entry.
    Ref(Cat, String),
    /// `.git/<category>/<ref>/<sub_path>` – a sub-path inside a ref's tree.
    /// `sub_path` uses forward slashes, never starts with `/`.
    RefTree(Cat, String, String),
}

impl VirtualGitPath {
    /// Convenience: the category for category-shaped variants. `None` for `Root`.
    #[allow(dead_code, reason = "Public helper for downstream IPC consumers")]
    pub fn category(&self) -> Option<Cat> {
        match self {
            VirtualGitPath::Root => None,
            VirtualGitPath::Category(c) => Some(*c),
            VirtualGitPath::Ref(c, _) => Some(*c),
            VirtualGitPath::RefTree(c, _, _) => Some(*c),
        }
    }
}

/// Cheap shape check: does this path live under any worktree's `.git/` dir?
///
/// Walks `path`'s ancestors looking for a `.git` segment. We don't open a
/// repo here – the volume hooks call this on every method invocation so it
/// has to be fast. Repo discovery happens later, only for paths that
/// actually need it.
pub fn is_virtual(path: &Path) -> bool {
    path.components().any(|c| match c {
        Component::Normal(s) => s == ".git",
        _ => false,
    })
}

/// Discovers the worktree root containing `path`, then classifies the rest
/// of the path against the repo's refs.
///
/// Returns `None` when:
/// - The path isn't inside any `.git/` (the caller should run real-FS code).
/// - We can't open the repo (broken `.git`, permission denied, etc.).
/// - The path points at a real `.git/*` entry that isn't a virtual category
///   (`HEAD`, `config`, `hooks/`, `objects/`, etc.). The volume hook then
///   falls through to the real-FS code path.
///
/// Errors are surfaced via the friendly-error path on actual operations,
/// not here – this function is a router.
pub fn classify(path: &Path) -> Option<(VirtualGitPath, RepoHandle, PathBuf)> {
    let (worktree_root, after_dot_git) = split_at_dot_git(path)?;
    let (handle, canonical_root) = super::repo::discover_repo(&worktree_root).ok()?;

    let parsed = parse_after_dot_git(&after_dot_git, &handle)?;
    Some((parsed, handle, canonical_root))
}

/// Turns a `VirtualGitPath` back into the absolute path used in URLs.
///
/// `repo_root` must be the canonical worktree root (as returned by
/// `discover_repo`). The resulting path is `<root>/.git/<...>`.
#[allow(
    dead_code,
    reason = "Inverse of classify; used by tests + future IPC consumers (link copying, drag-drop)"
)]
pub fn to_path(virt: &VirtualGitPath, repo_root: &Path) -> PathBuf {
    let mut out = repo_root.join(".git");
    match virt {
        VirtualGitPath::Root => {}
        VirtualGitPath::Category(cat) => {
            out.push(cat.as_segment());
        }
        VirtualGitPath::Ref(cat, name) => {
            out.push(cat.as_segment());
            out.push(name);
        }
        VirtualGitPath::RefTree(cat, name, sub) => {
            out.push(cat.as_segment());
            out.push(name);
            // Push each segment so OS-native separators are used.
            for piece in sub.split('/').filter(|p| !p.is_empty()) {
                out.push(piece);
            }
        }
    }
    out
}

/// Splits a path at its first `.git` segment, returning `(worktree_root, rest_after_dot_git)`.
///
/// `rest_after_dot_git` is empty for `<root>/.git` itself. The returned
/// worktree root is `path` truncated at (excluding) the `.git` component;
/// for `<root>/.git/branches/main/src/foo.rs` we return `<root>` and
/// `["branches", "main", "src", "foo.rs"]` (as a Vec of OsString).
fn split_at_dot_git(path: &Path) -> Option<(PathBuf, Vec<String>)> {
    let mut comps = Vec::new();
    let mut after = Vec::new();
    let mut hit = false;
    for c in path.components() {
        if hit {
            if let Component::Normal(s) = c {
                after.push(s.to_string_lossy().into_owned());
            } else {
                // Shouldn't happen for well-formed paths after `.git`,
                // but handle defensively.
                return None;
            }
            continue;
        }
        match c {
            Component::Normal(s) if s == ".git" => {
                hit = true;
            }
            _ => comps.push(c),
        }
    }
    if !hit {
        return None;
    }
    let mut root = PathBuf::new();
    for c in comps {
        match c {
            Component::RootDir => root.push("/"),
            Component::Prefix(p) => root.push(p.as_os_str()),
            Component::Normal(s) => root.push(s),
            Component::CurDir | Component::ParentDir => {}
        }
    }
    Some((root, after))
}

/// Parses the segments after `.git/` against the repo's refs.
///
/// Returns `None` for paths that point at real `.git/*` entries (anything
/// whose first segment isn't a virtual category). The volume hook then
/// returns `None` so the caller falls through to real-FS reads.
fn parse_after_dot_git(segments: &[String], handle: &RepoHandle) -> Option<VirtualGitPath> {
    if segments.is_empty() {
        return Some(VirtualGitPath::Root);
    }

    let cat_seg = &segments[0];
    let cat = Cat::from_segment(cat_seg)?;

    let rest = &segments[1..];
    if rest.is_empty() {
        return Some(VirtualGitPath::Category(cat));
    }

    // Greedy-match ref name against the repo's known refs for branches/tags.
    // For all other categories the first segment is the entry name (a SHA
    // for `commits/`, an index for `stash/`, a worktree/submodule name for
    // `worktrees/` and `submodules/`).
    if matches!(cat, Cat::Branches | Cat::Tags) {
        let known = ref_names_for_cat(handle, cat);
        if let Some((ref_name, sub)) = match_ref_name(rest, &known) {
            return Some(if sub.is_empty() {
                VirtualGitPath::Ref(cat, ref_name)
            } else {
                VirtualGitPath::RefTree(cat, ref_name, sub)
            });
        }
    }

    // Default shape: first segment = entry, remainder = sub-path.
    let entry = rest[0].clone();
    let sub = rest[1..].join("/");
    Some(if sub.is_empty() {
        VirtualGitPath::Ref(cat, entry)
    } else {
        VirtualGitPath::RefTree(cat, entry, sub)
    })
}

fn match_ref_name(segments: &[String], known: &[String]) -> Option<(String, String)> {
    // Try the longest possible match first so `feature/foo` wins over `feature`.
    for cut in (1..=segments.len()).rev() {
        let candidate = segments[..cut].join("/");
        if known.iter().any(|n| n == &candidate) {
            let sub = segments[cut..].join("/");
            return Some((candidate, sub));
        }
    }
    None
}

fn ref_names_for_cat(handle: &RepoHandle, cat: Cat) -> Vec<String> {
    let repo = handle.to_thread_local();
    let Ok(platform) = repo.references() else {
        return Vec::new();
    };
    let iter = match cat {
        Cat::Branches => platform.local_branches().ok(),
        Cat::Tags => platform.tags().ok(),
        Cat::Commits | Cat::Stash | Cat::Worktrees | Cat::Submodules => return Vec::new(),
    };
    let Some(iter) = iter else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for r in iter.flatten() {
        let full = r.name().as_bstr().to_string();
        let short = strip_ref_prefix(&full, cat);
        if !short.is_empty() {
            out.push(short);
        }
    }
    out
}

pub(crate) fn strip_ref_prefix(full: &str, cat: Cat) -> String {
    match cat {
        Cat::Branches => full.strip_prefix("refs/heads/").unwrap_or(full).to_string(),
        Cat::Tags => full.strip_prefix("refs/tags/").unwrap_or(full).to_string(),
        Cat::Commits | Cat::Stash | Cat::Worktrees | Cat::Submodules => full.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn is_virtual_detects_dot_git_anywhere() {
        assert!(is_virtual(Path::new("/tmp/repo/.git")));
        assert!(is_virtual(Path::new("/tmp/repo/.git/branches/main")));
        assert!(is_virtual(Path::new("/tmp/repo/.git/HEAD")));
        assert!(!is_virtual(Path::new("/tmp/repo/src/main.rs")));
        assert!(!is_virtual(Path::new("/tmp/repo")));
    }

    #[test]
    fn split_at_dot_git_works_for_root() {
        let (root, rest) = split_at_dot_git(Path::new("/tmp/repo/.git")).unwrap();
        assert_eq!(root, Path::new("/tmp/repo"));
        assert!(rest.is_empty());
    }

    #[test]
    fn split_at_dot_git_works_for_nested() {
        let (root, rest) = split_at_dot_git(Path::new("/tmp/repo/.git/branches/feature/foo/src/main.rs")).unwrap();
        assert_eq!(root, Path::new("/tmp/repo"));
        assert_eq!(
            rest,
            vec!["branches", "feature", "foo", "src", "main.rs"]
                .into_iter()
                .map(String::from)
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn split_returns_none_without_dot_git() {
        assert!(split_at_dot_git(Path::new("/tmp/repo/src")).is_none());
    }

    #[test]
    fn cat_segment_round_trip() {
        for cat in Cat::ALL {
            let s = cat.as_segment();
            assert_eq!(Cat::from_segment(s), Some(cat));
        }
    }

    #[test]
    fn cat_from_segment_rejects_real_dot_git_entries() {
        // These are real `.git/*` names. They must not classify as virtual.
        for name in ["HEAD", "config", "hooks", "info", "objects", "refs", "raw", "logs"] {
            assert_eq!(Cat::from_segment(name), None, "{} should not be a virtual cat", name);
        }
    }

    #[test]
    fn match_ref_name_picks_longest() {
        let known = vec!["feature".to_string(), "feature/foo".to_string()];
        let segs = vec!["feature".into(), "foo".into(), "src".into()];
        let (name, sub) = match_ref_name(&segs, &known).unwrap();
        assert_eq!(name, "feature/foo");
        assert_eq!(sub, "src");
    }

    #[test]
    fn match_ref_name_handles_no_subpath() {
        let known = vec!["main".to_string()];
        let segs = vec!["main".into()];
        let (name, sub) = match_ref_name(&segs, &known).unwrap();
        assert_eq!(name, "main");
        assert_eq!(sub, "");
    }

    #[test]
    fn match_ref_name_returns_none_when_unknown() {
        let known = vec!["main".to_string()];
        let segs = vec!["other".into(), "src".into()];
        assert!(match_ref_name(&segs, &known).is_none());
    }

    #[test]
    fn to_path_round_trips_root() {
        let root = Path::new("/repo");
        assert_eq!(to_path(&VirtualGitPath::Root, root), Path::new("/repo/.git"));
    }

    #[test]
    fn to_path_round_trips_category() {
        let root = Path::new("/repo");
        assert_eq!(
            to_path(&VirtualGitPath::Category(Cat::Branches), root),
            Path::new("/repo/.git/branches")
        );
    }

    #[test]
    fn to_path_round_trips_ref_with_slashes() {
        let root = Path::new("/repo");
        assert_eq!(
            to_path(&VirtualGitPath::Ref(Cat::Branches, "feature/foo".into()), root),
            Path::new("/repo/.git/branches/feature/foo")
        );
    }

    #[test]
    fn to_path_round_trips_ref_tree() {
        let root = Path::new("/repo");
        let v = VirtualGitPath::RefTree(Cat::Branches, "main".into(), "src/lib.rs".into());
        assert_eq!(to_path(&v, root), Path::new("/repo/.git/branches/main/src/lib.rs"));
    }
}
