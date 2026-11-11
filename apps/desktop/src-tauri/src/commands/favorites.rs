//! IPC commands for user-editable favorites.
//!
//! Pass-throughs over the `favorites::store` module. Each mutation persists `favorites.json`
//! (a filesystem write, so it runs on the blocking pool with a timeout) and then re-emits
//! `volumes-changed` so every surface listing favorites refreshes live (subscribe-don't-poll).
//! Listing rides the existing `list_volumes` / `volumes-changed` path, so there's no
//! `list_favorites` command.
//!
//! [`add_favorite`] carries one piece of judgment: THE add gate, the single place that decides
//! whether a path is somewhere a favorite can point, and which volume it lives on. Every add
//! surface goes through it (the palette / Go menu command, the folder-row and `..` context menus,
//! and the MCP `favorites` tool), which is why it sits at the command rather than in
//! `crate::favorites`: the store stays sync and `AppHandle`-free because discovery reads it from a
//! sync, handle-less path, and it deliberately resolves no volumes. Why the rule exists at all,
//! and which pane kinds it refuses: `../../favorites/DETAILS.md` § The add gate.

use std::path::Path;

use cmdr_fs::volume::app_paths::path_is_under;
use cmdr_fs::volume::{BackendKind, VolumeScheme};
use serde::{Deserialize, Serialize};
use tokio::time::Duration;

use crate::commands::volumes::PathVolumeResolution;
use crate::favorites::store::{self, FavoriteVolume};
use crate::volume_listing::{LocationCategory, LocationInfo, NetworkMount};
use cmdr_fs::deadline::{DeadlineError, blocking_typed_result_with_timeout};

/// 5s matches the write timeout other persisting commands use. The store write is local-only, but a
/// hung data-dir mount must never freeze the IPC thread.
///
/// The store swallows its own write errors (a favorite that doesn't persist still
/// applies in memory), so a missed deadline is the ONLY thing the three editing
/// commands can report; hence [`DeadlineError`] rather than a vocabulary of their own.
/// [`add_favorite`] can also REFUSE, so it answers in [`AddFavoriteError`].
const PERSIST_TIMEOUT: Duration = Duration::from_secs(5);

/// Why an add didn't happen.
///
/// Its own vocabulary rather than [`DeadlineError`], which is for commands whose work can't refuse
/// at all: this one can, and a refusal the frontend has to recognize by its wording is a refusal
/// that breaks on the next copy edit.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum AddFavoriteError {
    /// `path` isn't a folder on a disk, a share, a server, or a phone: an archive-inner or
    /// `.git`-portal path, a search-results snapshot, the servers hub, or a path no volume
    /// contains. See [`favorite_volume_for`].
    NotAPlace,
    /// `path` is on a saved place (a server, a phone) that isn't connected right now. Cmdr only
    /// favorites a folder it has seen live, so the caller connects first.
    PlaceNotConnected,
    /// The work didn't finish inside the command's wait. ❗ It was NOT cancelled.
    TimedOut,
    /// The task panicked, so no answer is coming.
    Unexpected {
        /// What the runtime reported, for the log.
        detail: String,
    },
}

impl From<DeadlineError> for AddFavoriteError {
    fn from(error: DeadlineError) -> Self {
        match error {
            DeadlineError::TimedOut => Self::TimedOut,
            DeadlineError::Unexpected { detail } => Self::Unexpected { detail },
        }
    }
}

impl std::fmt::Display for AddFavoriteError {
    /// ❗ For logs and debugging only.
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NotAPlace => f.write_str("not a folder a favorite can point at"),
            Self::PlaceNotConnected => f.write_str("on a place that isn't connected"),
            Self::TimedOut => f.write_str("timed out"),
            Self::Unexpected { detail } => write!(f, "unexpected: {detail}"),
        }
    }
}

impl std::error::Error for AddFavoriteError {}

/// Adds a favorite for `path`, on the volume the add gate names. When `name` is omitted, the label
/// defaults to the path's file name. A re-add of the same folder on the same volume moves the
/// existing favorite rather than adding a second.
///
/// Refuses a path a favorite can't point at ([`favorite_volume_for`]). The frontend greys its add
/// affordance out on the pane's kind, but that's an affordance and this is the enforcement: the MCP
/// `favorites` tool and the native folder-row menus never touch that frontend predicate.
///
/// A folder on an SMB share nothing has saved (Finder mounted it) saves the share first, unpinned
/// (`network::smb_saved_shares::remember_favorited_share`): otherwise nothing could dial the
/// favorite once the share unmounts.
#[tauri::command]
#[specta::specta]
pub async fn add_favorite(path: String, name: Option<String>) -> Result<(), AddFavoriteError> {
    let volume = favorite_volume_for(&path).await.inspect_err(|refusal| {
        log::info!(
            target: "favorites",
            "refused to favorite {path:?}: {refusal}",
            path = cmdr_fs::log_private::LogPrivate::path(&path)
        );
    })?;
    persist(move || {
        if VolumeScheme::of(&volume.id) == VolumeScheme::Smb {
            crate::network::smb_saved_shares::remember_favorited_share(&volume.root, &volume.id);
        }
        store::add(&path, name, volume);
        Ok(())
    })
    .await
    .inspect_err(|error| {
        // The gate passed, so this is the save itself (a hung data-dir mount, a panicked write).
        log::warn!(target: "favorites", "couldn't save the favorite: {error}");
    })?;
    crate::volume_broadcast::emit_volumes_changed();
    Ok(())
}

/// The add gate: the volume a favorite on `path` lives on, or why there can't be one.
///
/// Typed readings only, ❌ none of them a test on the path string:
///
/// 1. [`path_routes_over_its_parent`](crate::file_system::volume::manager::path_routes_over_its_parent):
///    an archive-inner or `.git`-portal path resolves to the parent DRIVE, yet has no folder of its
///    own there.
/// 2. A folder on a network mount the registry holds at that mount point is answered there, with
///    no round trip to the server ([`favorite_volume_on_network_mount`]). Anything else goes on.
/// 3. The canonical path→volume resolver, protocol arms first (`mtp://`, `adb://`, `sftp://`,
///    `webdav://`, `s3://`, `smb://` → the servers hub), the mount table otherwise. A timeout is
///    UNKNOWN, ❌ never a refusal.
/// 4. The volume must CONTAIN the path, by whole segments. A scheme with no protocol arm
///    (`search-results://`) reaches the mount table, which walks UP to `/`, which doesn't contain
///    it; that's what refuses the next unarmed scheme too, with no list to extend.
/// 5. The volume must be REGISTERED, which means the folder was seen live, and its backend admitted
///    by [`backend_holds_favorites`]. An unregistered saved place or listed device is
///    [`AddFavoriteError::PlaceNotConnected`]; anything else unregistered (the servers hub) is
///    [`AddFavoriteError::NotAPlace`].
///
/// `path` is the pane's own path, so this costs one resolver call on the add path and nothing
/// anywhere else.
async fn favorite_volume_for(path: &str) -> Result<FavoriteVolume, AddFavoriteError> {
    favorite_volume_for_with_timeout(path, crate::commands::volumes::VOLUME_TIMEOUT).await
}

/// [`favorite_volume_for`] with an injectable resolver timeout: correctness tests mustn't inherit
/// the production scheduling deadline.
async fn favorite_volume_for_with_timeout(
    path: &str,
    fs_timeout: Duration,
) -> Result<FavoriteVolume, AddFavoriteError> {
    favorite_volume_for_with(path, crate::volume_listing::network_mount_under, |path| {
        crate::commands::volumes::resolve_path_volume_with_timeout(path, fs_timeout)
    })
    .await
}

/// [`favorite_volume_for_with_timeout`] with the mount-table read and the resolver injected, so a
/// test can put a path on a share no test machine has, and see whether the gate asked the resolver
/// at all. ❗ Not a zero deadline for that: the resolver's answer races the timer, and on Linux,
/// where it's a lexical `/proc/mounts` walk, it wins.
async fn favorite_volume_for_with<R: Future<Output = PathVolumeResolution>>(
    path: &str,
    network_mount_under: impl FnOnce(&Path) -> Option<NetworkMount>,
    resolve: impl FnOnce(String) -> R,
) -> Result<FavoriteVolume, AddFavoriteError> {
    if crate::file_system::volume::manager::path_routes_over_its_parent(Path::new(path)) {
        return Err(AddFavoriteError::NotAPlace);
    }
    let manager = crate::file_system::volume::manager::get_volume_manager();
    let on_a_connected_share = path
        .starts_with('/')
        .then(|| network_mount_under(Path::new(path)))
        .flatten()
        .and_then(|mount| {
            favorite_volume_on_network_mount(path, mount, |root| {
                manager
                    .find_by_root(root)
                    .map(|(id, volume)| (id, volume.backend_kind()))
            })
        });
    if let Some(volume) = on_a_connected_share {
        return Ok(volume);
    }
    let resolution = resolve(path.to_string()).await;
    favorite_volume_from(path, resolution, |id| {
        crate::file_system::volume::manager::get_volume_manager()
            .get(id)
            .map(|volume| volume.backend_kind())
    })
}

/// Step 2, the shortcut: a folder on a network mount the registry already holds, answered from the
/// cached mount table and the registry, with no `statfs` on the path. A slow server holds that
/// `statfs` past the resolver's deadline, and a share Cmdr is connected to needs no proof it's
/// live (cmdr-reports#40).
///
/// `registered_at` answers the id and backend of the volume registered at a mount root, `None` for
/// none. ❗ The registered volume must sit AT the mount point the table names, ❌ never merely
/// above it: a mount nested inside a registered share is another filesystem, and a prefix match
/// would hand it the share's id. `None` sends the caller the long way, never a refusal.
///
/// Network mounts only: a local disk's `statfs` is cheap, and the resolver also knows what the
/// table can't (a cloud folder on the boot disk is its own row).
fn favorite_volume_on_network_mount(
    path: &str,
    mount: NetworkMount,
    registered_at: impl FnOnce(&Path) -> Option<(String, BackendKind)>,
) -> Option<FavoriteVolume> {
    if !path_is_under(path, &mount.mount_point) {
        return None;
    }
    let (id, kind) = registered_at(Path::new(&mount.mount_point))?;
    backend_holds_favorites(kind).then_some(FavoriteVolume {
        id,
        root: mount.mount_point,
        name: mount.name,
    })
}

/// Steps 4 and 5 of [`favorite_volume_for`], over a resolution already made. `registered_backend`
/// answers which backend serves a registered volume id, `None` for one the registry doesn't hold.
fn favorite_volume_from(
    path: &str,
    resolution: PathVolumeResolution,
    registered_backend: impl FnOnce(&str) -> Option<BackendKind>,
) -> Result<FavoriteVolume, AddFavoriteError> {
    if resolution.timed_out {
        return Err(AddFavoriteError::TimedOut);
    }
    let Some(volume) = resolution.volume else {
        return Err(AddFavoriteError::NotAPlace);
    };
    if !path_is_under(path, &volume.path) {
        return Err(AddFavoriteError::NotAPlace);
    }
    match registered_backend(&volume.id) {
        Some(kind) if backend_holds_favorites(kind) => Ok(FavoriteVolume {
            id: volume.id,
            root: volume.path,
            name: volume.name,
        }),
        Some(_) => Err(AddFavoriteError::NotAPlace),
        None if is_a_place_to_connect(&volume) => {
            log::debug!(target: "favorites", "volume {} isn't connected; a favorite needs it live", volume.id);
            Err(AddFavoriteError::PlaceNotConnected)
        }
        None => Err(AddFavoriteError::NotAPlace),
    }
}

/// Whether a folder on a volume this backend serves can be a favorite.
///
/// Exhaustive on purpose, ❌ no `_` arm: a new backend doesn't compile until it answers, rather
/// than being waved through. An archive and a `.git` portal are views inside a drive, with no
/// folder of their own a favorite could reopen.
///
/// ❗ The frontend's `kindCanBeFavorited` (`src/lib/file-explorer/pane/volume-capabilities.ts`)
/// gives the same answer set from a different reading (the pane's kind), so a change to one is a
/// prompt to check the other.
fn backend_holds_favorites(kind: BackendKind) -> bool {
    match kind {
        BackendKind::Local
        | BackendKind::Smb
        | BackendKind::Sftp
        | BackendKind::Webdav
        | BackendKind::S3
        | BackendKind::Nfs
        | BackendKind::Mtp
        | BackendKind::Adb => true,
        BackendKind::Archive | BackendKind::GitPortal | BackendKind::Restic => false,
    }
}

/// Whether an unregistered resolved row is a place that becomes a volume once connected: a saved
/// share or server (it carries a session state) or a listed device. The servers hub is neither.
fn is_a_place_to_connect(volume: &LocationInfo) -> bool {
    volume.connection_state.is_some() || volume.category == LocationCategory::MobileDevice
}

/// Removes a favorite by id. No-op when the id isn't present.
#[tauri::command]
#[specta::specta]
pub async fn remove_favorite(id: String) -> Result<(), DeadlineError> {
    persist(move || {
        store::remove(&id);
        Ok(())
    })
    .await?;
    crate::volume_broadcast::emit_volumes_changed();
    Ok(())
}

/// Renames a favorite by id. No-op when the id isn't present.
///
/// Takes no path and can't move one, so the add gate doesn't apply: a favorite that passed it
/// stays where it was pointed.
#[tauri::command]
#[specta::specta]
pub async fn rename_favorite(id: String, name: String) -> Result<(), DeadlineError> {
    persist(move || {
        store::rename(&id, &name);
        Ok(())
    })
    .await?;
    crate::volume_broadcast::emit_volumes_changed();
    Ok(())
}

/// Assigns or clears the letter that opens a favorite from its menu. Reusing a letter transfers it.
#[tauri::command]
#[specta::specta]
pub async fn set_favorite_shortcut(id: String, shortcut: Option<String>) -> Result<(), SetFavoriteShortcutError> {
    if shortcut
        .as_ref()
        .is_some_and(|letter| letter.len() != 1 || !letter.as_bytes()[0].is_ascii_alphabetic())
    {
        return Err(SetFavoriteShortcutError::InvalidLetter);
    }
    persist(move || {
        store::set_shortcut(&id, shortcut.as_deref());
        Ok(())
    })
    .await?;
    crate::volume_broadcast::emit_volumes_changed();
    Ok(())
}

/// A shortcut must be one ASCII letter; deadline variants match the other favorite edits.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum SetFavoriteShortcutError {
    InvalidLetter,
    TimedOut,
    Unexpected { detail: String },
}

impl From<DeadlineError> for SetFavoriteShortcutError {
    fn from(error: DeadlineError) -> Self {
        match error {
            DeadlineError::TimedOut => Self::TimedOut,
            DeadlineError::Unexpected { detail } => Self::Unexpected { detail },
        }
    }
}

/// Reorders the favorites to match `ordered_ids`. Unknown ids are ignored; favorites missing from
/// the list are appended in their current order, so a stale order never drops an entry.
#[tauri::command]
#[specta::specta]
pub async fn reorder_favorites(ordered_ids: Vec<String>) -> Result<(), DeadlineError> {
    persist(move || {
        store::reorder(&ordered_ids);
        Ok(())
    })
    .await?;
    crate::volume_broadcast::emit_volumes_changed();
    Ok(())
}

/// Runs one favorites write under the shared deadline, in the typed vocabulary
/// the four commands answer with.
async fn persist(f: impl FnOnce() -> Result<(), DeadlineError> + Send + 'static) -> Result<(), DeadlineError> {
    blocking_typed_result_with_timeout(
        PERSIST_TIMEOUT,
        || DeadlineError::TimedOut,
        |detail| DeadlineError::Unexpected { detail },
        f,
    )
    .await
}

/// The add gate. ❗ Every refusing case stops before [`persist`], so none of these touches the
/// real `favorites.json`; the accepting cases ask the gate, never the command.
#[cfg(test)]
mod add_gate_tests {
    use super::*;
    use crate::file_system::volume::manager::get_volume_manager;
    use cmdr_fs::volume::ConnectionState;
    use cmdr_local::LocalPosixVolume;
    use std::sync::Arc;

    /// Correctness tests must not inherit the production scheduling deadline. The mount-table read
    /// is fast, but a saturated blocking pool can delay when it starts.
    const TEST_FS_TIMEOUT: Duration = Duration::from_secs(3600);

    /// The boot volume, so an ordinary local path has something registered behind it.
    /// `register_if_absent` because the registry is process-wide and shared with every other test
    /// in this binary.
    fn ensure_root_volume() {
        get_volume_manager().register_if_absent(
            "root",
            Arc::new(LocalPosixVolume::local_folder(
                "Test root",
                "/",
                crate::volume_host::host(),
            )),
        );
    }

    /// A resolved row, the way the resolver hands one back.
    fn row(id: &str, path: &str, category: LocationCategory, state: Option<ConnectionState>) -> LocationInfo {
        LocationInfo {
            id: id.to_string(),
            name: format!("{id} name"),
            path: path.to_string(),
            category,
            icon: None,
            is_ejectable: false,
            fs_type: None,
            supports_trash: false,
            mount_is_read_only: false,
            is_disk_image: false,
            shares_disk_with_others: false,
            is_cloud_mount: false,
            case_sensitive: crate::volumes::LOCAL_DISK_CASE_SENSITIVE,
            connection_state: state,
            pinned: None,
            landing_path: None,
            device_readiness: None,
            usb_speed: None,
            capabilities: None,
            favorite_shortcut: None,
            favorite_target: None,
            root_label: None,
            mount_account: None,
        }
    }

    fn resolved(volume: LocationInfo) -> PathVolumeResolution {
        PathVolumeResolution {
            volume: Some(volume),
            timed_out: false,
        }
    }

    #[tokio::test]
    async fn an_ordinary_local_folder_is_favorited_on_its_volume() {
        ensure_root_volume();
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().to_string_lossy().into_owned();
        let volume = favorite_volume_for_with_timeout(&path, TEST_FS_TIMEOUT)
            .await
            .expect("a local folder is a place");
        assert!(path_is_under(&path, &volume.root), "{path} under {}", volume.root);
    }

    #[test]
    fn a_volume_resolution_timeout_stays_a_timeout() {
        let result = favorite_volume_from(
            "/x",
            PathVolumeResolution {
                volume: None,
                timed_out: true,
            },
            |_| unreachable!("a timeout never asks the registry"),
        );
        assert!(matches!(result, Err(AddFavoriteError::TimedOut)));
    }

    /// A protocol path with nothing behind it (no such phone, no such server) resolves to no volume.
    #[tokio::test]
    async fn a_protocol_path_nothing_serves_is_not_a_place() {
        for path in [
            "sftp://nobody@nowhere.invalid:22/home/me",
            "webdav://nobody@nowhere.invalid:443/dav",
            "mtp://no-such-phone/65537/DCIM",
            "adb://no-such-serial/sdcard/DCIM",
        ] {
            assert!(
                matches!(
                    favorite_volume_for_with_timeout(path, TEST_FS_TIMEOUT).await,
                    Err(AddFavoriteError::NotAPlace)
                ),
                "{path} should be refused"
            );
        }
    }

    /// ❗ `search-results://` has no protocol arm, so the mount table walks UP and answers the boot
    /// volume. Containment is what refuses it, and what will refuse the next unarmed scheme too.
    #[tokio::test]
    async fn a_search_results_path_is_refused_by_containment() {
        ensure_root_volume();
        assert!(matches!(
            favorite_volume_for_with_timeout("search-results://8f3a1c", TEST_FS_TIMEOUT).await,
            Err(AddFavoriteError::NotAPlace)
        ));
        let boot = row("root", "/", LocationCategory::MainVolume, None);
        assert!(matches!(
            favorite_volume_from("search-results://8f3a1c", resolved(boot), |_| Some(BackendKind::Local)),
            Err(AddFavoriteError::NotAPlace)
        ));
    }

    /// The servers hub (`smb://`) isn't registered and isn't a place: a favorite there would name
    /// no volume at all.
    #[test]
    fn the_servers_hub_is_not_a_place() {
        let hub = row("network", "smb://", LocationCategory::Network, None);
        assert!(matches!(
            favorite_volume_from("smb://nas/share", resolved(hub), |_| None),
            Err(AddFavoriteError::NotAPlace)
        ));
    }

    /// The `.zip` FILE sits on an ordinary drive and a path INSIDE it resolves to that same drive,
    /// so the volume reading alone would wave it through. `path_routes_over_its_parent` is what
    /// catches it, and it catches a `.git`-portal path the same way.
    #[tokio::test]
    async fn an_archive_inner_path_cannot_be_favorited() {
        ensure_root_volume();
        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("bundle.zip");
        std::fs::write(&zip, b"PK\x03\x04not-a-real-archive-body").expect("write zip magic");

        assert!(matches!(
            favorite_volume_for_with_timeout(&zip.join("docs").to_string_lossy(), TEST_FS_TIMEOUT).await,
            Err(AddFavoriteError::NotAPlace)
        ));
        // ❗ The archive FILE itself is an ordinary file on an ordinary drive, and its containing
        // folder is a perfectly good favorite. Only a path CONTINUING inside one is refused.
        assert!(
            favorite_volume_for_with_timeout(&dir.path().to_string_lossy(), TEST_FS_TIMEOUT)
                .await
                .is_ok()
        );
    }

    /// A share registered at the mount point the table names, the way a connected one is.
    fn register_share(id: &str, root: &str) {
        get_volume_manager().register_if_absent(
            id,
            Arc::new(LocalPosixVolume::local_folder(
                "Share",
                root,
                crate::volume_host::host(),
            )),
        );
    }

    fn network_mount(mount_point: &str) -> NetworkMount {
        NetworkMount {
            mount_point: mount_point.to_string(),
            name: "share on nas".to_string(),
        }
    }

    /// The resolver a server answering in 2-3 s looks like to the gate: its `statfs` on the path
    /// misses the deadline. A gate that answers `TimedOut` over it asked it.
    async fn a_slow_server(_path: String) -> PathVolumeResolution {
        PathVolumeResolution {
            volume: None,
            timed_out: true,
        }
    }

    /// ❗ cmdr-reports#40: favoriting a folder on a share Cmdr already holds must not wait on the
    /// server, so the resolver is never asked.
    #[tokio::test]
    async fn a_folder_on_a_connected_share_is_favorited_without_asking_the_server() {
        register_share("smb-gate-test-connected", "/Volumes/gate-test-connected");
        let volume = favorite_volume_for_with(
            "/Volumes/gate-test-connected/docs",
            |path| {
                assert_eq!(path, Path::new("/Volumes/gate-test-connected/docs"));
                Some(network_mount("/Volumes/gate-test-connected"))
            },
            |_| async { unreachable!("a connected share never waits on the server") },
        )
        .await
        .expect("a connected share answers from the registry");
        assert_eq!(
            volume,
            FavoriteVolume {
                id: "smb-gate-test-connected".to_string(),
                root: "/Volumes/gate-test-connected".to_string(),
                name: "share on nas".to_string(),
            }
        );
    }

    /// A network mount nothing registered (an adopter pass that hasn't run yet) still goes the long
    /// way, and a slow answer there stays a timeout.
    #[tokio::test]
    async fn an_unregistered_network_mount_still_asks_the_resolver() {
        let result = favorite_volume_for_with(
            "/Volumes/gate-test-unregistered/docs",
            |_| Some(network_mount("/Volumes/gate-test-unregistered")),
            a_slow_server,
        )
        .await;
        assert!(matches!(result, Err(AddFavoriteError::TimedOut)), "{result:?}");
    }

    /// ❗ The registered volume must sit at the mount point itself. A mount nested inside a
    /// registered share is another filesystem, so the share's id would be the wrong answer.
    #[tokio::test]
    async fn a_mount_nested_inside_a_registered_share_is_not_that_share() {
        register_share("smb-gate-test-outer", "/Volumes/gate-test-outer");
        let result = favorite_volume_for_with(
            "/Volumes/gate-test-outer/inner/docs",
            |_| Some(network_mount("/Volumes/gate-test-outer/inner")),
            a_slow_server,
        )
        .await;
        assert!(matches!(result, Err(AddFavoriteError::TimedOut)), "{result:?}");
    }

    /// A mount whose name is a prefix of the path's first segment doesn't contain it, and a view
    /// backend registered at the mount holds no favorites; both go the long way.
    #[test]
    fn the_network_mount_shortcut_needs_containment_and_a_place_backend() {
        let share = || network_mount("/Volumes/share");
        assert_eq!(
            favorite_volume_on_network_mount("/Volumes/share2/docs", share(), |_| unreachable!(
                "containment fails before the registry is asked"
            )),
            None
        );
        assert_eq!(
            favorite_volume_on_network_mount("/Volumes/share/docs", share(), |_| Some((
                "routed-1".to_string(),
                BackendKind::Archive
            ))),
            None
        );
        assert_eq!(
            favorite_volume_on_network_mount("/Volumes/share/docs", share(), |root| {
                assert_eq!(root, Path::new("/Volumes/share"), "asked at the mount point itself");
                Some(("smb-1".to_string(), BackendKind::Smb))
            })
            .map(|volume| volume.id),
            Some("smb-1".to_string())
        );
    }

    #[tokio::test]
    async fn the_command_answers_a_refusal_with_its_own_variant() {
        let refusal = add_favorite("mtp://no-such-phone/65537".to_string(), None).await;
        assert!(matches!(refusal, Err(AddFavoriteError::NotAPlace)));
    }

    /// Servers and phones are places too: a registered one hands back the volume the favorite
    /// lives on, rooted where the resolver says.
    #[test]
    fn a_registered_server_or_phone_is_favorited_on_its_volume() {
        for (kind, id, root, path, category) in [
            (
                BackendKind::Sftp,
                "sftp-nas-1",
                "sftp://ada@nas:22/srv",
                "sftp://ada@nas:22/srv/data",
                LocationCategory::Network,
            ),
            (
                BackendKind::S3,
                "s3-bucket-1",
                "s3://key@host:443/photos",
                "s3://key@host:443/photos/2026",
                LocationCategory::Network,
            ),
            (
                BackendKind::Mtp,
                "mtp-pixel-1:65537",
                "mtp://pixel/65537",
                "mtp://pixel/65537/DCIM",
                LocationCategory::MobileDevice,
            ),
            (
                BackendKind::Adb,
                "adb-serial-1",
                "adb://serial",
                "adb://serial/sdcard/DCIM",
                LocationCategory::MobileDevice,
            ),
        ] {
            let volume = favorite_volume_from(path, resolved(row(id, root, category, None)), |asked| {
                assert_eq!(asked, id);
                Some(kind)
            })
            .unwrap_or_else(|refusal| panic!("{kind:?} should be accepted, got {refusal}"));
            assert_eq!(volume.id, id);
            assert_eq!(volume.root, root);
            assert_eq!(volume.name, format!("{id} name"));
        }
    }

    /// A view inside a drive has no folder of its own a favorite could reopen.
    #[test]
    fn an_archive_or_git_portal_backend_is_refused() {
        for kind in [BackendKind::Archive, BackendKind::GitPortal, BackendKind::Restic] {
            let volume = row("routed-1", "/Users/me/a.zip", LocationCategory::AttachedVolume, None);
            assert!(matches!(
                favorite_volume_from("/Users/me/a.zip/docs", resolved(volume), |_| Some(kind)),
                Err(AddFavoriteError::NotAPlace)
            ));
        }
    }

    /// The exhaustive match, cell by cell: what's admitted, and what isn't.
    #[test]
    fn every_place_backend_holds_favorites_and_no_view_does() {
        for kind in [
            BackendKind::Local,
            BackendKind::Smb,
            BackendKind::Sftp,
            BackendKind::Webdav,
            BackendKind::S3,
            BackendKind::Nfs,
            BackendKind::Mtp,
            BackendKind::Adb,
        ] {
            assert!(backend_holds_favorites(kind), "{kind:?}");
        }
        assert!(!backend_holds_favorites(BackendKind::Archive));
        assert!(!backend_holds_favorites(BackendKind::GitPortal));
        assert!(!backend_holds_favorites(BackendKind::Restic));
    }

    #[test]
    fn a_volume_whose_root_does_not_contain_the_path_is_refused() {
        let volume = row("vol-t7", "/Volumes/T7", LocationCategory::AttachedVolume, None);
        assert!(matches!(
            favorite_volume_from("/Volumes/T7-1/docs", resolved(volume), |_| Some(BackendKind::Local)),
            Err(AddFavoriteError::NotAPlace)
        ));
    }

    /// ❗ Registered-only: a favorite records a folder the app has seen live. A saved server that
    /// isn't connected (or a phone listed but not dialed) asks the caller to connect first, ❌
    /// never a guess that writes a folder nobody checked.
    #[test]
    fn a_saved_place_that_is_not_connected_says_so() {
        let saved = row(
            "sftp-nas-1",
            "sftp://ada@nas:22/srv",
            LocationCategory::Network,
            Some(ConnectionState::Saved),
        );
        assert!(matches!(
            favorite_volume_from("sftp://ada@nas:22/srv/data", resolved(saved), |_| None),
            Err(AddFavoriteError::PlaceNotConnected)
        ));
        let phone = row("adb-serial-1", "adb://serial", LocationCategory::MobileDevice, None);
        assert!(matches!(
            favorite_volume_from("adb://serial/sdcard", resolved(phone), |_| None),
            Err(AddFavoriteError::PlaceNotConnected)
        ));
    }
}
