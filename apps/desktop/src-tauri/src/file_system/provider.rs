//! File system provider trait for abstraction and testing.

use super::FileEntry;
use std::path::Path;

/// Trait for file system operations, enabling both real and mock implementations.
pub trait FileSystemProvider {
    /// Lists the contents of a directory.
    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, std::io::Error>;
}
