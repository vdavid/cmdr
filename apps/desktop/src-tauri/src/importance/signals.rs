//! Assemble a folder's [`FolderSignals`] from the drive index — the production
//! counterpart to the test fixture's `signals_for`.
//!
//! The scheduler walks a volume's index tree and, for each directory, calls
//! [`signals_for_dir`] with that directory's row, its direct children, its
//! reconstructed path, the user's home, and the optional visit/last-used inputs.
//! The categorical signals (denylist, path class, project marker, hidden) come
//! from the shared [`classify`](super::classify) module, so the production path
//! and the fixtures can't drift on what a signal means.
//!
//! Pure: values in (an entry row, its children, a path), a [`FolderSignals`] out.
//! No I/O — the caller reads the index; this only classifies.

use super::classify::{is_denylisted, is_hidden_or_system, leaf_name, path_class};
use super::scorer::{FolderSignals, PathClass};
use crate::indexing::store::EntryRow;

/// The optional, backend-dependent signals for a folder, resolved by the caller
/// from `importance.db`'s visit table and (macOS-local) Spotlight sampling.
/// Passed in so this stays pure and the scorer's `SignalSet` availability is set
/// by the caller per volume kind.
#[derive(Debug, Clone, Copy, Default)]
pub struct OptionalSignals {
    /// Navigation-visit count for this folder, if the visit signal is available.
    pub visit_count: Option<u32>,
    /// Sampled `kMDItemLastUsedDate` (Unix seconds), if sampled.
    pub last_used_secs: Option<u64>,
}

/// The listing-derived counts a folder's direct children collapse to. The
/// recompute walk builds these per directory by streaming file rows into a small
/// per-parent accumulator (so the whole entries table is never resident — the
/// O(dirs) memory shape), instead of handing this function a `Vec` of every child
/// row. All that reaches the signals are these three scalars.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChildAggregate {
    /// Distinct file extensions among the direct file children (folded), the
    /// extension-diversity input.
    pub distinct_extension_count: u32,
    /// Direct file-child count (directories excluded), the extension-count input.
    pub file_count: u32,
    /// Whether any direct child (file OR directory — `.git`/`.hg`/`.svn` are
    /// directories, `Cargo.toml`/`package.json` are files) is a project marker.
    pub has_direct_marker: bool,
}

/// Build the [`FolderSignals`] for one directory.
///
/// `dir` is the directory's own entry row (for its mtime); `children` is the
/// pre-aggregated summary of its direct children (extension diversity + count +
/// direct-marker flag — the walk folds each child into this so no child rows are
/// held); `path` is the directory's reconstructed absolute path; `home` is the
/// user's home dir for path classification. `has_marker_below` lets the caller
/// raise a folder whose project marker sits in a DESCENDANT (a `.git` deeper in
/// the subtree still marks the root, plan Decision 3) — `children.has_direct_marker`
/// handles the marker-in-this-folder case.
pub fn signals_for_dir(
    dir: &EntryRow,
    children: ChildAggregate,
    path: &str,
    home: &str,
    has_marker_below: bool,
    optional: OptionalSignals,
) -> FolderSignals {
    let name = leaf_name(path);
    let name_denylisted = is_denylisted(&name);
    let hidden_or_system = is_hidden_or_system(path, &name, home);

    let has_project_marker = children.has_direct_marker || has_marker_below;

    // A folder with a project marker (here or below) reads as a project root; its
    // path-class prior is raised to `ProjectRoot`, the strongest prior. Otherwise
    // the path alone classifies it.
    let path_class = if has_project_marker {
        PathClass::ProjectRoot
    } else {
        path_class(path, home)
    };

    FolderSignals {
        name_denylisted,
        hidden_or_system,
        distinct_extension_count: children.distinct_extension_count,
        file_count: children.file_count,
        mtime_secs: dir.modified_at,
        has_project_marker,
        path_class,
        visit_count: optional.visit_count,
        last_used_secs: optional.last_used_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn dir(id: i64, name: &str, mtime: Option<u64>) -> EntryRow {
        EntryRow {
            id,
            parent_id: 1,
            name: name.to_string(),
            is_directory: true,
            is_symlink: false,
            logical_size: None,
            physical_size: None,
            modified_at: mtime,
            inode: None,
        }
    }

    #[test]
    fn a_node_modules_dir_is_denylisted() {
        let d = dir(2, "node_modules", Some(100));
        let s = signals_for_dir(
            &d,
            ChildAggregate {
                distinct_extension_count: 1,
                file_count: 1,
                has_direct_marker: false,
            },
            "/Users/me/proj/node_modules",
            "/Users/me",
            false,
            Default::default(),
        );
        assert!(s.name_denylisted, "a node_modules folder is denylisted by name");
    }

    #[test]
    fn a_direct_marker_marks_the_folder_a_project_root() {
        let d = dir(2, "proj", Some(100));
        // The walk found a `.git` (dir) or `Cargo.toml` (file) among the children.
        let children = ChildAggregate {
            distinct_extension_count: 1,
            file_count: 1,
            has_direct_marker: true,
        };
        let s = signals_for_dir(&d, children, "/Users/me/proj", "/Users/me", false, Default::default());
        assert!(s.has_project_marker, "a direct marker child marks a project root");
        assert_eq!(
            s.path_class,
            PathClass::ProjectRoot,
            "a project root gets the ProjectRoot prior"
        );
    }

    #[test]
    fn a_marker_below_still_raises_the_folder() {
        let d = dir(2, "proj", Some(100));
        // No marker among the direct children, but the caller found one below.
        let s = signals_for_dir(
            &d,
            ChildAggregate {
                distinct_extension_count: 1,
                file_count: 1,
                has_direct_marker: false,
            },
            "/Users/me/proj",
            "/Users/me",
            true,
            Default::default(),
        );
        assert!(s.has_project_marker, "a marker in a descendant still raises the folder");
    }

    #[test]
    fn extension_diversity_and_count_come_from_the_aggregate() {
        let d = dir(2, "mixed", Some(100));
        let children = ChildAggregate {
            distinct_extension_count: 3,
            file_count: 3,
            has_direct_marker: false,
        };
        let s = signals_for_dir(&d, children, "/Users/me/mixed", "/Users/me", false, Default::default());
        assert_eq!(s.file_count, 3, "the aggregated file count carries through");
        assert_eq!(s.distinct_extension_count, 3, "three distinct extensions");
    }

    #[test]
    fn optional_signals_pass_through() {
        let d = dir(2, "docs", Some(100));
        let opt = OptionalSignals {
            visit_count: Some(5),
            last_used_secs: Some(999),
        };
        let s = signals_for_dir(
            &d,
            ChildAggregate::default(),
            "/Users/me/Documents/docs",
            "/Users/me",
            false,
            opt,
        );
        assert_eq!(s.visit_count, Some(5));
        assert_eq!(s.last_used_secs, Some(999));
        assert_eq!(s.path_class, PathClass::UserContent, "under Documents ⇒ user content");
    }
}
