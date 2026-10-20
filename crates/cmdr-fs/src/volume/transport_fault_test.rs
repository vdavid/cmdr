//! `VolumeError::transport_fault`: the link failing is a fault, a refusal or a
//! person's decision never is.

use super::{TransportFault, VolumeError, is_transport_errno};

fn io(code: i32) -> VolumeError {
    VolumeError::IoError {
        message: "from the OS".into(),
        raw_os_error: Some(code),
    }
}

/// The faults a retry and an outage wait exist for. Each one means "the
/// connection is gone", never "the request was refused".
#[test]
fn transport_blips_are_transport_faults() {
    assert_eq!(
        VolumeError::ConnectionTimeout("send timed out".into()).transport_fault(),
        Some(TransportFault::Timeout)
    );
    assert_eq!(
        VolumeError::DeviceDisconnected("session lost".into()).transport_fault(),
        Some(TransportFault::ConnectionLost)
    );
    assert_eq!(
        VolumeError::DeviceSessionReset("ptp reset".into()).transport_fault(),
        Some(TransportFault::SessionReset)
    );
    assert_eq!(
        VolumeError::StaleDestinationHandle("/Documents".into()).transport_fault(),
        Some(TransportFault::StaleHandle)
    );
}

#[cfg(unix)]
#[test]
fn transport_errnos_are_transport_faults_carrying_the_errno() {
    for code in [
        libc::ENOTCONN,
        libc::ENETDOWN,
        libc::ENETUNREACH,
        libc::EHOSTUNREACH,
        libc::ETIMEDOUT,
        libc::ECONNRESET,
    ] {
        assert_eq!(
            io(code).transport_fault(),
            Some(TransportFault::TransportErrno(code)),
            "errno {code}"
        );
    }
}

/// A write onto a share whose server went away mid-request (the failing
/// sender's own send broke).
#[cfg(unix)]
#[test]
fn an_epipe_is_a_transport_fault() {
    assert!(is_transport_errno(libc::EPIPE));
    assert_eq!(
        io(libc::EPIPE).transport_fault(),
        Some(TransportFault::TransportErrno(libc::EPIPE))
    );
}

#[cfg(unix)]
#[test]
fn an_econnaborted_is_a_transport_fault() {
    assert!(is_transport_errno(libc::ECONNABORTED));
    assert_eq!(
        io(libc::ECONNABORTED).transport_fault(),
        Some(TransportFault::TransportErrno(libc::ECONNABORTED))
    );
}

/// A cancel is the person's decision, and re-running the work would fight it.
/// The single most important negative.
#[test]
fn a_cancel_is_never_a_transport_fault() {
    assert_eq!(
        VolumeError::Cancelled("Operation cancelled by user".into()).transport_fault(),
        None
    );
}

/// Nothing is dialing a volume that was never connected; only opening it
/// connects, so waiting on it would wait for nothing.
#[test]
fn a_volume_never_connected_is_not_a_transport_fault() {
    assert_eq!(VolumeError::NotConnected("/smb/nas".into()).transport_fault(), None);
}

/// Refusals and facts about the data: asking again fails the same way.
#[test]
fn refusals_and_data_facts_are_not_transport_faults() {
    let refusals = [
        VolumeError::PermissionDenied {
            path: "/x".into(),
            raw_os_error: None,
        },
        VolumeError::NotFound("/x".into()),
        VolumeError::AlreadyExists("/x".into()),
        VolumeError::ReadOnly("/x".into()),
        VolumeError::StorageFull {
            message: "disk full".into(),
        },
        VolumeError::NotSupported,
        VolumeError::IsADirectory("/x".into()),
        VolumeError::NotADirectory("/x".into()),
        VolumeError::InvalidName("CON".into()),
        VolumeError::DeletePending("/x".into()),
        VolumeError::AmbiguousName("/x".into()),
        VolumeError::DuplicateName("/x".into()),
        VolumeError::SignatureInvalid("refused".into()),
        VolumeError::ColdStorage("/x".into()),
        VolumeError::SourceChanged("/x".into()),
        VolumeError::NeedsPassword { wrong_attempt: true },
        // No errno: nothing to classify on, so it's a report, not a blip.
        VolumeError::IoError {
            message: "something".into(),
            raw_os_error: None,
        },
    ];
    for refusal in refusals {
        assert_eq!(refusal.transport_fault(), None, "{refusal:?}");
    }
}

/// A refusal that DOES carry an errno still isn't transport.
#[cfg(unix)]
#[test]
fn a_refusal_errno_is_not_a_transport_fault() {
    for code in [libc::EACCES, libc::EPERM, libc::ENOSPC, libc::EROFS, libc::ENOENT] {
        assert!(!is_transport_errno(code), "errno {code}");
        assert_eq!(io(code).transport_fault(), None, "errno {code}");
    }
}
