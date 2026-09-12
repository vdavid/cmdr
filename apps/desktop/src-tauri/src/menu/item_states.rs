//! Which main-menu items are enabled right now, and which of the two macOS app-level menu bars
//! (main vs. viewer) is installed.
//!
//! [`apply_menu_item_states`] is the single writer of every main-menu item's enabled state,
//! recomputed from every [`MenuState`] input at once so no writer can undo another's verdict by
//! running after it. [`set_menu_context`] and the macOS-only `swap_to_main_menu` /
//! `swap_to_viewer_menu` decide which window's menu is installed and which items that leaves in
//! scope; both are called from `commands::menu::activate_window_menu` on focus-gain.

use std::collections::HashSet;
use std::sync::atomic::Ordering;

use tauri::{AppHandle, Manager, Runtime};

use crate::ignore_poison::IgnorePoison;

use super::{
    CLOSE_TAB_ID, CommandScope, EDIT_PASTE_MOVE_ID, FILE_COMPRESS_ID, FILE_COPY_ID, FILE_DELETE_ID,
    FILE_DELETE_PERMANENTLY_ID, FILE_MOVE_ID, FILE_NEW_FILE_ID, FILE_NEW_FOLDER_ID, MenuState, OPEN_TERMINAL_HERE_ID,
    PIN_TAB_MENU_ID, RENAME_ID, REOPEN_CLOSED_TAB_ID, VIEW_SET_MODE_COMMAND_ID, VIEW_SHOW_HIDDEN_COMMAND_ID,
    menu_id_to_command,
};

/// Swaps the app-level menu bar to the main menu, if a different menu is installed.
///
/// After the swap, re-runs the macOS Edit-item cleanup and re-applies SF Symbol icons (neither
/// reliably survives `app.set_menu()`). Skips all of this when the main menu is already active.
#[cfg(target_os = "macos")]
pub(crate) fn swap_to_main_menu<R: Runtime>(app: &AppHandle<R>) {
    use crate::menu::ActiveMenuKind;
    let menu_state = app.state::<MenuState<R>>();

    {
        let mut active = menu_state.active_menu_kind.lock_ignore_poison();
        if *active == ActiveMenuKind::Main {
            return;
        }
        let main_menu = menu_state.main_menu.lock_ignore_poison();
        let Some(main_menu) = main_menu.as_ref() else {
            log::warn!(target: "menu", "main menu not stored; cannot swap app menu back to main");
            return;
        };
        if let Err(e) = app.set_menu(main_menu.clone()) {
            log::warn!(target: "menu", "Failed to swap app menu to main: {e}");
            return;
        }
        *active = ActiveMenuKind::Main;
    }

    // macOS re-injects Edit items on every `set_menu`, and SF Symbol icons don't survive the swap,
    // so re-run both on the main thread (mirrors the startup ordering in `lib.rs`).
    crate::menu::cleanup_macos_menus_from_command(app);
    crate::menu::set_macos_menu_icons_from_command(app);
}

/// Swaps the app-level menu bar to the shared viewer menu, if a different menu is installed.
///
/// After the swap, re-runs the macOS Edit-item cleanup. Skips all of this when the viewer menu is
/// already active.
#[cfg(target_os = "macos")]
pub(crate) fn swap_to_viewer_menu<R: Runtime>(app: &AppHandle<R>) {
    use crate::menu::ActiveMenuKind;
    let menu_state = app.state::<MenuState<R>>();

    {
        let mut active = menu_state.active_menu_kind.lock_ignore_poison();
        if *active == ActiveMenuKind::Viewer {
            return;
        }
        let viewer_menu = menu_state.viewer_menu.lock_ignore_poison();
        let Some(viewer_menu) = viewer_menu.as_ref() else {
            log::warn!(target: "menu", "viewer menu not stored; cannot swap app menu to viewer");
            return;
        };
        if let Err(e) = app.set_menu(viewer_menu.clone()) {
            log::warn!(target: "menu", "Failed to swap app menu to viewer: {e}");
            return;
        }
        *active = ActiveMenuKind::Viewer;
    }

    crate::menu::cleanup_macos_menus_from_command(app);
}

/// Records which window's menu the explorer items answer to, and recomputes every item from it.
/// - `"explorer"`: the main file explorer has focus
/// - `"other"`: Settings or Debug has focus, so every non-App item greys out except Close tab (⌘W),
///   which doubles as "close the focused window" (standard macOS behavior)
///
/// Private helper behind `activate_window_menu`: the focus-gain command owns the menu swap (macOS)
/// and then calls this to set the per-item enabled state. A rebuilt menu bar comes up with fresh,
/// enabled items, and the frontend re-runs `activate_window_menu` after one, so this is also what
/// restores every stored verdict after a language change.
pub(crate) fn set_menu_context<R: Runtime>(app: AppHandle<R>, context: String) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    menu_state
        .explorer_menu_active
        .store(context == "explorer", Ordering::Relaxed);
    apply_menu_item_states(&menu_state);
    Ok(())
}

/// Everything a main-menu item's enabled state is derived from.
struct MenuItemInputs<'a> {
    /// The main file explorer owns the menu (`activate_window_menu("main")`).
    explorer_menu_active: bool,
    /// The main window can't start a file operation right now (`set_file_operations_blocked`).
    file_operations_blocked: bool,
    /// The focused pane sits somewhere a shell can `cd` into (`set_open_terminal_here_enabled`).
    open_terminal_here_enabled: bool,
    /// The focused pane's closed-tab stack has entries (`set_reopen_closed_tab_enabled`).
    reopen_closed_tab_enabled: bool,
    /// The commands the main window's dialog gate refuses right now (`set_commands_refused_over_dialog`).
    refused: &'a HashSet<String>,
}

/// Whether the main-menu item `id` is enabled. The ONE place that's decided, from every input at
/// once, so no writer can undo another's verdict by running later.
fn menu_item_enabled(id: &str, inputs: &MenuItemInputs) -> bool {
    let command = menu_id_to_command(id);
    let refused = command.is_some_and(|(command_id, _)| inputs.refused.contains(command_id));
    // Close tab doubles as "close the focused window" (`handle_menu_event`), so it only greys out
    // while the main window is the one in front and can't close a tab.
    if id == CLOSE_TAB_ID {
        return !(inputs.explorer_menu_active && refused);
    }
    let in_scope = matches!(command, Some((_, CommandScope::App))) || inputs.explorer_menu_active;
    let own_verdict = match id {
        REOPEN_CLOSED_TAB_ID => inputs.reopen_closed_tab_enabled,
        OPEN_TERMINAL_HERE_ID => inputs.open_terminal_here_enabled,
        _ if OPERATION_START_ITEM_IDS.contains(&id) => !inputs.file_operations_blocked,
        _ => true,
    };
    in_scope && own_verdict && !refused
}

/// Recomputes every main-menu item's enabled state from the stored inputs.
///
/// Every writer of an input stores it and calls this, ❌ never `set_enabled` on an item directly:
/// an item with two writers keeps whichever ran last, which is how a focus round-trip through
/// Settings once offered Copy again with a dialog still up.
pub(crate) fn apply_menu_item_states<R: Runtime>(menu_state: &MenuState<R>) {
    // A copy, so the lock isn't held across the item locks below: `handle_menu_event` takes a check
    // item's lock first and this one second.
    let refused = menu_state.commands_refused_over_dialog.lock_ignore_poison().clone();
    let inputs = MenuItemInputs {
        explorer_menu_active: menu_state.explorer_menu_active.load(Ordering::Relaxed),
        file_operations_blocked: menu_state.file_operations_blocked.load(Ordering::Relaxed),
        open_terminal_here_enabled: menu_state.open_terminal_here_enabled.load(Ordering::Relaxed),
        reopen_closed_tab_enabled: menu_state.reopen_closed_tab_enabled.load(Ordering::Relaxed),
        refused: &refused,
    };

    for (id, entry) in menu_state.items.lock_ignore_poison().iter() {
        let _ = entry.item.set_enabled(menu_item_enabled(id, &inputs));
    }

    // The items held in their own `MenuState` fields, outside `items`. All of them are explorer
    // items with no verdict of their own. The check items name their command by const, since they
    // emit their own events instead of `execute-command`.
    let explorer_item = |command_id: &str| inputs.explorer_menu_active && !refused.contains(command_id);
    let pin_tab_command = menu_id_to_command(PIN_TAB_MENU_ID).map_or("", |(command_id, _)| command_id);
    let pin_tab_enabled = explorer_item(pin_tab_command);
    let show_hidden_enabled = explorer_item(VIEW_SHOW_HIDDEN_COMMAND_ID);
    let view_mode_enabled = explorer_item(VIEW_SET_MODE_COMMAND_ID);

    if let Some(ref item) = *menu_state.pin_tab.lock_ignore_poison() {
        let _ = item.set_enabled(pin_tab_enabled);
    }
    if let Some(ref item) = *menu_state.show_hidden_files.lock_ignore_poison() {
        let _ = item.set_enabled(show_hidden_enabled);
    }
    for view_mode_item in [
        &menu_state.view_mode_full_left,
        &menu_state.view_mode_brief_left,
        &menu_state.view_mode_full_right,
        &menu_state.view_mode_brief_right,
    ] {
        if let Some(ref item) = *view_mode_item.lock_ignore_poison() {
            let _ = item.set_enabled(view_mode_enabled);
        }
    }
    // The parent "Left pane" / "Right pane" submenus too, so they appear greyed out instead of
    // opening to reveal disabled items.
    for pane_submenu in [&menu_state.view_left_pane_submenu, &menu_state.view_right_pane_submenu] {
        if let Some(ref submenu) = *pane_submenu.lock_ignore_poison() {
            let _ = submenu.set_enabled(view_mode_enabled);
        }
    }
    // The sort items themselves are in `items`, each greyed by its own command.
    if let Some(ref submenu) = *menu_state.sort_submenu.lock_ignore_poison() {
        let _ = submenu.set_enabled(inputs.explorer_menu_active);
    }
}

/// The menu items that would START a file operation, greyed out while the main
/// window can't take one.
///
/// ❌ Not the ones that steer a RUNNING operation, and not Cut / Copy: marking a
/// clipboard selection starts nothing.
///
/// ⚠️ Every id here must be `FileScoped`. `Edit > Paste` is deliberately absent
/// even though pasting files DOES start a copy: it's `App`-scoped because in
/// Settings and the viewer it forwards the native `paste:` selector, so greying it
/// for a main-window dialog would kill ⌘V in those windows' text fields. Pasting
/// files is still refused honestly by `pane/operation-start-gate.ts`; only the
/// chrome differs. `every_gated_item_is_a_real_file_scoped_menu_item` pins this.
const OPERATION_START_ITEM_IDS: &[&str] = &[
    FILE_COPY_ID,
    FILE_MOVE_ID,
    FILE_COMPRESS_ID,
    FILE_NEW_FOLDER_ID,
    FILE_NEW_FILE_ID,
    FILE_DELETE_ID,
    FILE_DELETE_PERMANENTLY_ID,
    RENAME_ID,
    EDIT_PASTE_MOVE_ID,
];

#[cfg(test)]
#[path = "item_states_test.rs"]
mod item_states_test;
