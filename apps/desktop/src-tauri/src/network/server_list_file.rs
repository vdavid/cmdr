//! Reading and writing a backend's saved-server list.
//!
//! ❗ **A saved-server list is a convenience, ❌ never access to anything.** No
//! secret lives in one (those go to the Keychain, keyed by the backend's own
//! service string), so losing the file costs one re-entry. That is why every
//! failure here is swallowed into an empty store or a log line: a startup that
//! refused to run because a JSON file was truncated would be a far worse trade.
//!
//! Each backend keeps its own in-memory mirror and its own `OnceLock<PathBuf>`;
//! what lives here is only the file handling the two have in common.

use std::fs;
use std::path::{Path, PathBuf};

use serde::Serialize;
use serde::de::DeserializeOwned;

/// Reads `file_name` out of the app data dir into `T`, and hands back the path
/// to write it to.
///
/// ❗ A `.tmp` left by a crash mid-write is swept first: `durable_write_json`
/// writes there and renames, so one still sitting beside the file is a write
/// that never landed and would otherwise be picked up by nothing and cleaned by
/// no one.
///
/// `None` only when the app data dir can't be resolved, which means there is
/// nowhere to persist to at all. A missing or unreadable file is `T::default()`
/// with a live path, so the next write still lands.
pub fn load<R: tauri::Runtime, T: Default + DeserializeOwned>(
    app: &tauri::AppHandle<R>,
    file_name: &str,
) -> Option<(T, PathBuf)> {
    let dir = crate::config::resolved_app_data_dir(app).ok()?;
    let path = dir.join(file_name);
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }
    let store = fs::read_to_string(&path)
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .unwrap_or_default();
    Some((store, path))
}

/// Writes `store` back to `path`, durably (temp + rename).
///
/// `list` names the list in the warning a failure logs, so two backends' lines
/// can't be read as one another's.
pub fn save<T: Serialize>(path: &Path, list: &str, store: &T) {
    let Ok(json) = serde_json::to_string_pretty(store) else {
        return;
    };
    if let Err(e) = crate::config::durable_write_json(path, &path.with_extension("json.tmp"), &json) {
        log::warn!(target: "volume", "couldn't write {list}: {e}");
    }
}
