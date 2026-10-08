//! The wire vocabulary `servers.rs`'s commands speak: every type a command
//! hands the frontend. Split out of `servers.rs` to keep that file under its
//! length cap; the commands themselves, and the facade's own rationale, stay
//! there.

use serde::{Deserialize, Serialize};

use crate::commands::sftp::SftpHostKeyIdentity;
use crate::network::s3_known_places::S3ProviderChoice;
use crate::network::saved_server_fields;
use cmdr_sftp::transport::HostKeyPrompt;

/// Which protocol an account speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ServerProtocol {
    /// An SMB host. ❗ The host row is never pinned; its saved shares are, see [`SavedServer`].
    Smb,
    /// An SFTP server, one account per entry.
    Sftp,
    /// A WebDAV server, one account per entry.
    Webdav,
    /// An S3 account (an endpoint plus an access key id), whose places are its
    /// saved buckets and, when saved, its root.
    S3,
}

/// Whether a person NAMED this server, or the label is a stand-in.
///
/// ❗ The hub's Name column ranks three names (a name a person chose, the Bonjour
/// name mDNS found, a stand-in nobody chose), and the top rank is a FACT this
/// enum publishes, ❌ never a guess at the string's shape. Only the store that
/// wrote the label knows where it came from.
///
/// ❗ An SMB host is [`User`](Self::User) only when a person named it in the add
/// or edit sheet (`manual_servers::ManualServerEntry::is_named`); a host only the
/// share history knows is always a stand-in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ServerNameSource {
    /// A person typed the NAME itself, in the sign-in sheet's Name field.
    User,
    /// A stand-in the app derived, because nothing better existed: an SFTP or
    /// WebDAV account's `username@host`, the SMB mount's `server_name` (which
    /// `statfs` spells as the server answered, `smb-consumer-guest` rather than
    /// `SMB Test (Guest)`), or the address typed into "Add server" when the Name
    /// field stayed empty.
    Fallback,
}

impl ServerNameSource {
    /// An SFTP or WebDAV account's answer: `User` when its stored name isn't blank.
    pub(super) fn of_account(stored_name: &str) -> Self {
        if saved_server_fields::is_named(stored_name) {
            Self::User
        } else {
            Self::Fallback
        }
    }
}

/// One mountable thing under an account: an SFTP or WebDAV root, later an S3
/// bucket or a shared drive. What a tab, a favorite, and a path point at.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SavedPlace {
    /// The id the registry files the volume under, and the id a `saved` row in
    /// the switcher already carries, so activating either dials the same entry.
    pub volume_id: String,
    /// What the switcher shows.
    pub name: String,
    /// Whether it belongs in the volume switcher.
    pub pinned: bool,
    /// Whether a session is live right now.
    pub connected: bool,
    /// The app-facing root this place addresses its files by
    /// (`sftp://ada@nas.local:22/srv`), which is what a tab, a favorite, and an
    /// MCP row point at.
    ///
    /// ❗ Read from `server_volumes::server_places()`, the one place that mints
    /// the spelling, ❌ never re-derived here: a second spelling of the prefix
    /// misses the volume its own id names.
    ///
    /// An SMB share's is where its last mount sat (`/Volumes/Container`), or
    /// `smb://<host>/<share>` for one no mount went through yet, which has no
    /// place in the volume list to land on.
    pub app_root: String,
    /// The account this place is opened as: the SFTP or WebDAV account, the S3
    /// access key id, or the account an SMB share was last mounted with (`None`
    /// for guest).
    pub username: Option<String>,
    /// This place's own "Reconnect automatically" switch. `None` for an SMB
    /// share, which has none. ❗ Per PLACE: an S3 account's buckets each carry
    /// their own, so a row menu reads it here rather than off the account.
    pub auto_reconnect: Option<bool>,
}

/// An endpoint plus an identity, as the hub lists it.
///
/// ❗ **An SMB host's places are its SAVED shares** (`known_shares.rs` share rows,
/// `docs/specs/saved-smb-shares.md`): each carries the volume id its last mount
/// had, read off `statfs`, so a pin points at the id the mounted volume really
/// has. The host row itself is never pinned; its shares are.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SavedServer {
    /// Stable across launches. For a one-place protocol it IS the place's volume
    /// id; for an SMB host it is the manual-server id shape.
    pub id: String,
    /// Which protocol, so the hub can show a Type column without parsing an
    /// address.
    pub protocol: ServerProtocol,
    /// What the hub calls it: a name a person chose, or a stand-in when nobody
    /// did (an account's `username@host`, an SMB host's address). `name_source`
    /// says which, so the frontend never re-derives one.
    pub display_name: String,
    /// Whether a person named it, which is what lets the hub prefer a Bonjour
    /// name over a stand-in nobody chose.
    pub name_source: ServerNameSource,
    /// What the user typed, near enough to paste back: `host:port` for SFTP, the
    /// base URL for WebDAV, the host for SMB.
    pub address: String,
    /// The account, where the protocol has one. For an SMB host, the account the
    /// person typed for it (a preference, not its identity), or `None`.
    pub username: Option<String>,
    /// Whether this account's place belongs in the switcher. Always `false` for an
    /// SMB host: its SHARES carry their own pins.
    pub pinned: bool,
    /// ISO 8601, so a hub can sort by recency. `None` when nothing recorded one.
    pub last_connected_at: Option<String>,
    /// The "Reconnect automatically" switch: redial on its own when a session
    /// DROPS (never a connect at startup). `None` for SMB, which has no such
    /// switch. What the row menus' checkbox shows; [`set_place_auto_reconnect`]
    /// moves it.
    ///
    /// [`set_place_auto_reconnect`]: crate::commands::servers::set_place_auto_reconnect
    pub auto_reconnect: Option<bool>,
    /// The mountable things under it. One for SFTP and WebDAV; for SMB, the
    /// host's saved shares, possibly none.
    pub places: Vec<SavedPlace>,
}

/// Which server to dial, in add mode.
///
/// ❗ A tagged union of the two existing param shapes rather than a widened
/// common one: an SFTP key file and a WebDAV base URL have no counterpart in the
/// other protocol, and a shape carrying both would have every call site guessing
/// which half applies. SMB stays on its own commands, because its connect is a
/// share mount rather than a session.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "protocol", rename_all_fields = "camelCase")]
pub enum ServerTarget {
    /// An SFTP account: display name, host, port, account, remote root, an
    /// optional start folder, an optional key file, and the agent/auto-reconnect
    /// switches.
    Sftp {
        /// What to call it in the UI.
        display_name: String,
        /// The server, as the user typed it.
        host: String,
        /// Its port. 22 everywhere but a jump box or a container.
        port: u16,
        /// The account to sign in as. ❗ Part of the identity.
        username: String,
        /// The remote directory the place is rooted at. Absolute, server-side.
        remote_root: String,
        /// Where a pane lands when the place itself is opened: an absolute
        /// server-side path at or under `remote_root`. `None` is the root.
        start_folder: Option<String>,
        /// A private key file to offer. ❗ A path, ❌ never a secret.
        key_file: Option<String>,
        /// Whether the running ssh-agent may be asked.
        use_agent: bool,
        /// Whether Cmdr may redial unattended when the session drops.
        auto_reconnect: bool,
    },
    /// A WebDAV account: display name, base URL, account, remote root, an
    /// optional start folder, and the auto-reconnect switch.
    Webdav {
        /// What to call it in the UI.
        display_name: String,
        /// The base URL, `http` or `https`.
        url: String,
        /// The account to sign in as. ❗ Part of the identity.
        username: String,
        /// The collection the place is rooted at, under the base URL.
        remote_root: String,
        /// Where a pane lands when the place itself is opened: a path at or
        /// under `remote_root`, in the same space. `None` is the root.
        start_folder: Option<String>,
        /// Whether Cmdr may re-probe unattended when a request finds it gone.
        auto_reconnect: bool,
    },
    /// One S3 place: display name, provider (which fixes the endpoint), access
    /// key id, an optional bucket, and the auto-reconnect switch.
    S3 {
        /// What to call it in the UI.
        display_name: String,
        /// The provider preset, and with it the endpoint.
        provider: S3ProviderChoice,
        /// The account's key. ❗ Part of the identity.
        access_key_id: String,
        /// The bucket this place is, or `None` for the account root, which lists
        /// the buckets. ❗ Part of the identity: each bucket is its own place.
        bucket: Option<String>,
        /// Whether Cmdr may re-probe unattended when a request finds it gone.
        auto_reconnect: bool,
    },
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
    /// WebDAV only: the TLS handshake didn't trust the certificate. ❌ Not
    /// approvable from here; the fix is trusting the CA in the OS store.
    CertificateUntrusted,
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
    /// The start folder isn't the root or under it. ❗ Refused before dialing,
    /// so nothing was registered or saved.
    StartFolderOutsideRoot,
    /// The handshake didn't finish inside the connect budget.
    TimedOut,
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
