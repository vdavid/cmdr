//! Mock file system provider for testing.

use super::{FileEntry, provider::FileSystemProvider};
use std::path::Path;

/// Mock file system provider with configurable data for testing.
/// Can be used for stress testing with large file counts (e.g., 50k+ files).
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
                    name: name.clone(),
                    path: format!("/mock/file_{:06}.txt", i),
                    is_directory: is_dir,
                    is_symlink: i % 50 == 0, // Every 50th is a symlink for testing
                    size: Some(1024 * (i as u64)),
                    modified_at: Some(1640000000 + i as u64),
                    created_at: Some(1639000000 + i as u64),
                    added_at: Some(1638000000 + i as u64),
                    opened_at: Some(1641000000 + i as u64),
                    permissions: 0o644,
                    owner: "testuser".to_string(),
                    group: "staff".to_string(),
                    icon_id: if is_dir {
                        "dir".to_string()
                    } else {
                        "ext:txt".to_string()
                    },
                    extended_metadata_loaded: true,
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
