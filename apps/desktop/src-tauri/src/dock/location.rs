//! Whether the running copy of Cmdr sits somewhere we're willing to pin.

use std::path::Path;

/// The name of an Applications folder, in both places one is allowed to live.
const APPLICATIONS: &str = "Applications";

/// Whether the bundle at `bundle_path` sits in an Applications folder: `/Applications`, or the
/// current user's `~/Applications` (`home`).
///
/// Anywhere else, we stay quiet. Pinning a tile that points into `~/Downloads`, a mounted disk
/// image, or a translocated `…/AppTranslocation/<uuid>/d/Cmdr.app` leaves a tile that stops working
/// the moment the copy moves, which is worse than never offering.
///
/// Nested is fine (`/Applications/Utilities/Cmdr.app`): people organize their Applications folder,
/// and a bundle under one is still installed. Comparison is component-wise, so `/ApplicationsOld`
/// isn't an Applications folder.
pub(super) fn in_an_applications_folder(bundle_path: &Path, home: Option<&Path>) -> bool {
    if bundle_path.starts_with(Path::new("/").join(APPLICATIONS)) {
        return true;
    }
    home.is_some_and(|home| bundle_path.starts_with(home.join(APPLICATIONS)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn home() -> PathBuf {
        PathBuf::from("/Users/jane")
    }

    #[test]
    fn a_bundle_in_the_system_applications_folder_counts() {
        assert!(in_an_applications_folder(
            Path::new("/Applications/Cmdr.app"),
            Some(&home())
        ));
    }

    #[test]
    fn a_bundle_in_the_users_own_applications_folder_counts() {
        assert!(in_an_applications_folder(
            Path::new("/Users/jane/Applications/Cmdr.app"),
            Some(&home())
        ));
    }

    #[test]
    fn a_bundle_nested_under_an_applications_folder_counts() {
        assert!(in_an_applications_folder(
            Path::new("/Applications/Utilities/Cmdr.app"),
            Some(&home())
        ));
    }

    #[test]
    fn a_bundle_in_downloads_does_not_count() {
        assert!(!in_an_applications_folder(
            Path::new("/Users/jane/Downloads/Cmdr.app"),
            Some(&home())
        ));
    }

    #[test]
    fn a_translocated_bundle_does_not_count() {
        // Gatekeeper runs a freshly-downloaded app from a read-only shadow copy. A tile pointing
        // there is dead the moment the quarantine clears.
        assert!(!in_an_applications_folder(
            Path::new("/private/var/folders/hz/x/T/AppTranslocation/1E2D/d/Cmdr.app"),
            Some(&home())
        ));
    }

    #[test]
    fn a_folder_merely_starting_with_applications_does_not_count() {
        assert!(!in_an_applications_folder(
            Path::new("/ApplicationsOld/Cmdr.app"),
            Some(&home())
        ));
    }

    #[test]
    fn another_users_applications_folder_does_not_count() {
        assert!(!in_an_applications_folder(
            Path::new("/Users/sam/Applications/Cmdr.app"),
            Some(&home())
        ));
    }

    #[test]
    fn a_dev_build_running_out_of_target_does_not_count() {
        assert!(!in_an_applications_folder(
            Path::new("/Users/jane/code/cmdr/target/release/Cmdr.app"),
            Some(&home())
        ));
    }

    #[test]
    fn the_system_applications_folder_still_counts_without_a_home() {
        assert!(in_an_applications_folder(Path::new("/Applications/Cmdr.app"), None));
    }

    #[test]
    fn a_user_applications_bundle_does_not_count_without_a_home() {
        // No home means we can't tell whose `~/Applications` this is, so we don't guess.
        assert!(!in_an_applications_folder(
            Path::new("/Users/jane/Applications/Cmdr.app"),
            None
        ));
    }
}
