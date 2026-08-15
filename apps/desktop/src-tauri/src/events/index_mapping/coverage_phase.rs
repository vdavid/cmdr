//! Which of its own answers a coverage phase is.
//!
//! The index covers a drive in an order the APP supplied — `priority_roots` is
//! answered here, in `priority/roots.rs` — and then reports each phase by its
//! root. Turning that root back into "the folders you use most" / "the rest of
//! your home folder" / "the rest of the drive" is therefore the app's question,
//! and belongs on this side of the seam: the crate would need a second
//! description of an order it already keeps in its queue.
//!
//! Two paths decide it, and neither is a guess:
//!
//! - the phase root EQUALS the volume root (the crate sends its own, so nothing
//!   here has to hold a second idea of where a volume is mounted) ⇒ the last
//!   phase, the rest of the drive;
//! - the phase root equals this machine's home folder ⇒ the home phase.
//!
//! Everything else is a folder this user cares about: a root the app named, or
//! one they opened while the walk was running, which is the same question
//! answered less well.
//!
//! ❌ Never "the boot volume's phases are the interesting ones": a folder the
//! user opens mid-run becomes a phase on whatever volume it lives on.

use std::path::Path;

use serde::{Deserialize, Serialize};

/// Which phase of a drive's first index is running, in the terms its owner would
/// recognize. The frontend renders one label per variant.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CoveragePhaseLabel {
    /// A folder this user cares about: one the app named up front, or one they
    /// opened while the walk was running.
    PriorityFolders,
    /// The rest of their home folder, after the folders above it.
    Home,
    /// The rest of the drive, which is the last phase.
    WholeDrive,
}

/// Classify one phase, given this machine's home folder (`None` when it has
/// none, which leaves every phase either the drive or a folder they care about).
pub(crate) fn label_for(root: &str, volume_root: &str, home: Option<&Path>) -> CoveragePhaseLabel {
    if same_folder(root, volume_root) {
        return CoveragePhaseLabel::WholeDrive;
    }
    if home.is_some_and(|home| same_folder(root, &home.to_string_lossy())) {
        return CoveragePhaseLabel::Home;
    }
    CoveragePhaseLabel::PriorityFolders
}

/// Whether two absolute paths name the same folder. A trailing separator is the
/// only difference worth normalizing: both sides come from the same path space,
/// so ❌ no case folding and ❌ no canonicalization (which would stat a folder on
/// the event thread).
fn same_folder(a: &str, b: &str) -> bool {
    trim(a) == trim(b)
}

fn trim(path: &str) -> &str {
    if path.len() > 1 {
        path.strip_suffix('/').unwrap_or(path)
    } else {
        path
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOME: &str = "/Users/someone";

    fn label(root: &str, volume_root: &str) -> CoveragePhaseLabel {
        label_for(root, volume_root, Some(Path::new(HOME)))
    }

    #[test]
    fn the_volume_root_is_the_last_phase() {
        assert_eq!(label("/", "/"), CoveragePhaseLabel::WholeDrive);
        assert_eq!(
            label("/Volumes/Backups", "/Volumes/Backups"),
            CoveragePhaseLabel::WholeDrive
        );
    }

    #[test]
    fn home_is_its_own_phase() {
        assert_eq!(label(HOME, "/"), CoveragePhaseLabel::Home);
    }

    /// The ones the app itself named, and the one the user opened mid-run. Both
    /// answer "which folders does this person care about", so both read the same.
    #[test]
    fn everything_else_is_a_folder_this_user_cares_about() {
        assert_eq!(
            label("/Users/someone/Downloads", "/"),
            CoveragePhaseLabel::PriorityFolders
        );
        assert_eq!(label("/opt/tools", "/"), CoveragePhaseLabel::PriorityFolders);
    }

    /// A folder opened on an external drive is a phase on THAT drive, and it is
    /// still one of theirs. ❌ Not the whole-drive phase just because the volume
    /// isn't the boot disk.
    #[test]
    fn a_folder_on_another_drive_is_still_one_of_theirs() {
        assert_eq!(
            label("/Volumes/Backups/2019", "/Volumes/Backups"),
            CoveragePhaseLabel::PriorityFolders
        );
    }

    #[test]
    fn a_trailing_separator_is_the_same_folder() {
        assert_eq!(label("/Users/someone/", "/"), CoveragePhaseLabel::Home);
        assert_eq!(
            label("/Volumes/Backups/", "/Volumes/Backups"),
            CoveragePhaseLabel::WholeDrive
        );
    }

    /// A machine with no home folder still gets honest labels for the two phases
    /// that can happen on it.
    #[test]
    fn no_home_folder_leaves_the_other_two_answers_intact() {
        assert_eq!(label_for("/", "/", None), CoveragePhaseLabel::WholeDrive);
        assert_eq!(label_for("/opt", "/", None), CoveragePhaseLabel::PriorityFolders);
    }
}
