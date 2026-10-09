//! Why a per-drive "Turn on indexing" didn't start the drive's index.

/// Why a drive couldn't be indexed, whatever its transport. Typed (and
/// serialized as a snake_case tag) so callers and the per-drive UX classify by
/// variant on BOTH sides of the IPC boundary, never by message substring.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum DriveIndexRefusal {
    /// Nothing is registered under this id right now: the drive is unplugged or
    /// unmounted, a share is offline, or a phone hasn't been dialed (its USB
    /// debugging prompt may still be waiting for a tap). Connecting it is the fix.
    NotConnected,
    /// No drive index can serve this volume: its backend has no index
    /// transport (SFTP, WebDAV, S3), or it's a mount the SMB gate can't upgrade
    /// because it isn't an SMB share.
    NotIndexable,
    /// The share is OS-mounted but the upgrade to a direct smb2 session failed
    /// (network unreachable, server refused). Indexing stays disabled.
    UpgradeFailed,
    /// The upgrade needs credentials Cmdr doesn't have cached. The user must
    /// sign in (the FE reconnect/credentials flow) before indexing can start.
    CredentialsNeeded,
    /// The share is registered but its smb2 session is `Disconnected`.
    /// Reconnect first.
    Disconnected,
    /// The master drive-indexing switch is off, so no drive may index. Nothing is
    /// wrong with the drive; the user turned indexing off in settings.
    IndexingDisabled,
}

impl std::fmt::Display for DriveIndexRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Diagnostic / log text only. Classification is by variant, never by
        // parsing this string.
        let s = match self {
            Self::NotConnected => "the drive isn't connected",
            Self::NotIndexable => "no drive index can serve this volume",
            Self::UpgradeFailed => "upgrade to a direct smb2 connection failed",
            Self::CredentialsNeeded => "a direct smb2 connection needs credentials",
            Self::Disconnected => "the smb2 session is disconnected",
            Self::IndexingDisabled => "drive indexing is off in settings",
        };
        f.write_str(s)
    }
}
