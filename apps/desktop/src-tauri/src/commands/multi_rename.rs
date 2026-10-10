//! IPC for the Multi-Rename Tool (⌃M). Thin: the work is `crate::multi_rename::session` and `run`.

use std::sync::Arc;

use tokio::time::Duration;

use crate::deadline::{BlockingBudget, blocking_typed_result_with_timeout, timeout_detached_typed};
use crate::multi_rename::error::MultiRenameError;
use crate::multi_rename::history::{FIELD_HISTORY, FieldHistoryEntry, MAX_FIELD_HISTORY, history_entries};
use crate::multi_rename::plan::{MultiRenameSpec, PreviewRow, RenameExample, render_examples};
use crate::multi_rename::presets::{
    LAST_SPEC, LastSpec, LoadedPreset, MAX_PRESETS, MultiRenameLastSettings, MultiRenamePreset, PRESETS, rename_in,
    update_spec_in,
};
use crate::multi_rename::run::{MultiRenameStarted, apply};
use crate::multi_rename::session::{self, MultiRenameOpened, MultiRenamePreview, PreviewFilter};

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

/// Rows `offset..offset + limit` of preview `preview_id` (or of its problem rows
/// alone), for the list's window. `previewOutOfDate` when a newer preview replaced it.
#[tauri::command]
#[specta::specta]
pub async fn get_multi_rename_preview_rows(
    session_id: String,
    preview_id: u64,
    offset: usize,
    limit: usize,
    filter: PreviewFilter,
) -> Result<Vec<PreviewRow>, MultiRenameError> {
    session::page(&session_id, preview_id, offset, limit, filter)
}

/// Each example's new name for the sheet's tooltips, rendered on made-up files
/// by the same engine as the preview; `None` for a spec that doesn't run.
#[tauri::command]
#[specta::specta]
pub async fn render_multi_rename_examples(examples: Vec<RenameExample>) -> Vec<Option<String>> {
    render_examples(&examples)
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
    let events = Arc::new(crate::file_system::write_operations::TauriEventSink::new(app.clone()));
    // Read before apply: the spec of the preview it starts from, for the fields' history.
    let spec = session::spec_of(&session_id, preview_id).ok();
    let started = apply(events, session_id, preview_id).await?;
    // What the fields held when a rename ran: TC's per-field history (↓ in a field).
    for entry in spec.as_ref().map(history_entries).unwrap_or_default() {
        FIELD_HISTORY.add(&app, entry, MAX_FIELD_HISTORY);
    }
    Ok(started)
}

/// The text fields' history, newest first, every field together.
#[tauri::command]
#[specta::specta]
pub fn get_multi_rename_history() -> Vec<FieldHistoryEntry> {
    FIELD_HISTORY.entries(None)
}

/// Results (⌥⏎): writes preview `preview_id`'s rows as `old<TAB>new` lines to a
/// text file and returns its path, for the user's editor.
#[tauri::command]
#[specta::specta]
pub async fn write_multi_rename_names(session_id: String, preview_id: u64) -> Result<String, MultiRenameError> {
    blocking_typed_result_with_timeout(
        Duration::from_secs(5),
        || MultiRenameError::TimedOut,
        |detail| MultiRenameError::Internal { detail },
        move || session::write_names(&session_id, preview_id).map(|path| path.to_string_lossy().into_owned()),
    )
    .await
}

/// Reads the session's Results file back. Returns how many rows now carry a name
/// the user typed; the next preview shows them.
#[tauri::command]
#[specta::specta]
pub async fn read_multi_rename_names(session_id: String) -> Result<usize, MultiRenameError> {
    blocking_typed_result_with_timeout(
        Duration::from_secs(2),
        || MultiRenameError::TimedOut,
        |detail| MultiRenameError::Internal { detail },
        move || session::read_names(&session_id),
    )
    .await
}

/// Drops the names typed in Results, and its file: every row follows the settings again.
#[tauri::command]
#[specta::specta]
pub async fn clear_multi_rename_names(session_id: String) -> Result<(), MultiRenameError> {
    blocking_typed_result_with_timeout(
        Duration::from_secs(2),
        || MultiRenameError::TimedOut,
        |detail| MultiRenameError::Internal { detail },
        move || session::clear_names(&session_id),
    )
    .await
}

/// Ends the session when the sheet closes. No-op when it's already gone.
#[tauri::command]
#[specta::specta]
pub async fn close_multi_rename(session_id: String) {
    session::close(&session_id);
}

/// The settings the sheet last closed with, if any.
#[tauri::command]
#[specta::specta]
pub fn get_multi_rename_last_settings() -> Option<MultiRenameLastSettings> {
    LAST_SPEC.entries(Some(1)).into_iter().next().map(|last| last.settings)
}

/// Remembers the settings the sheet closes with, and the preset they came from,
/// for the next ⌃M.
#[tauri::command]
#[specta::specta]
pub fn save_multi_rename_last_settings(app: tauri::AppHandle, spec: MultiRenameSpec, preset: Option<LoadedPreset>) {
    LAST_SPEC.add(&app, LastSpec::new(spec, preset), 1);
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

/// Renames a preset in place; another preset with that name is replaced (the sheet
/// asks first). No-op for an unknown id or an empty name.
#[tauri::command]
#[specta::specta]
pub fn rename_multi_rename_preset(app: tauri::AppHandle, id: String, name: String) {
    PRESETS.edit(&app, "a rename", |presets| rename_in(presets, &id, &name));
}

/// Gives a preset new settings in place, so the menu's numbers don't move. No-op
/// for an unknown id.
#[tauri::command]
#[specta::specta]
pub fn update_multi_rename_preset(app: tauri::AppHandle, id: String, spec: MultiRenameSpec) {
    PRESETS.edit(&app, "an update", |presets| update_spec_in(presets, &id, &spec));
}
