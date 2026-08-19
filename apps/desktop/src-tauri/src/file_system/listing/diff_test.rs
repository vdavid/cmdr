//! Tests for `compute_diff`: what a full re-read reports as added, removed,
//! modified, and moved.

use super::FileEntry;
use super::diff::{DiffChangeType, compute_diff};

fn make_entry(name: &str, size: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        permissions: 0o644,
        owner: "user".to_string(),
        group: "group".to_string(),
        extended_metadata_loaded: true,
        ..FileEntry::new(name.to_string(), format!("/test/{}", name), false, false)
    }
}

#[test]
fn test_compute_diff_addition() {
    let old = vec![make_entry("a.txt", Some(100))];
    let new = vec![make_entry("a.txt", Some(100)), make_entry("b.txt", Some(200))];

    let diff = compute_diff(&old, &new);
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].change_type, DiffChangeType::Add);
    assert_eq!(diff[0].entry.name, "b.txt");
    assert_eq!(diff[0].index, 1); // index in new listing
}

#[test]
fn test_compute_diff_removal() {
    let old = vec![make_entry("a.txt", Some(100)), make_entry("b.txt", Some(200))];
    let new = vec![make_entry("a.txt", Some(100))];

    let diff = compute_diff(&old, &new);
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].change_type, DiffChangeType::Remove);
    assert_eq!(diff[0].entry.name, "b.txt");
    assert_eq!(diff[0].index, 1); // index in old listing
}

#[test]
fn test_compute_diff_modification() {
    let old = vec![make_entry("a.txt", Some(100))];
    let new = vec![make_entry("a.txt", Some(200))]; // Size changed

    let diff = compute_diff(&old, &new);
    assert_eq!(diff.len(), 1);
    assert_eq!(diff[0].change_type, DiffChangeType::Modify);
    assert_eq!(diff[0].entry.size, Some(200));
    assert_eq!(diff[0].index, 0); // index in new listing
}

#[test]
fn test_compute_diff_no_change() {
    let old = vec![make_entry("a.txt", Some(100))];
    let new = vec![make_entry("a.txt", Some(100))];

    let diff = compute_diff(&old, &new);
    assert!(diff.is_empty());
}

// ============================================================================
// is_entry_modified axis coverage (cargo-mutants survivors)
// ============================================================================
//
// is_entry_modified returns true when ANY of: size, modified_at, permissions,
// is_directory, is_symlink differ. The existing tests only varied size, so
// the `||` chain mutants (each → `&&`) and the per-field `!= → ==` mutants
// all survived. These tests pin each axis individually.
//
// is_entry_modified is private; we exercise it via compute_diff, which marks
// the entry as "modify" only when is_entry_modified returns true.

#[test]
fn diff_marks_entry_modified_when_modified_at_differs() {
    let mut old_entry = make_entry("a.txt", Some(100));
    let mut new_entry = make_entry("a.txt", Some(100));
    old_entry.modified_at = Some(1000);
    new_entry.modified_at = Some(2000);
    let diff = compute_diff(&[old_entry], &[new_entry]);
    assert_eq!(diff.len(), 1, "modified_at change should produce a modify diff");
    assert_eq!(diff[0].change_type, DiffChangeType::Modify);
}

#[test]
fn diff_marks_entry_modified_when_permissions_differ() {
    let mut old_entry = make_entry("a.txt", Some(100));
    let mut new_entry = make_entry("a.txt", Some(100));
    old_entry.permissions = 0o644;
    new_entry.permissions = 0o755;
    let diff = compute_diff(&[old_entry], &[new_entry]);
    assert_eq!(diff.len(), 1, "permissions change should produce a modify diff");
    assert_eq!(diff[0].change_type, DiffChangeType::Modify);
}

#[test]
fn diff_marks_entry_modified_when_is_directory_flips() {
    // Same path/name but the entry transitioned from file → directory
    // (atomic-replace of a file with a dir of the same name). The watcher
    // should report this as a modify so the UI rerenders the icon and clears
    // the size column. Kills the `is_directory != → ==` mutant and the
    // `|| → &&` mutant on its line.
    let old_entry = FileEntry {
        is_directory: false,
        ..make_entry("thing", Some(100))
    };
    let new_entry = FileEntry {
        is_directory: true,
        size: Some(100),
        ..make_entry("thing", Some(100))
    };
    let diff = compute_diff(&[old_entry], &[new_entry]);
    assert_eq!(diff.len(), 1, "is_directory flip should produce a modify diff");
    assert_eq!(diff[0].change_type, DiffChangeType::Modify);
}

#[test]
fn diff_marks_entry_modified_when_is_symlink_flips() {
    let old_entry = FileEntry {
        is_symlink: false,
        ..make_entry("thing", Some(100))
    };
    let new_entry = FileEntry {
        is_symlink: true,
        ..make_entry("thing", Some(100))
    };
    let diff = compute_diff(&[old_entry], &[new_entry]);
    assert_eq!(diff.len(), 1, "is_symlink flip should produce a modify diff");
    assert_eq!(diff[0].change_type, DiffChangeType::Modify);
}

#[test]
fn diff_does_not_mark_modified_when_only_owner_or_group_change() {
    // Negative case for the `|| → &&` mutants on every axis: if any of those
    // flipped, this test (which only changes a field is_entry_modified
    // doesn't watch) would suddenly start producing a modify diff.
    let mut old_entry = make_entry("a.txt", Some(100));
    let mut new_entry = make_entry("a.txt", Some(100));
    old_entry.owner = "alice".to_string();
    new_entry.owner = "bob".to_string();
    new_entry.group = "wheel".to_string();
    let diff = compute_diff(&[old_entry], &[new_entry]);
    assert!(
        diff.is_empty(),
        "owner/group changes alone must NOT trigger a modify diff (is_entry_modified watches only size, mtime, perms, kind, symlink)"
    );
}

// ============================================================================
// compute_diff structural pins (mixed adds + removes + modifies)
// ============================================================================

#[test]
fn diff_includes_add_modify_and_remove_in_one_pass() {
    // Old: a.txt (size 100), b.txt (size 200)
    // New: a.txt (size 300, modified), c.txt (size 50, added)
    // → 3 changes: remove b, modify a, add c.
    // Also pins the index semantics: removes use the OLD index, adds/modifies use the NEW index.
    let old = vec![make_entry("a.txt", Some(100)), make_entry("b.txt", Some(200))];
    let new = vec![make_entry("a.txt", Some(300)), make_entry("c.txt", Some(50))];

    let diff = compute_diff(&old, &new);
    assert_eq!(diff.len(), 3, "expected add + modify + remove");

    let by_type: std::collections::HashMap<DiffChangeType, &super::diff::DiffChange> =
        diff.iter().map(|c| (c.change_type, c)).collect();
    let modify = by_type.get(&DiffChangeType::Modify).expect("modify present");
    let add = by_type.get(&DiffChangeType::Add).expect("add present");
    let remove = by_type.get(&DiffChangeType::Remove).expect("remove present");

    assert_eq!(modify.entry.name, "a.txt");
    assert_eq!(modify.index, 0, "modify uses NEW index");
    assert_eq!(add.entry.name, "c.txt");
    assert_eq!(add.index, 1, "add uses NEW index");
    assert_eq!(remove.entry.name, "b.txt");
    assert_eq!(remove.index, 1, "remove uses OLD index");
}

#[test]
fn diff_reports_a_row_that_jumped_the_queue_as_a_move() {
    // A date-sorted pane while a big folder is being deleted: the folder's own mtime
    // keeps bumping, so it jumps to the top. Reported as a move, the pane can ride the
    // cursor along; reported as a remove plus an add, the cursor would stay behind on
    // whoever took the vacated row.
    let old = vec![
        make_entry("a.txt", Some(100)),
        make_entry("b.txt", Some(100)),
        make_entry("c.txt", Some(100)),
        make_entry("jumper", Some(100)),
    ];
    let mut jumper = make_entry("jumper", Some(100));
    jumper.modified_at = Some(9000);
    let new = vec![
        jumper,
        make_entry("a.txt", Some(100)),
        make_entry("b.txt", Some(100)),
        make_entry("c.txt", Some(100)),
    ];

    let diff = compute_diff(&old, &new);
    assert_eq!(diff.len(), 1, "only the row that jumped changed");
    assert_eq!(diff[0].change_type, DiffChangeType::Move);
    assert_eq!(diff[0].entry.name, "jumper");
    assert_eq!(diff[0].previous_index, Some(3), "move carries where the row sat");
    assert_eq!(diff[0].index, 0, "move uses NEW index");
    assert_eq!(diff[0].entry.modified_at, Some(9000), "a move carries the fresh entry");
}

#[test]
fn diff_calls_only_one_side_of_a_swap_moved() {
    // Two rows trade places. Marking both would double-count the shift; the minimal
    // set is one, and the other row's slide falls out of the remove + add arithmetic.
    let old = vec![make_entry("a.txt", Some(100)), make_entry("b.txt", Some(100))];
    let mut bumped = make_entry("b.txt", Some(100));
    bumped.modified_at = Some(9000);
    let new = vec![bumped, make_entry("a.txt", Some(100))];

    let diff = compute_diff(&old, &new);
    let moves: Vec<_> = diff.iter().filter(|c| c.change_type == DiffChangeType::Move).collect();
    assert_eq!(moves.len(), 1, "a swap is one move, not two");
}

#[test]
fn diff_does_not_call_the_rows_an_add_or_a_remove_shifted_moved() {
    // Every row below an insertion or a deletion changes index without moving. Calling
    // those moves would make the pane chase the cursor around on any ordinary change.
    let old = vec![
        make_entry("b.txt", Some(100)),
        make_entry("c.txt", Some(100)),
        make_entry("d.txt", Some(100)),
    ];
    let new = vec![
        make_entry("a.txt", Some(100)),
        make_entry("b.txt", Some(100)),
        make_entry("d.txt", Some(100)),
    ];

    let diff = compute_diff(&old, &new);
    assert!(
        diff.iter().all(|c| c.change_type != DiffChangeType::Move),
        "an add above and a remove in the middle shift rows, they don't move them"
    );
    assert_eq!(diff.len(), 2, "expected exactly the add and the remove");
}
