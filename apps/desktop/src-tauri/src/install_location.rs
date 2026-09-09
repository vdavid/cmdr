//! Whether the running copy of Cmdr sits somewhere it's safe to point other things at.
//!
//! Two features ask exactly this, and they must not disagree:
//!
//! - `dock/` pins a tile, which is dead the moment the copy it names moves.
//! - `reveal/` writes our bundle id into the machine-wide `NSFileViewer` key, and a copy that
//!   gets deleted leaves that key dangling: macOS does NOT fall back to Finder, so "Show in
//!   Finder" silently stops working in every app (measured on macOS 26.6, 2026-09-09).
//!
//! Both dangers are the same danger — a copy that's about to move or be thrown away — so the rule
//! lives here once.
//!
//! ❌ Not the same question as `updater::bundle_location::classify`, which asks whether an update
//! can be WRITTEN into the bundle. A copy in `~/Applications` is writable and installed; a copy on
//! a mounted disk image is neither.

use std::path::Path;

/// The name of an Applications folder, in both places one is allowed to live.
const APPLICATIONS: &str = "Applications";

/// Whether the bundle at `bundle_path` sits in an Applications folder: `/Applications`, or the
/// current user's `~/Applications` (`home`).
///
/// Anywhere else, we stay quiet. `~/Downloads`, a mounted disk image, and Gatekeeper's
/// translocated `…/AppTranslocation/<uuid>/d/Cmdr.app` shadow copy are all places a copy is about
/// to move away from.
///
/// Nested is fine (`/Applications/Utilities/Cmdr.app`): people organize their Applications folder,
/// and a bundle under one is still installed. Comparison is component-wise, so `/ApplicationsOld`
/// isn't an Applications folder.
pub fn in_an_applications_folder(bundle_path: &Path, home: Option<&Path>) -> bool {
    if bundle_path.starts_with(Path::new("/").join(APPLICATIONS)) {
        return true;
    }
    home.is_some_and(|home| bundle_path.starts_with(home.join(APPLICATIONS)))
}

/// Whether THIS process is running from a bundle in an Applications folder.
///
/// A copy that isn't a `.app` at all (a dev build out of `target/`) answers `false` for the same
/// reason a copy in `~/Downloads` does: nothing should point at it.
pub fn running_copy_is_installed() -> bool {
    crate::updater::installer::running_bundle()
        .is_ok_and(|bundle| in_an_applications_folder(&bundle, dirs::home_dir().as_deref()))
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
        // Gatekeeper runs a freshly-downloaded app from a read-only shadow copy. Anything
        // pointing there is dead the moment the quarantine clears.
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

    #[test]
    fn the_test_binary_is_not_an_installed_copy() {
        // The suite runs out of `target/`, with no `.app` ancestor. Every feature gated on this
        // is therefore off in development without anything having to remember to switch it off.
        assert!(!running_copy_is_installed());
    }
}
