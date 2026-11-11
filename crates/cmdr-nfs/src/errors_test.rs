use cmdr_fs::volume::{TransportFault, VolumeError};

use super::*;
use crate::proto::rpc::AuthStat;
use crate::xdr::XdrError;

const PATH: &str = "/photos/beach.jpg";

fn status(number: u32) -> VolumeError {
    map_nfs_status(NfsStat3::from_number(number), PATH)
}

#[test]
fn the_path_carrying_statuses_carry_the_path_and_never_server_text() {
    assert!(matches!(status(2), VolumeError::NotFound(p) if p == PATH));
    // A stale handle is a file that's gone by the time we asked.
    assert!(matches!(status(70), VolumeError::NotFound(p) if p == PATH));
    assert!(matches!(status(10001), VolumeError::NotFound(p) if p == PATH));
    assert!(matches!(
        status(13),
        VolumeError::PermissionDenied { path, raw_os_error: Some(libc::EACCES) } if path == PATH
    ));
    assert!(matches!(
        status(1),
        VolumeError::PermissionDenied { path, raw_os_error: None } if path == PATH
    ));
    assert!(matches!(status(17), VolumeError::AlreadyExists(p) if p == PATH));
    assert!(matches!(status(20), VolumeError::NotADirectory(p) if p == PATH));
    assert!(matches!(status(21), VolumeError::IsADirectory(p) if p == PATH));
    assert!(matches!(status(30), VolumeError::ReadOnly(p) if p == PATH));
}

#[test]
fn the_errno_shaped_statuses_carry_their_errno() {
    let errno = |number| status(number).raw_os_error();
    assert_eq!(errno(66), Some(libc::ENOTEMPTY));
    assert_eq!(errno(10008), Some(libc::EAGAIN));
    assert_eq!(errno(5), Some(libc::EIO));
    assert_eq!(errno(18), Some(libc::EXDEV));
    assert_eq!(errno(22), Some(libc::EINVAL));
    assert_eq!(errno(27), Some(libc::EFBIG));
}

#[test]
fn a_full_disk_or_quota_is_storage_full_and_a_long_name_is_invalid() {
    assert!(matches!(status(28), VolumeError::StorageFull { .. }));
    assert!(matches!(status(69), VolumeError::StorageFull { .. }));
    assert!(matches!(status(63), VolumeError::InvalidName(_)));
    assert!(matches!(status(10004), VolumeError::NotSupported));
}

#[test]
fn an_unknown_status_is_kept_unclassified_rather_than_guessed() {
    assert_eq!(NfsStat3::from_number(4242), NfsStat3::Unknown(4242));
    assert!(matches!(status(4242), VolumeError::IoError { raw_os_error: None, .. }));
}

#[test]
fn a_lost_connection_is_a_transport_fault_and_a_refusal_is_not() {
    let lost = map_nfs_error(&NfsError::Call(CallError::Disconnected("reset".into())), PATH);
    assert_eq!(lost.transport_fault(), Some(TransportFault::ConnectionLost));
    let retired = map_call_error(&CallError::Retired, PATH);
    assert_eq!(retired.transport_fault(), Some(TransportFault::ConnectionLost));
    let timed_out = map_call_error(&CallError::TimedOut, PATH);
    assert_eq!(timed_out.transport_fault(), Some(TransportFault::Timeout));
    // An unanswered CREATE may have happened: the timeout errno, which a
    // caller must settle by looking.
    let unanswered = map_call_error(&CallError::Unanswered, PATH);
    assert_eq!(
        unanswered.transport_fault(),
        Some(TransportFault::OsTransport(cmdr_fs::os_error::OsErrorClass::TimedOut))
    );

    let refused = map_call_error(&CallError::Denied(Rejection::Auth(AuthStat::TooWeak)), PATH);
    assert!(matches!(
        &refused,
        VolumeError::PermissionDenied { path, raw_os_error: None } if path == PATH
    ));
    assert_eq!(refused.transport_fault(), None);
    let garbage = map_call_error(&CallError::Decode(XdrError::Truncated { needed: 8, left: 2 }), PATH);
    assert_eq!(garbage.raw_os_error(), Some(libc::EBADMSG));
}
