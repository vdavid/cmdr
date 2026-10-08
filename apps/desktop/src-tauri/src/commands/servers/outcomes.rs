//! Each wiring's connect outcome, widened into the superset the frontend
//! branches on once ([`ServerConnectOutcome`]), and the saved entry an id names.
//! Split out of `servers.rs` to keep that file under its length cap.

use super::super::sftp::SftpHostKeyIdentity;
use super::webdav_params;
use super::wire::ServerConnectOutcome;
use crate::network::s3_volume_wiring::S3Connection;
use crate::network::sftp_volume_wiring::SftpConnection;
use crate::network::webdav_volume_wiring::WebdavConnection;
use crate::network::{known_shares, s3_known_places, sftp_known_servers, webdav_known_servers};

/// A saved entry, found by the id its place carries.
pub(super) enum SavedEntry {
    Sftp(sftp_known_servers::KnownSftpServer),
    Webdav(webdav_known_servers::KnownWebdavServer),
    /// One saved S3 place (a bucket, or an account root).
    S3(s3_known_places::KnownS3Place),
    /// A saved SMB share whose last mount had that id.
    Smb(known_shares::KnownNetworkShare),
}

/// The saved entry whose derived volume id is `volume_id`.
///
/// ❗ Derived rather than stored: the id funnel (`cmdr_fs::volume::ids`) is the
/// one place an id is minted, so looking one up means re-deriving it from the
/// same tuple rather than keeping a second copy that can drift.
pub(super) fn saved_by_id(volume_id: &str) -> Option<SavedEntry> {
    let sftp = sftp_known_servers::all()
        .into_iter()
        .find(|entry| cmdr_fs::volume::sftp_volume_id(&entry.host, entry.port, &entry.username) == volume_id);
    if let Some(entry) = sftp {
        return Some(SavedEntry::Sftp(entry));
    }
    let webdav = webdav_known_servers::all().into_iter().find(|entry| {
        webdav_params(&entry.url, &entry.username, "/").is_some_and(|params| {
            cmdr_fs::volume::webdav_volume_id(params.host(), params.port(), &entry.username) == volume_id
        })
    });
    if let Some(entry) = webdav {
        return Some(SavedEntry::Webdav(entry));
    }
    if let Some(entry) = s3_known_places::find(volume_id) {
        return Some(SavedEntry::S3(entry));
    }
    // ❗ Stored, ❌ not derived, unlike the two above: an SMB id comes off the
    // mount's `statfs`, and only the row knows which spelling the mount got.
    known_shares::share_by_volume_id(volume_id).map(SavedEntry::Smb)
}

/// The SFTP wiring's outcome, widened into the superset.
pub(super) fn outcome_from_sftp(connection: SftpConnection) -> ServerConnectOutcome {
    match connection {
        // The rung is dropped here: it is a fact about THIS dial, and the
        // superset's consumers ask `get_sftp_unattended_reconnect` when a banner
        // renders rather than deriving one from a stale rung.
        SftpConnection::Connected { volume_id, .. } => ServerConnectOutcome::Connected { volume_id },
        SftpConnection::NeedsHostKeyApproval(prompt) => ServerConnectOutcome::NeedsHostKeyApproval(prompt),
        SftpConnection::HostKeyRevoked { algorithm, fingerprint } => {
            ServerConnectOutcome::HostKeyRevoked(SftpHostKeyIdentity { algorithm, fingerprint })
        }
        SftpConnection::AuthenticationRejected => ServerConnectOutcome::AuthenticationRejected,
        SftpConnection::NeedsCredentials => ServerConnectOutcome::NeedsCredentials,
        SftpConnection::TimedOut => ServerConnectOutcome::TimedOut,
        SftpConnection::Unreachable => ServerConnectOutcome::Unreachable,
        SftpConnection::Cancelled => ServerConnectOutcome::Cancelled,
    }
}

/// The WebDAV wiring's outcome, widened into the superset.
pub(super) fn outcome_from_webdav(connection: WebdavConnection) -> ServerConnectOutcome {
    match connection {
        WebdavConnection::Connected { volume_id } => ServerConnectOutcome::Connected { volume_id },
        WebdavConnection::AuthenticationRejected => ServerConnectOutcome::AuthenticationRejected,
        WebdavConnection::NeedsCredentials => ServerConnectOutcome::NeedsCredentials,
        // ❗ Its own variant, ❌ never folded into `AuthenticationRejected`: a
        // Digest-only server never saw the password.
        WebdavConnection::AuthMethodUnsupported => ServerConnectOutcome::AuthMethodUnsupported,
        WebdavConnection::CertificateUntrusted => ServerConnectOutcome::CertificateUntrusted,
        WebdavConnection::NotAWebdavServer => ServerConnectOutcome::NotAWebdavServer,
        WebdavConnection::TimedOut => ServerConnectOutcome::TimedOut,
        WebdavConnection::Unreachable => ServerConnectOutcome::Unreachable,
        WebdavConnection::Cancelled => ServerConnectOutcome::Cancelled,
    }
}

/// The S3 wiring's outcome, widened into the superset.
pub(super) fn outcome_from_s3(connection: S3Connection) -> ServerConnectOutcome {
    match connection {
        S3Connection::Connected { volume_id } => ServerConnectOutcome::Connected { volume_id },
        // The keys were offered and named wrong: the same "check what you typed"
        // the other protocols' refusals ask for.
        S3Connection::KeysRejected => ServerConnectOutcome::AuthenticationRejected,
        S3Connection::AccessDenied => ServerConnectOutcome::AccessDenied,
        S3Connection::BucketListRefused => ServerConnectOutcome::BucketListRefused,
        S3Connection::BucketNotFound => ServerConnectOutcome::BucketNotFound,
        S3Connection::RegionMismatch { region } => ServerConnectOutcome::RegionMismatch { region },
        S3Connection::ClockSkewed => ServerConnectOutcome::ClockSkewed,
        S3Connection::NeedsCredentials => ServerConnectOutcome::NeedsCredentials,
        S3Connection::CertificateUntrusted => ServerConnectOutcome::CertificateUntrusted,
        S3Connection::NotAnS3Endpoint => ServerConnectOutcome::NotAnS3Endpoint,
        S3Connection::InvalidProvider => ServerConnectOutcome::InvalidUrl,
        S3Connection::TimedOut => ServerConnectOutcome::TimedOut,
        S3Connection::Unreachable => ServerConnectOutcome::Unreachable,
        S3Connection::Cancelled => ServerConnectOutcome::Cancelled,
    }
}
