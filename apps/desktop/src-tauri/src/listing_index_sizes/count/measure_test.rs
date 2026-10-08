//! The local walk: running totals, the stop, and an unreadable subfolder skipped
//! rather than ending the count.

use std::os::unix::fs::PermissionsExt;
use std::sync::Arc;
use tokio::time::Instant;

use super::jobs::{self, Joined};
use super::measure::{Live, MeasureError, walk_local};
use crate::test_support::TestDir;

fn fresh_job(tag: &str) -> Arc<jobs::Job> {
    match jobs::enqueue(tag, Vec::new(), Instant::now()) {
        Joined::Owner(job) => job,
        Joined::Waiter { .. } | Joined::Retiring(_) => panic!("a fresh tag owns its job"),
    }
}

#[test]
fn the_walk_counts_files_folders_and_bytes() {
    let dir = TestDir::new("measure-count");
    std::fs::create_dir_all(dir.join("a/b")).expect("mkdir");
    std::fs::write(dir.join("a/one.txt"), vec![0u8; 100]).expect("write");
    std::fs::write(dir.join("a/b/two.txt"), vec![0u8; 50]).expect("write");
    let live = Live::default();

    let measured = walk_local(&dir.join("a"), &fresh_job("measure-count"), &live).expect("readable");

    assert_eq!(
        (measured.progress.files, measured.progress.dirs, measured.progress.bytes),
        (2, 1, 150)
    );
    assert_eq!(measured.skipped, 0);
    assert_eq!(
        live.snapshot(),
        measured.progress,
        "the running total ends at the total"
    );
    jobs::cancel("measure-count");
}

#[test]
fn an_unreadable_subfolder_is_skipped_and_the_rest_still_counted() {
    // Root bypasses chmod restrictions (including in the Linux test container).
    // SAFETY: geteuid takes no arguments, accesses no caller memory, and cannot fail.
    if unsafe { libc::geteuid() } == 0 {
        return;
    }
    let dir = TestDir::new("measure-skip");
    std::fs::create_dir_all(dir.join("a/locked")).expect("mkdir");
    std::fs::write(dir.join("a/open.txt"), vec![0u8; 10]).expect("write");
    std::fs::write(dir.join("a/locked/secret.txt"), vec![0u8; 99]).expect("write");
    let locked = dir.join("a/locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).expect("chmod");

    let measured = walk_local(&dir.join("a"), &fresh_job("measure-skip"), &Live::default());

    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).expect("chmod back");
    let measured = measured.expect("the folder itself is readable");
    assert_eq!(measured.progress.bytes, 10, "what it could read");
    assert!(measured.skipped >= 1, "and how much it couldn't");
    jobs::cancel("measure-skip");
}

#[test]
fn a_folder_that_isnt_there_is_unreadable_and_a_stop_stops() {
    let dir = TestDir::new("measure-missing");
    let stopped = fresh_job("measure-missing");

    let missing = walk_local(&dir.join("nope"), &stopped, &Live::default());
    assert!(matches!(missing, Err(MeasureError::Unreadable(_))));

    std::fs::create_dir_all(dir.join("x")).expect("mkdir");
    jobs::cancel("measure-missing");
    assert_eq!(
        walk_local(&dir.join("x"), &stopped, &Live::default()),
        Err(MeasureError::Stopped)
    );
}
