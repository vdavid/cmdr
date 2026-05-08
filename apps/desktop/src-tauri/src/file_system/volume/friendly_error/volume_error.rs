//! `VolumeError → FriendlyError`.
//!
//! Most variants delegate to the canonical `kinds::*` constructors so users see
//! the same copy regardless of which layer the error originated in. A few
//! variants are unique to this layer:
//! - `IoError { raw_os_error: Some(_) }` dispatches to `errno::friendly_error_from_errno`
//!   for per-errno copy
//! - `FriendlyGit(_)` carries a fully-shaped `FriendlyError` from the git module

use std::path::Path;

use super::errno::friendly_error_from_errno;
use super::{ErrorCategory, FriendlyError, kinds};
use crate::file_system::volume::VolumeError;

/// Converts a `VolumeError` into a user-facing `FriendlyError`.
///
/// For `IoError` with a `raw_os_error`, matches against platform-specific errno codes.
/// For typed `VolumeError` variants, delegates to the shared `kinds::*` constructors.
///
/// Git failures arrive as `VolumeError::FriendlyGit(FriendlyGitError)` from the
/// `file_system::git` volume hooks; we hand the carried payload straight to
/// `to_friendly_error` so `ErrorPane` shows git-specific titles and suggestions
/// instead of the generic I/O copy.
pub fn friendly_error_from_volume_error(err: &VolumeError, path: &Path) -> FriendlyError {
    let path_display = path.display().to_string();
    let raw = err.to_string();

    match err {
        VolumeError::FriendlyGit(git_err) => git_err.to_friendly_error(),
        VolumeError::NotFound(_) => kinds::not_found(&path_display, raw),
        VolumeError::PermissionDenied(_) => kinds::permission_denied(&path_display, raw),
        VolumeError::AlreadyExists(_) => kinds::already_exists(&path_display, raw),
        VolumeError::NotSupported => kinds::not_supported(raw),
        VolumeError::DeviceDisconnected(_) => kinds::device_disconnected(&path_display, raw),
        VolumeError::ReadOnly(_) => kinds::read_only(raw),
        VolumeError::StorageFull { .. } => kinds::storage_full(raw),
        VolumeError::ConnectionTimeout(_) => kinds::connection_timeout(raw),
        VolumeError::Cancelled(_) => kinds::cancelled(raw),
        VolumeError::IsADirectory(_) => FriendlyError {
            category: ErrorCategory::NeedsAction,
            title: "This is a folder, not a file".into(),
            explanation: format!("Cmdr tried to open `{}` as a file, but it's a folder.", path_display),
            suggestion: "Navigate into the folder instead of opening it as a file.".into(),
            raw_detail: raw,
            retry_hint: false,
            action_kind: None,
        },
        VolumeError::IoError {
            raw_os_error: Some(errno),
            ..
        } => friendly_error_from_errno(*errno, path, err),
        VolumeError::IoError {
            raw_os_error: None,
            message,
        } => kinds::io_serious(&path_display, message, raw),
    }
}
