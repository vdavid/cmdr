//! The presets file: one list in the user's order, cached in memory and backed by
//! `multi-rename-presets.json` in the app data dir. Every change is one locked step
//! and one durable temp+rename write, and a file that won't read is set aside
//! (`crate::state_recovery`), never saved over.
//!
//! The lock discipline is `crate::recents`'s: two locks, never held at once.

use std::path::{Path, PathBuf};
use std::sync::Mutex;

use serde::{Deserialize, Serialize};

use crate::ignore_poison::IgnorePoison;
use crate::state_recovery::StateFile;

use super::presets::MultiRenamePreset;

const FILENAME: &str = "multi-rename-presets.json";
const LOG_TARGET: &str = "multi_rename::presets";

/// The file's format. Version 1 is also what the file was while presets were a
/// recents list (newest first), so an older build's file reads as it is.
const SCHEMA_VERSION: u32 = 1;

/// On-disk shape. Generic over the entries so a write can serialize a borrowed slice.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Envelope<C> {
    #[serde(rename = "_schemaVersion")]
    schema_version: u32,
    #[serde(default)]
    entries: C,
}

/// The presets, in the user's order. Declared once as `presets::PRESETS`.
pub struct PresetStore {
    cached: Mutex<Vec<MultiRenamePreset>>,
    /// Serializes the flush from cache to disk, so two IPC commands landing together
    /// can't clobber each other's write.
    disk: Mutex<()>,
}

impl Default for PresetStore {
    fn default() -> Self {
        Self::new()
    }
}

impl PresetStore {
    pub const fn new() -> Self {
        Self {
            cached: Mutex::new(Vec::new()),
            disk: Mutex::new(()),
        }
    }

    /// A snapshot of the list, top to bottom.
    pub fn entries(&self) -> Vec<MultiRenamePreset> {
        self.cached.lock_ignore_poison().clone()
    }

    pub(crate) fn load_at(&self, path: &Path) {
        // The disk guard is released before the cache guard is taken.
        let loaded = {
            let _disk = self.disk.lock_ignore_poison();
            read(path)
        };
        *self.cached.lock_ignore_poison() = loaded;
    }

    /// Runs `change` on the list as ONE locked step, flushes it when `change` says
    /// it changed something, and answers the list as it is after. A `None` path (the
    /// app data dir didn't resolve) still changes the list in memory.
    pub(crate) fn edit_at(
        &self,
        path: Option<&Path>,
        op: &str,
        change: impl FnOnce(&mut Vec<MultiRenamePreset>) -> bool,
    ) -> Vec<MultiRenamePreset> {
        let (snapshot, changed) = {
            let mut cached = self.cached.lock_ignore_poison();
            let changed = change(&mut cached);
            (cached.clone(), changed)
        };
        if changed && let Some(path) = path {
            let _disk = self.disk.lock_ignore_poison();
            if let Err(e) = write(path, &snapshot) {
                log::warn!(target: LOG_TARGET, "Couldn't write the multi-rename presets after {op}: {e}");
            }
        }
        snapshot
    }

    /// Loads the file into the cache. Call once at startup.
    pub fn load<R: tauri::Runtime>(&self, app: &tauri::AppHandle<R>) {
        if let Some(path) = path(app) {
            self.load_at(&path);
        }
    }

    /// [`Self::edit_at`] on the app data dir's file.
    pub fn edit<R: tauri::Runtime>(
        &self,
        app: &tauri::AppHandle<R>,
        op: &str,
        change: impl FnOnce(&mut Vec<MultiRenamePreset>) -> bool,
    ) -> Vec<MultiRenamePreset> {
        self.edit_at(path(app).as_deref(), op, change)
    }
}

fn path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    crate::config::resolved_app_data_dir(app)
        .ok()
        .map(|dir| dir.join(FILENAME))
}

/// Reads the list at `path`. A file that won't read or parse, or has a schema
/// version we don't know, is set aside and the list starts empty.
fn read(path: &Path) -> Vec<MultiRenamePreset> {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        // A killed write's leftover, so it can't shadow a later one.
        let _ = std::fs::remove_file(&tmp);
    }
    crate::state_recovery::read_state_file(
        path,
        StateFile::MultiRenamePresets,
        |bytes| match serde_json::from_slice::<Envelope<Vec<MultiRenamePreset>>>(bytes) {
            Ok(envelope) if envelope.schema_version == SCHEMA_VERSION => Ok(envelope.entries),
            Ok(envelope) => Err(format!(
                "schema version {}, expected {SCHEMA_VERSION}",
                envelope.schema_version
            )),
            Err(e) => Err(e.to_string()),
        },
    )
    .into_value()
    .unwrap_or_default()
}

fn write(path: &Path, entries: &[MultiRenamePreset]) -> Result<(), crate::state_recovery::StateWriteError> {
    let envelope = Envelope {
        schema_version: SCHEMA_VERSION,
        entries,
    };
    let json = serde_json::to_string_pretty(&envelope).map_err(std::io::Error::other)?;
    crate::state_recovery::write_state_file(path, &json)
}
