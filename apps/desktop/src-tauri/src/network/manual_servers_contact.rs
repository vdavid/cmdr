//! A manual server saved without contacting it (an approved Ask Cmdr suggestion, or
//! the add sheet's "Add anyway"),
//! and the first connect that makes it an ordinary one. A child of `manual_servers`
//! so it shares that store's lock and file helpers.

use std::path::Path;

use log::info;
use tauri::{AppHandle, Runtime};

use super::{
    AddServerError, Contact, HostEdit, MANUAL_SERVERS_FILENAME, ManualConnectResult, ManualServerEntry, get_store_lock,
    get_store_path, parse_server_address, read_store_from_path, save_parsed_at, write_store_to_path,
};
use crate::network::NetworkHost;
use crate::network::server_identity::SmbServer;

/// Saves a manual server, and the share its address names, with NO network contact:
/// no reachability check, no discovery injection (which would let the servers view
/// list its shares), and no GUID probe (`smb_server_guid::rows_to_probe` skips a
/// share no mount went through). The host joins the discovery list once the user
/// first connects it ([`note_first_connect`]).
///
/// ❗ The save behind an approved Ask Cmdr suggestion. The add sheet's "Add anyway"
/// (`add_manual_server` with `Reachability::Skip`) saves the same way: saving a
/// server is a store write, and contact starts when the user connects (#777).
pub fn save_manual_server_without_contact<R: Runtime>(
    input: &str,
    details: &HostEdit,
    app_handle: &AppHandle<R>,
) -> Result<ManualConnectResult, AddServerError> {
    let parsed = parse_server_address(input).map_err(|e| AddServerError::InvalidAddress { message: e.to_string() })?;
    Ok(save_parsed_at(
        get_store_path(app_handle).as_deref(),
        parsed,
        details,
        Contact::AwaitingFirstConnect,
    ))
}

/// A mount of `server` just went through: a host saved without contact that it names
/// becomes an ordinary one, which the next load puts in the discovery list
/// (`load_manual_servers`, on the next network action such as opening the servers
/// view). Answers whether one was waiting.
///
/// ❗ Reads the store through the standalone data dir: it runs from the mount's
/// bookkeeping (`smb_saved_shares::remember_mount`), which holds no `AppHandle`.
pub fn note_first_connect(server: &SmbServer) -> bool {
    let Some(path) = crate::config::standalone_app_data_dir().map(|dir| dir.join(MANUAL_SERVERS_FILENAME)) else {
        return false;
    };
    note_first_connect_at_path(&path, server, &crate::network::fresh_discovered_hosts())
}

/// [`note_first_connect`] on the store at `path`. The lock is taken only when an
/// entry is waiting, so an ordinary mount reads the file and writes nothing.
pub(super) fn note_first_connect_at_path(path: &Path, server: &SmbServer, hosts: &[NetworkHost]) -> bool {
    let waits = |entry: &ManualServerEntry| {
        entry.awaiting_first_connect && SmbServer::new(&entry.address, entry.port).is(server, hosts)
    };
    if !read_store_from_path(path).servers.iter().any(waits) {
        return false;
    }
    let _guard = get_store_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut store = read_store_from_path(path);
    let mut woke = false;
    for entry in store.servers.iter_mut().filter(|entry| waits(entry)) {
        entry.awaiting_first_connect = false;
        woke = true;
    }
    if woke {
        write_store_to_path(path, &store);
        info!("A manual server saved without contact was connected; it joins the discovery list");
    }
    woke
}

/// The entries that belong in the discovery list: every one but a host saved without
/// contact that no mount went through yet. Pure.
pub(super) fn discoverable(entries: &[ManualServerEntry]) -> impl Iterator<Item = &ManualServerEntry> {
    entries.iter().filter(|entry| !entry.awaiting_first_connect)
}

#[cfg(test)]
#[path = "manual_servers_contact_test.rs"]
mod tests;
