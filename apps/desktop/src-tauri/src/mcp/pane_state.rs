//! Pane state storage for MCP context tools.
//!
//! Stores the current state of both panes so MCP tools can access it.

use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use tauri::{AppHandle, Manager};

/// Represents a file entry in a pane.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileEntry {
    pub name: String,
    pub path: String,
    pub is_directory: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub modified: Option<String>,
}

/// State of a single pane.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PaneState {
    /// Current directory path
    pub path: String,
    /// Volume ID (if applicable)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_id: Option<String>,
    /// Volume name for display
    #[serde(skip_serializing_if = "Option::is_none")]
    pub volume_name: Option<String>,
    /// Currently visible files
    pub files: Vec<FileEntry>,
    /// Index of the file under the cursor (0-based)
    pub cursor_index: usize,
    /// View mode (brief or full)
    pub view_mode: String,
    /// Indices of selected files
    #[serde(default)]
    pub selected_indices: Vec<usize>,
    /// Sort field (name, size, date, extension)
    #[serde(default)]
    pub sort_field: String,
    /// Sort order (asc or desc)
    #[serde(default)]
    pub sort_order: String,
    /// Total number of files in directory
    #[serde(default)]
    pub total_files: usize,
    /// Start index of loaded range
    #[serde(default)]
    pub loaded_start: usize,
    /// End index of loaded range
    #[serde(default)]
    pub loaded_end: usize,
    /// Whether hidden files are shown
    #[serde(default)]
    pub show_hidden: bool,
}

/// Shared state for both panes.
#[derive(Debug, Default)]
pub struct PaneStateStore {
    pub left: RwLock<PaneState>,
    pub right: RwLock<PaneState>,
    pub focused_pane: RwLock<String>,
}

impl PaneStateStore {
    pub fn new() -> Self {
        Self {
            left: RwLock::new(PaneState::default()),
            right: RwLock::new(PaneState::default()),
            focused_pane: RwLock::new("left".to_string()),
        }
    }

    pub fn get_left(&self) -> PaneState {
        self.left.read().unwrap().clone()
    }

    pub fn get_right(&self) -> PaneState {
        self.right.read().unwrap().clone()
    }

    pub fn get_focused_pane(&self) -> String {
        self.focused_pane.read().unwrap().clone()
    }

    pub fn set_left(&self, state: PaneState) {
        *self.left.write().unwrap() = state;
    }

    pub fn set_right(&self, state: PaneState) {
        *self.right.write().unwrap() = state;
    }

    pub fn set_focused_pane(&self, pane: String) {
        *self.focused_pane.write().unwrap() = pane;
    }
}

/// Tauri command to update left pane state from frontend.
#[tauri::command]
pub fn update_left_pane_state(app: AppHandle, state: PaneState) {
    if let Some(store) = app.try_state::<PaneStateStore>() {
        store.set_left(state);
    }
}

/// Tauri command to update right pane state from frontend.
#[tauri::command]
pub fn update_right_pane_state(app: AppHandle, state: PaneState) {
    if let Some(store) = app.try_state::<PaneStateStore>() {
        store.set_right(state);
    }
}

/// Tauri command to update focused pane from frontend.
#[tauri::command]
pub fn update_focused_pane(app: AppHandle, pane: String) {
    if let Some(store) = app.try_state::<PaneStateStore>() {
        store.set_focused_pane(pane);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pane_state_store() {
        let store = PaneStateStore::new();

        let state = PaneState {
            path: "/tmp".to_string(),
            volume_id: None,
            volume_name: None,
            files: vec![FileEntry {
                name: "test.txt".to_string(),
                path: "/tmp/test.txt".to_string(),
                is_directory: false,
                size: Some(100),
                modified: None,
            }],
            cursor_index: 0,
            view_mode: "brief".to_string(),
            selected_indices: vec![],
            sort_field: "name".to_string(),
            sort_order: "asc".to_string(),
            total_files: 1,
            loaded_start: 0,
            loaded_end: 1,
            show_hidden: false,
        };

        store.set_left(state.clone());
        let retrieved = store.get_left();

        assert_eq!(retrieved.path, "/tmp");
        assert_eq!(retrieved.files.len(), 1);
        assert_eq!(retrieved.files[0].name, "test.txt");
    }

    #[test]
    fn test_focused_pane() {
        let store = PaneStateStore::new();

        assert_eq!(store.get_focused_pane(), "left");

        store.set_focused_pane("right".to_string());
        assert_eq!(store.get_focused_pane(), "right");
    }
}
