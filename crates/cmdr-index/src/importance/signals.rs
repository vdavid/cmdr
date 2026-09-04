//! Assemble a folder's [`FolderSignals`] from the drive index — the production
//! counterpart to the test fixture's `signals_for`.
//!
//! The scheduler walks a volume's index tree and, for each directory, calls
//! [`signals_for_dir`] with that directory's mtime, its aggregated children, its
//! reconstructed path, the user's home, and the optional visit/last-used inputs.
//! The categorical signals (denylist, path class, project marker, hidden) come
//! from the shared [`classify`](super::classify) module, so the production path
//! and the fixtures can't drift on what a signal means.
//!
//! Pure: values in (an mtime, a child aggregate, a path), a [`FolderSignals`] out.
//! No I/O — the caller reads the index; this only classifies.

use super::classify::{is_denylisted, is_hidden_or_system, leaf_name, path_class_with_marker};
use super::scorer::FolderSignals;

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
/// `mtime_secs` is the directory's own modification time, the ONLY column this
/// reads off its entry row (so the walk keeps that one field, not the row);
/// `children` is the pre-aggregated summary of its direct children (extension
/// diversity + count + direct-marker flag — the walk folds each child into this so
/// no child rows are held); `path` is the directory's reconstructed absolute path;
/// `home` is the user's home dir for path classification. `has_marker_below` lets the caller
/// raise a folder whose project marker sits in a DESCENDANT (a `.git` deeper in
/// the subtree still marks the root) — `children.has_direct_marker`
/// handles the marker-in-this-folder case. `under_floored_ancestor` is the mirror
/// image on the floor side: the caller sets it when a self-flooring ancestor (a
/// denylisted / hidden / system folder) sits above this one, so the whole subtree
/// under a `node_modules` floors, not just the named folder.
pub fn signals_for_dir(
    mtime_secs: Option<u64>,
    children: ChildAggregate,
    path: &str,
    home: &str,
    has_marker_below: bool,
    under_floored_ancestor: bool,
    optional: OptionalSignals,
) -> FolderSignals {
    let name = leaf_name(path);
    let name_denylisted = is_denylisted(&name);
    let hidden_or_system = is_hidden_or_system(path, &name, home);

    let has_project_marker = children.has_direct_marker || has_marker_below;

    // A folder with a project marker (here or below) reads as a project root; its
    // path-class prior is raised to `ProjectRoot`, the strongest prior. Otherwise
    // the path alone classifies it. `$HOME`, volume roots, and system/cache paths
    // are exempt from the promotion; see `classify::path_class_with_marker`.
    let path_class = path_class_with_marker(path, home, has_project_marker);

    FolderSignals {
        name_denylisted,
        hidden_or_system,
        under_floored_ancestor,
        distinct_extension_count: children.distinct_extension_count,
        file_count: children.file_count,
        mtime_secs,
        has_project_marker,
        path_class,
        visit_count: optional.visit_count,
        last_used_secs: optional.last_used_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::importance::scorer::PathClass;

    #[test]
    fn a_node_modules_dir_is_denylisted() {
        let mtime = Some(100);
        let s = signals_for_dir(
            mtime,
            ChildAggregate {
                distinct_extension_count: 1,
                file_count: 1,
                has_direct_marker: false,
            },
            "/Users/me/proj/node_modules",
            "/Users/me",
            false,
            false,
            Default::default(),
        );
        assert!(s.name_denylisted, "a node_modules folder is denylisted by name");
    }

    #[test]
    fn a_direct_marker_marks_the_folder_a_project_root() {
        let mtime = Some(100);
        // The walk found a `.git` (dir) or `Cargo.toml` (file) among the children.
        let children = ChildAggregate {
            distinct_extension_count: 1,
            file_count: 1,
            has_direct_marker: true,
        };
        let s = signals_for_dir(
            mtime,
            children,
            "/Users/me/proj",
            "/Users/me",
            false,
            false,
            Default::default(),
        );
        assert!(s.has_project_marker, "a direct marker child marks a project root");
        assert_eq!(
            s.path_class,
            PathClass::ProjectRoot,
            "a project root gets the ProjectRoot prior"
        );
    }

    #[test]
    fn a_marker_below_still_raises_the_folder() {
        let mtime = Some(100);
        // No marker among the direct children, but the caller found one below.
        let s = signals_for_dir(
            mtime,
            ChildAggregate {
                distinct_extension_count: 1,
                file_count: 1,
                has_direct_marker: false,
            },
            "/Users/me/proj",
            "/Users/me",
            true,
            false,
            Default::default(),
        );
        assert!(s.has_project_marker, "a marker in a descendant still raises the folder");
    }

    /// A busy folder that looks like a working directory: recent, mixed contents,
    /// and a project marker among its children.
    fn busy_children() -> ChildAggregate {
        ChildAggregate {
            distinct_extension_count: 6,
            file_count: 20,
            has_direct_marker: true,
        }
    }

    #[test]
    fn a_marker_in_home_doesnt_make_home_a_project_root() {
        // A `.git` or `Makefile` sitting directly in `$HOME` means dotfiles, not a
        // project the user is working in. Observed 2026-09-03: `$HOME` stored at
        // `score=0.954, pathClass=projectRoot`, which pushed the agent's wake
        // interest over its 0.7 hot threshold for every file written anywhere in it.
        let s = signals_for_dir(
            Some(100),
            busy_children(),
            "/Users/me",
            "/Users/me",
            false,
            false,
            Default::default(),
        );
        assert_ne!(
            s.path_class,
            PathClass::ProjectRoot,
            "$HOME never earns the project-root prior"
        );
        assert!(
            !s.hidden_or_system,
            "$HOME must NOT floor — that would propagate to the whole home directory"
        );
    }

    #[test]
    fn a_marker_at_a_volume_root_doesnt_promote_the_whole_disk() {
        for root in volume_roots() {
            let s = signals_for_dir(
                Some(100),
                busy_children(),
                root,
                "/Users/me",
                false,
                false,
                Default::default(),
            );
            assert_ne!(
                s.path_class,
                PathClass::ProjectRoot,
                "{root} is a volume root, not a project"
            );
        }
    }

    #[cfg(target_os = "macos")]
    fn volume_roots() -> &'static [&'static str] {
        &["/", "/Volumes/backup"]
    }
    #[cfg(target_os = "linux")]
    fn volume_roots() -> &'static [&'static str] {
        &["/", "/mnt/backup", "/media/backup"]
    }
    #[cfg(not(any(target_os = "macos", target_os = "linux")))]
    fn volume_roots() -> &'static [&'static str] {
        &["/"]
    }

    #[test]
    fn a_real_project_dir_still_becomes_a_project_root_and_scores_high() {
        use crate::importance::scorer::{SignalSet, Weights, score};

        let now = 1_800_000_000;
        let s = signals_for_dir(
            Some(now - 60),
            busy_children(),
            "/Users/me/projects-git/vdavid/cmdr",
            "/Users/me",
            false,
            false,
            Default::default(),
        );
        assert_eq!(
            s.path_class,
            PathClass::ProjectRoot,
            "an ordinary project dir keeps the strongest prior"
        );
        let scored = score(&s, &SignalSet::listing_only(), &Weights::default(), now);
        assert!(
            scored.value() > 0.7,
            "an active project must stay well above the agent's hot threshold, got {}",
            scored.value()
        );
    }

    #[test]
    fn a_temp_root_floors_instead_of_scoring() {
        let s = signals_for_dir(
            Some(100),
            busy_children(),
            "/private/tmp",
            "/Users/me",
            false,
            false,
            Default::default(),
        );
        assert!(s.hidden_or_system, "a system temp root is system-owned, so it floors");
        assert_eq!(s.path_class, PathClass::SystemOrCache);
    }

    #[test]
    fn extension_diversity_and_count_come_from_the_aggregate() {
        let mtime = Some(100);
        let children = ChildAggregate {
            distinct_extension_count: 3,
            file_count: 3,
            has_direct_marker: false,
        };
        let s = signals_for_dir(
            mtime,
            children,
            "/Users/me/mixed",
            "/Users/me",
            false,
            false,
            Default::default(),
        );
        assert_eq!(s.file_count, 3, "the aggregated file count carries through");
        assert_eq!(s.distinct_extension_count, 3, "three distinct extensions");
    }

    #[test]
    fn optional_signals_pass_through() {
        let mtime = Some(100);
        let opt = OptionalSignals {
            visit_count: Some(5),
            last_used_secs: Some(999),
        };
        let s = signals_for_dir(
            mtime,
            ChildAggregate::default(),
            "/Users/me/Documents/docs",
            "/Users/me",
            false,
            false,
            opt,
        );
        assert_eq!(s.visit_count, Some(5));
        assert_eq!(s.last_used_secs, Some(999));
        assert_eq!(s.path_class, PathClass::UserContent, "under Documents ⇒ user content");
    }
}
