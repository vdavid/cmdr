//! Soft dialog tracking for MCP context tools.
//!
//! Tracks in-page overlay dialogs (about, license, copy-confirmation, etc.).
//! Window-based dialogs (settings, file viewers) are derived from Tauri's window manager
//! in resources.rs — no manual tracking needed for those.
//!
//! The frontend registers all known soft dialog IDs at startup via
//! `register_known_dialogs`, so the MCP "available dialogs" resource
//! stays in sync with the actual Svelte components automatically.

use std::collections::HashSet;
use std::sync::RwLock;
use serde::Deserialize;
use tauri::{AppHandle, Manager};

/// A dialog type registered by the frontend at startup.
#[derive(Debug, Clone, Deserialize)]
pub struct KnownDialog {
    pub id: String,
    pub description: Option<String>,
}

/// Tracks which soft (overlay) dialogs are currently open,
/// and which dialog types are known (registered at startup).
#[derive(Debug, Default)]
pub struct SoftDialogTracker {
    open: RwLock<HashSet<String>>,
    known: RwLock<Vec<KnownDialog>>,
}

impl SoftDialogTracker {
    pub fn new() -> Self {
        Self {
            open: RwLock::new(HashSet::new()),
            known: RwLock::new(Vec::new()),
        }
    }

    pub fn open(&self, dialog_type: String) {
        self.open.write().unwrap().insert(dialog_type);
    }

    pub fn close(&self, dialog_type: &str) {
        self.open.write().unwrap().remove(dialog_type);
    }

    pub fn get_open_types(&self) -> Vec<String> {
        self.open.read().unwrap().iter().cloned().collect()
    }

    pub fn register_known(&self, dialogs: Vec<KnownDialog>) {
        *self.known.write().unwrap() = dialogs;
    }

    pub fn get_known_dialogs(&self) -> Vec<KnownDialog> {
        self.known.read().unwrap().clone()
    }
}

/// Tauri command: frontend notifies that a soft dialog opened.
#[tauri::command]
pub fn notify_dialog_opened(app: AppHandle, dialog_type: String) {
    if let Some(tracker) = app.try_state::<SoftDialogTracker>() {
        SoftDialogTracker::open(&tracker, dialog_type);
    }
}

/// Tauri command: frontend notifies that a soft dialog closed.
#[tauri::command]
pub fn notify_dialog_closed(app: AppHandle, dialog_type: String) {
    if let Some(tracker) = app.try_state::<SoftDialogTracker>() {
        SoftDialogTracker::close(&tracker, &dialog_type);
    }
}

/// Tauri command: frontend registers all known soft dialog types at startup.
#[tauri::command]
pub fn register_known_dialogs(app: AppHandle, dialogs: Vec<KnownDialog>) {
    if let Some(tracker) = app.try_state::<SoftDialogTracker>() {
        tracker.register_known(dialogs);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_soft_dialog_tracker() {
        let tracker = SoftDialogTracker::new();
        assert!(tracker.get_open_types().is_empty());

        tracker.open("about".to_string());
        assert_eq!(tracker.get_open_types().len(), 1);
        assert!(tracker.get_open_types().contains(&"about".to_string()));

        tracker.open("copy-confirmation".to_string());
        assert_eq!(tracker.get_open_types().len(), 2);

        tracker.close("about");
        assert_eq!(tracker.get_open_types().len(), 1);
        assert!(tracker.get_open_types().contains(&"copy-confirmation".to_string()));
    }

    #[test]
    fn test_duplicate_open_is_idempotent() {
        let tracker = SoftDialogTracker::new();

        tracker.open("about".to_string());
        tracker.open("about".to_string());
        assert_eq!(tracker.get_open_types().len(), 1);
    }

    #[test]
    fn test_close_nonexistent_is_safe() {
        let tracker = SoftDialogTracker::new();
        tracker.close("nonexistent"); // Should not panic
        assert!(tracker.get_open_types().is_empty());
    }

    #[test]
    fn test_register_known_dialogs() {
        let tracker = SoftDialogTracker::new();
        assert!(tracker.get_known_dialogs().is_empty());

        let dialogs = vec![
            KnownDialog { id: "about".to_string(), description: None },
            KnownDialog { id: "alert".to_string(), description: None },
            KnownDialog {
                id: "copy-confirmation".to_string(),
                description: Some("Opened by the copy tool".to_string()),
            },
        ];
        tracker.register_known(dialogs);

        let known = tracker.get_known_dialogs();
        assert_eq!(known.len(), 3);
        assert_eq!(known[0].id, "about");
        assert_eq!(known[2].description.as_deref(), Some("Opened by the copy tool"));
    }

    #[test]
    fn test_register_known_replaces_previous() {
        let tracker = SoftDialogTracker::new();

        tracker.register_known(vec![
            KnownDialog { id: "about".to_string(), description: None },
        ]);
        assert_eq!(tracker.get_known_dialogs().len(), 1);

        tracker.register_known(vec![
            KnownDialog { id: "about".to_string(), description: None },
            KnownDialog { id: "alert".to_string(), description: None },
        ]);
        assert_eq!(tracker.get_known_dialogs().len(), 2);
    }
}
