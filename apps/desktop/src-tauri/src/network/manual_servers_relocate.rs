//! An SMB host moving to a new address, as the manual store records it
//! (`server_move::smb`). A child of `manual_servers` so it shares that store's lock and
//! file helpers.

use std::path::Path;

use tauri::{AppHandle, Runtime};

use super::{
    HostEdit, ManualServerEntry, create_network_host, generate_server_id, get_store_lock, get_store_path,
    read_store_from_path, typed_account, write_store_to_path,
};
use crate::network::server_identity::SmbServer;
use crate::network::{on_host_found, on_host_lost};

/// Moves the host `old_id` to `to` in ONE write under `STORE_LOCK`, with the name and
/// account the edit set: the entry at the old id is replaced in place (its `added_at`
/// kept), or a new one joins the list when only the share history knew the host, since
/// moving a host is a request to keep it. Answers the entry as stored.
///
/// ❗ An entry another host already holds at the new id is `Err` with what the UI calls
/// that host, and nothing is written: merging would keep one entry's name and account and
/// silently drop the other's.
pub(super) fn relocate_at_path(
    path: &Path,
    old_id: &str,
    to: &SmbServer,
    edit: &HostEdit,
) -> Result<ManualServerEntry, String> {
    let _guard = get_store_lock().lock().unwrap_or_else(|e| e.into_inner());
    let mut store = read_store_from_path(path);
    let new_id = generate_server_id(to.host(), to.port());
    if let Some(holder) = store.servers.iter().find(|s| s.id == new_id && s.id != old_id) {
        return Err(holder.label());
    }
    let existing = store.servers.iter().position(|s| s.id == old_id);
    let entry = ManualServerEntry {
        id: new_id,
        display_name: edit.name.trim().to_string(),
        address: to.host().to_string(),
        port: to.port(),
        added_at: existing.map_or_else(
            || chrono::Utc::now().to_rfc3339(),
            |at| store.servers[at].added_at.clone(),
        ),
        username: typed_account(edit.username.as_deref()),
    };
    match existing {
        Some(at) => store.servers[at] = entry.clone(),
        None => store.servers.push(entry.clone()),
    }
    write_store_to_path(path, &store);
    Ok(entry)
}

/// Moves the host `old_id` to `to` ([`relocate_at_path`]), and swaps it in the discovery
/// list: the old address leaves, the new one joins the way a typed host does. `Ok(false)`
/// when the store has no file to write; `Err` names the host already at the new address.
pub fn relocate_manual_server<R: Runtime>(
    old_id: &str,
    to: &SmbServer,
    edit: &HostEdit,
    app_handle: &AppHandle<R>,
) -> Result<bool, String> {
    let Some(path) = get_store_path(app_handle) else {
        return Ok(false);
    };
    let entry = relocate_at_path(&path, old_id, to, edit)?;
    on_host_lost(old_id, app_handle);
    on_host_found(create_network_host(&entry.address, entry.port), app_handle);
    Ok(true)
}

#[cfg(test)]
#[path = "manual_servers_relocate_test.rs"]
mod tests;
