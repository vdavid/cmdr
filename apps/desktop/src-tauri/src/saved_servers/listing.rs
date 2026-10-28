//! Every server the user has saved, across every store, as the hub lists them.

use super::nfs_places::nfs_servers;
use super::s3_accounts::s3_accounts;
use super::smb_hosts::smb_hosts;
use super::webdav_params;
use super::wire::{SavedPlace, SavedServer, ServerNameSource, ServerProtocol};
use crate::network::{known_shares, manual_servers, sftp_known_servers, webdav_known_servers};

/// The listing, over a manual-server list the caller supplies.
///
/// ❗ Cached state only, ❌ never the wire: the hub re-reads this on every
/// `volumes-changed`, and a probe here would turn a refresh into a round of
/// network traffic.
///
/// The manual SMB store reads a FILE through an `AppHandle`, and the union itself
/// is a pure fold that a cell can drive: the share store and the discovery list
/// come in as arguments too, so a cell never races another over the
/// process-global copies.
pub(crate) fn listing(
    manual: Vec<manual_servers::ManualServerEntry>,
    known: Vec<known_shares::KnownNetworkShare>,
    hosts: &[crate::network::NetworkHost],
) -> Vec<SavedServer> {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    // The app roots, from the module that mints them for the volume listing, so
    // the hub's rows and the switcher's rows spell one prefix.
    let app_roots: std::collections::HashMap<String, String> = crate::server_volumes::server_places()
        .into_iter()
        .map(|place| (place.id, place.app_root))
        .collect();
    let mut servers = Vec::new();

    for entry in sftp_known_servers::all() {
        let volume_id = cmdr_fs::volume::sftp_volume_id(&entry.host, entry.port, &entry.username);
        let Some(app_root) = app_roots.get(&volume_id).cloned() else {
            log::warn!(target: "volume", "a saved SFTP server has no place in the volume listing; leaving it out");
            continue;
        };
        let label = entry.label();
        servers.push(SavedServer {
            places: vec![SavedPlace {
                connected: manager.get(&volume_id).is_some(),
                volume_id: volume_id.clone(),
                name: label.clone(),
                pinned: entry.pinned,
                app_root,
                username: Some(entry.username.clone()),
                auto_reconnect: Some(entry.auto_reconnect),
            }],
            id: volume_id,
            protocol: ServerProtocol::Sftp,
            display_name: label,
            name_source: ServerNameSource::of_account(&entry.display_name),
            address: format!("{}:{}", entry.host, entry.port),
            username: Some(entry.username),
            pinned: entry.pinned,
            last_connected_at: Some(entry.last_connected_at),
            auto_reconnect: Some(entry.auto_reconnect),
            awaiting_first_connect: false,
        });
    }

    for entry in webdav_known_servers::all() {
        let Some(params) = webdav_params(&entry.url, &entry.username, "/") else {
            log::warn!(target: "volume", "a saved WebDAV server's address isn't a URL any more; leaving it out of the listing");
            continue;
        };
        let volume_id = cmdr_fs::volume::webdav_volume_id(params.host(), params.port(), &entry.username);
        let Some(app_root) = app_roots.get(&volume_id).cloned() else {
            log::warn!(target: "volume", "a saved WebDAV server has no place in the volume listing; leaving it out");
            continue;
        };
        let label = entry.label();
        servers.push(SavedServer {
            places: vec![SavedPlace {
                connected: manager.get(&volume_id).is_some(),
                volume_id: volume_id.clone(),
                name: label.clone(),
                pinned: entry.pinned,
                app_root,
                username: Some(entry.username.clone()),
                auto_reconnect: Some(entry.auto_reconnect),
            }],
            id: volume_id,
            protocol: ServerProtocol::Webdav,
            display_name: label,
            name_source: ServerNameSource::of_account(&entry.display_name),
            address: entry.url,
            username: Some(entry.username),
            pinned: entry.pinned,
            last_connected_at: Some(entry.last_connected_at),
            auto_reconnect: Some(entry.auto_reconnect),
            awaiting_first_connect: false,
        });
    }

    servers.extend(s3_accounts(manager, &app_roots));
    servers.extend(nfs_servers(manager, &app_roots));
    servers.extend(smb_hosts(manual, known, hosts));
    servers
}

/// Every saved server as the hub lists it, plus each SMB host with the store rows
/// behind it (its address and port live there, not on the row), read from the
/// stores as they are now. For a caller that CHECKS against what's saved (Ask
/// Cmdr's server proposals) rather than showing it.
///
/// ❗ Cached state only, like [`listing`].
pub(crate) fn saved_now<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
) -> (Vec<SavedServer>, Vec<super::smb_hosts::SmbHostGroup>) {
    let manual = manual_servers::all(app);
    let known = known_shares::get_all_known_shares();
    let hosts = crate::network::fresh_discovered_hosts();
    let smb = super::smb_hosts::smb_host_groups(manual.clone(), known.clone(), &hosts);
    (listing(manual, known, &hosts), smb)
}

#[cfg(test)]
#[path = "listing_test.rs"]
mod listing_test;
