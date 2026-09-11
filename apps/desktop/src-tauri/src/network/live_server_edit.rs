//! Saving an edit to a place that is connected right now: which folders the
//! live session confirms first, and installing the result with no redial.
//!
//! Protocol-free on purpose. Each `*_volume_wiring.rs` knows how to build an
//! instance over its own connection (`sharing_connection`) and which redial
//! params move; the order, the refusals, and the event are identical between
//! SFTP and WebDAV, so a fix to them lands once. The whole flow:
//! `DETAILS.md` § "Editing a connected place".

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use cmdr_fs::volume::remote_paths::RemoteRoot;
use cmdr_fs::volume::{Volume, VolumeError};

use super::saved_server_fields::SavedServerOutcome;
use crate::file_system::volume::manager::{RootReplacement, VolumeManager};
use crate::volume_broadcast::{self, VolumeRootChanged};

/// How long the live session gets to confirm the folders an edit names, shared
/// by the root and the start folder. The IPC writes tier: a person is waiting on
/// a Save button.
pub const CHECK_BUDGET: Duration = Duration::from_secs(5);

/// A saved place as it is registered right now.
pub struct ConnectedPlace {
    /// The id it's registered under, which an edit never changes.
    pub volume_id: String,
    /// The instance serving it.
    pub live: Arc<dyn Volume>,
    /// The prefix every app path on it carries, from `cmdr_fs::volume::ids`.
    pub app_prefix: String,
    /// The start folder the store held before this edit.
    pub saved_start_folder: Option<String>,
}

/// The fields of an edit the live volume cares about.
pub struct PlaceEdit<'a> {
    /// What the place is called now (`saved_server_fields::server_label`).
    pub label: &'a str,
    /// The server-side root.
    pub remote_root: &'a str,
    /// The start folder, already checked to sit at or under `remote_root` and
    /// normalized (`saved_server_fields::start_folder_under_root`).
    pub start_folder: Option<&'a str>,
}

/// An edit the live session accepted: what to install once the store holds it.
#[must_use]
pub struct AcceptedEdit {
    volume_id: String,
    /// The instance the check asked, and the only one the install replaces.
    checked: Arc<dyn Volume>,
    /// The instance to swap in, when the label or the root moved.
    successor: Option<Arc<dyn Volume>>,
    /// What to tell the panes, when the root or the landing moved.
    root_changed: Option<VolumeRootChanged>,
}

/// Asks the live session to confirm the folders `edit` names, and plans what
/// installing it changes. A refusal is typed, and nothing has moved yet.
///
/// `successor_for(label, remote_root)` builds an instance over the live
/// connection. It's called only when the label or the root moved, and FIRST:
/// the live instance refuses any path above its own root, so a wider root can
/// only be confirmed through the instance that will serve it.
///
/// ❗ Asks the server only about a folder that MOVED: the root when it changed,
/// the start folder when the root or the landing did. An edit that moves neither
/// asks nothing, because the "reconnect automatically" switch is saved through
/// here too and is most needed while the session is down.
pub async fn check(
    place: &ConnectedPlace,
    edit: &PlaceEdit<'_>,
    successor_for: impl FnOnce(&str, &Path) -> Arc<dyn Volume>,
    budget: Duration,
) -> Result<AcceptedEdit, SavedServerOutcome> {
    let deadline = tokio::time::Instant::now() + budget;
    let new_root = RemoteRoot::new(place.app_prefix.clone(), Path::new(edit.remote_root));
    let old_root = place.live.root().to_path_buf();
    let root_moved = new_root.app_root() != old_root;
    let successor =
        (root_moved || edit.label != place.live.name()).then(|| successor_for(edit.label, new_root.remote_root()));

    // `to_app_path` only prefixes, so the new root's spells a start folder the
    // way either root would.
    let old_landing = place
        .saved_start_folder
        .as_deref()
        .map(|folder| new_root.to_app_path(folder))
        // A saved start folder the live root doesn't hold can't be where the
        // place landed: the store drifted from the session.
        .filter(|landing| landing.starts_with(&old_root))
        .unwrap_or_else(|| old_root.clone());
    let new_landing = edit.start_folder.map_or_else(
        || new_root.app_root().to_path_buf(),
        |folder| new_root.to_app_path(folder),
    );
    let landing_moved = new_landing != old_landing;

    let asked = successor.as_ref().unwrap_or(&place.live);
    if root_moved {
        confirm_folder(asked, new_root.app_root(), deadline, SavedServerOutcome::RootNotFound).await?;
    }
    if edit.start_folder.is_some() && (root_moved || landing_moved) {
        confirm_folder(asked, &new_landing, deadline, SavedServerOutcome::StartFolderNotFound).await?;
    }

    let root_changed = (root_moved || landing_moved).then(|| VolumeRootChanged {
        volume_id: place.volume_id.clone(),
        old_root: old_root.to_string_lossy().into_owned(),
        new_root: new_root.app_root().to_string_lossy().into_owned(),
        old_landing: old_landing.to_string_lossy().into_owned(),
        new_landing: new_landing.to_string_lossy().into_owned(),
    });
    Ok(AcceptedEdit {
        volume_id: place.volume_id.clone(),
        checked: Arc::clone(&place.live),
        successor,
        root_changed,
    })
}

/// Whether `path` is a folder this account can open, asked of `volume` by the
/// shared `deadline`. A "no" (missing, a file, or refused) becomes `missing`;
/// anything that isn't an answer becomes `Unreachable`.
///
/// ❗ The stat runs on its own task and the JOIN HANDLE is what times out, so a
/// slow server finishes in the background rather than being dropped mid-request
/// (`commands/CLAUDE.md` § timeouts).
async fn confirm_folder(
    volume: &Arc<dyn Volume>,
    path: &Path,
    deadline: tokio::time::Instant,
    missing: SavedServerOutcome,
) -> Result<(), SavedServerOutcome> {
    let volume = Arc::clone(volume);
    let path: PathBuf = path.to_path_buf();
    let stat = tokio::spawn(async move { volume.is_directory(&path).await });
    match tokio::time::timeout_at(deadline, stat).await {
        Ok(Ok(Ok(true))) => Ok(()),
        Ok(Ok(Ok(false) | Err(VolumeError::NotFound(_) | VolumeError::PermissionDenied(_)))) => Err(missing),
        Ok(Ok(Err(error))) => {
            log::info!(target: "volume", "an edited place's folder couldn't be confirmed: {error:?}");
            Err(SavedServerOutcome::Unreachable)
        }
        Ok(Err(join_error)) => {
            log::warn!(target: "volume", "confirming an edited place's folder stopped early: {join_error}");
            Err(SavedServerOutcome::Unreachable)
        }
        Err(_elapsed) => {
            log::info!(target: "volume", "an edited place's server didn't confirm a folder in time");
            Err(SavedServerOutcome::Unreachable)
        }
    }
}

impl AcceptedEdit {
    /// Installs an accepted edit, AFTER the store holds it: swaps the successor
    /// in through the registry's non-retiring replace, then tells the panes.
    pub fn install(self, manager: &VolumeManager) {
        if let Some(successor) = self.successor {
            match manager.replace_root_in_place(&self.volume_id, &self.checked, successor) {
                RootReplacement::Replaced { .. } => volume_broadcast::emit_volumes_changed(),
                // The place disconnected between the check and now. The store holds
                // the edit and the next connect dials it, so there's no live root to
                // announce.
                RootReplacement::NotRegistered => {
                    log::info!(target: "volume", "{} disconnected while its edit was saved; the next connect uses it", self.volume_id);
                    return;
                }
                // It disconnected AND reconnected: a fresh instance serves it, and the
                // successor was built over the closed connection. Same answer as a
                // disconnect: the next connect dials the stored edit.
                RootReplacement::Superseded => {
                    log::info!(target: "volume", "{} reconnected while its edit was saved; the next connect uses it", self.volume_id);
                    return;
                }
            }
        }
        if let Some(change) = self.root_changed {
            volume_broadcast::emit_volume_root_changed(change);
        }
    }
}

#[cfg(test)]
#[path = "live_server_edit_test.rs"]
mod live_server_edit_test;
