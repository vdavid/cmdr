use cmdr_fs::log_error::LogError;
use cmdr_fs::log_private::LogPrivate;

use super::*;

/// A phone folder's path, relative to its storage: no regex claims it in prose.
const PATH: &str = "DCIM/Lake House 2026";

/// ❗ Same-named siblings are a device's own refusal, ❌ never SMB's look-alike
/// `AmbiguousName`: the two read differently to a person (#523), and the
/// frontend picks its words by the variant alone.
#[test]
fn same_named_siblings_map_to_a_duplicate_name() {
    let mapped = map_mtp_error(MtpConnectionError::AmbiguousName {
        device_id: "mtp-1".to_string(),
        path: "/DCIM/IMG_1.JPG".to_string(),
    });
    assert!(
        matches!(&mapped, VolumeError::DuplicateName(path) if path == "/DCIM/IMG_1.JPG"),
        "got {mapped:?}"
    );
}

/// A name past what a PTP string holds can only fail again under the same name, so
/// it's the destination's name refusal (ADB's `ENAMETOOLONG` lands on the same
/// variant), ❌ never the retryable `IoError`.
#[test]
fn a_name_too_long_for_the_phone_maps_to_an_invalid_name() {
    let mapped = map_mtp_error(MtpConnectionError::NameTooLong {
        device_id: "mtp-1".to_string(),
        units: 255,
    });
    assert!(matches!(mapped, VolumeError::InvalidName(_)), "got {mapped:?}");
}

/// A refusal that names a folder keeps the connection error typed in the volume error's
/// detail, so a log line marks the folder; the errno a classifier reads stays.
#[test]
fn a_refused_delete_of_a_non_empty_folder_marks_the_folder() {
    let error = map_mtp_error(MtpConnectionError::DirectoryNotEmpty {
        device_id: "mtp-20-5".to_string(),
        path: PATH.to_string(),
    });
    assert!(matches!(
        error,
        VolumeError::IoError {
            raw_os_error: Some(ENOTEMPTY),
            ..
        }
    ));
    let logged = LogError(&error).to_string();
    let span = LogPrivate::dir_path(PATH).to_string();
    assert!(logged.contains(&span), "{logged:?}");
}
