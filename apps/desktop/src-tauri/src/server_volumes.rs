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
//! ❗ **A `saved` row and the volume it becomes share one id**
//! (`cmdr_fs::volume::sftp_volume_id` / `webdav_volume_id`, the same ids the
//! registry uses), which is what makes a tab on a server restorable: a tab holds
//! `(volumeId, path)`, the switcher shows the same id greyed, and activating
//! either dials the same saved entry.

use cmdr_fs::volume::remote_paths::RemoteRoot;
use cmdr_fs::volume::{BackendKind, ConnectionState};

use crate::network::{sftp_known_servers, webdav_known_servers};
use crate::volume_listing::{LocationCategory, LocationInfo};

/// One SFTP or WebDAV place, from the stores and the registry.
#[derive(Debug, Clone)]
pub(crate) struct ServerPlace {
    /// The volume id, from the same funnel the registry keys on.
    pub id: String,
    /// The saved display name.
    pub name: String,
    /// The app-facing root: `<prefix><remote root>`
    /// (`cmdr_fs::volume::remote_paths`).
    pub app_root: String,
    /// `"sftp"` or `"webdav"`. ❗ Load-bearing beyond display: the MCP volumes
    /// resource keys `kind` on it, and the frontend's `volumeKindFor` reads it
    /// AHEAD of the `category === 'network'` arm, so a row without it is an SMB
    /// pane.
    pub fs_type: &'static str,
    /// How live the session is: `Direct` for a registered volume, `Saved` for a
    /// server that is only remembered.
    pub state: ConnectionState,
}

/// Every SFTP and WebDAV place the app knows: one per saved server, marked
/// `Direct` when a volume under its id is registered right now.
///
/// ❗ Includes UNPINNED servers. Pins govern the switcher, not identity, and a
/// tab restored onto an unpinned server still has to find its way home. The
/// listing filters; the resolver does not.
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
            name: server.display_name,
            app_root: root.app_root().to_string_lossy().into_owned(),
            fs_type: "sftp",
        });
    }
    for server in webdav_known_servers::all() {
        let Some((host, port)) = webdav_endpoint(&server.url) else {
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
            name: server.display_name,
            app_root: root.app_root().to_string_lossy().into_owned(),
            fs_type: "webdav",
        });
    }
    places
}

/// The `(host, port)` a saved WebDAV URL names, or `None` when it isn't a URL.
///
/// ❗ Through the crate's own params, so the pair this derives an id and a prefix
/// from is exactly the pair the dial derives them from.
fn webdav_endpoint(url: &str) -> Option<(String, u16)> {
    let parsed = url::Url::parse(url.trim()).ok()?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return None;
    }
    let params = cmdr_webdav::WebdavConnectionParams::new(parsed, "", "/");
    Some((params.host().to_string(), params.port()))
}

/// The row a place becomes.
///
/// ❗ `is_ejectable: false` and `supports_trash: false`: a server has nothing to
/// unplug (its control says "Disconnect") and no trash to move a file to.
/// `capabilities: None` because enrichment fills that from the registered
/// `Volume` afterwards, which is why the servers arm folds BEFORE it.
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
        device_readiness: None,
        usb_speed: None,
        capabilities: None,
    }
}

/// The server volume an `sftp://` or `webdav://` path belongs to, registered or
/// merely saved.
///
/// ❗ **This never dials.** The neighbouring `adb://` arm in
/// `commands/volumes.rs` connects the device as a side effect of resolving, and
/// copying that shape would make every restored server tab dial at launch, which
/// is exactly what a saved row exists to avoid. Resolving says WHERE a path
/// lives; activating the row is what brings it to life.
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
