//! Which main-menu items are enabled right now, and which of the two macOS app-level menu bars
//! (main vs. viewer) is installed.
//!
//! [`apply_menu_item_states`] is the single writer of every main-menu item's enabled state,
//! recomputed from every [`MenuState`] input at once through the pure fold in
//! `cmdr_menu::enabled`, so no writer can undo another's verdict by running after it. [`set_menu_context`] and the macOS-only `swap_to_main_menu` /
//! `swap_to_viewer_menu` decide which window's menu is installed and which items that leaves in
//! scope; both are called from `commands::menu_state::activate_window_menu` on focus-gain.

use std::sync::atomic::Ordering;

use cmdr_menu::enabled::{FieldItem, MenuItemInputs, field_item_enabled, menu_item_enabled, submenu_enabled};
#[cfg(target_os = "macos")]
pub(crate) use cmdr_menu::enabled::{note_viewer_search_focus, viewer_text_edit_enabled};
use cmdr_menu::spec::{Pane, SubmenuRole};
use tauri::{AppHandle, Manager, Runtime};

use crate::ignore_poison::IgnorePoison;

use super::{MenuState, cloned_handle};

/// Swaps the app-level menu bar to the main menu, if a different menu is installed.
///
/// After the swap, re-runs the macOS Edit-item cleanup and re-applies the SF Symbol icons and the
/// display-only accelerators (none of the three survives `app.set_menu()`, which builds fresh
/// `NSMenuItem`s). Skips all of this when the main menu is already active.
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

    // macOS re-injects Edit items on every `set_menu`, and neither the SF Symbol icons nor the
    // display-only accelerators survive the swap, so re-run all three on the main thread
    // (mirrors the startup ordering in `lib.rs`).
    crate::menu::cleanup_macos_menus_from_command(app);
    crate::menu::set_macos_menu_icons_from_command(app);
    crate::menu::set_display_accelerators_from_command(app);
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
        undo_trash_available: menu_state.undo_trash_available.load(Ordering::Relaxed),
        refused: &refused,
    };

    // Decided under the lock, applied after it: `set_enabled` waits on the main thread, and an
    // off-main writer holding `items` across it deadlocks this very function on the main thread.
    let verdicts: Vec<_> = menu_state
        .items
        .lock_ignore_poison()
        .iter()
        .map(|(id, entry)| (entry.item.clone(), menu_item_enabled(id, &inputs)))
        .collect();
    for (item, enabled) in verdicts {
        let _ = item.set_enabled(enabled);
    }

    // The items held in their own `MenuState` fields, outside `items`, and the submenus greyed as a
    // whole: `cmdr_menu::enabled` decides each from the same inputs.
    let pin_tab_enabled = field_item_enabled(FieldItem::PinTab, &inputs);
    let show_hidden_enabled = field_item_enabled(FieldItem::ShowHiddenFiles, &inputs);
    let view_mode_enabled = field_item_enabled(FieldItem::ViewMode, &inputs);

    if let Some(item) = cloned_handle(&menu_state.pin_tab) {
        let _ = item.set_enabled(pin_tab_enabled);
    }
    if let Some(item) = cloned_handle(&menu_state.show_hidden_files) {
        let _ = item.set_enabled(show_hidden_enabled);
    }
    for view_mode_item in [
        &menu_state.view_mode_full_left,
        &menu_state.view_mode_brief_left,
        &menu_state.view_mode_full_right,
        &menu_state.view_mode_brief_right,
    ] {
        if let Some(item) = cloned_handle(view_mode_item) {
            let _ = item.set_enabled(view_mode_enabled);
        }
    }
    // The parent "Left pane" / "Right pane" submenus too, so they appear greyed out instead of
    // opening to reveal disabled items.
    for (pane, pane_submenu) in [
        (Pane::Left, &menu_state.view_left_pane_submenu),
        (Pane::Right, &menu_state.view_right_pane_submenu),
    ] {
        if let Some(submenu) = cloned_handle(pane_submenu) {
            let _ = submenu.set_enabled(submenu_enabled(SubmenuRole::Pane(pane), &inputs));
        }
    }
    // The sort items themselves are in `items`, each greyed by its own command.
    if let Some(submenu) = cloned_handle(&menu_state.sort_submenu) {
        let _ = submenu.set_enabled(submenu_enabled(SubmenuRole::SortBy, &inputs));
    }

    // The VIEWER bar's Edit > Cut / Paste, which answer to a different question than everything
    // above: not the explorer's focus or its dialog gate, but whether the viewer in front has its
    // search box focused. They're here because the rule stands — one writer of `set_enabled`.
    #[cfg(target_os = "macos")]
    {
        let enabled = {
            let holder = menu_state.viewer_search_focus.lock_ignore_poison();
            viewer_text_edit_enabled(holder.as_deref())
        };
        for item in [&menu_state.viewer_edit_cut, &menu_state.viewer_edit_paste] {
            if let Some(item) = cloned_handle(item) {
                let _ = item.set_enabled(enabled);
            }
        }

        // Go > Back / Forward follow the front viewer's document history.
        let history = *menu_state.viewer_history.lock_ignore_poison();
        for (item, enabled) in [
            (&menu_state.viewer_go_back, history.can_go_back),
            (&menu_state.viewer_go_forward, history.can_go_forward),
        ] {
            if let Some(item) = cloned_handle(item) {
                let _ = item.set_enabled(enabled);
            }
        }
    }
}
