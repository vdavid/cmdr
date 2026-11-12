//! Maps raw `std::io::Error` values into typed `WriteOperationError` variants.
//!
//! Only the errno's `OsErrorClass` and `ErrorKind` are consulted, never the
//! formatted message. The
//! `IoResultExt` extension trait and the
//! `From<std::io::Error>` impl are the two entry points local-FS code uses to
//! attach a typed variant (and a path) to an IO failure; the native copy calls
//! (`macos_copy`, `linux_copy`) use `classify_copy_io_error`, which also picks
//! the path.

use std::path::Path;

use cmdr_fs::os_error::OsErrorClass;

use super::types::{PermissionSide, ReadOnlySide, WriteOperationError};

/// Classifies a raw `std::io::Error` into a specific `WriteOperationError` variant.
///
/// Only the errno's class and `ErrorKind` are consulted — never the formatted message.
/// Backend errors (SMB, MTP, etc.) are typed and flow through
/// `transfer/volume/transfer_error.rs::map_volume_error`, so this function only sees
/// `std::io::Error` values produced by local-FS calls, which always carry an OS
/// code on Unix. Pre-fix the function had a lowercase-substring
/// fallback (`"disconnect"`, `"read-only"`, `"connection"`, `"operation not
/// permitted"`, …) that quietly misclassified errors on localized macOS
/// (the wording localizes; the substrings don't match) and was the exact
/// shape AGENTS.md bans.
pub(super) fn classify_io_error(e: &std::io::Error, path: String) -> WriteOperationError {
    if let Some(class) = OsErrorClass::of_io(e) {
        match class {
            OsErrorClass::ReadOnlyFilesystem => {
                // EROFS came back from a WRITE, so the refusing half is the
                // destination by construction.
                return WriteOperationError::ReadOnlyDevice {
                    path,
                    device_name: None,
                    side: ReadOnlySide::Destination,
                };
            }
            OsErrorClass::NameTooLong => return WriteOperationError::NameTooLong { path },
            // The refused write measured nothing, so it's the sizeless variant.
            OsErrorClass::NoSpace | OsErrorClass::QuotaExceeded => {
                return WriteOperationError::DestinationFull { path };
            }
            // The one transport set, shared with the transfer engine's retry and
            // outage wait, so both agree on what "the link dropped" is.
            class if class.is_transport() => {
                return WriteOperationError::ConnectionInterrupted { path };
            }
            // Both are a drive that isn't there any more: `ENODEV` from a mount
            // that's gone, `ENXIO` from a device node whose hardware left. The
            // SIDE is filled in at the operation's boundary, where the volumes
            // it was handed at start are still known (`transfer_sides.rs`).
            OsErrorClass::NoDevice | OsErrorClass::DeviceNotConfigured => {
                return WriteOperationError::DeviceDisconnected { path, side: None };
            }
            // Windows' sharing and lock violations: another app holds the file open, the
            // fact an SMB share's `FileInUse` states. ❌ Never `Busy` (`EBUSY`).
            OsErrorClass::InUseByAnotherApp => return WriteOperationError::FileInUse { path },
            _ => {} // Fall through to ErrorKind classification
        }
    }

    match e.kind() {
        std::io::ErrorKind::NotFound => WriteOperationError::SourceNotFound { path },
        // The errno rides along (Rust folds `EACCES` and `EPERM` into one kind, and
        // they're opposite advice), and the probe asks the OS which folder actually
        // said no rather than letting the copy guess from the operation's shape.
        std::io::ErrorKind::PermissionDenied => {
            let refused_folder = super::validation::refusing_folder(Path::new(&path));
            // No side: a `rename(2)` needs both parents, and its errno can't say which
            // one refused.
            WriteOperationError::permission_denied(path, e.to_string(), e.raw_os_error(), refused_folder, None)
        }
        std::io::ErrorKind::AlreadyExists => WriteOperationError::DestinationExists { path },
        _ => WriteOperationError::IoError {
            path,
            message: cmdr_fs::error_detail::ErrorDetail::from_io(e),
        },
    }
}

/// Classifies a failure from a native copy call (`copyfile`, `copy_file_range`),
/// which touches both ends, so the errno also decides which path the error names.
///
/// A refusal of the WRITE names the destination (read-only, full, a name it can't
/// hold, no permission, occupied); a missing source, a dropped link, or anything
/// unclassified names the source the user copied from. ❌ Don't give a platform
/// its own copy of this table: the macOS one drifted and sent `EROFS` to the user
/// as a generic failure.
#[cfg(unix)]
pub(super) fn classify_copy_io_error(err: &std::io::Error, source: &Path, destination: &Path) -> WriteOperationError {
    let refused_by_destination = matches!(
        err.kind(),
        std::io::ErrorKind::PermissionDenied | std::io::ErrorKind::AlreadyExists
    ) || matches!(
        OsErrorClass::of_io(err),
        Some(
            OsErrorClass::ReadOnlyFilesystem
                | OsErrorClass::NoSpace
                | OsErrorClass::QuotaExceeded
                | OsErrorClass::NameTooLong
        )
    );
    let path = if refused_by_destination { destination } else { source };
    classify_io_error(err, path.display().to_string())
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

pub(super) use crate::local_disk::is_locked_item;

impl WriteOperationError {
    /// A `rename(2)` of `source` refused with no side named, answered for the one
    /// case the errno can't say but the item can: `source` is itself locked, so it's
    /// the source that refused, and the dialog gives the "uncheck Locked" advice
    /// instead of "no permission to move files here". Any other error passes through
    /// untouched.
    pub(crate) fn sided_by_locked_source(self, source: &Path) -> Self {
        match self {
            Self::PermissionDenied {
                path,
                message,
                errno,
                refused_folder,
                side: None,
                ..
            } if is_locked_item(source) => {
                Self::permission_denied(path, message, errno, refused_folder, Some(PermissionSide::Source))
            }
            other => other,
        }
    }
}

impl WriteOperationError {
    /// Whether this outcome is expected, recoverable control flow rather than a
    /// genuine failure.
    ///
    /// Callers log expected-recoverable outcomes at `warn`, not `error`, so they
    /// stay below the error-reporter's auto-report threshold (error level IS that
    /// threshold — see the app's `error_reporter/CLAUDE.md`). Without this an
    /// encrypted-archive extract would queue a false-positive auto error report on
    /// every password prompt and every wrong attempt.
    ///
    /// `ArchiveNeedsPassword` is the sole case today: extracting from an encrypted
    /// archive raises it purely to prompt the user for a password and retry.
    pub(crate) fn is_expected_recoverable(&self) -> bool {
        matches!(self, WriteOperationError::ArchiveNeedsPassword { .. })
    }

    /// Whether this failure is an expected refusal from the world, which the error
    /// dialog explains with a way forward, rather than something that may be our
    /// own defect.
    ///
    /// Callers log it at `warn`, for the same reason as
    /// [`is_expected_recoverable`](Self::is_expected_recoverable): an error log
    /// counts toward an auto-sent report (cmdr-reports#111: a file open elsewhere on
    /// a share refused three copies and auto-sent one). A wrapper that carries the
    /// reason it stopped answers for its cause, the way the frontend's
    /// `logLevelFor` does.
    ///
    /// Deliberately narrow: only the variants an auto-sent report has shown to be
    /// noise. The frontend's `transfer-error-log-level.ts` is the broader rule for
    /// the lines it writes.
    pub(crate) fn is_expected_refusal(&self) -> bool {
        match self {
            Self::FileInUse { .. } => true,
            Self::OriginalsKeptAside { cause, .. }
            | Self::SourceNotRemoved { cause, .. }
            | Self::NewFilesKept { cause, .. } => cause.is_expected_refusal(),
            _ => false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A full disk and a spent quota have one fix, room, and nothing was measured:
    /// neither may read as a generic failure, nor as "needs 0 bytes".
    #[cfg(unix)]
    #[test]
    fn a_full_disk_and_a_spent_quota_are_a_full_destination() {
        for errno in [libc::ENOSPC, libc::EDQUOT] {
            let err = classify_io_error(
                &std::io::Error::from_raw_os_error(errno),
                "/Volumes/Stick/a".to_string(),
            );
            assert!(
                matches!(&err, WriteOperationError::DestinationFull { path } if path == "/Volumes/Stick/a"),
                "errno {errno}: got {err:?}"
            );
        }
    }

    /// Every errno the transfer engine waits out as a dropped link reads as one
    /// here too: the two used to keep separate lists, and this one lacked
    /// `ECONNRESET` (plus `EPIPE` and `ECONNABORTED`, which neither had).
    #[cfg(unix)]
    #[test]
    fn a_dropped_link_is_an_interrupted_connection() {
        for errno in [
            libc::ECONNRESET,
            libc::EPIPE,
            libc::ECONNABORTED,
            libc::ENOTCONN,
            libc::ENETDOWN,
            libc::ENETUNREACH,
            libc::EHOSTUNREACH,
            libc::ETIMEDOUT,
        ] {
            let err = classify_io_error(
                &std::io::Error::from_raw_os_error(errno),
                "/Volumes/naspi/a".to_string(),
            );
            assert!(
                matches!(&err, WriteOperationError::ConnectionInterrupted { path } if path == "/Volumes/naspi/a"),
                "errno {errno}: got {err:?}"
            );
        }
    }

    #[test]
    fn archive_needs_password_is_expected_recoverable() {
        // Both the fresh prompt and the wrong-attempt re-prompt are recoverable.
        for wrong_attempt in [false, true] {
            let err = WriteOperationError::ArchiveNeedsPassword {
                path: "/secret.zip".to_string(),
                container: cmdr_fs::volume::LockedContainer::Archive,
                wrong_attempt,
            };
            assert!(
                err.is_expected_recoverable(),
                "needs-password must stay below the auto-report threshold (wrong_attempt={wrong_attempt})"
            );
        }
    }

    /// ❗ A same-volume move of a Finder-locked file refuses with `EPERM` from
    /// `rename(2)`, whose errno can't say which side refused; the item's own lock
    /// flag can, and it's the source. Without it the dialog said "You don't have
    /// permission to move files here" (QA round 2, M5).
    #[cfg(target_os = "macos")]
    #[test]
    fn a_rename_refused_over_a_locked_source_names_the_source() {
        let dir = tempfile::tempdir().expect("temp dir");
        let locked = dir.path().join("locked.txt");
        std::fs::write(&locked, "hi").expect("write");
        let chflags = |flags: &str| {
            let done = std::process::Command::new("chflags").arg(flags).arg(&locked).status();
            assert!(done.is_ok_and(|status| status.success()), "chflags {flags}");
        };
        chflags("uchg");
        let refused = std::fs::rename(&locked, dir.path().join("moved.txt")).expect_err("a locked file won't rename");
        let error = classify_io_error(&refused, locked.display().to_string()).sided_by_locked_source(&locked);
        chflags("nouchg");

        assert!(
            matches!(
                error,
                WriteOperationError::PermissionDenied {
                    side: Some(PermissionSide::Source),
                    ..
                }
            ),
            "got {error:?}"
        );
    }

    /// cmdr-reports#111: a file open elsewhere on a share refused three copies, and
    /// each logged at error, which crossed the auto-send threshold. The refusal is
    /// the world's doing and the dialog says what to do, so it logs at warn, on
    /// its own and as the cause a wrapper carries.
    #[test]
    fn a_file_in_use_is_an_expected_refusal() {
        let in_use = || WriteOperationError::FileInUse {
            path: "/Volumes/share/report.pdf".to_string(),
        };
        assert!(in_use().is_expected_refusal());
        let not_removed = WriteOperationError::SourceNotRemoved {
            path: "/Volumes/share/report.pdf".to_string(),
            landed_at: "/Users/me/report.pdf".to_string(),
            cause: Box::new(in_use()),
        };
        assert!(not_removed.is_expected_refusal());
    }

    /// A local file another app holds open (Windows' `ERROR_SHARING_VIOLATION` and
    /// `ERROR_LOCK_VIOLATION`) is the refusal an SMB share's sharing violation is. No
    /// Unix code means this, so the cell runs only there.
    #[cfg(windows)]
    #[test]
    fn a_windows_sharing_violation_is_a_file_in_use() {
        for code in [32, 33] {
            let err = classify_io_error(&std::io::Error::from_raw_os_error(code), "C:\\a.pdf".to_string());
            assert!(
                matches!(err, WriteOperationError::FileInUse { .. }),
                "{code}: got {err:?}"
            );
        }
    }

    #[test]
    fn genuine_failures_are_not_expected_refusals() {
        let failures = [
            WriteOperationError::IoError {
                path: "/x".to_string(),
                message: "boom".into(),
            },
            WriteOperationError::SourceNotRemoved {
                path: "/x".to_string(),
                landed_at: "/y".to_string(),
                cause: Box::new(WriteOperationError::IoError {
                    path: "/x".to_string(),
                    message: "boom".into(),
                }),
            },
        ];
        for err in failures {
            assert!(!err.is_expected_refusal(), "must still log at error: {err:?}");
        }
    }

    #[test]
    fn genuine_failures_are_not_expected_recoverable() {
        let failures = [
            WriteOperationError::IoError {
                path: "/x".to_string(),
                message: "boom".into(),
            },
            WriteOperationError::SourceNotFound { path: "/x".to_string() },
            WriteOperationError::permission_denied("/x".to_string(), "denied".to_string(), None, None, None),
        ];
        for err in failures {
            assert!(
                !err.is_expected_recoverable(),
                "genuine failure must still log at error and auto-report: {err:?}"
            );
        }
    }

    /// What one errno's write failure becomes, and which end the native copy names.
    #[cfg(unix)]
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum Outcome {
        PermissionDenied(crate::types::PermissionRefusal),
        SourceNotFound,
        DestinationExists,
        DeviceDisconnected,
        DestinationFull,
        ReadOnlyDevice,
        NameTooLong,
        ConnectionInterrupted,
        IoError,
    }

    /// The outcome, and the path the error names.
    #[cfg(unix)]
    fn outcome_of(err: &WriteOperationError) -> (Outcome, &str) {
        match err {
            WriteOperationError::PermissionDenied { refusal, path, .. } => (Outcome::PermissionDenied(*refusal), path),
            WriteOperationError::SourceNotFound { path } => (Outcome::SourceNotFound, path),
            WriteOperationError::DestinationExists { path } => (Outcome::DestinationExists, path),
            WriteOperationError::DeviceDisconnected { path, side: None } => (Outcome::DeviceDisconnected, path),
            WriteOperationError::DestinationFull { path } => (Outcome::DestinationFull, path),
            WriteOperationError::ReadOnlyDevice {
                path,
                side: ReadOnlySide::Destination,
                device_name: None,
            } => (Outcome::ReadOnlyDevice, path),
            WriteOperationError::NameTooLong { path } => (Outcome::NameTooLong, path),
            WriteOperationError::ConnectionInterrupted { path } => (Outcome::ConnectionInterrupted, path),
            WriteOperationError::IoError { path, .. } => (Outcome::IoError, path),
            other => panic!("an outcome the table doesn't know: {other:?}"),
        }
    }

    /// The characterization table, every errno: what a local write failure becomes,
    /// and whether a native copy's failure names the destination. Each named errno
    /// is listed; every other one is a generic failure naming the source.
    #[cfg(unix)]
    #[test]
    fn every_errno_keeps_its_write_outcome_and_its_side() {
        use crate::types::PermissionRefusal::{FolderPermissions, SystemProtected};

        let named = [
            (libc::EPERM, Outcome::PermissionDenied(SystemProtected), true),
            (libc::EACCES, Outcome::PermissionDenied(FolderPermissions), true),
            (libc::ENOENT, Outcome::SourceNotFound, false),
            (libc::EEXIST, Outcome::DestinationExists, true),
            (libc::ENXIO, Outcome::DeviceDisconnected, false),
            (libc::ENODEV, Outcome::DeviceDisconnected, false),
            (libc::ENOSPC, Outcome::DestinationFull, true),
            (libc::EDQUOT, Outcome::DestinationFull, true),
            (libc::EROFS, Outcome::ReadOnlyDevice, true),
            (libc::ENAMETOOLONG, Outcome::NameTooLong, true),
            (libc::EPIPE, Outcome::ConnectionInterrupted, false),
            (libc::ECONNRESET, Outcome::ConnectionInterrupted, false),
            (libc::ECONNABORTED, Outcome::ConnectionInterrupted, false),
            (libc::ENOTCONN, Outcome::ConnectionInterrupted, false),
            (libc::ETIMEDOUT, Outcome::ConnectionInterrupted, false),
            (libc::ENETDOWN, Outcome::ConnectionInterrupted, false),
            (libc::ENETUNREACH, Outcome::ConnectionInterrupted, false),
            (libc::EHOSTUNREACH, Outcome::ConnectionInterrupted, false),
        ];
        let (source, destination) = (Path::new("/src/a"), Path::new("/dst/a"));
        for code in 0..=4096 {
            let (outcome, names_destination) = named
                .iter()
                .find(|(errno, ..)| *errno == code)
                .map_or((Outcome::IoError, false), |&(_, outcome, side)| (outcome, side));
            let err = std::io::Error::from_raw_os_error(code);
            let classified = classify_io_error(&err, "/p".to_string());
            assert_eq!(outcome_of(&classified), (outcome, "/p"), "errno {code}");
            let copied = classify_copy_io_error(&err, source, destination);
            let expected = if names_destination { destination } else { source };
            let expected = expected.display().to_string();
            assert_eq!(outcome_of(&copied), (outcome, expected.as_str()), "errno {code}");
        }
    }

    /// A refusal's advice, every errno: only `EACCES` and `EPERM` say who refused.
    #[cfg(unix)]
    #[test]
    fn every_errno_keeps_its_permission_advice() {
        use crate::types::PermissionRefusal;

        for code in 0..=4096 {
            let expected = match code {
                libc::EACCES => PermissionRefusal::FolderPermissions,
                libc::EPERM => PermissionRefusal::SystemProtected,
                _ => PermissionRefusal::Unclassified,
            };
            assert_eq!(PermissionRefusal::from_errno(Some(code)), expected, "errno {code}");
        }
        assert_eq!(PermissionRefusal::from_errno(None), PermissionRefusal::Unclassified);
    }
}
