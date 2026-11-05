//! `InMemoryVolume::subtree_totals`: the drive index's counting rules, read off
//! the store.

use std::path::{Path, PathBuf};

use super::{HeldAt, InMemoryVolume, SubtreeTotals};
use crate::entry::FileEntry;

fn file(path: &str, size: u64) -> FileEntry {
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    FileEntry {
        size: Some(size),
        ..FileEntry::new(name, path.to_string(), false, false)
    }
}

fn dir(path: &str) -> FileEntry {
    let name = path.rsplit('/').next().unwrap_or(path).to_string();
    FileEntry::new(name, path.to_string(), true, false)
}

fn volume() -> InMemoryVolume {
    let volume = InMemoryVolume::with_entries(
        "test",
        vec![
            dir("/docs"),
            file("/docs/a.txt", 100),
            dir("/docs/deep"),
            dir("/docs/deep/er"),
            file("/docs/deep/er/b.bin", 2_000),
            dir("/docs-sibling"),
            file("/docs-sibling/c.txt", 7),
            dir("/empty"),
        ],
    );
    volume.plant_symlink(Path::new("/docs/deep/link"), Path::new("/docs/a.txt"));
    volume
}

fn ask(volume: &InMemoryVolume, path: &str) -> Option<HeldAt> {
    volume.subtree_totals(&[PathBuf::from(path)]).pop().flatten()
}

#[test]
fn a_folder_counts_everything_below_it_and_not_itself() {
    let Some(HeldAt::Folder(totals)) = ask(&volume(), "/docs") else {
        panic!("`/docs` is a folder");
    };
    // `a.txt`, `b.bin`, and the symlink (a file to the index, with its own size).
    assert_eq!(totals.files, 3);
    assert_eq!(totals.dirs, 2);
    assert!(totals.has_symlinks);
    assert!(
        totals.size >= 2_100,
        "both files' bytes, plus the link's own: {}",
        totals.size
    );
    assert_eq!(
        totals.physical_size, totals.size,
        "no physical size modeled: memory holds the logical bytes"
    );
}

#[test]
fn a_prefix_sibling_is_not_below() {
    let Some(HeldAt::Folder(totals)) = ask(&volume(), "/docs-sibling") else {
        panic!("a folder");
    };
    assert_eq!(
        totals,
        SubtreeTotals {
            size: 7,
            physical_size: 7,
            files: 1,
            dirs: 0,
            has_symlinks: false
        }
    );
}

#[test]
fn the_root_counts_the_whole_store_and_an_empty_folder_counts_zero() {
    let volume = volume();
    let Some(HeldAt::Folder(root)) = ask(&volume, "/") else {
        panic!("the root is always a folder");
    };
    assert_eq!(root.files, 4);
    assert_eq!(root.dirs, 5);
    assert_eq!(ask(&volume, "/empty"), Some(HeldAt::Folder(SubtreeTotals::default())));
}

#[test]
fn a_file_answers_its_own_size_and_a_missing_path_none() {
    let volume = volume();
    assert_eq!(ask(&volume, "/docs/a.txt"), Some(HeldAt::Leaf { size: 100 }));
    assert_eq!(ask(&volume, "/nowhere"), None);
}

#[test]
fn held_entries_lists_every_path_below_the_root_with_its_facts() {
    let mut held = volume().held_entries();
    held.sort_by(|a, b| a.path.cmp(&b.path));
    let paths: Vec<&str> = held.iter().map(|e| e.path.as_str()).collect();
    assert_eq!(
        paths,
        [
            "/docs",
            "/docs-sibling",
            "/docs-sibling/c.txt",
            "/docs/a.txt",
            "/docs/deep",
            "/docs/deep/er",
            "/docs/deep/er/b.bin",
            "/docs/deep/link",
            "/empty",
        ]
    );
    let file = held.iter().find(|e| e.path == "/docs/a.txt").expect("held");
    assert_eq!((file.is_directory, file.size), (false, Some(100)));
    let link = held.iter().find(|e| e.path == "/docs/deep/link").expect("held");
    assert!(link.is_symlink);
}
