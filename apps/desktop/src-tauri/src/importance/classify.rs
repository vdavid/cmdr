//! Pure path/name classifiers shared by the production signal-assembly
//! (`scheduler`/`signals`) and the test fixture generator (`fixtures`).
//!
//! These decide the categorical signals a folder's [`FolderSignals`] carries:
//! whether its name is denylisted, its path class, and whether it looks
//! hidden/system. Keeping them in ONE place is load-bearing: the fixtures doc
//! warns that the test stand-in and the real assembler "must agree on what each
//! signal means", and the only way to guarantee that is to share the code, not
//! re-derive it. All pure (values in, category out), so the classification is
//! unit-testable and matches between fixtures and production by construction.
//!
//! [`FolderSignals`]: super::scorer::FolderSignals

use super::scorer::PathClass;

/// The last path component (folder name). A path with no final component (the
/// root `/`) folds back to the whole string.
pub fn leaf_name(path: &str) -> String {
    std::path::Path::new(path)
        .file_name()
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_string())
}

/// Whether a folder name is on the known-unimportant denylist: a set-membership
/// check on the folded name against the project-wide system-dir exclude list
/// (`node_modules`, `.git`, caches, build output), never a substring match (the
/// `no-string-matching` rule). Reusing `search::SYSTEM_DIR_EXCLUDES` keeps
/// importance and search agreeing on what counts as machine output.
pub fn is_denylisted(name: &str) -> bool {
    let folded = name.to_lowercase();
    crate::search::SYSTEM_DIR_EXCLUDES
        .iter()
        .any(|d| d.to_lowercase() == folded)
}

/// Whether a folder is hidden or system-owned: a dotfile name, or a path that
/// classifies as [`PathClass::SystemOrCache`]. A FLOOR override in the scorer.
pub fn is_hidden_or_system(path: &str, name: &str, home: &str) -> bool {
    name.starts_with('.') || matches!(path_class(path, home), PathClass::SystemOrCache)
}

/// The project markers whose presence in a folder (or a descendant) marks it as
/// at/above a project root, raising the whole subtree (plan Decision 3). A
/// set-membership check on the folded child name.
pub const PROJECT_MARKERS: &[&str] = &[
    ".git",
    "cargo.toml",
    "package.json",
    "go.mod",
    "pyproject.toml",
    "pom.xml",
    "build.gradle",
    "makefile",
    ".hg",
    ".svn",
];

/// Whether a folded child name is a project marker.
pub fn is_project_marker(folded_child_name: &str) -> bool {
    PROJECT_MARKERS.contains(&folded_child_name)
}

/// Classify a path into its [`PathClass`] prior, relative to the user's home.
///
/// A `~/Library` subtree is `SystemOrCache` even under the home (it stays low);
/// `Downloads`/`Desktop`/`Documents` and their subtrees are `UserContent`;
/// everything else is `Neutral`. `ProjectRoot` is NOT decided here — it's set by
/// the project-marker signal at assembly time, since it depends on directory
/// contents, not the path alone.
pub fn path_class(path: &str, home: &str) -> PathClass {
    let library = format!("{home}/Library");
    if path == library || path.starts_with(&format!("{library}/")) {
        return PathClass::SystemOrCache;
    }
    for content in ["Downloads", "Desktop", "Documents"] {
        let root = format!("{home}/{content}");
        if path == root || path.starts_with(&format!("{root}/")) {
            return PathClass::UserContent;
        }
    }
    PathClass::Neutral
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn denylist_is_folded_set_membership() {
        assert!(is_denylisted("node_modules"));
        assert!(is_denylisted("NODE_MODULES"), "folded, so case doesn't matter");
        assert!(is_denylisted(".git"));
        assert!(!is_denylisted("my_project"));
        // Set-membership, not substring: a name CONTAINING a denylisted word isn't denylisted.
        assert!(!is_denylisted("node_modules_backup"));
    }

    #[test]
    fn path_class_matches_the_fixture_rules() {
        let home = "/Users/test";
        assert_eq!(path_class("/Users/test/Downloads", home), PathClass::UserContent);
        assert_eq!(
            path_class("/Users/test/Documents/invoices", home),
            PathClass::UserContent
        );
        assert_eq!(path_class("/Users/test/Library/Caches", home), PathClass::SystemOrCache);
        assert_eq!(path_class("/Users/test/projects/webapp", home), PathClass::Neutral);
    }

    #[test]
    fn hidden_or_system_covers_dotfiles_and_library() {
        let home = "/Users/test";
        assert!(is_hidden_or_system("/Users/test/.config", ".config", home));
        assert!(is_hidden_or_system("/Users/test/Library/Caches", "Caches", home));
        assert!(!is_hidden_or_system("/Users/test/Downloads", "Downloads", home));
    }

    #[test]
    fn project_markers_are_folded_membership() {
        assert!(is_project_marker(".git"));
        assert!(is_project_marker("cargo.toml"));
        assert!(is_project_marker("package.json"));
        assert!(!is_project_marker("readme.md"));
    }
}
