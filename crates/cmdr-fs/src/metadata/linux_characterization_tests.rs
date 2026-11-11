//! What every metadata class does on Linux today, pinned before the per-OS bodies move behind
//! `platform/`: the macOS half is `tests.rs`, against `testing::metadata_fixture`.

use std::fs::{self, File};
use std::os::unix::fs::PermissionsExt as _;
use std::time::{Duration, SystemTime};

use super::*;
use crate::testing::TestDir;

fn at(secs: u64) -> SystemTime {
    SystemTime::UNIX_EPOCH + Duration::from_secs(secs)
}

fn file_with(dir: &TestDir, name: &str, mode: u32) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, b"bytes").expect("write the fixture file");
    fs::set_permissions(&path, fs::Permissions::from_mode(mode)).expect("set the fixture's mode");
    path
}

#[test]
fn a_local_file_carries_its_mode_and_user_xattrs_and_no_flags() {
    let dir = TestDir::new("meta_linux_carried");
    let path = file_with(&dir, "a.txt", 0o640);
    let tagged = xattr::set(&path, "user.cmdr-test", b"value").is_ok();
    let file = File::open(&path).unwrap();
    let carried = CarriedMetadata::of_local_file(&file, &file.metadata().unwrap());

    assert_eq!(carried.mode, Some(0o640));
    assert_eq!(carried.flags, UserFlags::NONE, "Linux carries no user flags");
    if tagged {
        assert_eq!(carried.xattrs.len(), 1);
        assert_eq!(carried.xattrs[0].name, "user.cmdr-test");
        assert_eq!(carried.xattrs[0].value, b"value");
    }
}

#[test]
fn apply_staged_on_a_file_puts_on_xattrs_mode_and_the_mtime() {
    let dir = TestDir::new("meta_linux_staged");
    let path = file_with(&dir, "a.txt", 0o644);
    let carried = CarriedMetadata {
        created: Some(at(1_000_000)),
        mode: Some(0o600),
        flags: UserFlags::NONE,
        xattrs: vec![Xattr {
            name: "user.cmdr-test".into(),
            value: b"v".to_vec(),
        }],
    };
    let times = StagedTimes {
        modified: Some(at(2_000_000)),
        accessed: Some(at(3_000_000)),
    };
    let file = fs::OpenOptions::new().write(true).open(&path).unwrap();
    let report = apply_staged(StagedItem::File(&file), &carried, times);
    drop(file);

    assert_eq!(report.mode, ClassOutcome::Kept);
    assert_eq!(fs::metadata(&path).unwrap().permissions().mode() & 0o7777, 0o600);
    assert_eq!(report.modified, ClassOutcome::Kept);
    assert_eq!(fs::metadata(&path).unwrap().modified().unwrap(), at(2_000_000));
    assert!(
        matches!(report.xattrs, ClassOutcome::Kept | ClassOutcome::Refused),
        "{:?}",
        report.xattrs
    );
    assert_eq!((report.acl, report.flags), (ClassOutcome::Absent, ClassOutcome::Absent));
}

#[test]
fn apply_staged_on_a_link_takes_no_mode_and_no_dates() {
    let dir = TestDir::new("meta_linux_link");
    let target = file_with(&dir, "target", 0o644);
    let link = dir.join("link");
    std::os::unix::fs::symlink(&target, &link).unwrap();
    let carried = CarriedMetadata {
        created: None,
        mode: Some(0o600),
        flags: UserFlags::NONE,
        xattrs: Vec::new(),
    };
    let times = StagedTimes {
        modified: Some(at(2_000_000)),
        accessed: None,
    };
    let report = apply_staged(StagedItem::Link(&link), &carried, times);

    assert_eq!(report.mode, ClassOutcome::NotApplicable);
    assert_eq!(report.modified, ClassOutcome::NotApplicable);
    assert_eq!(
        fs::metadata(&target).unwrap().permissions().mode() & 0o7777,
        0o644,
        "the target is untouched"
    );
}

#[test]
fn apply_landed_puts_a_folders_mode_on_and_adds_no_flags() {
    let dir = TestDir::new("meta_linux_landed");
    let folder = dir.join("folder");
    fs::create_dir(&folder).unwrap();
    let report = apply_landed(
        &folder,
        &LandedMetadata::default().with_mode(0o750).with_flags(UserFlags::HIDDEN),
    );

    assert_eq!(report.mode, ClassOutcome::Kept);
    assert_eq!(fs::metadata(&folder).unwrap().permissions().mode() & 0o7777, 0o750);
    assert_eq!(
        report.flags,
        ClassOutcome::NotApplicable,
        "Linux has no user flags to add"
    );
    assert_eq!(report.acl, ClassOutcome::Absent);
}

#[test]
fn a_plain_files_acl_is_only_its_mode_so_nothing_travels() {
    let dir = TestDir::new("meta_linux_acl");
    let path = file_with(&dir, "a.txt", 0o644);
    assert!(!LandedMetadata::of_source_acl(&path).has_acl());
}

#[test]
fn nothing_blocks_a_removal_on_linux() {
    let dir = TestDir::new("meta_linux_blockers");
    let path = file_with(&dir, "a.txt", 0o444);
    assert!(matches!(clear_blockers(&path), Unblock::NothingToClear));
    assert!(matches!(removal_lock(&path), RemovalLock::Free));
}

#[test]
fn a_read_only_folder_opens_for_one_removal_and_closes_again() {
    let dir = TestDir::new("meta_linux_parent");
    let folder = dir.join("folder");
    fs::create_dir(&folder).unwrap();
    let child = folder.join("child");
    fs::write(&child, b"x").unwrap();
    fs::set_permissions(&folder, fs::Permissions::from_mode(0o555)).unwrap();

    let ParentOpening::Opened(opened) = open_parent_folder(&child) else {
        panic!("a read-only parent opens");
    };
    assert_eq!(fs::metadata(&folder).unwrap().permissions().mode() & 0o200, 0o200);
    let report = opened.close();
    assert_eq!(report.mode, ClassOutcome::Kept);
    assert_eq!(fs::metadata(&folder).unwrap().permissions().mode() & 0o7777, 0o555);
}

#[test]
fn the_default_modes_are_the_umask_applied_to_0666_and_0777() {
    let umask = 0o777 & !default_folder_mode();
    assert_eq!(default_file_mode(), 0o666 & !umask);
}
