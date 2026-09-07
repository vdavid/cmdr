//! The one IPC command the macOS Services menu needs: what it acts on right now.
//!
//! Thin over `crate::services_menu::selection`, which owns the store and Finder's
//! cursor-vs-selection rule.

/// Pushes what `Cmdr > Services` should act on: the focused pane's selection, or
/// its cursor row when nothing is selected.
///
/// Sync on purpose. It's a value store swap, no I/O and no AppKit, and it runs on
/// every selection change, so an `async` hop would cost more than the work.
///
/// Separate from `update_menu_context` because the two answer different questions
/// at different moments. That one is the RIGHT-CLICKED row and every context menu
/// overwrites it; this one is what is selected RIGHT NOW, which is what AppKit asks
/// for whenever the user opens the Services submenu.
#[tauri::command]
#[specta::specta]
pub fn update_services_selection(selection: crate::services_menu::ServicesSelection) {
    crate::services_menu::selection::set(selection);
}
