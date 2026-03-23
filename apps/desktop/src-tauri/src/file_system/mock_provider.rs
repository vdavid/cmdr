//! Mock file system provider for testing.

use super::{FileEntry, provider::FileSystemProvider};
use std::path::Path;

/// Mock file system provider with configurable data for testing.
/// Can be used for stress testing with large file counts (like 50k+ files).
pub struct MockFileSystemProvider {
    entries: Vec<FileEntry>,
}

impl MockFileSystemProvider {
    /// Creates a new mock provider with the given entries.
    pub fn new(entries: Vec<FileEntry>) -> Self {
        Self { entries }
    }

    /// Creates a mock provider with a configurable number of test files.
    /// Useful for stress testing with large file counts.
    pub fn with_file_count(count: usize) -> Self {
        let entries = (0..count)
            .map(|i| {
                let is_dir = i % 10 == 0;
                let name = format!("file_{:06}.txt", i);
                FileEntry {
                    size: Some(1024 * (i as u64)),
                    modified_at: Some(1640000000 + i as u64),
                    created_at: Some(1639000000 + i as u64),
                    added_at: Some(1638000000 + i as u64),
                    opened_at: Some(1641000000 + i as u64),
                    permissions: 0o644,
                    owner: "testuser".to_string(),
                    group: "staff".to_string(),
                    extended_metadata_loaded: true,
                    ..FileEntry::new(name, format!("/mock/file_{:06}.txt", i), is_dir, i % 50 == 0)
                }
            })
            .collect();
        Self::new(entries)
    }
}

impl FileSystemProvider for MockFileSystemProvider {
    fn list_directory(&self, _path: &Path) -> Result<Vec<FileEntry>, std::io::Error> {
        Ok(self.entries.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn test_mock_provider_returns_entries() {
        let entries = vec![
            FileEntry {
                size: Some(1024),
                modified_at: Some(1640000000),
                created_at: Some(1639000000),
                added_at: Some(1638000000),
                opened_at: Some(1641000000),
                permissions: 0o644,
                owner: "testuser".to_string(),
                group: "staff".to_string(),
                extended_metadata_loaded: true,
                ..FileEntry::new("test.txt".to_string(), "/test/test.txt".to_string(), false, false)
            },
            FileEntry {
                modified_at: Some(1640000000),
                created_at: Some(1639000000),
                added_at: Some(1638000000),
                permissions: 0o755,
                owner: "testuser".to_string(),
                group: "staff".to_string(),
                extended_metadata_loaded: true,
                ..FileEntry::new("folder".to_string(), "/test/folder".to_string(), true, false)
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
}
