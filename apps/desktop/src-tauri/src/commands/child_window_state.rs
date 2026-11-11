//! Tauri commands for the in-session child-window position cache, and a child window's show and
//! close. See `crate::child_window_state` for the position cache and `crate::child_window_lifecycle`
//! for the show and close.

use tauri::State;

use crate::child_window_state::{ChildWindowRect, ChildWindowRectStore};

/// Close a child window: slide out, hide, then destroy it a moment later. Escape and the in-page
/// close buttons reach it through `closeSelfWindow`; every other close (the red button, ⌘W, a
/// `close()` call) arrives through `CloseRequested` instead. Both land in
/// `crate::child_window_lifecycle::close`, which owns the WebKit-safe teardown.
#[tauri::command]
#[specta::specta]
pub fn close_child_window(app: tauri::AppHandle, label: String) {
    crate::child_window_lifecycle::close(&app, &label);
}

/// Show a child window that was created hidden, sliding it in on macOS (unless Reduce motion is
/// on). Its page calls this once rendered; the opener calls it again after a delay as a fallback,
/// a no-op on a window that's already up. See `crate::child_window_lifecycle::show`.
#[tauri::command]
#[specta::specta]
pub fn show_child_window(app: tauri::AppHandle, label: String) {
    crate::child_window_lifecycle::show(&app, label);
}

/// Returns the saved rect for a child window label, or `None` if no entry
/// exists (first open in this session, or never opened).
#[tauri::command]
#[specta::specta]
pub fn get_child_window_rect(label: String, store: State<'_, ChildWindowRectStore>) -> Option<ChildWindowRect> {
    store.get(&label)
}

/// Saves the rect for a child window label. Called from the frontend's
/// move/resize listeners on the child windows (`trackOwnRect`).
#[tauri::command]
#[specta::specta]
pub fn set_child_window_rect(label: String, rect: ChildWindowRect, store: State<'_, ChildWindowRectStore>) {
    // Mid-slide the window isn't where the user put it.
    if crate::child_window_lifecycle::accepts_rect_write(&label) {
        store.set(label, rect);
    }
}
