//! Volume trait for abstracting file system access.
//!
//! This module provides the `Volume` trait which abstracts file system operations,
//! enabling different storage backends (local filesystem, in-memory for testing, etc.).

// TODO: Remove this once Volume is integrated into operations.rs (Phase 2)
#![allow(dead_code, reason = "Volume abstraction not yet integrated into operations.rs")]

use crate::file_system::listing::FileEntry;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// Result of scanning a path for copy operation.
#[derive(Debug, Clone)]
pub struct CopyScanResult {
    /// Number of files found.
    pub file_count: usize,
    /// Number of directories found.
    pub dir_count: usize,
    /// Total bytes of all files.
    pub total_bytes: u64,
}

/// A conflict detected during pre-copy scanning: a source item that already exists at the destination.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ScanConflict {
    /// Source path (relative to volume root).
    pub source_path: String,
    /// Destination path (relative to volume root).
    pub dest_path: String,
    /// Size of source file in bytes.
    pub source_size: u64,
    /// Size of existing destination file in bytes.
    pub dest_size: u64,
    /// Source file modification time (Unix timestamp in seconds).
    pub source_modified: Option<i64>,
    /// Destination file modification time (Unix timestamp in seconds).
    pub dest_modified: Option<i64>,
}

/// Space information for a volume.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpaceInfo {
    /// Total capacity in bytes.
    pub total_bytes: u64,
    /// Available (free) space in bytes.
    pub available_bytes: u64,
    /// Used space in bytes.
    pub used_bytes: u64,
}

/// Information about a source item for conflict scanning.
#[derive(Debug, Clone)]
pub struct SourceItemInfo {
    /// File/directory name.
    pub name: String,
    /// Size in bytes.
    pub size: u64,
    /// Modification time (Unix timestamp in seconds).
    pub modified: Option<i64>,
}

/// Error type for volume operations.
#[derive(Debug, Clone)]
pub enum VolumeError {
    /// Path not found
    NotFound(String),
    /// Permission denied
    PermissionDenied(String),
    /// Path already exists
    AlreadyExists(String),
    /// Operation not supported by this volume type
    NotSupported,
    /// Generic I/O error
    IoError(String),
}

impl std::fmt::Display for VolumeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotFound(path) => write!(f, "Path not found: {}", path),
            Self::PermissionDenied(path) => write!(f, "Permission denied: {}", path),
            Self::AlreadyExists(path) => write!(f, "Already exists: {}", path),
            Self::NotSupported => write!(f, "Operation not supported"),
            Self::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for VolumeError {}

/// A stream of bytes read from a volume.
///
/// This is a synchronous, blocking iterator-style interface for reading
/// file data in chunks. Used for streaming transfers between volumes.
pub trait VolumeReadStream: Send {
    /// Returns the next chunk of data, or None if complete.
    fn next_chunk(&mut self) -> Option<Result<Vec<u8>, VolumeError>>;

    /// Total size of the file in bytes.
    fn total_size(&self) -> u64;

    /// Bytes read so far (for progress tracking).
    fn bytes_read(&self) -> u64;
}

impl From<std::io::Error> for VolumeError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound(err.to_string()),
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied(err.to_string()),
            std::io::ErrorKind::AlreadyExists => Self::AlreadyExists(err.to_string()),
            _ => Self::IoError(err.to_string()),
        }
    }
}

/// Trait for volume file system operations.
///
/// Implementations provide access to different storage backends:
/// - `LocalPosixVolume`: Real local file system
/// - `InMemoryVolume`: In-memory file system for testing
///
/// All path parameters are relative to the volume root. The volume handles
/// translating these to actual storage locations.
pub trait Volume: Send + Sync {
    /// Returns the display name for this volume (e.g., "Macintosh HD", "Dropbox").
    fn name(&self) -> &str;

    /// Returns the root path of this volume.
    fn root(&self) -> &Path;

    // ========================================
    // Required: All volumes must implement
    // ========================================

    /// Lists directory contents at the given path (relative to volume root).
    ///
    /// Returns entries sorted with directories first, then files, both alphabetically.
    fn list_directory(&self, path: &Path) -> Result<Vec<FileEntry>, VolumeError>;

    /// Gets metadata for a single path (relative to volume root).
    fn get_metadata(&self, path: &Path) -> Result<FileEntry, VolumeError>;

    /// Checks if a path exists (relative to volume root).
    fn exists(&self, path: &Path) -> bool;

    /// Checks if a path is a directory.
    /// Returns Ok(true) if directory, Ok(false) if file, Err if path doesn't exist.
    fn is_directory(&self, path: &Path) -> Result<bool, VolumeError>;

    // ========================================
    // Optional: Default to NotSupported
    // ========================================

    /// Creates a file with the given content.
    fn create_file(&self, path: &Path, content: &[u8]) -> Result<(), VolumeError> {
        let _ = (path, content);
        Err(VolumeError::NotSupported)
    }

    /// Creates a directory.
    fn create_directory(&self, path: &Path) -> Result<(), VolumeError> {
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    /// Deletes a file or empty directory.
    fn delete(&self, path: &Path) -> Result<(), VolumeError> {
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    /// Renames/moves a file or directory within this volume.
    ///
    /// Both source and destination paths are relative to the volume root.
    fn rename(&self, from: &Path, to: &Path) -> Result<(), VolumeError> {
        let _ = (from, to);
        Err(VolumeError::NotSupported)
    }

    // ========================================
    // Watching: Optional, default no-op
    // ========================================

    /// Returns true if this volume supports file watching.
    fn supports_watching(&self) -> bool {
        false
    }

    // ========================================
    // Copy/Export: Optional, default no-op
    // ========================================

    /// Returns whether this volume supports copy/export operations.
    fn supports_export(&self) -> bool {
        false
    }

    /// Scans a path recursively to get statistics for a copy operation.
    /// Returns file count, directory count, and total bytes.
    fn scan_for_copy(&self, path: &Path) -> Result<CopyScanResult, VolumeError> {
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    /// Downloads/exports a file or directory from this volume to a local path.
    /// For local volumes, this is a file copy. For MTP, this downloads.
    /// Returns bytes transferred.
    fn export_to_local(&self, source: &Path, local_dest: &Path) -> Result<u64, VolumeError> {
        let _ = (source, local_dest);
        Err(VolumeError::NotSupported)
    }

    /// Imports/uploads a file or directory from a local path to this volume.
    /// For local volumes, this is a file copy. For MTP, this uploads.
    /// Returns bytes transferred.
    fn import_from_local(&self, local_source: &Path, dest: &Path) -> Result<u64, VolumeError> {
        let _ = (local_source, dest);
        Err(VolumeError::NotSupported)
    }

    /// Checks destination for conflicts with source items.
    /// Returns list of files that already exist at destination.
    fn scan_for_conflicts(
        &self,
        source_items: &[SourceItemInfo],
        dest_path: &Path,
    ) -> Result<Vec<ScanConflict>, VolumeError> {
        let _ = (source_items, dest_path);
        Err(VolumeError::NotSupported)
    }

    /// Gets space information for this volume.
    fn get_space_info(&self) -> Result<SpaceInfo, VolumeError> {
        Err(VolumeError::NotSupported)
    }

    // ========================================
    // Capability hints for copy optimization
    // ========================================

    /// Returns the local filesystem path if this volume is backed by one.
    /// Used to optimize local-to-local copies using native OS APIs (e.g., copyfile on macOS).
    /// Returns None for non-local volumes (MTP, S3, FTP, etc.).
    fn local_path(&self) -> Option<std::path::PathBuf> {
        None
    }

    /// Returns true if this volume supports streaming read/write operations.
    fn supports_streaming(&self) -> bool {
        false
    }

    /// Opens a streaming reader for the given path.
    ///
    /// Returns a VolumeReadStream that yields chunks of data.
    /// The stream must be fully consumed or dropped before other operations.
    fn open_read_stream(&self, path: &Path) -> Result<Box<dyn VolumeReadStream>, VolumeError> {
        let _ = path;
        Err(VolumeError::NotSupported)
    }

    /// Writes data from a stream to the given path.
    ///
    /// # Arguments
    /// * `dest` - Destination path (file will be created/overwritten)
    /// * `size` - Total size in bytes (required for protocols like MTP)
    /// * `stream` - Source data stream
    fn write_from_stream(&self, dest: &Path, size: u64, stream: Box<dyn VolumeReadStream>) -> Result<u64, VolumeError> {
        let _ = (dest, size, stream);
        Err(VolumeError::NotSupported)
    }
}

// Implementations
mod in_memory;
mod local_posix;
pub(crate) mod manager;
mod mtp;

pub use in_memory::InMemoryVolume;
pub use local_posix::LocalPosixVolume;
pub use mtp::MtpVolume;

// Re-export types defined in this module for convenience
// (they're already public since defined in mod.rs)

#[cfg(test)]
mod in_memory_test;
#[cfg(test)]
mod inmemory_test;
#[cfg(test)]
mod local_posix_test;
