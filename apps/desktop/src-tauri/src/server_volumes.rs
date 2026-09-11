//! The SFTP and WebDAV servers, as rows in the volume list.
//!
//! The server twin of `device_volumes`: that module turns attached devices into
//! volumes, this one turns SAVED and LIVE servers into them. A server is not
//! plugged in, so nothing else reminds the user it exists; a place that is
//! pinned shows in the switcher greyed until it is opened, and opening it dials.
//!
//! ❗ **Cached state only, never the wire.** The volume list is rebuilt on every
//! `volumes-changed`, so a probe here would turn a refresh into a round of
//! network traffic. The two saved-server stores and the volume registry are all
//! this reads.
//!
//! ❗ **The list holds every place; the SWITCHER holds the pinned ones.** A
//! volume id with no row is one the app denies exists, and a hub Enter, a
//! restored tab, and a favorite all reach for a row by id. So `pinned` rides on
//! the row and `navigation/volume-grouping.ts` applies the user's cap.
//!
//! ❗ **A `saved` row and the volume it becomes share one id**
//! (`cmdr_fs::volume::sftp_volume_id` / `webdav_volume_id`, the same ids the
//! registry uses), which is what makes a tab on a server restorable: a tab holds
//! `(volumeId, path)`, the switcher shows the same id greyed, and activating
//! either dials the same saved entry.

use cmdr_fs::volume::remote_paths::RemoteRoot;
use cmdr_fs::volume::{BackendKind, ConnectionState};

use crate::network::{saved_server_fields, sftp_known_servers, webdav_known_servers};
use crate::volume_listing::{LocationCategory, LocationInfo};

/// One SFTP or WebDAV place, from the stores and the registry.
#[derive(Debug, Clone)]
pub(crate) struct ServerPlace {
    /// The volume id, from the same funnel the registry keys on.
    pub id: String,
    /// What the UI calls it: the stored name, or `username@host` for a server
    /// nobody named (the stores' `label()`).
    pub name: String,
    /// The app-facing root: `<prefix><remote root>`
    /// (`cmdr_fs::volume::remote_paths`).
    pub app_root: String,
    /// Where opening the place lands when that isn't `app_root`: the saved start
    /// folder as an app path. `None` when there is none, when the root no longer
    /// holds it, and for a live volume nothing saved.
    pub landing_path: Option<String>,
    /// `"sftp"` or `"webdav"`. ❗ Load-bearing beyond display: the MCP volumes
    /// resource keys `kind` on it, and the frontend's `volumeKindFor` reads it
    /// AHEAD of the `category === 'network'` arm, so a row without it is an SMB
    /// pane.
    pub fs_type: &'static str,
    /// Whether this place belongs in the volume switcher. ❗ The user's own cap
    /// on how many saved things crowd their disks, so the listing honors it and
    /// the resolver ignores it.
    pub pinned: bool,
    /// How live the session is: `Direct` for a registered volume, `Saved` for a
    /// server that is only remembered.
    pub state: ConnectionState,
}

/// Every SFTP and WebDAV place the app knows: one per saved server, marked
/// `Direct` when a volume under its id is registered right now, plus one per
/// registered volume nothing has saved.
///
/// ❗ Includes UNPINNED servers. Pins govern the switcher, not identity, and a
/// tab restored onto an unpinned server still has to find its way home. The
/// listing filters; the resolver does not.
///
/// ❗ The "registered volume nothing has saved" sweep covers a REAL window, ❌
/// not a hypothetical one: `forget_server` removes the store entry first, emits
/// `VolumeUnmounted`, and only THEN disconnects, so between the removal and the
/// disconnect a live session has no saved row. ❌ Don't delete the sweep on the
/// reading that the order makes it unnecessary — it doesn't, and what a volume
/// with no row costs is a pane standing on one the switcher denies exists. The
/// cost of keeping it is one pass over a registry snapshot.
pub(crate) fn server_places() -> Vec<ServerPlace> {
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let registered = |id: &str, kind: BackendKind| {
        manager
            .get(id)
            .filter(|volume| volume.backend_kind() == kind)
            .map(|volume| volume.connection_state().unwrap_or(ConnectionState::Direct))
    };

    let mut places = Vec::new();
    for server in sftp_known_servers::all() {
        let id = cmdr_fs::volume::sftp_volume_id(&server.host, server.port, &server.username);
        let root = RemoteRoot::new(
            cmdr_fs::volume::sftp_app_root(&server.host, server.port, &server.username),
            std::path::Path::new(&server.remote_root),
        );
        places.push(ServerPlace {
            state: registered(&id, BackendKind::Sftp).unwrap_or(ConnectionState::Saved),
            id,
            name: server.label(),
            app_root: root.app_root().to_string_lossy().into_owned(),
            landing_path: landing_under(&root, &server.remote_root, server.start_folder.as_deref()),
            fs_type: "sftp",
            pinned: server.pinned,
        });
    }
    for server in webdav_known_servers::all() {
        let Some((host, port)) = server.endpoint() else {
            // A stored URL that no longer parses names no server. ❌ Not a
            // panic and not a guess: the row simply isn't offered.
            log::warn!(target: "volume", "a saved WebDAV server's address isn't a URL any more; skipping its row");
            continue;
        };
        let id = cmdr_fs::volume::webdav_volume_id(&host, port, &server.username);
        let root = RemoteRoot::new(
            cmdr_fs::volume::webdav_app_root(&host, port, &server.username),
            std::path::Path::new(&server.remote_root),
        );
        places.push(ServerPlace {
            state: registered(&id, BackendKind::Webdav).unwrap_or(ConnectionState::Saved),
            id,
            name: server.label(),
            app_root: root.app_root().to_string_lossy().into_owned(),
            landing_path: landing_under(&root, &server.remote_root, server.start_folder.as_deref()),
            fs_type: "webdav",
            pinned: server.pinned,
        });
    }

    // ❗ And every REGISTERED server volume the stores don't know about, so a
    // live session is never a ghost. `forget_server` removes the saved entry
    // before it disconnects, and in that window a volume with no row is one a
    // pane can sit on while the switcher denies it exists.
    for (id, volume) in manager.list_volumes_with_handles() {
        let fs_type = match volume.backend_kind() {
            BackendKind::Sftp => "sftp",
            BackendKind::Webdav => "webdav",
            _ => continue,
        };
        if places.iter().any(|place| place.id == id) {
            continue;
        }
        places.push(ServerPlace {
            id,
            name: volume.name().to_string(),
            app_root: volume.root().to_string_lossy().into_owned(),
            // No saved entry, so no start folder: it lands at its root.
            landing_path: None,
            fs_type,
            // Nothing saved says otherwise, and a live session earns its
            // switcher row through the session rather than through a pin.
            pinned: false,
            state: volume.connection_state().unwrap_or(ConnectionState::Direct),
        });
    }
    places
}

/// A saved start folder as the app path a pane lands on, or `None` for the root.
///
/// ❗ Silent where `saved_server_fields::start_folder_for_root` logs: the list is
/// rebuilt on every `volumes-changed`, so a start folder the root no longer holds
/// would log once per refresh. It lands at the root either way.
fn landing_under(root: &RemoteRoot, remote_root: &str, start_folder: Option<&str>) -> Option<String> {
    saved_server_fields::start_folder_under_root(remote_root, start_folder)
        .ok()
        .flatten()
        .map(|folder| root.to_app_path(&folder).to_string_lossy().into_owned())
}

/// The row a place becomes.
///
/// ❗ `is_ejectable: false` and `supports_trash: false`: a server has nothing to
/// unplug (its control says "Disconnect") and no trash to move a file to.
/// `capabilities: None` because enrichment fills that from the registered
/// `Volume` afterwards, which is why the servers arm folds BEFORE it. `pinned`
/// rides along so the SWITCHER can apply the cap without the list hiding an
/// identity from everything else that resolves one.
pub(crate) fn location_from_place(place: ServerPlace) -> LocationInfo {
    LocationInfo {
        id: place.id,
        name: place.name,
        path: place.app_root,
        category: LocationCategory::Network,
        icon: None,
        is_ejectable: false,
        mount_is_read_only: false,
        is_disk_image: false,
        fs_type: Some(place.fs_type.to_string()),
        supports_trash: false,
        connection_state: Some(place.state),
        pinned: Some(place.pinned),
        landing_path: place.landing_path,
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
    }
}

/// Appends a row for EVERY SFTP and WebDAV place the app knows: registered,
/// saved, pinned, unpinned alike.
///
/// ❗ **Folded BEFORE `enrich_from_volume_registry`**, which copies `capabilities`
/// and the connection state FROM the registered volume. Anything appended after
/// it ships `capabilities: None`, and the pane falls back to per-kind defaults
/// instead of what the backend actually offers.
///
/// ❗ **The pin filters the SWITCHER, not this list.** The volume list is what
/// says a volume id exists: the hub's Enter, a restored tab, a favorite, and the
/// pane's own lookup all reach for a row by id, and an unpinned server with no
/// row is an id that resolves to nothing and a pane that lands on the boot disk.
/// So every place gets a row carrying its own `pinned`, and
/// `navigation/volume-grouping.ts` is where the user's cap is applied.
pub(crate) fn append_server_volumes(volumes: &mut Vec<LocationInfo>) {
    volumes.extend(server_places().into_iter().map(location_from_place));
}

/// The app-facing root of the place `volume_id` names, for an event payload that
/// wants to say WHERE as well as which.
pub(crate) fn place_root(volume_id: &str) -> Option<String> {
    server_places()
        .into_iter()
        .find(|place| place.id == volume_id)
        .map(|place| place.app_root)
}

/// The server volume an `sftp://` or `webdav://` path belongs to, registered or
/// merely saved.
///
/// ❗ **This never dials.** Resolving says WHERE a path lives; activating the
/// row is what brings it to life. A dial here would make every restored server
/// tab connect at launch, which is exactly what a saved row exists to avoid. The
/// neighbouring `adb://` arm in `commands/volumes.rs` answers from the cached
/// device row the same way.
pub(crate) fn server_volume_for_path(path: &str) -> Option<LocationInfo> {
    server_places()
        .into_iter()
        .find(|place| path_is_under(path, &place.app_root))
        .map(location_from_place)
}

/// Whether `path` is `root` or sits under it, by whole `/`-separated segments.
///
/// ❗ Segment-wise, ❌ never a raw string prefix: `sftp://ada@nas:22/srv/data`
/// would otherwise claim `sftp://ada@nas:22/srv/data-1`, a different volume's
/// tree.
fn path_is_under(path: &str, root: &str) -> bool {
    let root = root.trim_end_matches('/');
    path == root || path.strip_prefix(root).is_some_and(|rest| rest.starts_with('/'))
}

#[cfg(test)]
#[path = "server_volumes_test.rs"]
mod server_volumes_test;
