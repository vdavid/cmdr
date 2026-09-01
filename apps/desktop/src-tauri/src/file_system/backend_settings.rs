//! The app's answer to "what has the user tuned for this backend?".
//!
//! A storage backend reads its live knobs through
//! `cmdr_fs::volume::host::settings::BackendSettings` rather than a settings
//! file, an env var, or a preferences store. This resolves them from what
//! `settings::load_settings` already pushed into `file_system`.
//!
//! Live by design: the value is read per batch dispatch, so moving the slider
//! takes effect on the next batch without remounting.
//!
//! Each knob resolves through a table keyed by the backend's settings namespace,
//! so a backend reads what the user tuned FOR IT and nothing else. Wiring a new
//! backend up is adding a row; leaving one out is a conservative default, never
//! another backend's number.

use cmdr_fs::volume::host::settings::{BackendName, BackendSettings};

/// Answers a backend's live knobs from the app's stored settings.
pub struct AppBackendSettings;

/// One row of the table below: a backend's settings namespace, and the accessor
/// that reads its live value.
type ConcurrencySource = (BackendName, fn() -> usize);

/// Where each backend's concurrency setting comes from, keyed by its namespace.
///
/// ❗ **A row is where a backend's number comes from, not a promise that a user
/// can change it.** Two rows today, and only one of them is a slider:
///
/// - `"smb"` reads `network.smbConcurrency` (Settings > Advanced, default 10,
///   clamped to `1..=32` by `set_smb_concurrency`), whose label and help text
///   both say SMB. ❌ No other backend may be pointed at it: the right number is
///   a property of the server, and an FTP server that allows four connections
///   answers the fifth with a refusal or a temporary ban, so a NAS tuned to 32
///   would break it.
/// - `"sftp"` reads a constant. Four Cmdr operations share ONE SSH connection
///   there — a second connection means a second authentication, and with
///   keyboard-interactive a second 2FA prompt — so the number is a property of
///   the design rather than something to tune. It still needs a row, because a
///   namespace without one silently gets
///   [`UNREGISTERED_MAX_CONCURRENT_OPERATIONS`].
/// - `"webdav"` reads a constant too: one in-flight request per HTTP/1.1
///   connection, and a server's per-host connection limit is what an extra one
///   would run into.
///
/// Adding a backend means adding its row here next to its own accessor, whether
/// or not the number is ever exposed.
const MAX_CONCURRENT_OPERATIONS_SOURCES: &[ConcurrencySource] = &[
    ("smb", super::smb_concurrency),
    ("sftp", sftp_concurrency),
    ("webdav", webdav_concurrency),
];

/// How many operations an SFTP volume runs at once.
///
/// David's call, 2026-08-22, and not a user-facing knob. Four operations share
/// one SSH connection and one SFTP channel, which keeps us clear of a server's
/// `MaxSessions` and `MaxStartups` and avoids a second authentication. The
/// per-stream request window is a separate lever and lives in the backend.
const SFTP_MAX_CONCURRENT_OPERATIONS: usize = 4;

fn sftp_concurrency() -> usize {
    SFTP_MAX_CONCURRENT_OPERATIONS
}

/// How many operations a WebDAV volume runs at once.
///
/// Not a user-facing knob. Each operation holds one HTTP/1.1 connection to the
/// host while its request is in flight, and four is what keeps both the server
/// (a per-client connection cap is common on a NAS) and the `reqwest` pool
/// happy: enough to overlap a listing with a transfer, never enough to look like
/// a flood.
const WEBDAV_MAX_CONCURRENT_OPERATIONS: usize = 4;

fn webdav_concurrency() -> usize {
    WEBDAV_MAX_CONCURRENT_OPERATIONS
}

/// What a backend with no row above gets.
///
/// Timid on purpose, because the day someone adds a backend and forgets its row
/// is the day this number ships. Two is under the 2–4 simultaneous-connection
/// cap that FTP servers commonly enforce, and still beats a strictly serial
/// dispatch; the failure mode of forgetting a row is "slower than it could be",
/// never "the server stopped answering". A backend that can sustain more says so
/// by earning a row, not by inheriting one.
const UNREGISTERED_MAX_CONCURRENT_OPERATIONS: usize = 2;

const _: () = assert!(
    UNREGISTERED_MAX_CONCURRENT_OPERATIONS <= 4,
    "the fallback has to stay under the connection cap FTP servers commonly enforce, or forgetting a row turns into a \
     ban rather than a slowdown"
);

impl BackendSettings for AppBackendSettings {
    fn max_concurrent_operations(&self, backend: BackendName) -> usize {
        // A lookup keyed by the namespace, never a `match` on it: the seam's
        // contract is that the namespace names a settings bucket rather than
        // classifying the backend.
        MAX_CONCURRENT_OPERATIONS_SOURCES
            .iter()
            .find(|(namespace, _)| *namespace == backend)
            .map_or(UNREGISTERED_MAX_CONCURRENT_OPERATIONS, |(_, read)| read())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Mutex, MutexGuard};

    use super::*;
    use crate::file_system::{set_smb_concurrency, smb_concurrency};

    /// The concurrency setting is one process-global atomic, so two tests that
    /// both write it and then read it back have to take turns. Without this they
    /// see each other's values and fail on whichever ran second.
    static SETTING: Mutex<()> = Mutex::new(());

    fn one_writer_at_a_time() -> MutexGuard<'static, ()> {
        SETTING.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
    }

    /// The seam has to see what the user last set, not a value captured when the
    /// volume mounted. The clamp is the app's, and it applies through the seam
    /// too.
    #[test]
    fn smb_reads_the_live_setting_and_its_clamp() {
        let _turn = one_writer_at_a_time();
        let previous = smb_concurrency();

        set_smb_concurrency(7);
        assert_eq!(AppBackendSettings.max_concurrent_operations("smb"), 7);

        set_smb_concurrency(9_000);
        assert_eq!(
            AppBackendSettings.max_concurrent_operations("smb"),
            32,
            "a misconfigured settings file can't overwhelm a server through the seam either"
        );

        set_smb_concurrency(previous);
    }

    /// SFTP's row is a constant rather than a setting, and it still has to be a
    /// row: a namespace without one silently gets the cautious 2, which would
    /// halve every SFTP transfer for no reason anyone could see.
    #[test]
    fn sftp_reads_its_own_constant_and_not_the_smb_slider() {
        let _turn = one_writer_at_a_time();
        let previous = smb_concurrency();

        set_smb_concurrency(32);
        assert_eq!(
            AppBackendSettings.max_concurrent_operations("sftp"),
            SFTP_MAX_CONCURRENT_OPERATIONS
        );
        assert_ne!(
            AppBackendSettings.max_concurrent_operations("sftp"),
            UNREGISTERED_MAX_CONCURRENT_OPERATIONS,
            "a missing row is invisible at runtime, so this is what says the row is there"
        );

        set_smb_concurrency(previous);
    }

    /// WebDAV's row is a constant as well, and the same argument applies: a
    /// missing row would silently halve every transfer.
    #[test]
    fn webdav_reads_its_own_constant_and_not_the_smb_slider() {
        let _turn = one_writer_at_a_time();
        let previous = smb_concurrency();

        set_smb_concurrency(32);
        assert_eq!(
            AppBackendSettings.max_concurrent_operations("webdav"),
            WEBDAV_MAX_CONCURRENT_OPERATIONS
        );
        assert_ne!(
            AppBackendSettings.max_concurrent_operations("webdav"),
            UNREGISTERED_MAX_CONCURRENT_OPERATIONS,
            "a missing row is invisible at runtime, so this is what says the row is there"
        );

        set_smb_concurrency(previous);
    }

    /// The slider is SMB's, so a namespace with no row must not inherit it. A
    /// NAS tuned to 32 would be a temp-ban on an FTP server that allows four
    /// connections.
    #[test]
    fn a_namespace_with_no_row_gets_the_cautious_default_not_the_smb_slider() {
        let _turn = one_writer_at_a_time();
        let previous = smb_concurrency();

        set_smb_concurrency(32);
        assert_eq!(
            AppBackendSettings.max_concurrent_operations("ftp"),
            UNREGISTERED_MAX_CONCURRENT_OPERATIONS,
            "a backend nobody wired a setting to reads the cautious default, never SMB's slider"
        );

        set_smb_concurrency(previous);
    }
}
