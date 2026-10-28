//! The NFS half of the saved servers: a server's saved exports grouped into one
//! hub row, and the saved entry a target describes.

use super::wire::{SavedPlace, SavedServer, ServerNameSource, ServerProtocol, ServerTarget};
use super::{ServerConnectOutcome, outcome_from_nfs};
use crate::network::nfs_known_places::{self, KnownNfsPlace};
use crate::network::nfs_volume_wiring;

/// Every saved NFS place, grouped under its SERVER: one [`SavedServer`] per host
/// plus port, its places the saved exports, each with its own id, pin, and
/// switch.
///
/// ❗ The server's id is its ROOT export's place id
/// (`nfs_known_places::server_id`), which names the server whether or not that
/// export is saved, the way an S3 account's id is its root place's. The server
/// row carries no pin of its own: its places do.
///
/// ❗ The server row carries the NAME (the one a person typed, else the host),
/// and each export reads as its own path (`nfs_known_places::place_label`).
pub(super) fn nfs_servers(
    manager: &crate::file_system::volume::manager::VolumeManager,
    app_roots: &std::collections::HashMap<String, String>,
) -> Vec<SavedServer> {
    let mut servers: Vec<SavedServer> = Vec::new();
    for entry in nfs_known_places::all() {
        let volume_id = entry.volume_id();
        let Some(app_root) = app_roots.get(&volume_id).cloned() else {
            log::warn!(target: "volume", "a saved NFS place has no place in the volume listing; leaving it out");
            continue;
        };
        let server_id = entry.server_id();
        let place = SavedPlace {
            connected: manager.get(&volume_id).is_some(),
            volume_id,
            name: nfs_known_places::place_label(&entry),
            pinned: entry.pinned,
            app_root,
            // ❗ No account: the uid is a setting of the place, ❌ not who it's
            // opened as in the sense a password would mean.
            username: None,
            auto_reconnect: Some(entry.auto_reconnect),
        };
        match servers.iter_mut().find(|server| server.id == server_id) {
            Some(server) => {
                server.places.push(place);
                if server.last_connected_at.as_deref() < Some(entry.last_connected_at.as_str()) {
                    server.last_connected_at = Some(entry.last_connected_at.clone());
                }
            }
            None => servers.push(SavedServer {
                id: server_id,
                protocol: ServerProtocol::Nfs,
                display_name: nfs_known_places::server_label(&entry),
                name_source: ServerNameSource::of_account(&nfs_known_places::server_name(&entry)),
                address: address(&entry),
                username: None,
                pinned: false,
                last_connected_at: Some(entry.last_connected_at.clone()),
                auto_reconnect: Some(entry.auto_reconnect),
                places: vec![place],
                awaiting_first_connect: false,
            }),
        }
    }
    servers
}

/// What the hub shows as an NFS server's address: the host, and the port only
/// when the address named one. An IPv6 literal goes in brackets before a port.
fn address(entry: &KnownNfsPlace) -> String {
    match entry.port {
        None => entry.host.clone(),
        Some(port) if entry.host.contains(':') => format!("[{}]:{port}", entry.host),
        Some(port) => format!("{}:{port}", entry.host),
    }
}

/// The saved entry an NFS target describes, and the SERVER's name it carried,
/// or `None` for a target of another protocol. ❗ `pinned: true` is only ever
/// read for a NEW entry (`nfs_known_places::remember` keeps a stored pin). The
/// name is the server's, so it travels beside the entry, ❌ never in it.
pub(super) fn nfs_place(target: ServerTarget) -> Option<(KnownNfsPlace, String)> {
    let ServerTarget::Nfs {
        display_name,
        host,
        port,
        export,
        uid,
        gid,
        start_folder,
        auto_reconnect,
    } = target
    else {
        return None;
    };
    let place = KnownNfsPlace {
        host: host.trim().to_string(),
        port,
        export: export.trim().to_string(),
        uid,
        gid,
        start_folder: start_folder.filter(|folder| !folder.trim().is_empty()),
        auto_reconnect,
        pinned: true,
        last_connected_at: String::new(),
    };
    Some((place, display_name))
}

/// This process's own uid and gid: the identity the add form falls back to when it can't read
/// who owns an export, and the one an NFS server saved without contact takes.
pub(crate) fn this_mac_identity() -> cmdr_nfs::NfsIdentity {
    // SAFETY: `getuid` and `getgid` take no arguments, can't fail, and touch no
    // memory of ours.
    let (uid, gid) = unsafe { (libc::getuid(), libc::getgid()) };
    cmdr_nfs::NfsIdentity { uid, gid }
}

/// Dials an NFS target the user just typed (`connect_server`'s NFS arm).
pub(crate) async fn connect(target: ServerTarget, attempt_id: &str) -> ServerConnectOutcome {
    let Some((place, server_name)) = nfs_place(target) else {
        return ServerConnectOutcome::Unreachable;
    };
    outcome_from_nfs(nfs_volume_wiring::connect_and_register(&server_name, place, attempt_id).await)
}
