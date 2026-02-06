//! Soft dialog tracking for MCP context tools.
//!
//! Tracks in-page overlay dialogs (about, license, copy-confirmation, mkdir-confirmation).
//! Window-based dialogs (settings, file viewers) are derived from Tauri's window manager
//! in resources.rs — no manual tracking needed for those.

use std::collections::HashSet;
use std::sync::RwLock;
use tauri::{AppHandle, Manager};

/// Tracks which soft (overlay) dialogs are currently open.
/// Uses a simple set of dialog type strings.
#[derive(Debug, Default)]
pub struct SoftDialogTracker {
    open: RwLock<HashSet<String>>,
}

impl SoftDialogTracker {
    pub fn new() -> Self {
        Self {
            open: RwLock::new(HashSet::new()),
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
}
