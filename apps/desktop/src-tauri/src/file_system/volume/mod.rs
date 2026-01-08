//! Volume trait for abstracting file system access.
//!
//! This module provides the `Volume` trait which abstracts file system operations,
//! enabling different storage backends (local filesystem, in-memory for testing, etc.).

// TODO: Remove this once Volume is integrated into operations.rs (Phase 2)
#![allow(dead_code)]

use super::FileEntry;
use std::path::Path;

/// Error type for volume operations.
#[derive(Debug, Clone)]
pub enum VolumeError {
    /// Path not found
    NotFound(String),
    /// Permission denied
    PermissionDenied(String),
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
            Self::NotSupported => write!(f, "Operation not supported"),
            Self::IoError(msg) => write!(f, "I/O error: {}", msg),
        }
    }
}

impl std::error::Error for VolumeError {}

impl From<std::io::Error> for VolumeError {
    fn from(err: std::io::Error) -> Self {
        match err.kind() {
            std::io::ErrorKind::NotFound => Self::NotFound(err.to_string()),
            std::io::ErrorKind::PermissionDenied => Self::PermissionDenied(err.to_string()),
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

    // ========================================
    // Watching: Optional, default no-op
    // ========================================

    /// Returns true if this volume supports file watching.
    fn supports_watching(&self) -> bool {
        false
    }
}

// Implementations
mod in_memory;
mod local_posix;

pub use in_memory::InMemoryVolume;
pub use local_posix::LocalPosixVolume;

#[cfg(test)]
mod in_memory_test;
#[cfg(test)]
mod local_posix_test;
