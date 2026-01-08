//! Tests for MockFileSystemProvider.

use super::*;
use std::path::Path;

#[test]
fn test_mock_provider_returns_entries() {
    let entries = vec![
        FileEntry {
            name: "test.txt".to_string(),
            path: "/test/test.txt".to_string(),
            is_directory: false,
            is_symlink: false,
            size: Some(1024),
            modified_at: Some(1640000000),
            created_at: Some(1639000000),
            added_at: Some(1638000000),
            opened_at: Some(1641000000),
            permissions: 0o644,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            icon_id: "ext:txt".to_string(),
            extended_metadata_loaded: true,
        },
        FileEntry {
            name: "folder".to_string(),
            path: "/test/folder".to_string(),
            is_directory: true,
            is_symlink: false,
            size: None,
            modified_at: Some(1640000000),
            created_at: Some(1639000000),
            added_at: Some(1638000000),
            opened_at: None,
            permissions: 0o755,
            owner: "testuser".to_string(),
            group: "staff".to_string(),
            icon_id: "dir".to_string(),
            extended_metadata_loaded: true,
        },
    ];

    let provider = MockFileSystemProvider::new(entries.clone());
    let result = provider.list_directory(Path::new("/test")).unwrap();

    assert_eq!(result.len(), 2);
    assert_eq!(result[0].name, "test.txt");
    assert_eq!(result[1].name, "folder");
}

#[test]
fn test_mock_provider_with_file_count() {
    let provider = MockFileSystemProvider::with_file_count(100);
    let result = provider.list_directory(Path::new("/test")).unwrap();

    assert_eq!(result.len(), 100);
    assert!(result[0].name.starts_with("file_"));
}

#[test]
fn test_mock_provider_stress_test() {
    // Verify we can handle large file counts for stress testing
    let provider = MockFileSystemProvider::with_file_count(50_000);
    let result = provider.list_directory(Path::new("/test")).unwrap();

    assert_eq!(result.len(), 50_000);
    assert!(result[0].name.starts_with("file_"));
    assert!(result[49_999].name.starts_with("file_"));
}
