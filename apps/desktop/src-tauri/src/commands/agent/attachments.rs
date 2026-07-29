//! Resolving what the user attached to a turn, by reference only.
//!
//! Kinds come from live [`PaneStateStore`] entries — the same source the envelope reads —
//! so nothing here stats the filesystem, and an attachment is always path + kind, never
//! contents.

use std::collections::HashMap;

use tauri::{AppHandle, Manager};

use super::{AttachmentKindView, AttachmentRef};
use crate::mcp::PaneStateStore;

/// "Ask about selection": attachment refs for the focused pane's current selection, or
/// its cursor item when nothing is selected. Reads [`PaneStateStore`] — the same live
/// source the envelope uses — so kinds come from known pane state, with no filesystem
/// stat. Empty when no pane state is registered.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_selection_attachments(app: AppHandle) -> Vec<AttachmentRef> {
    let Some(store) = app.try_state::<PaneStateStore>() else {
        return Vec::new();
    };
    let pane = if store.get_focused_pane() == "right" {
        store.get_right()
    } else {
        store.get_left()
    };
    let indices = if pane.selected_indices.is_empty() {
        vec![pane.cursor_index]
    } else {
        pane.selected_indices.clone()
    };
    indices
        .into_iter()
        .filter_map(|i| pane.files.get(i))
        .filter(|entry| !entry.path.is_empty())
        .map(pane_entry_to_attachment)
        .collect()
}

/// Resolve dragged local paths into typed attachment refs. Kinds come from the known
/// pane files (left + right) — no filesystem stat — defaulting to `File` for an unknown
/// path. The frontend only calls this for LOCAL drags; virtual-volume drag paths
/// mis-resolve after the pasteboard round-trip and are not supported in v1.
#[tauri::command]
#[specta::specta]
pub fn ask_cmdr_resolve_attachments(app: AppHandle, paths: Vec<String>) -> Vec<AttachmentRef> {
    let mut is_dir_by_path: HashMap<String, bool> = HashMap::new();
    if let Some(store) = app.try_state::<PaneStateStore>() {
        for pane in [store.get_left(), store.get_right()] {
            for entry in pane.files {
                is_dir_by_path.insert(entry.path, entry.is_directory);
            }
        }
    }
    paths
        .into_iter()
        .filter(|path| !path.is_empty())
        .map(|path| {
            let is_dir = is_dir_by_path.get(&path).copied().unwrap_or(false);
            AttachmentRef {
                kind: if is_dir {
                    AttachmentKindView::Folder
                } else {
                    AttachmentKindView::File
                },
                path,
            }
        })
        .collect()
}

/// Map a known pane file entry to an attachment ref (kind straight from `is_directory`).
fn pane_entry_to_attachment(entry: &crate::mcp::pane_state::PaneFileEntry) -> AttachmentRef {
    AttachmentRef {
        path: entry.path.clone(),
        kind: if entry.is_directory {
            AttachmentKindView::Folder
        } else {
            AttachmentKindView::File
        },
    }
}
