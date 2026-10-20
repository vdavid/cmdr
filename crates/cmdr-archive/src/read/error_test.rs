use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;
use cmdr_fs::volume::VolumeError;

use super::*;

/// A byte source on another volume fails with a typed `VolumeError`, handed over in an
/// `io::Error` (`VolumeByteSource::read_at`). The archive error keeps it typed, so a log line
/// marks the parent's path; its words stay the volume error's own.
#[test]
fn a_byte_source_failure_marks_the_path_its_volume_error_names() {
    let path = "Trips/Lake House/photos.zip";
    let cause = VolumeError::PermissionDenied {
        path: path.to_string(),
        raw_os_error: None,
    };
    let error = ArchiveError::from(std::io::Error::other(ErrorDetail::of(cause.clone())));
    assert!(matches!(error, ArchiveError::Io(_)), "{error:?}");
    let logged = LogError(&error).to_string();
    assert!(logged.contains(&LogPrivate::path(path).to_string()), "{logged:?}");
    assert_eq!(error.to_string(), format!("archive I/O error: {cause}"));
}
