//! Resolving what the user attached to a turn, by reference only.
//!
//! Kinds come from live [`PaneStateStore`] entries — the same source the envelope reads —
//! so nothing here stats the filesystem, and an attachment is always path + kind, never
//! contents. "Ask about selection" resolves on the frontend instead, against the backend
//! listing (`file-explorer/pane/selection-attachments-read.ts`): the store holds only each
//! pane's loaded window of rows, so a selection scrolled past it came back short.

use std::collections::HashMap;

use tauri::{AppHandle, Manager};

use super::{AttachmentKindView, AttachmentRef};
use crate::mcp::PaneStateStore;

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
