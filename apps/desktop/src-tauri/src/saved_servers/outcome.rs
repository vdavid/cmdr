//! How a connect ended, in the one vocabulary every caller branches on: the
//! frontend's sign-in flow, the servers facade (`commands::servers`), and Ask
//! Cmdr's `connect_volume`. Plus each wiring's own outcome, widened into it.

use serde::{Deserialize, Serialize};

use crate::network::nfs_volume_wiring::NfsConnection;
use crate::network::s3_volume_wiring::S3Connection;
use crate::network::sftp_volume_wiring::SftpConnection;
use crate::network::webdav_volume_wiring::WebdavConnection;
use cmdr_fs::volume::tls::CertificatePrompt;
use cmdr_sftp::transport::HostKeyPrompt;

/// One host key, named the way a human checks it against `ssh-keygen -lf`.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SftpHostKeyIdentity {
    /// The SSH key-type name the server presented.
    pub algorithm: String,
    /// Its OpenSSH `SHA256:…` fingerprint.
    pub fingerprint: String,
}

/// What a connect attempt produced, across every protocol this family speaks.
///
/// ❗ The superset of the two per-protocol enums, so a sign-in UI branches once.
/// Every outcome is a variant, including the ones that read as failures, and ❌
/// none may be recovered from a message.
///
/// ❗ `AuthMethodUnsupported` stops surfacing as `AuthenticationRejected` here: a
/// Digest-only server never saw the password, and "check your password" is the
/// wrong fix to put in front of someone.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "outcome", rename_all_fields = "camelCase")]
pub enum ServerConnectOutcome {
    /// A live volume, already registered and already in the saved list.
    Connected {
        /// The id every listing, tab, and index entry is filed under.
        volume_id: String,
    },
    /// SFTP only: the server's host key needs a human. ❗ No session is held
    /// across the prompt; approving is followed by dialing again.
    NeedsHostKeyApproval(HostKeyPrompt),
    /// SFTP only: the key is explicitly revoked in `~/.ssh/known_hosts`. ❌ Not
    /// approvable.
    HostKeyRevoked(SftpHostKeyIdentity),
    /// The credential offered was refused. ❗ Only a freshly typed one moves this
    /// forward; retrying the same secret can lock the account.
    AuthenticationRejected,
    /// Nothing was ever offered. ❗ Not a rejection: telling someone who has
    /// never entered a password that theirs is wrong is what collapsing the two
    /// does.
    NeedsCredentials,
    /// The server challenged with a scheme this app doesn't speak (Digest). ❗
    /// The secret was never offered, so nothing about it is known to be wrong.
    AuthMethodUnsupported,
    /// WebDAV only: macOS refused the certificate on trust, and a person may
    /// pin it (`approve_webdav_certificate`), then dial again. Carries whether
    /// it's first contact or a pinned certificate that CHANGED.
    NeedsCertificateApproval(CertificatePrompt),
    /// S3 only: the OS didn't trust the endpoint's certificate (a self-signed
    /// one), so adding it to the trust store is the fix. WebDAV asks instead
    /// (`NeedsCertificateApproval`).
    CertificateUntrusted,
    /// WebDAV and S3: the certificate is revoked, malformed, or badly signed, so
    /// no trust store or pin answers it. ❌ Not approvable, and the copy never
    /// says to trust it (#1111).
    CertificateRejected,
    /// WebDAV only: the URL answers HTTP but not WebDAV.
    NotAWebdavServer,
    /// WebDAV and S3: the address the user typed isn't a usable `http`/`https`
    /// URL (for S3, also a region, location, or account ID a host name can't
    /// carry).
    InvalidUrl,
    /// S3 only: the bucket refused this key, which is a wrong key or one without
    /// rights here (a bodyless 403 can't say which).
    AccessDenied,
    /// S3 only: the account root needs `ListBuckets` and this key may not (or,
    /// on some servers, its secret is wrong). A bucket name is the way in.
    BucketListRefused,
    /// S3 only: no bucket by that name on this endpoint.
    BucketNotFound,
    /// S3 only: the bucket lives in another region than the one chosen.
    RegionMismatch {
        /// The bucket's region, when the server named it.
        region: Option<String>,
    },
    /// S3 only: this Mac's clock is too far off for the server to accept a
    /// signature.
    ClockSkewed,
    /// S3 only: the address answers, but not as S3.
    NotAnS3Endpoint,
    /// NFS only: the server has no export by that path.
    ExportNotFound,
    /// NFS only: the server refused this Mac the export (its client list or
    /// its rules), although the request came from a reserved port.
    ExportRefused,
    /// NFS only: the export refused us, and this process couldn't bind a source
    /// port below 1024, which a `secure` export (the Linux default) demands. ❗
    /// The fix is the port or the export's `insecure` option, ❌ not the address.
    ReservedPortRequired,
    /// NFS only: the server speaks no NFSv3 over TCP. ❌ Never a fallback to
    /// another version.
    NoNfsV3,
    /// The start folder isn't the root or under it. ❗ Refused before dialing,
    /// so nothing was registered or saved.
    StartFolderOutsideRoot,
    /// The handshake didn't finish inside the connect budget.
    TimedOut,
    /// SMB on macOS only: the server answers, and macOS's network sign-in helper
    /// (`NetAuthSysAgent`) looks wedged, so no mount can go through until it's
    /// restarted (`network/os_mount/netfs_wedge.rs`).
    SignInHelperStuck,
    /// No route, refused, DNS, or a transport-level breakdown.
    Unreachable,
    /// The user called it off. ❗ Nothing was registered, remembered, or stored.
    Cancelled,
}

/// Why dialing a SAVED place couldn't even start.
///
/// ❗ Separate from [`ServerConnectOutcome`], because neither of these is
/// something a person did: both mean the caller picked the wrong move for this
/// volume's standing, which `servers/connect-flow.ts` decides. A user should
/// never see one.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "reason", rename_all_fields = "camelCase")]
pub enum SavedPlaceRefusal {
    /// Nothing saved has that place. A forget racing an activation lands here.
    NoSuchServer {
        /// The id that was asked for.
        volume_id: String,
    },
    /// ❗ A volume is registered under that id already. Re-dialing would register
    /// a SECOND volume; a session that dropped is mended by
    /// `reconnect_volume_with_credentials`, which is what enforces the read-only
    /// username rule and the never-seeds rule.
    AlreadyConnected {
        /// The id that is already live.
        volume_id: String,
    },
}

/// The SFTP wiring's outcome, widened into the superset.
pub(crate) fn outcome_from_sftp(connection: SftpConnection) -> ServerConnectOutcome {
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
pub(crate) fn outcome_from_webdav(connection: WebdavConnection) -> ServerConnectOutcome {
    match connection {
        WebdavConnection::Connected { volume_id } => ServerConnectOutcome::Connected { volume_id },
        WebdavConnection::AuthenticationRejected => ServerConnectOutcome::AuthenticationRejected,
        WebdavConnection::NeedsCredentials => ServerConnectOutcome::NeedsCredentials,
        // ❗ Its own variant, ❌ never folded into `AuthenticationRejected`: a
        // Digest-only server never saw the password.
        WebdavConnection::AuthMethodUnsupported => ServerConnectOutcome::AuthMethodUnsupported,
        WebdavConnection::NeedsCertificateApproval(prompt) => ServerConnectOutcome::NeedsCertificateApproval(prompt),
        WebdavConnection::CertificateRejected => ServerConnectOutcome::CertificateRejected,
        WebdavConnection::NotAWebdavServer => ServerConnectOutcome::NotAWebdavServer,
        WebdavConnection::TimedOut => ServerConnectOutcome::TimedOut,
        WebdavConnection::Unreachable => ServerConnectOutcome::Unreachable,
        WebdavConnection::Cancelled => ServerConnectOutcome::Cancelled,
    }
}

/// The S3 wiring's outcome, widened into the superset.
pub(crate) fn outcome_from_s3(connection: S3Connection) -> ServerConnectOutcome {
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
        S3Connection::CertificateRejected => ServerConnectOutcome::CertificateRejected,
        S3Connection::NotAnS3Endpoint => ServerConnectOutcome::NotAnS3Endpoint,
        S3Connection::InvalidProvider => ServerConnectOutcome::InvalidUrl,
        S3Connection::TimedOut => ServerConnectOutcome::TimedOut,
        S3Connection::Unreachable => ServerConnectOutcome::Unreachable,
        S3Connection::Cancelled => ServerConnectOutcome::Cancelled,
    }
}

/// The NFS wiring's outcome, widened into the superset.
pub(crate) fn outcome_from_nfs(connection: NfsConnection) -> ServerConnectOutcome {
    match connection {
        NfsConnection::Connected { volume_id } => ServerConnectOutcome::Connected { volume_id },
        NfsConnection::ExportNotFound => ServerConnectOutcome::ExportNotFound,
        NfsConnection::ExportRefused => ServerConnectOutcome::ExportRefused,
        NfsConnection::ReservedPortRequired => ServerConnectOutcome::ReservedPortRequired,
        NfsConnection::NoNfsV3 => ServerConnectOutcome::NoNfsV3,
        NfsConnection::TimedOut => ServerConnectOutcome::TimedOut,
        NfsConnection::Unreachable => ServerConnectOutcome::Unreachable,
        NfsConnection::Cancelled => ServerConnectOutcome::Cancelled,
    }
}
