//! The fields a saved SFTP or WebDAV server carries beside its identity, and the
//! rules both stores share for them.
//!
//! ❗ **A start folder sits at or under the remote root, compared by whole path
//! components.** The root is a CEILING nothing navigates above
//! (`RemoteRoot::to_remote_path` refuses), so a start folder outside it names a
//! place no pane could stand on. ❌ Never a string prefix: `/srv/data-1` is not
//! under `/srv/data`, however much of it it spells.
//!
//! ❗ **An unnamed server is called `username@host`, derived in ONE place**
//! ([`server_label`]), for SFTP and WebDAV alike. An empty stored name is what
//! "unnamed" means, so a store keeps only a name a person typed, and a name that
//! only repeats the server's own address is cleared on load
//! ([`spells_own_address`]).

use std::path::Path;

use cmdr_fs::volume::remote_paths::normalize_remote_path;
use serde::{Deserialize, Serialize};

/// A start folder that isn't its server's remote root or under it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StartFolderOutsideRoot;

/// What saving a server's fields without dialing produced.
///
/// ❗ A refusal writes NOTHING: a half-saved edit is a server that dials one way
/// and lists another.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum SavedServerOutcome {
    /// The store holds the edit.
    Saved,
    /// The start folder isn't the root or under it.
    StartFolderOutsideRoot,
    /// The place is connected and its new root isn't a folder this account can
    /// open on the server: missing, a file, or refused.
    RootNotFound,
    /// The place is connected and its start folder isn't a folder this account
    /// can open on the server: missing, a file, or refused.
    StartFolderNotFound,
    /// The place is connected, the edit needs the server to confirm a folder,
    /// and the session didn't answer in time (it dropped, or the server is slow).
    /// Nothing was checked, so nothing was saved.
    Unreachable,
}

/// The start folder as a store keeps it, or a refusal when it sits outside
/// `remote_root`.
///
/// Both paths go through `normalize_remote_path`, the spelling `RemoteRoot`
/// gives a root (`.` and `..` resolved, a relative path read from `/`), before
/// they're compared. An empty start folder and the root itself both mean "the
/// root" and come back as `None`, so one landing has one spelling in the store.
pub fn start_folder_under_root(
    remote_root: &str,
    start_folder: Option<&str>,
) -> Result<Option<String>, StartFolderOutsideRoot> {
    let Some(start_folder) = start_folder.filter(|folder| !folder.trim().is_empty()) else {
        return Ok(None);
    };
    let root = normalize_remote_path(Path::new(remote_root));
    let folder = normalize_remote_path(Path::new(start_folder));
    if folder == root {
        return Ok(None);
    }
    // `Path::starts_with` compares whole components, which is the whole point.
    if folder.starts_with(&root) {
        Ok(Some(folder.to_string_lossy().into_owned()))
    } else {
        Err(StartFolderOutsideRoot)
    }
}

/// The start folder a connect saves beside `remote_root`: the one it was handed
/// while that root still holds it, else the root (`None`).
///
/// ❗ The connect path's rule, where a caller carries a SAVED start folder across
/// a root that may have changed under it. An edit or an add refuses instead
/// ([`start_folder_under_root`]), because there a person just typed it.
pub fn start_folder_for_root(remote_root: &str, start_folder: Option<String>) -> Option<String> {
    start_folder_under_root(remote_root, start_folder.as_deref()).unwrap_or_else(|StartFolderOutsideRoot| {
        log::info!(target: "volume", "a saved start folder sits outside the root this connect dials; the place lands at its root");
        None
    })
}

/// Whether a person named the server: a stored name that isn't blank.
pub fn is_named(name: &str) -> bool {
    !name.trim().is_empty()
}

/// What the UI calls a saved SFTP or WebDAV server: its name when a person gave
/// it one, else `username@host` (the host alone for an account with no
/// username).
///
/// ❗ Every read of a name goes through here, by way of the stores' `label()`:
/// the volume listing, the hub's rows, and the name a live volume is built with.
/// No port, scheme, or path, because a label that looked exactly like the address
/// is what sent a person to edit the wrong field.
pub fn server_label(name: &str, username: &str, host: &str) -> String {
    if is_named(name) {
        name.to_string()
    } else if username.is_empty() {
        host.to_string()
    } else {
        format!("{username}@{host}")
    }
}

/// The account an address has to name to count as a saved server's own.
pub struct OwnAddress<'a> {
    /// Compared exactly: an account doesn't fold case.
    pub username: &'a str,
    /// Compared folding ASCII case, with any IPv6 brackets set aside.
    pub host: &'a str,
    /// The effective port.
    pub port: u16,
    /// The schemes this protocol's addresses open with, each with the port it
    /// implies when the address names none.
    pub schemes: &'a [(&'a str, u16)],
}

/// Whether `name` only spells this account's own address:
/// `[scheme://][username@]host[:port][/path]`.
///
/// ❗ Conservative, because a match ERASES a name: whatever isn't provably this
/// server's own address is a label someone chose, and stays. So a scheme-less
/// name has to name the account (a bare `nas.local` could be a chosen name),
/// another account or port is another server, and a query or fragment is no
/// address this app stores. An address with a scheme and no port means the
/// scheme's own port. A scheme-less `username@host` matches any port: the derived
/// label drops the port too, so clearing it can't change what anyone sees.
pub fn spells_own_address(name: &str, own: &OwnAddress<'_>) -> bool {
    let name = name.trim();
    let (rest, scheme_port) = match name.split_once("://") {
        Some((scheme, rest)) => match own.schemes.iter().find(|(known, _)| known.eq_ignore_ascii_case(scheme)) {
            Some(&(_, port)) => (rest, Some(port)),
            None => return false,
        },
        None => (name, None),
    };
    if rest.contains(['?', '#']) {
        return false;
    }
    // From the first `/` on it's a path, and any path is still this server.
    let authority = rest.split_once('/').map_or(rest, |(authority, _)| authority);
    let host_port = match authority
        .strip_prefix(own.username)
        .and_then(|after| after.strip_prefix('@'))
    {
        Some(host_port) => host_port,
        // Only an address with a scheme may leave the account out, and then it
        // mustn't name a different one.
        None if scheme_port.is_some() && !authority.contains('@') => authority,
        None => return false,
    };
    let Some((host, port)) = split_host_port(host_port) else {
        return false;
    };
    if host.is_empty() || !without_brackets(host).eq_ignore_ascii_case(without_brackets(own.host)) {
        return false;
    }
    // An explicit port wins, then the scheme's own; a scheme-less spelling with
    // neither matches any port.
    port.or(scheme_port).is_none_or(|port| port == own.port)
}

/// `host[:port]`, with an IPv6 literal in brackets. `None` when the port isn't a
/// port or a bracket doesn't close.
fn split_host_port(authority: &str) -> Option<(&str, Option<u16>)> {
    if let Some(bracketed) = authority.strip_prefix('[') {
        let (host, after) = bracketed.split_once(']')?;
        if after.is_empty() {
            return Some((host, None));
        }
        return Some((host, Some(after.strip_prefix(':')?.parse().ok()?)));
    }
    match authority.rsplit_once(':') {
        // More than one colon unbracketed is an IPv6 literal with no port.
        Some((host, port)) if !host.contains(':') => Some((host, Some(port.parse().ok()?))),
        _ => Some((authority, None)),
    }
}

/// A host with any IPv6 brackets set aside, so `[fe80::1]` and `fe80::1` compare
/// equal.
fn without_brackets(host: &str) -> &str {
    host.strip_prefix('[')
        .and_then(|inner| inner.strip_suffix(']'))
        .unwrap_or(host)
}

#[cfg(test)]
#[path = "saved_server_fields_test.rs"]
mod saved_server_fields_test;
