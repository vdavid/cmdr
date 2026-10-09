use super::*;

#[test]
fn a_path_is_under_its_root_or_a_slash_separated_child_of_it() {
    assert!(path_is_under("adb://serial", "adb://serial"));
    assert!(path_is_under("adb://serial/sdcard/DCIM", "adb://serial"));
    assert!(path_is_under("/Volumes/naspi/docs", "/Volumes/naspi"));
}

/// ❗ The bug a raw string prefix has: a sibling whose name extends the root's.
#[test]
fn a_sibling_sharing_a_name_prefix_is_not_under() {
    assert!(!path_is_under("adb://serial2", "adb://serial"));
    assert!(!path_is_under("mtp://dev/655370", "mtp://dev/65537"));
    assert!(!path_is_under(
        "sftp://ada@nas:22/srv/data-1",
        "sftp://ada@nas:22/srv/data"
    ));
    assert!(!path_is_under("/Volumes/naspi-1/docs", "/Volumes/naspi"));
}

/// The boot volume's `/` contains every absolute path, and nothing that isn't one: a scheme path
/// the mount table walked up to `/` for is not on the boot volume.
#[test]
fn the_slash_root_holds_absolute_paths_only() {
    assert!(path_is_under("/", "/"));
    assert!(path_is_under("/Users/me", "/"));
    assert!(!path_is_under("search-results://8f3a1c", "/"));
    assert!(!path_is_under("sftp://ada@nas:22/srv", "/"));
}

#[test]
fn a_trailing_slash_on_either_side_is_the_same_root() {
    assert_eq!(path_under("sftp://u@h:22", "sftp://u@h:22/"), Some(""));
    assert_eq!(path_under("sftp://u@h:22/", "sftp://u@h:22"), Some(""));
    assert_eq!(path_under("/Volumes/naspi/docs/", "/Volumes/naspi/"), Some("docs"));
}

#[test]
fn the_part_under_the_root_has_no_edge_separators() {
    assert_eq!(path_under("/Users/me/Docs", "/"), Some("Users/me/Docs"));
    assert_eq!(path_under("/Volumes/naspi/a/b", "/Volumes/naspi"), Some("a/b"));
    assert_eq!(path_under("/elsewhere", "/Volumes/naspi"), None);
}

#[test]
fn rebase_moves_the_folder_onto_the_new_root() {
    assert_eq!(
        rebase("/Volumes/naspi/docs", "/Volumes/naspi", "/Volumes/naspi-1").as_deref(),
        Some("/Volumes/naspi-1/docs")
    );
    assert_eq!(
        rebase("/Volumes/T7", "/Volumes/T7", "/Volumes/Backup").as_deref(),
        Some("/Volumes/Backup")
    );
    assert_eq!(rebase("/Users/me", "/", "/").as_deref(), Some("/Users/me"));
    assert_eq!(rebase("/elsewhere", "/Volumes/naspi", "/Volumes/naspi-1"), None);
}
