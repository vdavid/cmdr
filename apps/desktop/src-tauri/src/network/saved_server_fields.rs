//! The fields a saved SFTP or WebDAV server carries beside its identity, and the
//! rules both stores share for them.
//!
//! ❗ **A start folder sits at or under the remote root, compared by whole path
//! components.** The root is a CEILING nothing navigates above
//! (`RemoteRoot::to_remote_path` refuses), so a start folder outside it names a
//! place no pane could stand on. ❌ Never a string prefix: `/srv/data-1` is not
//! under `/srv/data`, however much of it it spells.

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

#[cfg(test)]
#[path = "saved_server_fields_test.rs"]
mod saved_server_fields_test;
