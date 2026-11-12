//! How a volume's refusal reads to the viewer: the one mapping from `VolumeError` to
//! `ViewerError`, shared by the viewer's own positioned reads and the host's pulls.

use cmdr_fs::error_detail::ErrorDetail;
use cmdr_fs::volume::{RoutedKind, VolumeError};

use super::{ArchiveFailureKind, ViewerError};

/// Maps a `VolumeError` from a materializing read into a typed `ViewerError`.
/// Path-shaped errors keep their twins whatever the source; the rest depend on it.
///
/// The archive family has its own frontend copy under `ViewerError::Archive`, which
/// the FE renders without inspecting the typed failure or message string. The typed
/// failure preserves `NotSupported` for non-UI consumers. A portal read and a plain
/// pull have no such family — a repository (git or restic) that can't be opened
/// or a phone that dropped mid-read is a fault, not a kind of file — so they stay a plain `Io`.
pub fn map_volume_error(err: VolumeError, routed: Option<RoutedKind>) -> ViewerError {
    match err {
        VolumeError::NotFound(path) => ViewerError::NotFound { path },
        VolumeError::IsADirectory(_) => ViewerError::IsDirectory,
        // Whichever route read it: a zip in cold storage is as unreadable as a
        // file there, and the fix is the same restore.
        VolumeError::ColdStorage(_) => ViewerError::ColdStorage,
        // Open somewhere else, on any route: a zip someone holds open is as closed to
        // a read as a file, and the fix is the same (cmdr-reports#110).
        VolumeError::FileInUse(_) => ViewerError::FileInUse,
        VolumeError::LinkTargetUnreachable(_) => ViewerError::LinkTargetUnreachable,
        VolumeError::LinkNeedsConsent(_) => ViewerError::LinkNeedsConsent,
        other => match routed {
            Some(RoutedKind::Archive | RoutedKind::ArchiveInSnapshot) => ViewerError::Archive {
                failure: match other {
                    VolumeError::NotSupported => ArchiveFailureKind::Unsupported,
                    VolumeError::NeedsPassword { .. } => ArchiveFailureKind::NeedsPassword,
                    _ => ArchiveFailureKind::Unreadable,
                },
                message: ErrorDetail::of(other),
            },
            Some(RoutedKind::GitPortal | RoutedKind::Restic) | None => ViewerError::Io {
                message: ErrorDetail::of(other),
            },
        },
    }
}
