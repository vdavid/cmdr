use std::io::ErrorKind;

use cmdr_fs::volume::VolumeError;

use super::*;

#[test]
fn errno_maps_to_the_volume_vocabulary_carrying_the_path() {
    let path = "/sdcard/DCIM/missing.jpg";
    assert!(matches!(volume_error_from_errno(ENOENT, path), VolumeError::NotFound(p) if p == path));
    assert!(
        matches!(volume_error_from_errno(EACCES, path), VolumeError::PermissionDenied { path: p, .. } if p == path)
    );
    assert!(matches!(volume_error_from_errno(EPERM, path), VolumeError::PermissionDenied { path: p, .. } if p == path));
    assert!(matches!(volume_error_from_errno(EEXIST, path), VolumeError::AlreadyExists(p) if p == path));
    assert!(matches!(volume_error_from_errno(EROFS, path), VolumeError::ReadOnly(p) if p == path));
    assert!(matches!(
        volume_error_from_errno(ENOSPC, path),
        VolumeError::StorageFull { .. }
    ));
    assert!(matches!(volume_error_from_errno(EISDIR, path), VolumeError::IsADirectory(p) if p == path));
    assert!(matches!(volume_error_from_errno(ENAMETOOLONG, path), VolumeError::InvalidName(p) if p == path));
}

/// An errno the named arms don't cover still arrives as what it MEANS on the device,
/// which is Linux whatever the host is.
#[test]
fn an_unlisted_device_errno_arrives_as_its_linux_class() {
    let cases = [
        (20, OsErrorClass::NotADirectory),
        (5, OsErrorClass::Io),
        (16, OsErrorClass::Busy),
        (18, OsErrorClass::CrossDevice),
        (40, OsErrorClass::LinkLoop),
        (95, OsErrorClass::Unsupported),
        (107, OsErrorClass::NotConnected),
        (110, OsErrorClass::TimedOut),
        (116, OsErrorClass::Stale),
        (122, OsErrorClass::QuotaExceeded),
    ];
    for (errno, class) in cases {
        let err = volume_error_from_errno(errno, "/x");
        assert!(matches!(err, VolumeError::IoError { .. }), "{errno}: {err:?}");
        assert_eq!(err.os_class(), Some(class), "device errno {errno}");
    }
}

/// The regression anchor: these Linux numbers each mean something ELSE on macOS (70 is
/// its `ESTALE`, 57 its `ENOTCONN`, 60 its `ETIMEDOUT`, 35 its `EAGAIN`), so passing
/// one through as the host's code retried a rare device error as a dropped link or
/// flagged a stale mount. Linux 70 is `ECOMM`, 57 `EBADSLT`, 60 `ENOSTR`, 35
/// `EDEADLK`: none has a class, so none carries a code.
#[test]
fn a_device_errno_with_no_class_carries_no_host_code() {
    for errno in [35, 57, 60, 70, 9999, -1] {
        let err = volume_error_from_errno(errno, "/x");
        assert!(
            matches!(err, VolumeError::IoError { raw_os_error: None, .. }),
            "device errno {errno}: {err:?}"
        );
        assert_eq!(err.os_class(), None, "device errno {errno}");
    }
}

/// The device table is written in literal Linux numbers. On a Linux host those are the
/// host's own, so the OS's table checks every row.
#[cfg(target_os = "linux")]
#[test]
fn the_device_table_matches_linux_itself() {
    for &(errno, class) in DEVICE_ERRNO_CLASSES {
        assert_eq!(OsErrorClass::of_code(errno), class, "errno {errno}");
    }
}

/// One row per number, so a lookup can't depend on the order.
#[test]
fn the_device_table_names_each_errno_once() {
    let mut seen = std::collections::HashSet::new();
    for &(errno, _) in DEVICE_ERRNO_CLASSES {
        assert!(seen.insert(errno), "errno {errno} is listed twice");
    }
}

#[test]
fn enotempty_is_translated_to_the_host_number() {
    let err = volume_error_from_errno(ENOTEMPTY_DEVICE, "/x");
    assert!(matches!(err, VolumeError::IoError { .. }));
    assert_eq!(err.os_class(), Some(OsErrorClass::NotEmpty));
}

#[test]
fn transport_failures_map_by_shape() {
    assert!(
        matches!(volume_error_from_adb(AdbError::DeviceGone, "/p"), VolumeError::DeviceDisconnected(p) if p == "/p")
    );
    assert!(matches!(volume_error_from_adb(AdbError::Timeout, "/p"), VolumeError::ConnectionTimeout(p) if p == "/p"));
    assert!(matches!(volume_error_from_adb(AdbError::Cancelled, "/p"), VolumeError::Cancelled(p) if p == "/p"));
    assert!(matches!(
        volume_error_from_adb(AdbError::Refused("nope".into()), "/p"),
        VolumeError::IoError { message, raw_os_error: None } if message == "nope"
    ));
    assert!(matches!(
        volume_error_from_adb(AdbError::Io(std::io::Error::other("x")), "/p"),
        VolumeError::IoError { raw_os_error: None, .. }
    ));
}

#[test]
fn io_errors_sort_into_typed_variants() {
    assert!(matches!(
        AdbError::from(std::io::Error::from(ErrorKind::UnexpectedEof)),
        AdbError::DeviceGone
    ));
    assert!(matches!(
        AdbError::from(std::io::Error::from(ErrorKind::ConnectionReset)),
        AdbError::DeviceGone
    ));
    assert!(matches!(
        AdbError::from(std::io::Error::from(ErrorKind::BrokenPipe)),
        AdbError::DeviceGone
    ));
    assert!(matches!(
        AdbError::from(std::io::Error::from(ErrorKind::TimedOut)),
        AdbError::Timeout
    ));
    assert!(matches!(
        AdbError::from(std::io::Error::from(ErrorKind::Other)),
        AdbError::Io(_)
    ));
}

#[test]
fn connect_errors_from_transport_errors() {
    assert!(matches!(
        AdbConnectError::from(AdbError::Timeout),
        AdbConnectError::TimedOut
    ));
    assert!(matches!(
        AdbConnectError::from(AdbError::Cancelled),
        AdbConnectError::Cancelled
    ));
    assert!(matches!(
        AdbConnectError::from(AdbError::Io(std::io::Error::from(ErrorKind::ConnectionRefused))),
        AdbConnectError::ServerUnreachable(_)
    ));
    assert!(matches!(AdbConnectError::from(AdbError::Refused("x".into())), AdbConnectError::Transport(m) if m == "x"));
    let gone = AdbConnectError::from(AdbError::DeviceGone).for_device("SER");
    assert!(matches!(gone, AdbConnectError::DeviceGone(s) if s == "SER"));
}

/// Every refusal names the path it was about, and a message variant marks it in a log line
/// rather than flattening it into words the report's regex has to guess the end of.
#[test]
fn a_message_variant_marks_the_path_it_names() {
    use cmdr_fs::log_error::LogError;
    use cmdr_fs::log_private::LogPrivate;

    let path = "/sdcard/DCIM/Lake House 2026/IMG 0001.jpg";
    let span = LogPrivate::path(path).to_string();
    let errors = [
        volume_error_from_errno(ENOSPC, path),
        volume_error_from_errno(ENAMETOOLONG, path),
        volume_error_from_errno(ENOTEMPTY_DEVICE, path),
        volume_error_from_errno(20, path),
        volume_error_from_adb(AdbError::DeviceGone, path),
        volume_error_from_adb(AdbError::Timeout, path),
        volume_error_from_adb(AdbError::Cancelled, path),
    ];
    for error in errors {
        let logged = LogError(&error).to_string();
        assert!(logged.contains(&span), "{logged:?}");
        assert!(error.to_string().contains(path), "{error}");
    }
}
