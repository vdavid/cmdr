//! Tauri commands for the in-session child-window position cache. See
//! `crate::child_window_state` for the design.

use tauri::State;

use crate::child_window_state::{ChildWindowRect, ChildWindowRectStore};

/// Returns the saved rect for a child window label, or `None` if no entry
/// exists (first open in this session, or never opened).
#[tauri::command]
#[specta::specta]
pub fn get_child_window_rect(label: String, store: State<'_, ChildWindowRectStore>) -> Option<ChildWindowRect> {
    store.get(&label)
}

/// Saves the rect for a child window label. Called from the frontend's
/// move/resize listeners on Settings and Debug.
#[tauri::command]
#[specta::specta]
pub fn set_child_window_rect(label: String, rect: ChildWindowRect, store: State<'_, ChildWindowRectStore>) {
    store.set(label, rect);
}
