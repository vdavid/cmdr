use std::fs;

use super::*;

#[test]
fn unknown_never_reads_as_different() {
    assert_eq!(Sameness::of(Some(1), Some(1)), Sameness::Same);
    assert_eq!(Sameness::of(Some(1), Some(2)), Sameness::Different);
    assert_eq!(Sameness::of(None, Some(1)), Sameness::Unknown);
    assert_eq!(Sameness::of(Some(1), None), Sameness::Unknown);
    assert_eq!(Sameness::of::<i32>(None, None), Sameness::Unknown);
    assert!(Sameness::Same.may_be_same());
    assert!(Sameness::Unknown.may_be_same(), "a guard must take the cautious branch");
    assert!(!Sameness::Different.may_be_same());
}

#[test]
fn a_node_id_too_wide_for_64_bits_has_no_64_bit_form() {
    let device = DeviceId::from_raw(7);
    assert_eq!(FileId::from_parts(device, 42).node_u64(), Some(42));
    assert_eq!(FileId::from_parts(device, u128::from(u64::MAX) + 1).node_u64(), None);
    assert_eq!(FileId::from_parts(device, 42).device(), device);
}

#[cfg(unix)]
mod on_a_real_disk {
    use super::*;

    #[test]
    fn a_hard_link_is_the_same_node_and_counts_two_names() {
        let dir = tempfile::tempdir().unwrap();
        let original = dir.path().join("a.txt");
        let link = dir.path().join("b.txt");
        fs::write(&original, b"x").unwrap();
        fs::hard_link(&original, &link).unwrap();
        let (a, b) = (fs::metadata(&original).unwrap(), fs::metadata(&link).unwrap());

        assert_eq!(same_node(&a, &b), Sameness::Same);
        assert_eq!(link_count(&a), Some(2));
    }

    #[test]
    fn two_files_side_by_side_differ_but_share_a_device() {
        let dir = tempfile::tempdir().unwrap();
        fs::write(dir.path().join("a"), b"1").unwrap();
        fs::write(dir.path().join("b"), b"2").unwrap();
        let a = fs::metadata(dir.path().join("a")).unwrap();
        let b = fs::metadata(dir.path().join("b")).unwrap();

        assert_eq!(same_node(&a, &b), Sameness::Different);
        assert_eq!(same_device(&a, &b), Sameness::Same);
        assert_eq!(link_count(&a), Some(1));
        assert_eq!(FileId::of(&a).map(FileId::device), device_of(&a));
    }

    #[test]
    fn a_followed_symlink_is_its_target_and_an_unfollowed_one_is_not() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("target");
        let link = dir.path().join("link");
        fs::write(&target, b"x").unwrap();
        crate::testing::symlink(&target, &link).unwrap();
        let target_meta = fs::metadata(&target).unwrap();

        assert_eq!(same_node(&fs::metadata(&link).unwrap(), &target_meta), Sameness::Same);
        assert_eq!(
            same_node(&fs::symlink_metadata(&link).unwrap(), &target_meta),
            Sameness::Different
        );
    }
}
