//! Maps raw `std::io::Error` values into typed `WriteOperationError` variants.
//!
//! Only `errno` and `ErrorKind` are consulted, never the formatted message (the
//! `no-string-matching` rule). The `IoResultExt` extension trait and the
//! `From<std::io::Error>` impl are the two entry points local-FS code uses to
//! attach a typed variant (and a path) to an IO failure.

use std::path::Path;

use super::types::WriteOperationError;

/// Classifies a raw `std::io::Error` into a specific `WriteOperationError` variant.
///
/// Only `errno` and `ErrorKind` are consulted — never the formatted message.
/// Backend errors (SMB, MTP, etc.) are typed and flow through
/// `transfer/volume_copy.rs::map_volume_error`, so this function only sees
/// `std::io::Error` values produced by local-FS calls, which always carry a
/// `raw_os_error()` on Unix. Pre-fix the function had a lowercase-substring
/// fallback (`"disconnect"`, `"read-only"`, `"connection"`, `"operation not
/// permitted"`, …) that quietly misclassified errors on localized macOS
/// (the wording localizes; the substrings don't match) and was the exact
/// shape AGENTS.md bans.
pub(super) fn classify_io_error(e: &std::io::Error, path: String) -> WriteOperationError {
    #[cfg(unix)]
    if let Some(code) = e.raw_os_error() {
        match code {
            libc::EROFS => {
                return WriteOperationError::ReadOnlyDevice {
                    path,
                    device_name: None,
                };
            }
            libc::ENAMETOOLONG => return WriteOperationError::NameTooLong { path },
            libc::ENOTCONN | libc::ENETDOWN | libc::ENETUNREACH | libc::EHOSTUNREACH | libc::ETIMEDOUT => {
                return WriteOperationError::ConnectionInterrupted { path };
            }
            libc::ENODEV => return WriteOperationError::DeviceDisconnected { path },
            _ => {} // Fall through to ErrorKind classification
        }
    }

    match e.kind() {
        std::io::ErrorKind::NotFound => WriteOperationError::SourceNotFound { path },
        std::io::ErrorKind::PermissionDenied => WriteOperationError::PermissionDenied {
            path,
            message: e.to_string(),
        },
        std::io::ErrorKind::AlreadyExists => WriteOperationError::DestinationExists { path },
        _ => WriteOperationError::IoError {
            path,
            message: e.to_string(),
        },
    }
}

/// Extension trait for converting `io::Result` to `Result<T, WriteOperationError>` with path
/// context.
pub(super) trait IoResultExt<T> {
    fn with_path(self, path: &Path) -> Result<T, WriteOperationError>;
}

impl<T> IoResultExt<T> for std::io::Result<T> {
    fn with_path(self, path: &Path) -> Result<T, WriteOperationError> {
        self.map_err(|e| classify_io_error(&e, path.display().to_string()))
    }
}

impl From<std::io::Error> for WriteOperationError {
    fn from(err: std::io::Error) -> Self {
        classify_io_error(&err, String::new())
    }
}
