//! Dialog state storage for MCP context tools.
//!
//! Stores the current state of open dialogs so MCP tools can query it.

use serde::{Deserialize, Serialize};
use std::sync::RwLock;
use tauri::{AppHandle, Manager};

/// Represents an open file viewer dialog.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileViewerEntry {
    pub path: String,
}

/// State of open dialogs.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct DialogState {
    /// Whether the settings dialog is open
    pub settings_open: bool,
    /// Whether the about dialog is open
    pub about_open: bool,
    /// Whether the volume picker is open
    pub volume_picker_open: bool,
    /// Whether a confirmation dialog is open
    pub confirmation_open: bool,
    /// Open file viewer dialogs (can have multiple)
    pub file_viewers: Vec<FileViewerEntry>,
}

/// Shared state for dialog tracking.
#[derive(Debug, Default)]
pub struct DialogStateStore {
    state: RwLock<DialogState>,
}

impl DialogStateStore {
    pub fn new() -> Self {
        Self {
            state: RwLock::new(DialogState::default()),
        }
    }

    pub fn get(&self) -> DialogState {
        self.state.read().unwrap().clone()
    }

    pub fn set_settings_open(&self, open: bool) {
        self.state.write().unwrap().settings_open = open;
    }

    pub fn set_about_open(&self, open: bool) {
        self.state.write().unwrap().about_open = open;
    }

    pub fn set_volume_picker_open(&self, open: bool) {
        self.state.write().unwrap().volume_picker_open = open;
    }

    pub fn set_confirmation_open(&self, open: bool) {
        self.state.write().unwrap().confirmation_open = open;
    }

    pub fn add_file_viewer(&self, path: String) {
        let mut state = self.state.write().unwrap();
        // Don't add duplicates
        if !state.file_viewers.iter().any(|v| v.path == path) {
            state.file_viewers.push(FileViewerEntry { path });
        }
    }

    pub fn remove_file_viewer(&self, path: &str) {
        let mut state = self.state.write().unwrap();
        state.file_viewers.retain(|v| v.path != path);
    }

    pub fn clear_all_file_viewers(&self) {
        self.state.write().unwrap().file_viewers.clear();
    }
}

/// Tauri command to update dialog state from frontend.
#[tauri::command]
pub fn update_dialog_state(app: AppHandle, dialog_type: String, action: String, path: Option<String>) {
    if let Some(store) = app.try_state::<DialogStateStore>() {
        match (dialog_type.as_str(), action.as_str()) {
            ("settings", "open") => store.set_settings_open(true),
            ("settings", "close") => store.set_settings_open(false),
            ("about", "open") => store.set_about_open(true),
            ("about", "close") => store.set_about_open(false),
            ("volume-picker", "open") => store.set_volume_picker_open(true),
            ("volume-picker", "close") => store.set_volume_picker_open(false),
            ("confirmation", "open") => store.set_confirmation_open(true),
            ("confirmation", "close") => store.set_confirmation_open(false),
            ("file-viewer", "open") => {
                if let Some(p) = path {
                    store.add_file_viewer(p);
                }
            }
            ("file-viewer", "close") => {
                if let Some(p) = path {
                    store.remove_file_viewer(&p);
                }
            }
            ("file-viewer", "close-all") => store.clear_all_file_viewers(),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_state_store() {
        let store = DialogStateStore::new();

        // Initial state should be all closed
        let state = store.get();
        assert!(!state.settings_open);
        assert!(!state.about_open);
        assert!(!state.volume_picker_open);
        assert!(!state.confirmation_open);
        assert!(state.file_viewers.is_empty());

        // Open settings
        store.set_settings_open(true);
        assert!(store.get().settings_open);

        // Close settings
        store.set_settings_open(false);
        assert!(!store.get().settings_open);
    }

    #[test]
    fn test_file_viewers() {
        let store = DialogStateStore::new();

        // Add a file viewer
        store.add_file_viewer("/path/to/file.txt".to_string());
        assert_eq!(store.get().file_viewers.len(), 1);
        assert_eq!(store.get().file_viewers[0].path, "/path/to/file.txt");

        // Don't add duplicates
        store.add_file_viewer("/path/to/file.txt".to_string());
        assert_eq!(store.get().file_viewers.len(), 1);

        // Add another file viewer
        store.add_file_viewer("/path/to/other.txt".to_string());
        assert_eq!(store.get().file_viewers.len(), 2);

        // Remove one
        store.remove_file_viewer("/path/to/file.txt");
        assert_eq!(store.get().file_viewers.len(), 1);
        assert_eq!(store.get().file_viewers[0].path, "/path/to/other.txt");

        // Clear all
        store.add_file_viewer("/path/to/file.txt".to_string());
        store.clear_all_file_viewers();
        assert!(store.get().file_viewers.is_empty());
    }
}
