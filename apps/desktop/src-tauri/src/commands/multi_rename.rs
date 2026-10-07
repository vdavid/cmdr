//! IPC for the Multi-Rename Tool (⌃M). Thin: the work is `crate::multi_rename::run`.

use std::sync::Arc;

use tokio::time::Duration;

use crate::deadline::blocking_typed_result_with_timeout;
use crate::multi_rename::plan::{MultiRenameSpec, PreviewRow};
use crate::multi_rename::presets::{MAX_PRESETS, MultiRenamePreset, PRESETS};
use crate::multi_rename::run::{ExpectedRename, MultiRenameError, MultiRenameStarted, apply, preview_rows};

/// The live preview: each row's new name and whether it can take it. `rows` are
/// backend row numbers in rename order; `None` previews every row the pane shows.
#[tauri::command]
#[specta::specta]
pub async fn preview_multi_rename(
    listing_id: String,
    include_hidden: bool,
    rows: Option<Vec<usize>>,
    spec: MultiRenameSpec,
) -> Result<Vec<PreviewRow>, MultiRenameError> {
    // Off the IPC thread: a big folder is a mask and a regex per row.
    blocking_typed_result_with_timeout(
        Duration::from_secs(5),
        || MultiRenameError::TimedOut,
        |detail| MultiRenameError::Internal { detail },
        move || preview_rows(&listing_id, include_hidden, rows.as_deref(), &spec),
    )
    .await
}

/// Renames the rows the user saw as ready (`expected`, from the preview they
/// started from), as one operation the queue shows and Undo reverses. Refuses
/// with `previewOutOfDate` when the folder changed since that preview.
#[tauri::command]
#[specta::specta]
pub async fn apply_multi_rename(
    app: tauri::AppHandle,
    listing_id: String,
    include_hidden: bool,
    rows: Option<Vec<usize>>,
    spec: MultiRenameSpec,
    expected: Vec<ExpectedRename>,
) -> Result<MultiRenameStarted, MultiRenameError> {
    let events = Arc::new(crate::file_system::write_operations::TauriEventSink::new(app));
    apply(events, listing_id, include_hidden, rows, spec, expected).await
}

/// The saved presets, newest first.
#[tauri::command]
#[specta::specta]
pub fn get_multi_rename_presets() -> Vec<MultiRenamePreset> {
    PRESETS.entries(None)
}

/// Saves a preset; one with the same name is replaced.
#[tauri::command]
#[specta::specta]
pub fn save_multi_rename_preset(app: tauri::AppHandle, preset: MultiRenamePreset) {
    PRESETS.add(&app, preset, MAX_PRESETS);
}

/// Deletes a preset by id. No-op when it isn't there.
#[tauri::command]
#[specta::specta]
pub fn delete_multi_rename_preset(app: tauri::AppHandle, id: String) {
    PRESETS.remove(&app, &id);
}
