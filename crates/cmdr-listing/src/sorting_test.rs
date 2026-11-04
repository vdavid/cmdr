//! Which column a listing sorts by, and in which direction.
//!
//! Covers `sort_entries` and the `entry_comparator` behind it: name, extension,
//! size, modified and created date, directories-first, and the edge cases
//! (empty lists, symlinks, absent sizes, dotfiles).
//!
//! Two neighbours own the questions this suite deliberately doesn't ask: how a
//! directory ranks against the files beside it is `sorting_dir_mode_test`, and
//! the order names themselves collate in is `collation_test`.

use super::sorting_test_support::{entry_comparator, make_dir_with_recursive_size, make_entry, sort_entries};
use super::{SortColumn, SortOrder};
use cmdr_fs::entry::FileEntry;
use cmdr_sort::DirectorySortMode;

// ============================================================================
// Natural sorting tests (alphanumeric)
// ============================================================================

#[test]
fn test_natural_sort_by_name() {
    let mut entries = vec![
        make_entry("img_10.jpg", false, Some(100), None),
        make_entry("img_2.jpg", false, Some(100), None),
        make_entry("img_1.jpg", false, Some(100), None),
        make_entry("img_20.jpg", false, Some(100), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["img_1.jpg", "img_2.jpg", "img_10.jpg", "img_20.jpg"]);
}

#[test]
fn test_natural_sort_descending() {
    let mut entries = vec![
        make_entry("file1.txt", false, Some(100), None),
        make_entry("file2.txt", false, Some(100), None),
        make_entry("file10.txt", false, Some(100), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["file10.txt", "file2.txt", "file1.txt"]);
}

// ============================================================================
// Directories first tests
// ============================================================================

#[test]
fn test_directories_first() {
    let mut entries = vec![
        make_entry("zebra.txt", false, Some(100), None),
        make_entry("alpha", true, None, None),
        make_entry("apple.txt", false, Some(100), None),
        make_entry("docs", true, None, None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    // Directories first, then files, both sorted alphabetically
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["alpha", "docs", "apple.txt", "zebra.txt"]);
}

#[test]
fn test_directories_first_descending() {
    let mut entries = vec![
        make_entry("zebra.txt", false, Some(100), None),
        make_entry("alpha", true, None, None),
        make_entry("apple.txt", false, Some(100), None),
        make_entry("docs", true, None, None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
    );

    // Directories still first, but both groups sorted descending
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["docs", "alpha", "zebra.txt", "apple.txt"]);
}

// ============================================================================
// Extension sorting tests
// ============================================================================

#[test]
fn test_sort_by_extension() {
    let mut entries = vec![
        make_entry("file.txt", false, Some(100), None),
        make_entry("script.js", false, Some(100), None),
        make_entry("readme", false, Some(100), None),     // No extension
        make_entry(".gitignore", false, Some(100), None), // Dotfile
        make_entry("config.json", false, Some(100), None),
        make_entry(".bashrc", false, Some(100), None), // Dotfile
    ];

    sort_entries(
        &mut entries,
        SortColumn::Extension,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    // Order: dotfiles first, then no extension, then by extension alphabetically
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            ".bashrc",     // dotfile
            ".gitignore",  // dotfile
            "readme",      // no extension
            "script.js",   // .js
            "config.json", // .json
            "file.txt",    // .txt
        ]
    );
}

/// The product's one extension rule (`cmdr_fs::file_extension`): what follows a
/// name's last dot is only an extension when it's short and plain, a compound
/// tar counts as one, and a folder's dot is part of its name. So the Ext column
/// and this order never disagree.
#[test]
fn extension_sort_follows_the_product_extension_rule() {
    let mut entries = vec![
        make_entry("Smlouva v. 2 final", false, Some(100), None),
        make_entry("photo.jpg_old", false, Some(100), None),
        make_entry("src.tar.gz", false, Some(100), None),
        make_entry("other.gz", false, Some(100), None),
        make_entry("alpha.tar", false, Some(100), None),
        make_entry("release.zz", true, None, None),
        make_entry("Project 1.5 backup", true, None, None),
        make_entry("Cmdr.app", true, None, None),
        make_entry("plain", true, None, None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Extension,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        vec![
            "plain",              // folder, no extension
            "Project 1.5 backup", // folder, its dot is part of the name
            "release.zz",         // same
            "Cmdr.app",           // package: `app`
            "photo.jpg_old",      // no extension: `_` isn't plain
            "Smlouva v. 2 final", // no extension
            "other.gz",           // gz
            "alpha.tar",          // tar
            "src.tar.gz",         // tar.gz
        ]
    );
}

/// A folder macOS called a package sorts by its extension, though the package
/// list doesn't know it: the sort reads the entry's `is_package`, not the name.
#[test]
fn extension_sort_reads_a_detected_package_off_the_entry() {
    let mut detected = make_entry("Sketch.mybundle", true, None, None);
    detected.set_package(true);
    let mut entries = vec![
        detected,
        make_entry("Cmdr.app", true, None, None),
        make_entry("Notes.zz", true, None, None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Extension,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["Notes.zz", "Cmdr.app", "Sketch.mybundle"]);
}

#[test]
fn test_extension_sort_same_ext_by_name() {
    let mut entries = vec![
        make_entry("zebra.txt", false, Some(100), None),
        make_entry("alpha.txt", false, Some(100), None),
        make_entry("beta.txt", false, Some(100), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Extension,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    // Same extension - fall back to name sorting
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["alpha.txt", "beta.txt", "zebra.txt"]);
}

// ============================================================================
// Size sorting tests
// ============================================================================

#[test]
fn test_sort_by_size_ascending() {
    let mut entries = vec![
        make_entry("medium.txt", false, Some(500), None),
        make_entry("large.txt", false, Some(1000), None),
        make_entry("small.txt", false, Some(100), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["small.txt", "medium.txt", "large.txt"]);
}

#[test]
fn test_sort_by_size_descending() {
    let mut entries = vec![
        make_entry("medium.txt", false, Some(500), None),
        make_entry("large.txt", false, Some(1000), None),
        make_entry("small.txt", false, Some(100), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["large.txt", "medium.txt", "small.txt"]);
}

#[test]
fn test_sort_by_size_with_directories() {
    let mut entries = vec![
        make_entry("dir_b", true, None, None),
        make_entry("medium.txt", false, Some(500), None),
        make_entry("dir_a", true, None, None),
        make_entry("small.txt", false, Some(100), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    // Directories first (sorted by name), then files by size
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["dir_a", "dir_b", "small.txt", "medium.txt"]);
}

// ============================================================================
// Modified date sorting tests
// ============================================================================

#[test]
fn test_sort_by_modified_ascending() {
    let mut entries = vec![
        make_entry("newest.txt", false, Some(100), Some(1700000003)),
        make_entry("oldest.txt", false, Some(100), Some(1700000001)),
        make_entry("middle.txt", false, Some(100), Some(1700000002)),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Modified,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["oldest.txt", "middle.txt", "newest.txt"]);
}

#[test]
fn test_sort_by_modified_descending() {
    let mut entries = vec![
        make_entry("newest.txt", false, Some(100), Some(1700000003)),
        make_entry("oldest.txt", false, Some(100), Some(1700000001)),
        make_entry("middle.txt", false, Some(100), Some(1700000002)),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Modified,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["newest.txt", "middle.txt", "oldest.txt"]);
}

#[test]
fn test_sort_by_modified_with_none() {
    let mut entries = vec![
        make_entry("has_date.txt", false, Some(100), Some(1700000001)),
        make_entry("no_date.txt", false, Some(100), None),
        make_entry("also_has.txt", false, Some(100), Some(1700000002)),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Modified,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    // None comes first
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["no_date.txt", "has_date.txt", "also_has.txt"]);
}

// ============================================================================
// Edge cases
// ============================================================================

#[test]
fn test_empty_list() {
    let mut entries: Vec<FileEntry> = vec![];
    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );
    assert!(entries.is_empty());
}

#[test]
fn test_single_entry() {
    let mut entries = vec![make_entry("only.txt", false, Some(100), None)];
    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "only.txt");
}

// ============================================================================
// Additional edge cases
// ============================================================================

#[test]
fn test_unicode_filenames() {
    let mut entries = vec![
        make_entry("日本語.txt", false, Some(100), None),
        make_entry("αβγ.txt", false, Some(100), None),
        make_entry("emoji_🎉.txt", false, Some(100), None),
        make_entry("中文.txt", false, Some(100), None),
    ];

    // Should not panic and should produce a stable sort
    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    assert_eq!(entries.len(), 4);
}

#[test]
fn test_long_filenames() {
    let long_name_a = "a".repeat(255);
    let long_name_z = "z".repeat(255);

    let mut entries = vec![
        make_entry(&long_name_z, false, Some(100), None),
        make_entry(&long_name_a, false, Some(100), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    assert_eq!(entries[0].name, long_name_a);
    assert_eq!(entries[1].name, long_name_z);
}

/// Creates a test entry that is a symlink
fn make_symlink(name: &str, size: Option<u64>) -> FileEntry {
    FileEntry {
        size,
        permissions: 0o777,
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        icon_id: "symlink".to_string(),
        ..FileEntry::new(name.to_string(), format!("/{}", name), false, true)
    }
}

#[test]
fn test_symlinks_sorted_as_files() {
    let mut entries = vec![
        make_entry("dir", true, None, None),
        make_symlink("link_to_something.txt", Some(100)),
        make_entry("file.txt", false, Some(100), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    // Directories first, then symlinks and files sorted together by name
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["dir", "file.txt", "link_to_something.txt"]);
}

#[test]
fn test_size_with_none_values() {
    let mut entries = vec![
        make_entry("has_size.txt", false, Some(100), None),
        make_entry("no_size.txt", false, None, None), // Rare but possible
        make_entry("big_size.txt", false, Some(1000), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    // None comes first (treated as 0 or less than any size)
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["no_size.txt", "has_size.txt", "big_size.txt"]);
}

#[test]
fn test_size_descending_with_none() {
    let mut entries = vec![
        make_entry("big.txt", false, Some(1000), None),
        make_entry("none.txt", false, None, None),
        make_entry("small.txt", false, Some(100), None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
    );

    // Descending: big first, then small, then None last
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["big.txt", "small.txt", "none.txt"]);
}

#[test]
fn test_sort_by_created() {
    let mut entries = vec![
        make_entry("newest.txt", false, Some(100), Some(1700000003)),
        make_entry("oldest.txt", false, Some(100), Some(1700000001)),
        make_entry("middle.txt", false, Some(100), Some(1700000002)),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Created,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["oldest.txt", "middle.txt", "newest.txt"]);
}

/// A row with no creation time sorts BELOW every dated one ascending (so first),
/// above them descending (so last), and by name among its kind, flipped with the
/// order. The `None` arms of the Created column, as they stand.
#[test]
fn test_sort_by_created_places_unknown_dates_first_ascending_and_last_descending() {
    let entry = |name: &str, created: Option<u64>| FileEntry {
        created_at: created,
        ..make_entry(name, false, Some(100), Some(1_700_000_000))
    };
    let fresh = || {
        vec![
            entry("dated_new.txt", Some(1_700_000_002)),
            entry("undated_b.txt", None),
            entry("dated_old.txt", Some(1_700_000_001)),
            entry("undated_a.txt", None),
        ]
    };

    let mut ascending = fresh();
    sort_entries(
        &mut ascending,
        SortColumn::Created,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );
    let names: Vec<&str> = ascending.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        ["undated_a.txt", "undated_b.txt", "dated_old.txt", "dated_new.txt"]
    );

    let mut descending = fresh();
    sort_entries(
        &mut descending,
        SortColumn::Created,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
    );
    let names: Vec<&str> = descending.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(
        names,
        ["dated_new.txt", "dated_old.txt", "undated_b.txt", "undated_a.txt"]
    );
}

#[test]
fn test_dotfiles_sorted_before_regular_files_by_name() {
    let mut entries = vec![
        make_entry("README.md", false, Some(100), None),
        make_entry(".gitignore", false, Some(100), None),
        make_entry("build", true, None, None),
        make_entry(".git", true, None, None),
    ];

    sort_entries(
        &mut entries,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    // Directories first (alphabetically, dotdirs before regular), then files
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // .git comes before build, .gitignore comes before README
    assert_eq!(names, vec![".git", "build", ".gitignore", "README.md"]);
}

// ============================================================================
// entry_comparator tests
// ============================================================================

#[test]
fn test_entry_comparator_matches_sort_entries_name_asc() {
    let entries = vec![
        make_entry("zebra.txt", false, Some(100), None),
        make_entry("alpha", true, None, None),
        make_entry("apple.txt", false, Some(200), None),
        make_entry("docs", true, None, None),
        make_entry(".hidden", false, Some(50), None),
    ];

    let mut via_sort = entries.clone();
    sort_entries(
        &mut via_sort,
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    );

    let mut via_cmp = entries;
    via_cmp.sort_by(entry_comparator(
        SortColumn::Name,
        SortOrder::Ascending,
        DirectorySortMode::LikeFiles,
    ));

    let names_sort: Vec<&str> = via_sort.iter().map(|e| e.name.as_str()).collect();
    let names_cmp: Vec<&str> = via_cmp.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names_sort, names_cmp);
}

#[test]
fn test_entry_comparator_matches_sort_entries_size_desc_dirs_first() {
    let mut entries = vec![
        make_dir_with_recursive_size("big_dir", Some(10000), None),
        make_dir_with_recursive_size("small_dir", Some(100), None),
        make_dir_with_recursive_size("unknown_dir", None, None),
        make_entry("medium.txt", false, Some(500), None),
        make_entry("tiny.txt", false, Some(10), None),
    ];

    let mut via_sort = entries.clone();
    sort_entries(
        &mut via_sort,
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
    );

    entries.sort_by(entry_comparator(
        SortColumn::Size,
        SortOrder::Descending,
        DirectorySortMode::LikeFiles,
    ));

    let names_sort: Vec<&str> = via_sort.iter().map(|e| e.name.as_str()).collect();
    let names_cmp: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names_sort, names_cmp);
}
