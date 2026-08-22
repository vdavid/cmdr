//! What one server said it can do, read once from the SFTP hello.
//!
//! SFTP v3 is a small protocol and every interesting capability is an
//! `@openssh.com` extension a server either advertises or doesn't. The engine
//! answers each one as a `support_*` predicate on a live session; this is that
//! answer set as a plain value, taken once at dial.
//!
//! ❗ **One value, read once.** The set is fixed for the life of a session (it
//! arrives in the `SSH_FXP_VERSION` hello), so re-asking per operation buys
//! nothing, and a plain value is what lets a fallback path be tested without
//! standing up a server that lacks the extension.
//!
//! ⚠️ **The predicates are all that's readable.** `max_read_len` and
//! `max_write_len` sit behind the engine's `__ci-tests` feature, and
//! `statvfs@openssh.com` has no predicate at all — nor a request to send it, so
//! free space is honestly unavailable (`DETAILS.md` § "The `Volume` answers").

use openssh_sftp_client::Sftp;

/// The five extensions this crate can ask a session about.
///
/// Two of them gate behaviour and three are a record: `DETAILS.md` § "What the
/// server said it can do" says what each would buy and why the unspent ones stay
/// unspent. ❌ Don't add a field for an extension the engine has no predicate
/// for — there is no way to answer it short of vendoring the protocol crate.
// DEFAULT-OK: the zero value is a server that advertised nothing, which is the
// truth about a v3 server with no extensions at all.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct ServerExtensions {
    /// `posix-rename@openssh.com`: renaming REPLACES the destination, atomically.
    ///
    /// ❗ A win on a forced rename and a hazard on a forceless one, which is why
    /// the two renames are written separately (`DETAILS.md` § "Renaming without
    /// clobbering").
    pub posix_rename: bool,
    /// `copy-data@openssh.com`: the server copies a byte range from one open
    /// handle to another, so a copy inside one server never crosses the wire.
    pub copy_data: bool,
    /// `fsync@openssh.com`: flush an open handle to stable storage.
    pub fsync: bool,
    /// `hardlink@openssh.com`: create a hard link.
    pub hardlink: bool,
    /// `expand-path@openssh.com`: resolve `~` and relative paths server-side.
    pub expand_path: bool,
}

impl ServerExtensions {
    /// Reads the set off a live session.
    pub fn probe(sftp: &Sftp) -> Self {
        Self {
            posix_rename: sftp.support_posix_rename(),
            copy_data: sftp.support_copy(),
            fsync: sftp.support_fsync(),
            hardlink: sftp.support_hardlink(),
            expand_path: sftp.support_expand_path(),
        }
    }

    /// A server that advertised nothing, for the fallback paths' unit cells.
    #[cfg(any(test, feature = "testing"))]
    pub fn none() -> Self {
        Self::default()
    }

    /// The names the server advertised, for one log line at connect.
    ///
    /// ❗ PII-free by construction: extension names are protocol constants, and
    /// nothing about the host, the account, or a path can reach this.
    pub(crate) fn advertised(&self) -> Vec<&'static str> {
        let mut names = Vec::with_capacity(5);
        for (present, name) in [
            (self.posix_rename, "posix-rename"),
            (self.copy_data, "copy-data"),
            (self.fsync, "fsync"),
            (self.hardlink, "hardlink"),
            (self.expand_path, "expand-path"),
        ] {
            if present {
                names.push(name);
            }
        }
        names
    }
}
