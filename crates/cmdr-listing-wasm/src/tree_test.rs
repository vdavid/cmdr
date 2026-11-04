use std::path::Path;

use cmdr_fs::volume::Volume;

use crate::tree::{DemoEntry, DemoKind, DemoTree, TreeError};

fn entry(path: &str, kind: DemoKind) -> DemoEntry {
    DemoEntry {
        path: path.to_string(),
        kind,
        size: None,
        modified_ms: None,
        created_ms: None,
        symlink_target: None,
    }
}

fn tree(entries: Vec<DemoEntry>) -> DemoTree {
    DemoTree {
        volume_id: "root".into(),
        volume_name: None,
        home: None,
        free_bytes: None,
        root: "/".into(),
        entries,
    }
}

/// With a home, the volume stands in for the computer's own disk, so a queue row
/// reads `~`, as the desktop's does, never `root/Users/you`.
#[test]
fn a_home_makes_the_volume_stand_in_for_the_own_disk() {
    let mut with_home = tree(vec![entry("/Users", DemoKind::Dir)]);
    with_home.home = Some("/Users/you".into());
    let volume = with_home.into_in_memory().expect("builds");
    assert_eq!(volume.stands_in_for_own_disk(), Some(Path::new("/Users/you")));
    let plain = tree(vec![]).into_in_memory().expect("builds");
    assert_eq!(plain.stands_in_for_own_disk(), None);
}

async fn list(volume: &dyn Volume, path: &str) -> Vec<cmdr_fs::entry::FileEntry> {
    let mut entries = volume.list_directory(Path::new(path), None).await.expect("lists");
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    entries
}

#[tokio::test]
async fn the_page_shape_parses_and_lists_by_folder() {
    let json = serde_json::json!({
        "volumeId": "root",
        "root": "/",
        "entries": [
            { "path": "/Users", "kind": "dir" },
            { "path": "/Users/you", "kind": "dir", "modifiedMs": 1_700_000_000_123u64 },
            { "path": "/Users/you/report.pdf", "kind": "file", "size": 4096, "modifiedMs": 1_700_000_000_999u64, "createdMs": 1_600_000_000_000u64 },
            { "path": "/Users/you/.zshrc", "kind": "file", "size": 12 },
        ],
    });
    let tree: DemoTree = serde_json::from_value(json).expect("the page's shape parses");
    let volume = tree.into_volume().expect("a valid tree");

    let rows = list(volume.as_ref(), "/Users/you").await;
    let names: Vec<&str> = rows.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, [".zshrc", "report.pdf"]);
    let report = &rows[1];
    assert_eq!(report.size, Some(4096));
    assert_eq!(report.modified_at, Some(1_700_000_000), "milliseconds become seconds");
    assert_eq!(report.created_at, Some(1_600_000_000));
    assert!(rows[0].is_hidden, "a dotfile is hidden by its name");
    assert_eq!(list(volume.as_ref(), "/").await.len(), 1);
}

#[tokio::test]
async fn a_symlink_to_a_folder_lists_as_a_folder() {
    let mut link = entry("/docs", DemoKind::Symlink);
    link.symlink_target = Some("/real".into());
    let mut dangling = entry("/nowhere", DemoKind::Symlink);
    dangling.symlink_target = Some("/missing".into());
    let volume = tree(vec![entry("/real", DemoKind::Dir), link, dangling])
        .into_volume()
        .expect("a valid tree");

    let rows = list(volume.as_ref(), "/").await;
    let docs = rows.iter().find(|e| e.name == "docs").expect("docs");
    assert!(docs.is_symlink && docs.is_directory);
    let nowhere = rows.iter().find(|e| e.name == "nowhere").expect("nowhere");
    assert!(nowhere.is_symlink && !nowhere.is_directory);
}

#[test]
fn a_tree_missing_a_parent_is_refused() {
    let refused = tree(vec![entry("/a/b.txt", DemoKind::File)]).into_volume().err();
    assert_eq!(refused, Some(TreeError::MissingParent("/a/b.txt".into())));
}

#[test]
fn a_file_cant_be_a_parent() {
    let refused = tree(vec![entry("/a", DemoKind::File), entry("/a/b", DemoKind::File)])
        .into_volume()
        .err();
    assert_eq!(refused, Some(TreeError::MissingParent("/a/b".into())));
}

#[test]
fn a_duplicate_path_is_refused() {
    let refused = tree(vec![entry("/a", DemoKind::Dir), entry("/a", DemoKind::File)])
        .into_volume()
        .err();
    assert_eq!(refused, Some(TreeError::Duplicate("/a".into())));
}

#[test]
fn relative_and_trailing_slash_paths_are_refused() {
    for bad in ["a", "/a/", "/"] {
        let refused = tree(vec![entry(bad, DemoKind::Dir)]).into_volume().err();
        assert_eq!(refused, Some(TreeError::BadPath(bad.into())), "{bad}");
    }
}

fn photo_tree() -> DemoTree {
    let mut photo = entry("/photo.jpg", DemoKind::File);
    photo.size = Some(200_000);
    tree(vec![photo])
}

#[tokio::test]
async fn a_file_reads_as_many_bytes_as_its_listing_says() {
    let volume = photo_tree().into_volume().expect("a valid tree");
    let mut stream = volume.open_read_stream(Path::new("/photo.jpg")).await.expect("opens");
    let mut read = 0;
    while let Some(chunk) = stream.next_chunk().await {
        read += chunk.expect("reads").len();
    }
    assert_eq!(read, 200_000);

    // The same tree generates the same bytes.
    let again = photo_tree().into_volume().expect("a valid tree");
    let first = volume
        .read_range(Path::new("/photo.jpg"), 1_000, 64)
        .await
        .expect("reads");
    let second = again
        .read_range(Path::new("/photo.jpg"), 1_000, 64)
        .await
        .expect("reads");
    assert_eq!(first, second);
    assert_eq!(first.len(), 64);
}
