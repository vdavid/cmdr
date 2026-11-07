use std::path::PathBuf;

use super::{DropFolders, is_drop_id};
use crate::test_support::TestDir;

fn folders(dir: &TestDir) -> DropFolders {
    DropFolders::new(dir.join("drops"))
}

fn fill(dir: &std::path::Path) -> PathBuf {
    let file = dir.join("cat.jpg");
    std::fs::write(&file, b"jpeg").expect("write a received file");
    file
}

#[test]
fn each_drop_gets_a_fresh_empty_folder() {
    let dir = TestDir::new("drop-create");
    let mut folders = folders(&dir);
    let (a, a_dir) = folders.create().expect("create");
    let (b, b_dir) = folders.create().expect("create");
    assert_ne!(a, b);
    assert!(a_dir.is_dir() && b_dir.is_dir());
    assert!(is_drop_id(&a) && is_drop_id(&b));
}

#[test]
fn an_unclaimed_drop_is_removed_on_release() {
    let dir = TestDir::new("drop-release");
    let mut folders = folders(&dir);
    let (id, drop_dir) = folders.create().expect("create");
    fill(&drop_dir);
    assert!(folders.release(&id));
    assert!(!drop_dir.exists());
    assert!(!folders.release(&id), "a second release finds nothing");
}

#[test]
fn a_claimed_drop_outlives_the_dialog_and_goes_when_its_copy_ends() {
    let dir = TestDir::new("drop-claim");
    let mut folders = folders(&dir);
    let (id, drop_dir) = folders.create().expect("create");
    let file = fill(&drop_dir);

    assert!(folders.claim("op-1", std::slice::from_ref(&file)));
    assert!(!folders.release(&id), "the copy reading from it holds it");
    assert!(drop_dir.exists());

    folders.release_for_operation("op-1");
    assert!(!drop_dir.exists(), "the copy ended, so the folder goes");
}

#[test]
fn a_retry_keeps_the_folder_until_the_last_reader_ends() {
    let dir = TestDir::new("drop-retry");
    let mut folders = folders(&dir);
    let (_, drop_dir) = folders.create().expect("create");
    let file = fill(&drop_dir);
    folders.claim("op-failed", std::slice::from_ref(&file));
    folders.claim("op-retry", std::slice::from_ref(&file));

    folders.release_for_operation("op-retry");
    assert!(drop_dir.exists(), "another operation still claims it");
    folders.release_for_operation("op-failed");
    assert!(!drop_dir.exists());
}

#[test]
fn a_copy_of_ordinary_files_claims_nothing() {
    let dir = TestDir::new("drop-ordinary");
    let mut folders = folders(&dir);
    let elsewhere = dir.join("photos").join("cat.jpg");
    assert!(!folders.claim("op-1", &[elsewhere]));
    folders.release_for_operation("op-1");
}

#[test]
fn release_never_reaches_outside_the_root() {
    let dir = TestDir::new("drop-escape");
    let mut folders = folders(&dir);
    let precious = dir.join("precious");
    std::fs::create_dir_all(&precious).expect("mkdir");
    for id in ["../precious", "..", "", "drop-1-2/../../precious", "precious"] {
        assert!(!folders.release(id), "{id:?}");
    }
    assert!(precious.exists());
}

#[test]
fn the_startup_sweep_removes_drop_folders_and_nothing_else() {
    let dir = TestDir::new("drop-sweep");
    let mut folders = folders(&dir);
    let (_, drop_dir) = folders.create().expect("create");
    fill(&drop_dir);
    let stranger = dir.join("drops").join("not-a-drop");
    std::fs::create_dir_all(&stranger).expect("mkdir");

    folders.sweep();
    assert!(!drop_dir.exists());
    assert!(stranger.exists(), "only our own folders go");
}

#[test]
fn a_copy_that_ends_before_its_claim_lands_still_frees_its_folder() {
    let dir = TestDir::new("drop-race");
    let mut folders = folders(&dir);
    let (_, drop_dir) = folders.create().expect("create");
    let file = fill(&drop_dir);

    folders.release_for_operation("op-fast");
    assert!(folders.claim("op-fast", std::slice::from_ref(&file)));
    assert!(!drop_dir.exists(), "the late claim releases at once");
}
