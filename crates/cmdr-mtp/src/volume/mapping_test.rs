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

/// A phone that went away mid-copy (a pulled cable) reads as the device leaving,
/// ❌ never the generic `IoError` "Couldn't copy the file" with a report button
/// (mtp-rs#53).
#[test]
fn a_dropped_session_maps_to_device_disconnected() {
    let mapped = map_mtp_error(MtpConnectionError::Disconnected {
        device_id: "mtp-1".to_string(),
    });
    assert!(matches!(mapped, VolumeError::DeviceDisconnected(_)), "got {mapped:?}");
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

/// A folder the phone won't let anyone read is the phone's refusal, carrying its path,
/// which is what the pane's and a copy's advice turn on (`refuser_of_path`), ❌ never
/// `ReadOnly`.
#[test]
fn a_refused_read_maps_to_a_permission_denial_on_its_path() {
    let mapped = map_mtp_error(MtpConnectionError::ReadRefused {
        device_id: "mtp-1".to_string(),
        path: PATH.to_string(),
    });
    assert!(
        matches!(&mapped, VolumeError::PermissionDenied { path, raw_os_error: None } if path == PATH),
        "got {mapped:?}"
    );
}

/// ❗ A write refused the same way keeps the read-only answer.
#[test]
fn a_refused_write_stays_read_only() {
    let mapped = map_mtp_error(MtpConnectionError::StoreReadOnly {
        device_id: "mtp-1".to_string(),
    });
    assert!(matches!(mapped, VolumeError::ReadOnly(_)), "got {mapped:?}");
}

/// ❗ These two are about this machine's USB access, so they can't reach the
/// `PermissionDenied` arm, where an `mtp://` path reads "this phone keeps this folder
/// private" (#841).
#[test]
fn a_usb_level_refusal_never_reads_as_the_phone_keeping_a_folder() {
    for error in [
        MtpConnectionError::ExclusiveAccess {
            device_id: "mtp-1".to_string(),
            blocking_process: None,
        },
        MtpConnectionError::PermissionDenied {
            device_id: "mtp-1".to_string(),
        },
    ] {
        let mapped = map_mtp_error(error);
        assert!(matches!(mapped, VolumeError::NotConnected(_)), "got {mapped:?}");
    }
}

/// A refusal that names a folder keeps the connection error typed in the volume error's
/// detail, so a log line marks the folder; the errno a classifier reads stays.
#[test]
fn a_refused_delete_of_a_non_empty_folder_marks_the_folder() {
    let error = map_mtp_error(MtpConnectionError::DirectoryNotEmpty {
        device_id: "mtp-20-5".to_string(),
        path: PATH.to_string(),
    });
    assert!(matches!(error, VolumeError::IoError { .. }));
    assert_eq!(error.os_class(), Some(OsErrorClass::NotEmpty));
    let logged = LogError(&error).to_string();
    let span = LogPrivate::dir_path(PATH).to_string();
    assert!(logged.contains(&span), "{logged:?}");
}
