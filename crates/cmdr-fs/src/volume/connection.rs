//! How a REMOTE volume describes itself: how live its session is, whether the
//! device behind it is reachable, which backend serves it, and what a sign-in on
//! it would ask a person for.
//!
//! Split out of `types` because these five are one vocabulary rather than five
//! unrelated payloads: the switcher dot, the pane's connect views, the reconnect
//! manager, the sign-in sheet, and the app's few transport-shaped decisions all
//! read from exactly this set, and it is the set that grows when a backend does
//! (S3's access keys, an OAuth provider, another device state). `mod.rs`
//! re-exports every item, so `cmdr_fs::volume::ConnectionState` is the path
//! either way.
//!
//! ❗ **Two pairs here are easy to confuse, and each pairing is a bug.**
//! [`ConnectionState`] is about a SESSION and [`BackendKind`] is about a
//! TRANSPORT: reading "has a connection state" as "is SMB" hands an SFTP volume
//! to the SMB indexer. [`ConnectionState`] is about a session and
//! [`DeviceReadiness`] is about PRESENCE: a phone waiting for its "Allow USB
//! debugging?" tap has no session to reconnect, and enrolling it in a backoff
//! loop dials nothing forever.

use serde::{Deserialize, Serialize};

/// How live a remote volume's SESSION is, for the switcher dot, the pane's
/// connect views, and the reconnect manager. Every connecting backend answers it
/// (SMB, SFTP, WebDAV, ADB); a local disk, an archive, and the git portal return
/// `None` from [`Volume::connection_state`](super::Volume::connection_state).
///
/// ❗ **A value here says nothing about WHICH backend serves the volume.** Ask
/// [`Volume::backend_kind`](super::Volume::backend_kind) for that: a `Some(_)`
/// used as an "is this SMB" test hands an SFTP volume to the SMB indexer.
///
/// Device PRESENCE is a different question and lives on [`DeviceReadiness`]: a
/// phone waiting for its "Allow USB debugging?" tap has no session to reconnect,
/// and must never start a backoff loop.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionState {
    /// A live session Cmdr owns: an smb2 session, an SFTP channel, a WebDAV
    /// server answering, a dialed ADB device. The fast path (green indicator).
    Direct,
    /// SMB only: the kernel mount is alive but Cmdr has no smb2 session of its
    /// own, so I/O goes through the OS. The slower fallback (yellow indicator).
    OsMount,
    /// The session dropped. The backoff loop is running, or it gave up.
    Disconnected,
    /// The backend stopped retrying because a credential is what's missing.
    /// Retrying costs an authentication attempt and buys nothing, so only the
    /// user moves this forward.
    NeedsSignIn,
    /// SFTP only: the server's host key isn't the one trusted for it. ❌ Never
    /// collapsed into [`NeedsSignIn`](Self::NeedsSignIn): putting a password box
    /// in front of a possible man-in-the-middle is how a password gets typed
    /// into one.
    NeedsHostKeyApproval,
    /// A saved place that isn't connected and has nothing in flight: the greyed
    /// switcher row with the hollow dot. Activating it dials.
    Saved,
}

impl ConnectionState {
    /// Whether the session is serving requests right now
    /// ([`Direct`](Self::Direct) or [`OsMount`](Self::OsMount)).
    ///
    /// The frontend's `isLiveSession` is the same predicate; both exist so a
    /// caller asking "can I trust an answer this volume just gave me" never
    /// spells out a variant list that a new state would silently fall out of.
    pub fn is_live(self) -> bool {
        matches!(self, Self::Direct | Self::OsMount)
    }
}

/// Whether the DEVICE behind a volume is reachable at all, which is a different
/// question from how live a session is ([`ConnectionState`]).
///
/// Set by the device providers only (MTP, ADB). A phone sitting on its "Allow USB
/// debugging?" prompt is present and answering the daemon, so there is nothing to
/// reconnect and no backoff loop to start; the pane waits for the tap instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum DeviceReadiness {
    /// The device answers and its storage is browsable.
    Ready,
    /// The device is there but hasn't authorized this host yet: the user has to
    /// tap "Allow" on the phone. Openable — the pane waits and navigates itself
    /// the moment the row turns ready.
    WaitingForAuthorization,
    /// The device is visible to the daemon but can't be used. The row is
    /// disabled and the reason is its tooltip.
    Unavailable {
        /// Why the device can't be used.
        reason: DeviceUnavailableReason,
    },
}

/// Why a listed device can't be used right now ([`DeviceReadiness::Unavailable`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DeviceUnavailableReason {
    /// The daemon lists the device but it isn't responding (a sleeping phone, a
    /// half-seated cable).
    Offline,
    /// The daemon can't claim the USB device (a permissions or driver problem on
    /// this machine).
    NoPermissions,
}

/// Which backend serves a volume, for the handful of app decisions that are
/// genuinely about the transport: which indexer transport may walk it, whether
/// the file viewer treats its paths as local, whether an SMB upgrade has anything
/// to do.
///
/// ❗ **Backend-side only.** The FRONTEND classifies a pane off `fsType` and
/// category and ❌ never off this: an OS-mounted SMB share that hasn't been
/// upgraded is served by `LocalPosixVolume`, so this would answer `Local` for a
/// share that is plainly SMB to the user
/// (`file-explorer/pane/volume-capabilities.ts` carries the rule).
///
/// The default is [`Local`](Self::Local) so a test double compiles without
/// naming one, which is also the safe way to be wrong: `Local` grants no remote
/// treatment to anything.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendKind {
    /// A real filesystem reached through `std::fs` (also the default).
    Local,
    /// An SMB share over Cmdr's own smb2 session.
    Smb,
    /// An SFTP server.
    Sftp,
    /// A WebDAV server.
    Webdav,
    /// A phone or camera over MTP/PTP.
    Mtp,
    /// An Android device over ADB.
    Adb,
    /// The inside of a zip, tar, or 7z.
    Archive,
    /// One of the virtual `.git` category trees.
    GitPortal,
}

impl BackendKind {
    /// Whether a drive index can be turned on for a volume this backend serves:
    /// the index has a transport that walks it (the local walker for a real
    /// filesystem, an OS-mounted share included, and the `Volume`-trait scanner
    /// for an smb2 session and a phone over MTP or ADB).
    ///
    /// ❗ The one answer both sides read: `Volume::capabilities` publishes it for
    /// the volume switcher's index affordances, and the index's `start_volume`
    /// refuses a registered volume this says no for. A server's scheme root is
    /// nothing the index has a transport for, and an archive or a `.git` portal
    /// is a view inside a drive that is indexed as itself.
    ///
    /// Exhaustive on purpose: a new backend doesn't compile until it answers.
    #[must_use]
    pub fn can_be_indexed(self) -> bool {
        match self {
            Self::Local | Self::Smb | Self::Mtp | Self::Adb => true,
            Self::Sftp | Self::Webdav | Self::Archive | Self::GitPortal => false,
        }
    }
}

/// What a "Sign in" affordance on a volume asks a person for: the FORM the sheet
/// renders, decided by the backend rather than by the sheet's mode or the
/// protocol's name.
///
/// ❗ **Read from the live volume at the moment the affordance renders**, ❌
/// never captured when the volume was opened. A backend that authenticates per
/// connection can prove itself with a different credential each time it dials,
/// so a value stored earlier describes a session that may no longer exist, and it
/// goes wrong in both directions: a stale [`Nothing`](Self::Nothing) leaves a
/// volume that now wants a password with no way in, and a stale
/// [`KeyPassphrase`](Self::KeyPassphrase) asks for a secret the session doesn't
/// use. [`Volume::sign_in_prompt`](super::Volume::sign_in_prompt) is the read.
///
/// ❗ **Whether the username is editable is a property of the VARIANT, not of the
/// sheet's mode.** [`Password`](Self::Password) and
/// [`KeyPassphrase`](Self::KeyPassphrase) render it read-only, because SFTP's and
/// WebDAV's `reconnect_with_credentials` refuse a changed username: the volume id
/// IS the account, and authenticating as somebody else under this volume's name
/// would index another account's files.
/// [`UsernamePassword`](Self::UsernamePassword) renders it editable, because SMB
/// accepts a new username and rewrites its params, which is how re-auth-as-
/// someone-else works. One implementer reading "read-only" as a mode rule would
/// break SMB; one reading "editable" as a mode rule would break SFTP.
///
/// **Reserved, ❌ not added until a producer exists**:
/// - `AccessKeys { session_token: bool }` for S3: an access key id, a secret
///   access key, and optionally a session token.
/// - `Oauth { provider }`: a "Continue in your browser" button and a waiting
///   state, with the callback coming home backend-side; "remember" is implicit
///   there (the refresh token is the only sane state), and a revoked token
///   surfaces as [`ConnectionState::NeedsSignIn`] behind the same banner.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case", rename_all_fields = "camelCase")]
pub enum SignInShape {
    /// ❌ Nothing to ask, so ❌ no sign-in button. The session comes back on its
    /// own (an ssh-agent identity, an unencrypted key file), and there is no
    /// secret a person could type that would help.
    Nothing,
    /// The account's password, under a read-only username.
    ///
    /// ❗ An attended sign-in REFRESHES a remembered secret and ❌ never seeds
    /// one: the store is written only where it already holds a secret for this
    /// account, so a user who declined to remember it stays declined
    /// (`crates/cmdr-sftp/DETAILS.md` § "The two switches").
    Password,
    /// The passphrase on a key file, under a read-only username.
    ///
    /// ❗ Same rule as [`Password`](Self::Password), refresh included: this is
    /// NOT a never-save variant. Encrypting a key asks that the passphrase not be
    /// left lying around, and a user who chose to remember it has already
    /// answered that question themselves.
    KeyPassphrase,
    /// A username AND a password, both editable: SMB, where the SHARE is the
    /// identity and the account is a field on it.
    UsernamePassword {
        /// Whether the sheet offers "Connect as guest" beside the two fields.
        guest_allowed: bool,
    },
}
