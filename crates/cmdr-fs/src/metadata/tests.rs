//! The helper against real macOS items: what it puts on, what it clears, and
//! that it never reaches through a symlink.

#[cfg(target_os = "macos")]
mod macos {
    use super::super::*;
    use crate::testing::metadata_fixture::{LOCKED_FLAG, MetadataFixture, flags, has_acl};

    fn locked_and_acld(fixture: &MetadataFixture, name: &str) -> std::path::PathBuf {
        let path = fixture.deny_delete_file(name);
        crate::testing::metadata_fixture::set_flags(&path, LOCKED_FLAG);
        path
    }

    #[test]
    fn apply_landed_puts_the_acl_then_the_flags_on() {
        let fixture = MetadataFixture::new("metadata_apply_landed");
        let source = fixture.deny_delete_file("source.txt");
        let landed = fixture.join("landed.txt");
        std::fs::write(&landed, b"x").expect("write");
        let mut set = LandedMetadata::of_source_acl(&source);
        set.merge(LandedMetadata::default().with_flags(UserFlags::LOCKED));

        let report = apply_landed(&landed, &set);

        assert_eq!(
            report,
            MetadataReport {
                acl: ClassOutcome::Kept,
                flags: ClassOutcome::Kept,
                ..MetadataReport::ABSENT
            }
        );
        assert!(has_acl(&landed));
        assert_eq!(flags(&landed) & LOCKED_FLAG, LOCKED_FLAG);
    }

    #[test]
    fn apply_landed_adds_flags_to_what_the_item_wears() {
        let fixture = MetadataFixture::new("metadata_apply_adds");
        let landed = fixture.rich_file("hidden.txt");
        let set = LandedMetadata::default().with_flags(UserFlags::LOCKED);

        apply_landed(&landed, &set);

        let raw = flags(&landed);
        assert_eq!(raw & LOCKED_FLAG, LOCKED_FLAG);
        assert_eq!(raw & libc::UF_HIDDEN, libc::UF_HIDDEN, "Hidden stays");
    }

    #[test]
    fn a_plain_source_has_an_empty_landed_set() {
        let fixture = MetadataFixture::new("metadata_plain_source");
        let plain = fixture.join("plain.txt");
        std::fs::write(&plain, b"x").expect("write");
        assert!(LandedMetadata::of_source_acl(&plain).is_empty());
    }

    #[test]
    fn clear_blockers_hands_back_what_it_cleared() {
        let fixture = MetadataFixture::new("metadata_clear");
        let path = locked_and_acld(&fixture, "both.txt");

        let Unblock::Cleared(cleared) = clear_blockers(&path) else {
            panic!("both blockers should clear");
        };

        assert_eq!(cleared.flags(), UserFlags::LOCKED);
        assert!(cleared.has_acl());
        assert_eq!(flags(&path) & LOCKED_FLAG, 0);
        assert!(!has_acl(&path));
        std::fs::remove_file(&path).expect("nothing refuses the unlink now");
    }

    #[test]
    fn clear_blockers_on_a_plain_file_clears_nothing() {
        let fixture = MetadataFixture::new("metadata_clear_plain");
        let plain = fixture.rich_file("plain.txt");
        assert_eq!(clear_blockers(&plain), Unblock::NothingToClear);
        assert_eq!(
            flags(&plain) & libc::UF_HIDDEN,
            libc::UF_HIDDEN,
            "Hidden isn't a blocker"
        );
    }

    #[test]
    fn removal_lock_names_the_users_lock_and_nothing_else() {
        let fixture = MetadataFixture::new("metadata_removal_lock");
        let locked = fixture.locked_file("locked.txt");
        let acl = fixture.deny_delete_file("acl.txt");
        let hidden = fixture.rich_file("hidden.txt");
        let folder = fixture.join("folder");
        std::fs::create_dir(&folder).expect("mkdir");
        crate::testing::metadata_fixture::add_acl_entry(&folder, "everyone deny delete_child");
        let inheriting = fixture.join("inheriting");
        std::fs::create_dir(&inheriting).expect("mkdir");
        crate::testing::metadata_fixture::add_acl_entry(&inheriting, "everyone deny delete,only_inherit,file_inherit");
        let link = fixture.join("link");
        crate::testing::symlink(&locked, &link).expect("symlink");

        assert_eq!(removal_lock(&locked), RemovalLock::Locked);
        assert_eq!(removal_lock(&acl), RemovalLock::Locked);
        assert_eq!(
            removal_lock(&folder),
            RemovalLock::Locked,
            "`delete_child` keeps its children in"
        );
        assert_eq!(removal_lock(&hidden), RemovalLock::Free, "Hidden refuses nothing");
        assert_eq!(
            removal_lock(&inheriting),
            RemovalLock::Free,
            "an inherit-only entry is for its children"
        );
        assert_eq!(removal_lock(&link), RemovalLock::Free, "the link itself is free");
        assert_eq!(removal_lock(&fixture.join("missing")), RemovalLock::Free);
    }

    /// A non-root test can't set `schg`, so the system arm is read off the word.
    #[test]
    fn a_system_flag_outranks_the_users_lock() {
        let never = || panic!("the flags decide it");
        assert_eq!(
            blockers::lock_of(libc::SF_IMMUTABLE | libc::UF_IMMUTABLE, never),
            RemovalLock::SystemLocked
        );
        assert_eq!(blockers::lock_of(libc::SF_APPEND, never), RemovalLock::SystemLocked);
        assert_eq!(blockers::lock_of(libc::UF_APPEND, never), RemovalLock::Locked);
        assert_eq!(blockers::lock_of(libc::UF_HIDDEN, || false), RemovalLock::Free);
        assert_eq!(blockers::lock_of(0, || true), RemovalLock::Locked);
    }

    /// The link is what's being removed; the user's Locked target stays Locked.
    #[test]
    fn clear_blockers_never_reaches_through_a_symlink() {
        let fixture = MetadataFixture::new("metadata_clear_link");
        let target = locked_and_acld(&fixture, "target.txt");
        let link = fixture.join("link");
        crate::testing::symlink(&target, &link).expect("symlink");

        assert_eq!(clear_blockers(&link), Unblock::NothingToClear);

        assert_eq!(flags(&target) & LOCKED_FLAG, LOCKED_FLAG);
        assert!(has_acl(&target));
    }

    #[test]
    fn the_removal_helpers_remove_what_refuses_an_unlink() {
        let fixture = MetadataFixture::new("metadata_remove");
        let locked = fixture.locked_file("locked.txt");
        let acl = fixture.deny_delete_file("acl.txt");
        let dir = fixture.join("dir");
        std::fs::create_dir(&dir).expect("mkdir");
        crate::testing::metadata_fixture::add_deny_delete_acl(&dir);

        remove_file_clearing_blockers(&locked).expect("the Locked file goes");
        remove_file_clearing_blockers(&acl).expect("the `deny delete` file goes");
        remove_dir_clearing_blockers(&dir).expect("the `deny delete` folder goes");

        assert!(!locked.exists() && !acl.exists() && !dir.exists());
    }

    /// A child of a Locked, read-only folder with `deny delete_child` refuses its
    /// unlink over its FOLDER's metadata, not its own: the folder is opened for
    /// the one removal and closed again, so it keeps all three.
    #[test]
    fn the_removal_helpers_open_a_blocking_folder_for_one_removal_and_close_it_again() {
        let fixture = MetadataFixture::new("metadata_remove_from_blocking_folder");
        let folder = fixture.locked_read_only_folder("folder");
        crate::testing::metadata_fixture::set_flags(&folder, 0);
        crate::testing::metadata_fixture::add_acl_entry(&folder, "everyone deny delete_child");
        crate::testing::metadata_fixture::set_flags(&folder, LOCKED_FLAG);

        remove_file_clearing_blockers(&folder.join("a.txt")).expect("the child goes");

        assert!(!folder.join("a.txt").exists());
        assert_eq!(mode(&folder), 0o500, "the folder's mode is back");
        assert_eq!(flags(&folder) & LOCKED_FLAG, LOCKED_FLAG, "the folder is Locked again");
        assert!(has_acl(&folder), "the folder wears its ACL again");
    }

    /// The journal rollback's half of the same thing: it removes through a
    /// volume, so it opens the folder, removes, and closes it itself.
    #[test]
    fn open_parent_folder_hands_back_what_it_changed_and_close_restores_it() {
        let fixture = MetadataFixture::new("metadata_open_parent");
        let folder = fixture.locked_read_only_folder("folder");
        let child = folder.join("a.txt");

        let ParentOpening::Opened(opened) = open_parent_folder(&child) else {
            panic!("the Locked read-only folder should open");
        };
        std::fs::remove_file(&child).expect("nothing refuses the unlink now");
        opened.close();

        assert_eq!(mode(&folder), 0o500);
        assert_eq!(flags(&folder) & LOCKED_FLAG, LOCKED_FLAG);
    }

    #[test]
    fn open_parent_folder_of_a_plain_folders_child_opens_nothing() {
        let fixture = MetadataFixture::new("metadata_open_parent_plain");
        let child = fixture.join("plain.txt");
        std::fs::write(&child, b"x").expect("write");
        assert!(matches!(open_parent_folder(&child), ParentOpening::NothingToOpen));
    }

    #[test]
    fn the_tree_removal_clears_every_level() {
        let fixture = MetadataFixture::new("metadata_remove_tree");
        let root = fixture.join("tree");
        std::fs::create_dir_all(root.join("inner")).expect("mkdir");
        locked_and_acld(&fixture, "tree/inner/both.txt");
        fixture.locked_file("tree/locked.txt");
        crate::testing::metadata_fixture::add_acl_entry(&root.join("inner"), "everyone deny delete_child");

        remove_tree_clearing_blockers(&root).expect("the whole tree goes");

        assert!(!root.exists());
    }

    #[test]
    fn a_plain_item_never_reaches_the_helper() {
        let fixture = MetadataFixture::new("metadata_plain_remove");
        let plain = fixture.join("plain.txt");
        std::fs::write(&plain, b"x").expect("write");
        let before = helper_calls_on_this_thread();

        remove_file_clearing_blockers(&plain).expect("removed");

        assert_eq!(helper_calls_on_this_thread(), before);
    }

    // ── apply_staged ────────────────────────────────────────────────────────

    use crate::testing::metadata_fixture::{
        ACCESSED_SECS, BIRTH_SECS, MODIFIED_SECS, PROVENANCE_XATTR, birth_secs, mode, modified_secs, xattrs,
    };
    use std::time::{Duration, SystemTime};

    fn at(secs: i64) -> SystemTime {
        SystemTime::UNIX_EPOCH + Duration::from_secs(secs.unsigned_abs())
    }

    fn rich_carried() -> CarriedMetadata {
        CarriedMetadata {
            created: Some(at(BIRTH_SECS)),
            mode: Some(0o444),
            flags: UserFlags::LOCKED,
            xattrs: vec![Xattr {
                name: "user.app".into(),
                value: b"app value".to_vec(),
            }],
        }
    }

    fn fixture_times() -> StagedTimes {
        StagedTimes {
            modified: Some(at(MODIFIED_SECS)),
            accessed: Some(at(ACCESSED_SECS)),
        }
    }

    /// On the open fd, a `0444` mode doesn't refuse the dates after it, and the
    /// flags stay for the landed half.
    #[test]
    fn apply_staged_on_a_file_puts_on_xattrs_mode_and_both_dates() {
        let fixture = MetadataFixture::new("metadata_staged_file");
        let path = fixture.join("temp");
        let file = std::fs::File::create(&path).expect("create");

        let report = apply_staged(StagedItem::File(&file), &rich_carried(), fixture_times());

        assert_eq!(
            report,
            MetadataReport {
                xattrs: ClassOutcome::Kept,
                mode: ClassOutcome::Kept,
                created: ClassOutcome::Kept,
                modified: ClassOutcome::Kept,
                ..MetadataReport::ABSENT
            }
        );
        assert_eq!(birth_secs(&path), BIRTH_SECS);
        assert_eq!(modified_secs(&path), MODIFIED_SECS);
        assert_eq!(mode(&path), 0o444);
        assert_eq!(xattrs(&path), vec![("user.app".to_string(), b"app value".to_vec())]);
        assert_eq!(flags(&path) & LOCKED_FLAG, 0, "flags wait for the landing");
    }

    /// Invariant 1: a refused time-set (an ACL denying `writeattr`) never undoes or
    /// skips the mode.
    #[test]
    fn a_refused_time_set_still_applies_the_mode() {
        let fixture = MetadataFixture::new("metadata_staged_refused_dates");
        let path = fixture.join("temp");
        let file = std::fs::File::create(&path).expect("create");
        crate::testing::metadata_fixture::add_acl_entry(&path, "everyone deny writeattr");
        let carried = CarriedMetadata {
            mode: Some(0o640),
            created: Some(at(BIRTH_SECS)),
            ..CarriedMetadata::default()
        };

        let report = apply_staged(StagedItem::File(&file), &carried, fixture_times());

        assert_eq!(report.created, ClassOutcome::Refused);
        assert_eq!(report.modified, ClassOutcome::Refused);
        assert_eq!(report.mode, ClassOutcome::Kept);
        assert_eq!(mode(&path), 0o640);
    }

    /// By path, xattrs go on before the mode: a path xattr write on a `0444`
    /// item would be refused (`EACCES`).
    #[test]
    fn apply_staged_by_path_writes_xattrs_before_a_read_only_mode() {
        let fixture = MetadataFixture::new("metadata_staged_path");
        let path = fixture.join("folder");
        std::fs::create_dir(&path).expect("mkdir");
        let carried = CarriedMetadata {
            mode: Some(0o555),
            ..rich_carried()
        };

        let report = apply_staged(StagedItem::Path(&path), &carried, fixture_times());

        assert_eq!(report.xattrs, ClassOutcome::Kept);
        assert_eq!(report.mode, ClassOutcome::Kept);
        assert_eq!(report.created, ClassOutcome::Kept);
        assert_eq!(birth_secs(&path), BIRTH_SECS);
        assert_eq!(modified_secs(&path), MODIFIED_SECS);
        assert_eq!(mode(&path), 0o555);
        assert_eq!(xattrs(&path).len(), 1);
    }

    /// A link's own dates and xattrs; the target keeps its own.
    #[test]
    fn apply_staged_on_a_link_never_reaches_its_target() {
        let fixture = MetadataFixture::new("metadata_staged_link");
        let target = fixture.join("target.txt");
        std::fs::write(&target, b"x").expect("write");
        let target_birth = birth_secs(&target);
        let link = fixture.join("link");
        crate::testing::symlink(&target, &link).expect("symlink");

        let report = apply_staged(StagedItem::Link(&link), &rich_carried(), fixture_times());

        assert_eq!(report.mode, ClassOutcome::NotApplicable);
        assert_eq!(birth_secs(&link), BIRTH_SECS);
        assert_eq!(modified_secs(&link), MODIFIED_SECS);
        assert_eq!(birth_secs(&target), target_birth);
        assert_eq!(xattrs(&target), Vec::new());
    }

    /// The source side: one read of the handle the copy holds, provenance left
    /// behind.
    #[test]
    fn a_local_file_carries_every_class_but_provenance() {
        let fixture = MetadataFixture::new("metadata_carried_file");
        let path = fixture.rich_file("f.txt");
        let file = std::fs::File::open(&path).expect("open");
        let meta = file.metadata().expect("fstat");

        let carried = CarriedMetadata::of_local_file(&file, &meta);

        assert_eq!(carried.created, Some(at(BIRTH_SECS)));
        assert_eq!(carried.mode, Some(0o750));
        assert_eq!(carried.flags, UserFlags::HIDDEN);
        let names: Vec<String> = carried
            .xattrs
            .iter()
            .map(|x| x.name.to_string_lossy().into_owned())
            .collect();
        assert_eq!(names.len(), 5, "{names:?}");
        assert!(!names.iter().any(|n| n == PROVENANCE_XATTR));
    }

    /// A folder's source side: the stat's classes and its xattrs by path.
    #[test]
    fn a_local_folder_carries_its_dates_mode_flags_and_xattrs() {
        let fixture = MetadataFixture::new("metadata_carried_folder");
        let folder = fixture.rich_folder("dir");
        let meta = std::fs::symlink_metadata(&folder).expect("lstat");

        let carried = CarriedMetadata::of_local_dir(&folder, &meta);

        assert_eq!(carried.created, Some(at(BIRTH_SECS)));
        assert_eq!(carried.mode, Some(0o700));
        assert_eq!(carried.flags, UserFlags::HIDDEN);
        assert_eq!(carried.xattrs.len(), 3, "tags, comment, FinderInfo");
    }

    /// A folder's final mode goes on after its children landed, and before the
    /// flags: `uchg` refuses a later `chmod`.
    #[test]
    fn apply_landed_puts_a_folders_mode_on_before_its_flags() {
        let fixture = MetadataFixture::new("metadata_landed_folder_mode");
        let folder = fixture.join("folder");
        std::fs::create_dir(&folder).expect("mkdir");
        let set = LandedMetadata::default().with_mode(0o500).with_flags(UserFlags::LOCKED);

        let report = apply_landed(&folder, &set);

        assert_eq!(report.mode, ClassOutcome::Kept);
        assert_eq!(report.flags, ClassOutcome::Kept);
        assert_eq!(mode(&folder), 0o500);
        assert_eq!(flags(&folder) & LOCKED_FLAG, LOCKED_FLAG);
    }

    #[test]
    fn apply_staged_with_nothing_carried_makes_no_call_and_reports_absent() {
        let fixture = MetadataFixture::new("metadata_staged_empty");
        let path = fixture.join("temp");
        let file = std::fs::File::create(&path).expect("create");

        let report = apply_staged(
            StagedItem::File(&file),
            &CarriedMetadata::default(),
            StagedTimes::default(),
        );

        assert_eq!(report, MetadataReport::ABSENT);
    }
}
