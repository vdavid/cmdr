//! Which column a listing's rows are actually ordered by, and when a re-stat
//! changes a row's place in that order.
//!
//! Covers `EffectiveSort::resolve` (the one fallback rule: a date the volume
//! doesn't record sorts by Modified), the Added column's own order, and
//! `sort_key_changed`, the predicate that decides whether an updated row is
//! re-placed. How a whole listing behaves in a fallback is `caching_test` and
//! `effective_sort_listing_test`.

use cmdr_fs::entry::FileEntry;
use cmdr_fs::volume::SupportedDates;

use cmdr_sort::{DirectorySortMode, SortColumn, SortOrder};

use super::sorting::{EffectiveSort, sort_key_changed};
use super::sorting_test_support::{every_date, make_dir_with_recursive_size, make_entry, sort_entries};

const ALL_COLUMNS: [SortColumn; 6] = [
    SortColumn::Name,
    SortColumn::Extension,
    SortColumn::Size,
    SortColumn::Modified,
    SortColumn::Created,
    SortColumn::Added,
];

fn modified_only() -> SupportedDates {
    SupportedDates::NONE
}

fn added_only() -> SupportedDates {
    SupportedDates {
        created: false,
        added: true,
    }
}

/// The whole truth table: every requested column on every combination of
/// recorded dates. Only an unrecorded Created or Added changes the column.
#[test]
fn resolve_falls_back_to_modified_only_for_an_unrecorded_date() {
    for supported in [
        SupportedDates::ALL,
        SupportedDates::CREATED,
        added_only(),
        modified_only(),
    ] {
        for requested in ALL_COLUMNS {
            let expected = match requested {
                SortColumn::Created if !supported.created => SortColumn::Modified,
                SortColumn::Added if !supported.added => SortColumn::Modified,
                other => other,
            };
            let effective = EffectiveSort::resolve(
                requested,
                SortOrder::Descending,
                DirectorySortMode::AlwaysByName,
                supported,
            );
            assert_eq!(effective.column(), expected, "{requested:?} on {supported:?}");
        }
    }

    // Spelled out, so a mistake in the table above can't hide one.
    let resolve = |requested, supported| {
        EffectiveSort::resolve(requested, SortOrder::Ascending, DirectorySortMode::LikeFiles, supported).column()
    };
    assert_eq!(
        resolve(SortColumn::Added, SupportedDates::CREATED),
        SortColumn::Modified
    );
    assert_eq!(resolve(SortColumn::Created, added_only()), SortColumn::Modified);
    assert_eq!(resolve(SortColumn::Added, added_only()), SortColumn::Added);
    assert_eq!(resolve(SortColumn::Created, SupportedDates::ALL), SortColumn::Created);
    assert_eq!(resolve(SortColumn::Size, modified_only()), SortColumn::Size);
}

/// The order and directory mode are the pane's in a fallback too: clicking the
/// header flips the rows the user sees.
#[test]
fn resolve_keeps_the_order_and_directory_mode_in_a_fallback() {
    let effective = EffectiveSort::resolve(
        SortColumn::Added,
        SortOrder::Descending,
        DirectorySortMode::MixedWithFiles,
        modified_only(),
    );
    assert_eq!(effective.column(), SortColumn::Modified);
    assert_eq!(effective.order(), SortOrder::Descending);
    assert_eq!(effective.dir_mode(), DirectorySortMode::MixedWithFiles);
}

fn added(name: &str, added_at: Option<u64>, modified: u64) -> FileEntry {
    FileEntry {
        added_at,
        ..make_entry(name, false, Some(100), Some(modified))
    }
}

/// Added orders by `added_at`, with the same unknown-date arms as Created: an
/// undated row sorts below every dated one ascending, above descending, and by
/// name among its kind.
#[test]
fn added_sorts_by_added_date_with_unknown_dates_first_ascending() {
    // Modified runs the OTHER way, so a sort by mtime can't pass for this one.
    let fresh = || {
        vec![
            added("added_new.txt", Some(1_700_000_002), 1),
            added("undated_b.txt", None, 2),
            added("added_old.txt", Some(1_700_000_001), 3),
            added("undated_a.txt", None, 4),
        ]
    };

    let mut ascending = fresh();
    sort_entries(
        &mut ascending,
        SortColumn::Added,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );
    let names: Vec<&str> = ascending.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        ["undated_a.txt", "undated_b.txt", "added_old.txt", "added_new.txt"]
    );

    let mut descending = fresh();
    sort_entries(
        &mut descending,
        SortColumn::Added,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
    );
    let names: Vec<&str> = descending.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        ["added_new.txt", "added_old.txt", "undated_b.txt", "undated_a.txt"]
    );
}

/// A fallback sorts by mtime, not by the date the volume doesn't record.
#[test]
fn a_fallback_sorts_by_modified() {
    let mut entries = vec![
        added("added_first.txt", Some(1), 300),
        added("added_last.txt", Some(3), 100),
        added("added_middle.txt", Some(2), 200),
    ];
    let collator = super::testing::collator_for("en");
    let sort = EffectiveSort::resolve(
        SortColumn::Added,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        modified_only(),
    );
    super::sorting::sort_entries(&collator, &mut entries, sort);
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["added_last.txt", "added_middle.txt", "added_first.txt"]);
}

/// A folder macOS calls a package gains an extension under the same name, so
/// it moves under the Ext sort and nowhere else.
#[test]
fn sort_key_changed_sees_a_folder_become_a_package_under_ext_only() {
    let folder = make_entry("Sketch.mybundle", true, None, None);
    let mut package = folder.clone();
    package.set_package(true);
    let by = |column| every_date(column, SortOrder::Ascending, DirectorySortMode::LikeFiles);

    assert!(sort_key_changed(&folder, &package, &by(SortColumn::Extension)));
    assert!(!sort_key_changed(&folder, &package, &by(SortColumn::Name)));
    assert!(!sort_key_changed(&folder, &package, &by(SortColumn::Size)));
}

/// Each date column's key is its own date: a change to it re-places the row,
/// a change to another date doesn't.
#[test]
fn sort_key_changed_reads_the_effective_columns_date() {
    let base = FileEntry {
        created_at: Some(10),
        added_at: Some(20),
        ..make_entry("a.txt", false, Some(100), Some(30))
    };
    let created = FileEntry {
        created_at: Some(11),
        ..base.clone()
    };
    let added = FileEntry {
        added_at: Some(21),
        ..base.clone()
    };
    let modified = FileEntry {
        modified_at: Some(31),
        ..base.clone()
    };
    let by = |column| every_date(column, SortOrder::Ascending, DirectorySortMode::LikeFiles);

    assert!(sort_key_changed(&base, &created, &by(SortColumn::Created)));
    assert!(sort_key_changed(&base, &added, &by(SortColumn::Added)));
    assert!(sort_key_changed(&base, &modified, &by(SortColumn::Modified)));

    assert!(!sort_key_changed(&base, &created, &by(SortColumn::Added)));
    assert!(!sort_key_changed(&base, &added, &by(SortColumn::Created)));
    assert!(!sort_key_changed(&base, &modified, &by(SortColumn::Name)));

    // In an Added fallback the rows are in mtime order, so mtime is the key.
    let fallback = EffectiveSort::resolve(
        SortColumn::Added,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
        modified_only(),
    );
    assert!(sort_key_changed(&base, &modified, &fallback));
    assert!(!sort_key_changed(&base, &added, &fallback));
}

/// A directory's Size key is its recursive size (and whether that's exact),
/// under every directory mode that ranks it by size.
#[test]
fn sort_key_changed_sees_a_directorys_recursive_size() {
    let old = make_dir_with_recursive_size("dir", Some(100), None);
    let grown = make_dir_with_recursive_size("dir", Some(5000), None);
    let finished = FileEntry {
        recursive_size_complete: Some(true),
        ..old.clone()
    };
    for mode in [DirectorySortMode::LikeFiles, DirectorySortMode::MixedWithFiles] {
        let size = every_date(SortColumn::Size, SortOrder::Ascending, mode);
        assert!(sort_key_changed(&old, &grown, &size), "{mode:?}: recursive size");
        assert!(sort_key_changed(&old, &finished, &size), "{mode:?}: completeness");
    }
}

/// A kind change always re-places the row: folders lead.
#[test]
fn sort_key_changed_sees_a_kind_change_under_every_column() {
    let file = make_entry("x", false, Some(100), Some(1));
    let folder = make_entry("x", true, Some(100), Some(1));
    for column in ALL_COLUMNS {
        let sort = every_date(column, SortOrder::Ascending, DirectorySortMode::LikeFiles);
        assert!(sort_key_changed(&file, &folder, &sort), "{column:?}");
    }
}
