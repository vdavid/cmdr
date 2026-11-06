use super::*;
use crate::test_support::TestDir;

#[test]
fn the_same_version_as_last_time_is_a_relaunch() {
    assert_eq!(classify(Some("0.60.0"), true, "0.60.0"), LaunchKind::SameVersion);
}

#[test]
fn a_different_stamped_version_is_an_update() {
    assert_eq!(classify(Some("0.59.0"), true, "0.60.0"), LaunchKind::FirstAfterUpdate);
}

/// The release that introduces the stamp arrives by an update, and its first launch is the one
/// whose Local Network permission is stuck: an install with update checks on record counts.
#[test]
fn no_stamp_on_an_install_that_ran_before_is_an_update() {
    assert_eq!(classify(None, true, "0.60.0"), LaunchKind::FirstAfterUpdate);
}

/// A fresh install's first dial can be refused while macOS still shows its Local Network prompt,
/// so it must not read as an update.
#[test]
fn no_stamp_and_no_earlier_run_is_a_first_install() {
    assert_eq!(classify(None, false, "0.60.0"), LaunchKind::FirstEver);
}

#[test]
fn the_stamp_makes_the_next_launch_a_relaunch() {
    let dir = TestDir::new("after-update-stamp");
    assert_eq!(classify_and_stamp(&dir, "0.60.0"), LaunchKind::FirstEver);
    assert_eq!(classify_and_stamp(&dir, "0.60.0"), LaunchKind::SameVersion);
    assert_eq!(classify_and_stamp(&dir, "0.61.0"), LaunchKind::FirstAfterUpdate);
    assert_eq!(classify_and_stamp(&dir, "0.61.0"), LaunchKind::SameVersion);
}

#[test]
fn a_garbled_stamp_reads_as_missing() {
    let dir = TestDir::new("after-update-garbled");
    std::fs::write(dir.join(STAMP_FILE_NAME), "not json").expect("write");
    assert_eq!(classify_and_stamp(&dir, "0.60.0"), LaunchKind::FirstEver);
}
