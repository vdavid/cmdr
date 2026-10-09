//! IPC for the Multi-Rename Tool (⌃M). Thin: the work is `crate::multi_rename::session` and `run`.

use std::sync::Arc;

use tokio::time::Duration;

use crate::deadline::{BlockingBudget, blocking_typed_result_with_timeout, timeout_detached_typed};
use crate::multi_rename::error::MultiRenameError;
use crate::multi_rename::plan::{MultiRenameSpec, PreviewRow};
use crate::multi_rename::presets::{MAX_PRESETS, MultiRenamePreset, PRESETS};
use crate::multi_rename::run::{MultiRenameStarted, apply};
use crate::multi_rename::session::{self, MultiRenameOpened, MultiRenamePreview};

/// Opens a session over the pane's selection: `selected_indices` are backend row
/// numbers in rename order (`None` for every row the pane shows), read at
/// `expected_sequence`. The files are resolved once, here; `selectionChanged`
/// when the rows aren't the listing's state any more.
#[tauri::command]
#[specta::specta]
pub async fn open_multi_rename(
    listing_id: String,
    include_hidden: bool,
    selected_indices: Option<Vec<usize>>,
    expected_sequence: u64,
) -> Result<MultiRenameOpened, MultiRenameError> {
    blocking_typed_result_with_timeout(
        Duration::from_secs(2),
        || MultiRenameError::TimedOut,
        |detail| MultiRenameError::Internal { detail },
        move || {
            session::open(
                &listing_id,
                include_hidden,
                selected_indices.as_deref(),
                expected_sequence,
            )
        },
    )
    .await
}

/// The previews the sheet re-issues on every edit (120 ms apart), which on a
/// huge folder take longer than that: two at a time, the rest wait their turn.
static PREVIEWS: BlockingBudget = BlockingBudget::new(2);

/// The live preview of the session's files: the counts and the first rows.
#[tauri::command]
#[specta::specta]
pub async fn preview_multi_rename(
    session_id: String,
    spec: MultiRenameSpec,
) -> Result<MultiRenamePreview, MultiRenameError> {
    // Off the IPC thread: a big folder is a mask and a regex per row.
    timeout_detached_typed(
        Duration::from_secs(5),
        || MultiRenameError::TimedOut,
        |detail| MultiRenameError::Internal { detail },
        async move {
            PREVIEWS
                .run(move || session::preview_session(&session_id, &spec))
                .await
                .map_err(|e| MultiRenameError::Internal { detail: e.to_string() })?
        },
    )
    .await
}

/// Rows `offset..offset + limit` of preview `preview_id`, for the table's window.
/// `previewOutOfDate` when a newer preview replaced it.
#[tauri::command]
#[specta::specta]
pub async fn get_multi_rename_preview_rows(
    session_id: String,
    preview_id: u64,
    offset: usize,
    limit: usize,
) -> Result<Vec<PreviewRow>, MultiRenameError> {
    session::page(&session_id, preview_id, offset, limit)
}

/// Renames the rows preview `preview_id` showed as ready, as one operation the
/// queue shows and Undo reverses. Refuses with `previewOutOfDate` when the folder
/// changed since that preview.
#[tauri::command]
#[specta::specta]
pub async fn apply_multi_rename(
    app: tauri::AppHandle,
    session_id: String,
    preview_id: u64,
) -> Result<MultiRenameStarted, MultiRenameError> {
    let events = Arc::new(crate::file_system::write_operations::TauriEventSink::new(app));
    apply(events, session_id, preview_id).await
}

/// Ends the session when the sheet closes. No-op when it's already gone.
#[tauri::command]
#[specta::specta]
pub async fn close_multi_rename(session_id: String) {
    session::close(&session_id);
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
