//! IPC commands for user-editable favorites.
//!
//! Pass-throughs over the `favorites::store` module. Each mutation persists `favorites.json`
//! (a filesystem write, so it runs on the blocking pool with a timeout) and then re-emits
//! `volumes-changed` so every surface listing favorites refreshes live (subscribe-don't-poll).
//! Listing rides the existing `list_volumes` / `volumes-changed` path, so there's no
//! `list_favorites` command.
//!
//! [`add_favorite`] carries one piece of judgment: THE add gate, the single place that decides
//! whether a path is somewhere a favorite can point. Every add surface goes through it (the
//! palette / Go menu command, the folder-row and `..` context menus, and the MCP `favorites` tool),
//! which is why it sits at the command rather than in `crate::favorites`: the store stays sync and
//! `AppHandle`-free because `volumes::get_favorites` reads it from a sync, handle-less path, and it
//! deliberately knows nothing about volumes. Why the rule exists at all, and which pane kinds it
//! refuses: `../../favorites/DETAILS.md` § The add gate.

use std::path::Path;

use serde::{Deserialize, Serialize};
use tokio::time::Duration;

use crate::deadline::{DeadlineError, blocking_typed_result_with_timeout};
use crate::favorites::store;

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
    /// `path` isn't one `volumes::get_favorites` would ever hand back, so storing it would grow
    /// `favorites.json` with an entry no pane can list or reach. See [`path_can_be_favorited`].
    NotAnOsVisiblePath,
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
            Self::NotAnOsVisiblePath => f.write_str("not a path a favorite can point at"),
            Self::TimedOut => f.write_str("timed out"),
            Self::Unexpected { detail } => write!(f, "unexpected: {detail}"),
        }
    }
}

impl std::error::Error for AddFavoriteError {}

/// Adds a favorite for `path`, deduping by normalized path. When `name` is omitted, the label
/// defaults to the path's file name.
///
/// Refuses a path a favorite can't point at ([`path_can_be_favorited`]). The frontend greys its
/// add affordance out on the same reading, but that's an affordance and this is the enforcement:
/// the MCP `favorites` tool and the native folder-row menus never touch that frontend predicate.
#[tauri::command]
#[specta::specta]
pub async fn add_favorite(path: String, name: Option<String>) -> Result<(), AddFavoriteError> {
    if !path_can_be_favorited(&path).await? {
        log::info!(target: "favorites", "refused to favorite {path:?}: get_favorites would filter it out");
        return Err(AddFavoriteError::NotAnOsVisiblePath);
    }
    persist(move || {
        store::add(&path, name);
        Ok(())
    })
    .await?;
    crate::volume_broadcast::emit_volumes_changed();
    Ok(())
}

/// Whether a favorite pointing at `path` would survive the read side's existence filter.
///
/// `volumes::get_favorites` builds the favorites list every surface renders, and drops any favorite
/// whose path isn't there on disk. So an `smb://`, `sftp://`, `webdav://`, `mtp://`, `adb://`, or
/// `search-results://` path stored today is written to `favorites.json` and then listed nowhere in
/// the app: no row, no error, a file that only grows. This is the gate that stops that, and it has
/// to agree with that filter, ❌ never be laxer. Naming the FILTER rather than any one surface is
/// deliberate: a third surface wouldn't change the rule.
///
/// Three typed readings, ❌ none of them a test on the path string:
///
/// - [`Path::is_absolute`]: a scheme path with no protocol arm in the resolver (`search-results://`
///   today, whatever is added tomorrow) would otherwise reach the mount table, which WALKS UP on a
///   failed `statfs` and so answers with the boot volume for a path that has nothing to do with it.
///   Asking `std::path` whether this is an absolute path at all is what keeps that generic rather
///   than a list of schemes somebody has to remember to extend.
/// - [`Volume::paths_are_os_visible`](crate::file_system::volume::Volume::paths_are_os_visible) on
///   the volume that serves the path: the same reading Quick Look, the drag commands, and "Open
///   terminal here" take. It admits local drives and SMB (whose paths are ordinary `/Volumes/…`
///   ones while the share is mounted) and refuses every protocol-only backend. ❌ Not
///   `supports_local_fs_access()`, which a direct-SMB volume answers `false` to while its paths are
///   exactly what Finder can open.
/// - [`path_routes_over_its_parent`](crate::file_system::volume::manager::path_routes_over_its_parent):
///   an archive-inner or `.git`-portal path resolves to the parent DRIVE, which is OS-visible, yet
///   the path itself has no file of its own there. Without this clause both would pass the first
///   reading and then be filtered out exactly like a protocol path.
///
/// `path` is the pane's own path, so this costs one mount-table read on the add path and nothing
/// anywhere else.
async fn path_can_be_favorited(path: &str) -> Result<bool, AddFavoriteError> {
    path_can_be_favorited_with_timeout(path, crate::commands::volumes::VOLUME_TIMEOUT).await
}

/// Add-gate body with an injectable resolver timeout. A resolver timeout means the gate has no
/// answer, so it stays a typed timeout rather than becoming a permanent-path refusal.
async fn path_can_be_favorited_with_timeout(path: &str, fs_timeout: Duration) -> Result<bool, AddFavoriteError> {
    let path = Path::new(path);
    if !path.is_absolute() || crate::file_system::volume::manager::path_routes_over_its_parent(path) {
        return Ok(false);
    }
    // The canonical path→volume resolver: protocol dispatch first, then the mount table under its
    // own timeout. A timeout is UNKNOWN rather than evidence that the path is invalid.
    let resolution =
        crate::commands::volumes::resolve_path_volume_with_timeout(path.to_string_lossy().into_owned(), fs_timeout)
            .await;
    resolved_path_can_be_favorited(resolution)
}

/// Classifies the resolver's three outcomes without turning an unknown result into a refusal.
fn resolved_path_can_be_favorited(
    resolution: crate::commands::volumes::PathVolumeResolution,
) -> Result<bool, AddFavoriteError> {
    if resolution.timed_out {
        return Err(AddFavoriteError::TimedOut);
    }
    let Some(volume) = resolution.volume else {
        return Ok(false);
    };
    Ok(volume_paths_are_os_visible(&volume.id))
}

/// Whether the registered volume `volume_id` names hands out paths the OS can reach.
///
/// ❗ Conservative where `quick_look` and "Open terminal here" are permissive: an id the registry
/// doesn't hold is a REFUSAL here. Those two would silence a working feature by refusing a live
/// path; this one would instead write a favorite that `volumes::get_favorites` filters out of the
/// list forever. A saved-but-not-connected server and an unplugged phone both resolve to a
/// `VolumeInfo` with no registered volume behind it, which is precisely the case to refuse.
fn volume_paths_are_os_visible(volume_id: &str) -> bool {
    match crate::file_system::volume::manager::get_volume_manager().get(volume_id) {
        Some(volume) => volume.paths_are_os_visible(),
        None => {
            log::debug!(target: "favorites", "volume {volume_id} isn't registered; treating its paths as not OS-visible");
            false
        }
    }
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
/// real `favorites.json`; the accepting cases only ask the predicate, never the command.
#[cfg(test)]
mod add_gate_tests {
    use super::*;
    use crate::file_system::volume::manager::get_volume_manager;
    use crate::file_system::volume::{LocalPosixVolume, Volume};
    use crate::test_support::CapabilityStub;
    use std::sync::Arc;

    /// Correctness tests must not inherit the production scheduling deadline. The mount-table read
    /// is fast, but a saturated blocking pool can delay when it starts.
    const TEST_FS_TIMEOUT: Duration = Duration::from_secs(3600);

    /// The boot volume, so an ordinary local path has something OS-visible behind it.
    /// `register_if_absent` because the registry is process-wide and shared with every other test
    /// in this binary.
    fn ensure_root_volume() {
        get_volume_manager().register_if_absent("root", Arc::new(LocalPosixVolume::local_folder("Test root", "/")));
    }

    #[tokio::test]
    async fn an_ordinary_local_folder_can_be_favorited() {
        ensure_root_volume();
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(
            path_can_be_favorited_with_timeout(&dir.path().to_string_lossy(), TEST_FS_TIMEOUT)
                .await
                .expect("local volume resolution should finish")
        );
    }

    #[test]
    fn a_volume_resolution_timeout_stays_a_timeout() {
        let result = resolved_path_can_be_favorited(crate::commands::volumes::PathVolumeResolution {
            volume: None,
            timed_out: true,
        });

        assert!(matches!(result, Err(AddFavoriteError::TimedOut)));
    }

    /// ❗ The silent-disappearance case, and the whole reason the gate exists: `volumes::get_favorites`
    /// drops a favorite whose path isn't on disk, so a stored protocol path would be written to
    /// `favorites.json` and then never shown — no row, no error, a file that only grows.
    ///
    /// ❗ `search-results://` is the one with no protocol arm in the resolver, so it reaches the
    /// mount table, which walks UP on a failed `statfs` and would hand back the boot volume. The
    /// `is_absolute` clause is what refuses it, and what will refuse the next scheme too.
    #[tokio::test]
    async fn a_protocol_path_cannot_be_favorited() {
        for path in [
            "smb://nas/share",
            "sftp://nas.local/home/me",
            "webdav://host.example/dav",
            "mtp://pixel/Internal storage/DCIM",
            "adb://pixel/sdcard/DCIM",
            "search-results://8f3a1c",
        ] {
            assert!(
                !path_can_be_favorited_with_timeout(path, TEST_FS_TIMEOUT)
                    .await
                    .expect("protocol path classification should finish"),
                "{path} should be refused"
            );
        }
    }

    /// The `.zip` FILE sits on an OS-visible drive and a path INSIDE it resolves to that same
    /// drive, so the volume reading alone would wave it through. `path_routes_over_its_parent` is
    /// what catches it, and it catches a `.git`-portal path the same way.
    #[tokio::test]
    async fn an_archive_inner_path_cannot_be_favorited() {
        ensure_root_volume();
        let dir = tempfile::tempdir().expect("tempdir");
        let zip = dir.path().join("bundle.zip");
        std::fs::write(&zip, b"PK\x03\x04not-a-real-archive-body").expect("write zip magic");

        assert!(
            !path_can_be_favorited_with_timeout(&zip.join("docs").to_string_lossy(), TEST_FS_TIMEOUT)
                .await
                .expect("archive path classification should finish")
        );
        // ❗ The archive FILE itself is an ordinary file on an ordinary drive, and its containing
        // folder is a perfectly good favorite. Only a path CONTINUING inside one is refused.
        assert!(
            path_can_be_favorited_with_timeout(&dir.path().to_string_lossy(), TEST_FS_TIMEOUT)
                .await
                .expect("local volume resolution should finish")
        );
    }

    #[tokio::test]
    async fn the_command_answers_a_refusal_with_its_own_variant() {
        let refusal = add_favorite("mtp://pixel/Internal storage".to_string(), None).await;
        assert!(matches!(refusal, Err(AddFavoriteError::NotAnOsVisiblePath)));
    }

    /// Registers a stub under a unique id (the registry is process-wide) and returns the gate's
    /// reading of it. The gate asks `paths_are_os_visible` alone, so the other flag is pinned
    /// `false`: a stub that answered `true` there would let a wrong gate pass.
    fn os_visibility_of(id: &str, paths_are_os_visible: bool) -> bool {
        let manager = get_volume_manager();
        manager.register(
            id,
            Arc::new(CapabilityStub {
                supports_local_fs_access: false,
                paths_are_os_visible,
            }) as Arc<dyn Volume>,
        );
        let answer = volume_paths_are_os_visible(id);
        manager.unregister(id);
        answer
    }

    /// The direct-SMB shape: Cmdr's own I/O rides smb2, but the share stays OS-mounted, so the
    /// `/Volumes/…` paths it hands out are ones the existence filter keeps. ❌ Not
    /// `supports_local_fs_access()`, which this volume answers `false` to.
    #[test]
    fn an_os_visible_volume_passes_even_without_local_fs_access() {
        assert!(os_visibility_of("fav-gate-smb-shaped", true));
    }

    /// The MTP / ADB / SFTP shape: no OS-reachable paths at all.
    #[test]
    fn a_volume_with_no_os_visible_paths_is_refused() {
        assert!(!os_visibility_of("fav-gate-protocol-shaped", false));
    }

    /// ❗ Opposite default to `quick_look` and "Open terminal here", which assume yes on an
    /// unknown id rather than silence a working feature. Here the cost of guessing yes is a stored
    /// favorite the read filter then hides forever, so an unknown id is a refusal. A saved-but-offline
    /// server and an unplugged phone both land here.
    #[test]
    fn an_unregistered_volume_is_refused() {
        assert!(!volume_paths_are_os_visible("fav-gate-never-registered"));
    }
}
