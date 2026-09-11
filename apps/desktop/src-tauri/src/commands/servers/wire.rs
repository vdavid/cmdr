//! The wire vocabulary `servers.rs`'s commands speak: every type a command
//! hands the frontend. Split out of `servers.rs` to keep that file under its
//! length cap; the commands themselves, and the facade's own rationale, stay
//! there.

use serde::{Deserialize, Serialize};

use crate::commands::sftp::SftpHostKeyIdentity;
use crate::network::saved_server_fields;
use cmdr_sftp::transport::HostKeyPrompt;

/// Which protocol an account speaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ServerProtocol {
    /// An SMB host. ❗ Listed, never pinned in this effort; see [`SavedServer`].
    Smb,
    /// An SFTP server, one account per entry.
    Sftp,
    /// A WebDAV server, one account per entry.
    Webdav,
}

/// Whether a person NAMED this server, or the label is a stand-in.
///
/// ❗ The hub's Name column ranks three names (a name a person chose, the Bonjour
/// name mDNS found, a stand-in nobody chose), and the top rank is a FACT this
/// enum publishes, ❌ never a guess at the string's shape. Only the store that
/// wrote the label knows where it came from.
///
/// ❗ Every SMB row is [`Fallback`](Self::Fallback) today because SMB has no name
/// field to fill in yet; adding one changes what a store answers here and nothing
/// else.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ServerNameSource {
    /// A person typed the NAME itself, in the sign-in sheet's Name field.
    User,
    /// A stand-in the app derived, because nothing better existed: an SFTP or
    /// WebDAV account's `username@host`, the SMB mount's `server_name` (which
    /// `statfs` spells as the server answered, `smb-consumer-guest` rather than
    /// `SMB Test (Guest)`), or the address typed into "Add server", which is all
    /// an SMB host is ever given.
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
    pub app_root: String,
}

/// An endpoint plus an identity, as the hub lists it.
///
/// ❗ **An SMB host lists NO places and cannot be pinned here.**
/// `known_shares.rs` stores no share rows (its only writer leaves `share_name`
/// empty), carries no port, and a mounted share's id comes from `statfs`, which
/// normalizes an mDNS name to an IP — so no id derivable from the store would
/// match the mounted volume, and a pin would point at nothing. SMB places keep
/// reaching the switcher as mounted volumes, and the hub opens an SMB host into
/// its live places list. A share-level writer at mount time is what pinnable SMB
/// shares need, and that is recorded as later work rather than half-built here.
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
    /// The account, where the protocol has one. `None` for an SMB host, which is
    /// not an account yet.
    pub username: Option<String>,
    /// Whether this account's place belongs in the switcher. Always `false` for
    /// SMB, per the type's own note.
    pub pinned: bool,
    /// ISO 8601, so a hub can sort by recency. `None` when nothing recorded one.
    pub last_connected_at: Option<String>,
    /// The mountable things under it. One for SFTP and WebDAV, none for SMB.
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
    /// WebDAV only: the address the user typed isn't a `http`/`https` URL.
    InvalidUrl,
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
