//! [`VolumeError::transport_fault`]: the one answer to "did the link fail, or did
//! the other end say no?", and [`is_transport_errno`], the one errno set behind it.
//!
//! Every layer that reacts to a dropped connection asks this, typed: the transfer
//! engine's per-file retry and outage wait, its settle of an unanswered create,
//! and the local engine's error mapping (`cmdr-operations`
//! `error_classification.rs`). ❌ Don't keep a second errno list or a second
//! "is this transport" match anywhere: two lists drift (they did: one knew
//! `ECONNRESET` and the other didn't).

use super::VolumeError;

/// Which way the link failed. The noun the retry policy, the outage wait, and the
/// error UX key off, so each can say WHY it waited without re-deriving it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransportFault {
    /// A deadline passed with no answer: the connection is likely dead and the
    /// socket doesn't know it yet ([`VolumeError::ConnectionTimeout`]).
    Timeout,
    /// The session or device went away mid-operation
    /// ([`VolumeError::DeviceDisconnected`]).
    ConnectionLost,
    /// The device's session died and a reopen is already running
    /// ([`VolumeError::DeviceSessionReset`], MTP).
    SessionReset,
    /// A cached handle went stale and the backend has already refreshed it
    /// ([`VolumeError::StaleDestinationHandle`], MTP).
    StaleHandle,
    /// The OS reported a transport errno ([`is_transport_errno`]), typically from
    /// an OS-mounted network share. Carries it.
    TransportErrno(i32),
}

impl VolumeError {
    /// The transport fault this error is, or `None` for a refusal, a fact about
    /// the data, or a person's decision.
    ///
    /// Matched exhaustively on purpose: a new variant must decide, never inherit
    /// an answer from a wildcard. Classification is by variant and errno, ❌ never
    /// by message text.
    #[must_use]
    pub fn transport_fault(&self) -> Option<TransportFault> {
        match self {
            Self::ConnectionTimeout(_) => Some(TransportFault::Timeout),
            Self::DeviceDisconnected(_) => Some(TransportFault::ConnectionLost),
            Self::DeviceSessionReset(_) => Some(TransportFault::SessionReset),
            Self::StaleDestinationHandle(_) => Some(TransportFault::StaleHandle),
            Self::IoError {
                raw_os_error: Some(code),
                ..
            } => is_transport_errno(*code).then_some(TransportFault::TransportErrno(*code)),

            // Below: a decision, a refusal, or a fact about the data. Asking again
            // gets the same answer.
            Self::Cancelled(_) // the person asked us to stop; ❌ never a fault
            | Self::NotConnected(_) // nothing is dialing it; only opening it connects
            | Self::NotFound(_)
            | Self::PermissionDenied { .. }
            | Self::AlreadyExists(_)
            | Self::NotSupported
            | Self::ReadOnly(_)
            | Self::StorageFull { .. }
            | Self::IsADirectory(_)
            | Self::NotADirectory(_) // a file is in the way until someone moves it
            | Self::InvalidName(_) // the destination can't hold this name
            | Self::DeletePending(_)
            | Self::ColdStorage(_) // archived until someone restores it
            | Self::SourceChanged(_) // a new version is there; copying it is the person's call
            | Self::AmbiguousName(_) // two stored names fit; asking again asks the same question
            | Self::DuplicateName(_) // two siblings share the name until someone renames one
            | Self::SignatureInvalid(_) // a refused reply, and a possible sign of tampering
            | Self::IoError { raw_os_error: None, .. } // nothing to classify on
            | Self::NeedsPassword { .. }
            | Self::FriendlyGit(_) => None,
        }
    }
}

/// Whether `code` is an errno that means "the link went away", not "the request
/// was refused".
///
/// Decided through [`std::io::ErrorKind`], which is portable and needs no `libc`
/// (this crate declares `libc` per target OS on purpose).
#[must_use]
pub fn is_transport_errno(code: i32) -> bool {
    use std::io::ErrorKind;

    matches!(
        std::io::Error::from_raw_os_error(code).kind(),
        ErrorKind::BrokenPipe
            | ErrorKind::ConnectionReset
            | ErrorKind::ConnectionAborted
            | ErrorKind::NotConnected
            | ErrorKind::TimedOut
            | ErrorKind::NetworkDown
            | ErrorKind::NetworkUnreachable
            | ErrorKind::HostUnreachable
    )
}
