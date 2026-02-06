//! Real file system provider implementation.

use super::FileEntry;
use super::listing::reading;
use super::provider::FileSystemProvider;
use std::path::Path;

/// Real file system provider that accesses the actual file system.
pub struct RealFileSystemProvider;

impl FileSystemProvider for RealFileSystemProvider {
    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, std::io::Error> {
        reading::list_directory(path)
    }
}
