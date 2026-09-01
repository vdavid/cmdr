//! The shared conflict pairing every listing-based backend reports through.

use super::super::SourceItemInfo;
use super::conflicts_in_listing;
use crate::entry::FileEntry;

fn dest_entry(name: &str, is_dir: bool, size: Option<u64>, modified_at: Option<u64>) -> FileEntry {
    let mut entry = FileEntry::new(name.to_string(), format!("/dest/{name}"), is_dir, false);
    entry.size = size;
    entry.modified_at = modified_at;
    entry
}

fn source_item(name: &str, is_directory: bool) -> SourceItemInfo {
    SourceItemInfo {
        name: name.to_string(),
        size: 7,
        modified: Some(1_700_000_000),
        is_directory,
    }
}

/// A same-named destination entry becomes one conflict carrying both sides'
/// facts; a missing destination size reads as zero.
#[test]
fn a_same_named_entry_is_a_conflict_with_both_sides_mapped() {
    let dest = [dest_entry("report.txt", false, None, Some(1_600_000_000))];
    let conflicts = conflicts_in_listing(&[source_item("report.txt", false)], &dest);

    assert_eq!(conflicts.len(), 1);
    let c = &conflicts[0];
    assert_eq!(c.source_path, "report.txt");
    assert_eq!(c.dest_path, "/dest/report.txt");
    assert_eq!((c.source_size, c.dest_size), (7, 0));
    assert_eq!(
        (c.source_modified, c.dest_modified),
        (Some(1_700_000_000), Some(1_600_000_000))
    );
    assert!(!c.source_is_directory);
    assert!(!c.dest_is_directory);
}

/// Directory-ness comes from each side separately, so the frontend can tell a
/// dir-over-dir merge from a dir-over-file collision.
#[test]
fn directory_flags_are_taken_from_each_side() {
    let dest = [dest_entry("Photos", false, Some(12), None)];
    let conflicts = conflicts_in_listing(&[source_item("Photos", true)], &dest);

    assert_eq!(conflicts.len(), 1);
    assert!(conflicts[0].source_is_directory);
    assert!(!conflicts[0].dest_is_directory);
    assert_eq!(conflicts[0].dest_size, 12);
}

/// Items with no same-named destination entry are silent, and the result keeps
/// the source order.
#[test]
fn unmatched_items_produce_nothing_and_order_follows_the_sources() {
    let dest = [dest_entry("b", false, None, None), dest_entry("a", true, None, None)];
    let items = [
        source_item("a", true),
        source_item("zzz", false),
        source_item("b", false),
    ];
    let conflicts = conflicts_in_listing(&items, &dest);

    let names: Vec<&str> = conflicts.iter().map(|c| c.source_path.as_str()).collect();
    assert_eq!(names, ["a", "b"]);
}
