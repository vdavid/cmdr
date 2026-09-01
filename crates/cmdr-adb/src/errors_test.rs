use std::io::ErrorKind;

use cmdr_fs::volume::VolumeError;

use super::*;

#[test]
fn errno_maps_to_the_volume_vocabulary_carrying_the_path() {
    let path = "/sdcard/DCIM/missing.jpg";
    assert!(matches!(volume_error_from_errno(ENOENT, path), VolumeError::NotFound(p) if p == path));
    assert!(matches!(volume_error_from_errno(EACCES, path), VolumeError::PermissionDenied(p) if p == path));
    assert!(matches!(volume_error_from_errno(EPERM, path), VolumeError::PermissionDenied(p) if p == path));
    assert!(matches!(volume_error_from_errno(EEXIST, path), VolumeError::AlreadyExists(p) if p == path));
    assert!(matches!(volume_error_from_errno(EROFS, path), VolumeError::ReadOnly(p) if p == path));
    assert!(matches!(
        volume_error_from_errno(ENOSPC, path),
        VolumeError::StorageFull { .. }
    ));
    assert!(matches!(volume_error_from_errno(EISDIR, path), VolumeError::IsADirectory(p) if p == path));
    assert!(matches!(volume_error_from_errno(ENAMETOOLONG, path), VolumeError::InvalidName(p) if p == path));
}

#[test]
fn unknown_errno_is_an_io_error_carrying_the_number() {
    let err = volume_error_from_errno(20, "/x");
    assert!(matches!(
        err,
        VolumeError::IoError {
            raw_os_error: Some(20),
            ..
        }
    ));
}

#[test]
fn enotempty_is_translated_to_the_host_number() {
    let err = volume_error_from_errno(ENOTEMPTY_DEVICE, "/x");
    assert!(matches!(err, VolumeError::IoError { raw_os_error: Some(n), .. } if n == ENOTEMPTY_HOST));
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
