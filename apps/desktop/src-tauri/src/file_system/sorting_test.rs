//! Tests for file sorting logic.
//!
//! These tests verify that sort_entries correctly sorts files by
//! name, extension, size, modified date, and created date.

use super::operations::{FileEntry, SortColumn, SortOrder, sort_entries};

/// Creates a test entry with the given name and properties.
fn make_entry(name: &str, is_dir: bool, size: Option<u64>, modified: Option<u64>) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        path: format!("/{}", name),
        is_directory: is_dir,
        is_symlink: false,
        size,
        modified_at: modified,
        created_at: modified, // Use same value for simplicity
        added_at: None,
        opened_at: None,
        permissions: if is_dir { 0o755 } else { 0o644 },
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        icon_id: if is_dir { "dir".to_string() } else { "file".to_string() },
        extended_metadata_loaded: true,
    }
}

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

    sort_entries(&mut entries, SortColumn::Name, SortOrder::Ascending);

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

    sort_entries(&mut entries, SortColumn::Name, SortOrder::Descending);

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

    sort_entries(&mut entries, SortColumn::Name, SortOrder::Ascending);

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

    sort_entries(&mut entries, SortColumn::Name, SortOrder::Descending);

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

    sort_entries(&mut entries, SortColumn::Extension, SortOrder::Ascending);

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

#[test]
fn test_extension_sort_same_ext_by_name() {
    let mut entries = vec![
        make_entry("zebra.txt", false, Some(100), None),
        make_entry("alpha.txt", false, Some(100), None),
        make_entry("beta.txt", false, Some(100), None),
    ];

    sort_entries(&mut entries, SortColumn::Extension, SortOrder::Ascending);

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

    sort_entries(&mut entries, SortColumn::Size, SortOrder::Ascending);

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

    sort_entries(&mut entries, SortColumn::Size, SortOrder::Descending);

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

    sort_entries(&mut entries, SortColumn::Size, SortOrder::Ascending);

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

    sort_entries(&mut entries, SortColumn::Modified, SortOrder::Ascending);

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

    sort_entries(&mut entries, SortColumn::Modified, SortOrder::Descending);

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

    sort_entries(&mut entries, SortColumn::Modified, SortOrder::Ascending);

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
    sort_entries(&mut entries, SortColumn::Name, SortOrder::Ascending);
    assert!(entries.is_empty());
}

#[test]
fn test_single_entry() {
    let mut entries = vec![make_entry("only.txt", false, Some(100), None)];
    sort_entries(&mut entries, SortColumn::Name, SortOrder::Ascending);
    assert_eq!(entries.len(), 1);
    assert_eq!(entries[0].name, "only.txt");
}

#[test]
fn test_case_insensitive_sort() {
    let mut entries = vec![
        make_entry("Zebra.txt", false, Some(100), None),
        make_entry("alpha.txt", false, Some(100), None),
        make_entry("BETA.txt", false, Some(100), None),
    ];

    sort_entries(&mut entries, SortColumn::Name, SortOrder::Ascending);

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["alpha.txt", "BETA.txt", "Zebra.txt"]);
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
    sort_entries(&mut entries, SortColumn::Name, SortOrder::Ascending);

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

    sort_entries(&mut entries, SortColumn::Name, SortOrder::Ascending);

    assert_eq!(entries[0].name, long_name_a);
    assert_eq!(entries[1].name, long_name_z);
}

/// Creates a test entry that is a symlink
fn make_symlink(name: &str, size: Option<u64>) -> FileEntry {
    FileEntry {
        name: name.to_string(),
        path: format!("/{}", name),
        is_directory: false,
        is_symlink: true,
        size,
        modified_at: None,
        created_at: None,
        added_at: None,
        opened_at: None,
        permissions: 0o777,
        owner: "testuser".to_string(),
        group: "staff".to_string(),
        icon_id: "symlink".to_string(),
        extended_metadata_loaded: true,
    }
}

#[test]
fn test_symlinks_sorted_as_files() {
    let mut entries = vec![
        make_entry("dir", true, None, None),
        make_symlink("link_to_something.txt", Some(100)),
        make_entry("file.txt", false, Some(100), None),
    ];

    sort_entries(&mut entries, SortColumn::Name, SortOrder::Ascending);

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

    sort_entries(&mut entries, SortColumn::Size, SortOrder::Ascending);

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

    sort_entries(&mut entries, SortColumn::Size, SortOrder::Descending);

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

    sort_entries(&mut entries, SortColumn::Created, SortOrder::Ascending);

    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, vec!["oldest.txt", "middle.txt", "newest.txt"]);
}

#[test]
fn test_dotfiles_sorted_before_regular_files_by_name() {
    let mut entries = vec![
        make_entry("README.md", false, Some(100), None),
        make_entry(".gitignore", false, Some(100), None),
        make_entry("build", true, None, None),
        make_entry(".git", true, None, None),
    ];

    sort_entries(&mut entries, SortColumn::Name, SortOrder::Ascending);

    // Directories first (alphabetically, dotdirs before regular), then files
    let names: Vec<&str> = entries.iter().map(|e| e.name.as_str()).collect();
    // .git comes before build, .gitignore comes before README
    assert_eq!(names, vec![".git", "build", ".gitignore", "README.md"]);
}
