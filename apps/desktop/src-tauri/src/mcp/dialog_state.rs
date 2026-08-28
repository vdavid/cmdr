//! Soft dialog tracking for MCP context tools.
//!
//! Tracks in-page overlay dialogs (about, license, transfer-confirmation, etc.).
//! Window-based dialogs (settings, file viewers) are derived from Tauri's window manager
//! in `resources/mod.rs`; no manual tracking needed for those.
//!
//! The frontend registers all known soft dialog IDs at startup via
//! `register_known_dialogs`, so the MCP "available dialogs" resource
//! stays in sync with the actual Svelte components automatically.

use crate::ignore_poison::RwLockIgnorePoison;
use serde::Deserialize;
use std::sync::RwLock;
use tauri::{AppHandle, Manager};

/// A dialog type registered by the frontend at startup.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KnownDialog {
    pub id: String,
    pub description: Option<String>,
    /// Whether an MCP tool that would START a file operation is refused while this
    /// dialog is open. Declared per dialog in the frontend's `dialog-registry.ts`,
    /// which is where a new dialog's author is forced to answer the question; this
    /// side only carries the answer across.
    pub blocks_operations: bool,
}

/// Tracks which soft (overlay) dialogs are currently open,
/// and which dialog types are known (registered at startup).
///
/// `open` is a `Vec` rather than a set so it keeps MOUNT ORDER: dialogs stack (a
/// rollback confirmation over the progress dialog, the quit countdown over
/// anything), and `blocking_dialog` has to name the topmost one, which is the one
/// that actually has to go.
#[derive(Debug, Default)]
pub struct SoftDialogTracker {
    open: RwLock<Vec<String>>,
    known: RwLock<Vec<KnownDialog>>,
}

impl SoftDialogTracker {
    pub fn new() -> Self {
        Self {
            open: RwLock::new(Vec::new()),
            known: RwLock::new(Vec::new()),
        }
    }

    pub fn open(&self, dialog_type: String) {
        let mut open = self.open.write_ignore_poison();
        if !open.contains(&dialog_type) {
            open.push(dialog_type);
        }
    }

    pub fn close(&self, dialog_type: &str) {
        self.open.write_ignore_poison().retain(|d| d != dialog_type);
    }

    pub fn get_open_types(&self) -> Vec<String> {
        self.open.read_ignore_poison().clone()
    }

    /// Drops every open dialog. The frontend calls this through
    /// `register_known_dialogs` on startup: a reload (or a crashed webview) leaves
    /// dialogs recorded that nothing will ever close, and a stuck entry would
    /// refuse every MCP file operation for the rest of the session.
    pub fn forget_open(&self) {
        self.open.write_ignore_poison().clear();
    }

    /// The open dialog standing in the way of starting a file operation, topmost
    /// first, or `None` when nothing is.
    ///
    /// An open dialog the frontend never registered counts as blocking: the
    /// conservative read is the safe one, and it can only happen while startup
    /// registration is still in flight.
    pub fn blocking_dialog(&self) -> Option<String> {
        let known = self.known.read_ignore_poison();
        self.open
            .read_ignore_poison()
            .iter()
            .rev()
            .find(|id| known.iter().find(|k| &&k.id == id).is_none_or(|k| k.blocks_operations))
            .cloned()
    }

    pub fn register_known(&self, dialogs: Vec<KnownDialog>) {
        *self.known.write_ignore_poison() = dialogs;
    }

    pub fn get_known_dialogs(&self) -> Vec<KnownDialog> {
        self.known.read_ignore_poison().clone()
    }
}

/// Tauri command: frontend notifies that a soft dialog opened.
#[tauri::command]
#[specta::specta]
pub fn notify_dialog_opened(app: AppHandle, dialog_type: String) {
    if let Some(tracker) = app.try_state::<SoftDialogTracker>() {
        SoftDialogTracker::open(&tracker, dialog_type);
    }
}

/// Tauri command: frontend notifies that a soft dialog closed.
#[tauri::command]
#[specta::specta]
pub fn notify_dialog_closed(app: AppHandle, dialog_type: String) {
    if let Some(tracker) = app.try_state::<SoftDialogTracker>() {
        SoftDialogTracker::close(&tracker, &dialog_type);
    }
}

/// Tauri command: frontend registers all known soft dialog types at startup.
#[tauri::command]
#[specta::specta]
pub fn register_known_dialogs(app: AppHandle, dialogs: Vec<KnownDialog>) {
    if let Some(tracker) = app.try_state::<SoftDialogTracker>() {
        // Startup means nothing is on screen yet. Clearing here is what stops a
        // reloaded webview's orphaned entry from blocking file operations forever.
        tracker.forget_open();
        tracker.register_known(dialogs);
    }
    // The archive-password mirror is the same shape of orphan: a reload never
    // fires the dismiss half, and a stale prompt would let `unlock_archive`
    // answer a question nobody is asking.
    if let Some(store) = app.try_state::<crate::mcp::ArchivePasswordPromptStore>() {
        store.clear();
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

        tracker.open("transfer-confirmation".to_string());
        assert_eq!(tracker.get_open_types().len(), 2);

        tracker.close("about");
        assert_eq!(tracker.get_open_types().len(), 1);
        assert!(tracker.get_open_types().contains(&"transfer-confirmation".to_string()));
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

    /// A registered dialog with the given gate verdict.
    fn known(id: &str, blocks_operations: bool) -> KnownDialog {
        KnownDialog {
            id: id.to_string(),
            description: None,
            blocks_operations,
        }
    }

    #[test]
    fn test_register_known_dialogs() {
        let tracker = SoftDialogTracker::new();
        assert!(tracker.get_known_dialogs().is_empty());

        let dialogs = vec![
            known("about", true),
            known("alert", true),
            KnownDialog {
                id: "transfer-confirmation".to_string(),
                description: Some("Opened by the copy tool".to_string()),
                blocks_operations: true,
            },
        ];
        tracker.register_known(dialogs);

        let known_dialogs = tracker.get_known_dialogs();
        assert_eq!(known_dialogs.len(), 3);
        assert_eq!(known_dialogs[0].id, "about");
        assert_eq!(known_dialogs[2].description.as_deref(), Some("Opened by the copy tool"));
    }

    #[test]
    fn test_register_known_replaces_previous() {
        let tracker = SoftDialogTracker::new();

        tracker.register_known(vec![known("about", true)]);
        assert_eq!(tracker.get_known_dialogs().len(), 1);

        tracker.register_known(vec![known("about", true), known("alert", true)]);
        assert_eq!(tracker.get_known_dialogs().len(), 2);
    }

    #[test]
    fn blocking_dialog_is_none_when_nothing_is_open() {
        let tracker = SoftDialogTracker::new();
        tracker.register_known(vec![known("transfer-progress", true)]);

        assert_eq!(tracker.blocking_dialog(), None);
    }

    #[test]
    fn blocking_dialog_names_an_open_blocker() {
        let tracker = SoftDialogTracker::new();
        tracker.register_known(vec![known("transfer-progress", true)]);
        tracker.open("transfer-progress".to_string());

        assert_eq!(tracker.blocking_dialog().as_deref(), Some("transfer-progress"));
    }

    #[test]
    fn blocking_dialog_ignores_one_that_lets_operations_through() {
        // The viewer's own sheets: another window's decision, and the main window
        // has no modal up. Refusing a copy over one would be busywork.
        let tracker = SoftDialogTracker::new();
        tracker.register_known(vec![known("viewer-copy-confirm", false)]);
        tracker.open("viewer-copy-confirm".to_string());

        assert_eq!(tracker.blocking_dialog(), None);
    }

    #[test]
    fn blocking_dialog_names_the_topmost_of_a_stack() {
        // Closing the progress dialog underneath isn't what clears the way.
        let tracker = SoftDialogTracker::new();
        tracker.register_known(vec![
            known("transfer-progress", true),
            known("rollback-confirmation", true),
        ]);
        tracker.open("transfer-progress".to_string());
        tracker.open("rollback-confirmation".to_string());

        assert_eq!(tracker.blocking_dialog().as_deref(), Some("rollback-confirmation"));
    }

    #[test]
    fn an_unregistered_open_dialog_counts_as_blocking() {
        // Only reachable while startup registration is in flight. Refusing is the
        // safe read; letting an operation through on missing information isn't.
        let tracker = SoftDialogTracker::new();
        tracker.open("something-nobody-registered".to_string());

        assert_eq!(
            tracker.blocking_dialog().as_deref(),
            Some("something-nobody-registered")
        );
    }

    #[test]
    fn registering_at_startup_forgets_dialogs_a_reload_left_behind() {
        // A webview reload never fires the close half of the pair, and a stuck
        // entry would refuse every MCP file operation for the rest of the session.
        let tracker = SoftDialogTracker::new();
        tracker.open("transfer-progress".to_string());

        tracker.forget_open();
        tracker.register_known(vec![known("transfer-progress", true)]);

        assert!(tracker.get_open_types().is_empty());
        assert_eq!(tracker.blocking_dialog(), None);
    }
}
