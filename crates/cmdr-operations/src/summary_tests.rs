use std::path::{Path, PathBuf};

use super::*;

fn paths(raw: &[&str]) -> Vec<PathBuf> {
    raw.iter().map(PathBuf::from).collect()
}

#[test]
fn a_path_under_home_reads_with_a_tilde() {
    let home = Path::new("/Users/test_user_3");
    assert_eq!(
        tildify(Path::new("/Users/test_user_3/Dropbox"), Some(home)),
        "~/Dropbox"
    );
    assert_eq!(tildify(Path::new("/Users/test_user_3"), Some(home)), "~");
    // A sibling whose name only starts with the home folder's isn't inside it.
    assert_eq!(
        tildify(Path::new("/Users/test_user_3son/x"), Some(home)),
        "/Users/test_user_3son/x"
    );
    assert_eq!(tildify(Path::new("/Volumes/USB"), Some(home)), "/Volumes/USB");
    assert_eq!(tildify(Path::new("/tmp"), None), "/tmp");
}

#[test]
fn a_path_on_a_volume_reads_as_its_name_then_the_path_inside_it() {
    let root = Path::new("s3://key@host:9000/cmdr-s3-test");
    assert_eq!(
        location_on(
            "cmdr-s3-test",
            root,
            Path::new("s3://key@host:9000/cmdr-s3-test/target/folder")
        ),
        "cmdr-s3-test/target/folder"
    );
    assert_eq!(location_on("cmdr-s3-test", root, root), "cmdr-s3-test");
    // A path the starter handed over relative to the volume's root.
    assert_eq!(
        location_on("Phone", Path::new("/"), Path::new("DCIM/Camera")),
        "Phone/DCIM/Camera"
    );
    assert_eq!(location_on("Phone", Path::new("/"), Path::new("/DCIM")), "Phone/DCIM");
}

/// The browser demo's one volume is "Macintosh HD" without being backed by a disk;
/// its labels read as the desktop's own disk's, never `root/Users/you`.
#[test]
fn a_volume_standing_in_for_the_own_disk_reads_like_it() {
    use cmdr_fs::volume::InMemoryVolume;
    let disk = InMemoryVolume::new("Macintosh HD").standing_in_for_own_disk("/Users/you");
    assert_eq!(volume_location(&disk, Path::new("/Users/you")), "~");
    assert_eq!(volume_location(&disk, Path::new("/Users/you/Documents")), "~/Documents");
    assert_eq!(volume_location(&disk, Path::new("/Applications")), "/Applications");
    let phone = InMemoryVolume::new("Phone");
    assert_eq!(volume_location(&phone, Path::new("/DCIM")), "Phone/DCIM");
}

#[test]
fn items_name_the_first_sources_and_count_all_of_them() {
    let sources: Vec<PathBuf> = (0..25).map(|i| PathBuf::from(format!("/a/item-{i}"))).collect();

    let items = operation_items(&sources, None, Some("~/a".into()), Some("~/b".into()));

    assert_eq!(items.count, 25);
    assert_eq!(items.names.len(), ITEM_NAMES_SHOWN);
    assert_eq!(items.names.first().map(String::as_str), Some("item-0"));
    assert_eq!((items.from.as_deref(), items.to.as_deref()), (Some("~/a"), Some("~/b")));
}

#[test]
fn kinds_that_dont_add_up_to_the_sources_are_dropped() {
    let sources = paths(&["/a/x", "/a/y", "/a/z"]);
    let honest = ItemKinds { files: 1, folders: 2 };
    let off = ItemKinds { files: 1, folders: 1 };

    assert_eq!(operation_items(&sources, Some(honest), None, None).kinds, Some(honest));
    assert_eq!(
        operation_items(&sources, Some(off), None, None).kinds,
        None,
        "a split that miscounts would name the wrong things, so the row says \"3 items\" instead"
    );
}

#[test]
fn a_local_summary_comes_from_the_sources_parent_and_goes_to_the_destination() {
    let sources = paths(&["/Volumes/USB/photos/a.jpg", "/Volumes/USB/photos/b.jpg"]);

    let summary = path_summary(
        &sources,
        Some(Path::new("/Volumes/Backup/2026")),
        Some(ItemKinds { files: 2, folders: 0 }),
    );

    let items = summary.items.expect("a transfer names its items");
    assert_eq!(items.from.as_deref(), Some("/Volumes/USB/photos"));
    assert_eq!(items.to.as_deref(), Some("/Volumes/Backup/2026"));
    assert_eq!(items.kinds, Some(ItemKinds { files: 2, folders: 0 }));
}

#[test]
fn a_delete_summary_has_no_destination() {
    let summary = path_summary(&paths(&["/tmp/x"]), None, None);

    assert_eq!(summary.items.expect("a delete names its items").to, None);
}
