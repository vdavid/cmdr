//! The per-pane view-mode menu items, kept in step with the app.
//!
//! Two levers, deliberately separate: a full rebuild when the active pane or a
//! shortcut changed (Tauri has no `set_accelerator()`, so the items are removed
//! and reinserted), and a cheap check-state sync when only the selection moved.

use std::sync::Mutex;

use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, Submenu},
};

use crate::ignore_poison::IgnorePoison;
use crate::intl::menu_t;

use super::menu_items::Mnemonics;
use super::{
    MenuState, VIEW_MODE_BRIEF_LEFT_ID, VIEW_MODE_BRIEF_RIGHT_ID, VIEW_MODE_FULL_LEFT_ID, VIEW_MODE_FULL_RIGHT_ID,
    ViewMode,
};

/// Rebuilds the four per-pane view-mode `CheckMenuItem`s with the current
/// state cached in `MenuState`: active pane, per-pane modes, and full/brief
/// shortcuts.
///
/// The accelerator is attached only to the active pane's pair, so that the
/// shortcut hint visually "follows" focus. Items are removed from the per-pane
/// submenu (Left pane / Right pane) and reinserted at the same position
/// (Full=0, Brief=1), since Tauri has no `set_accelerator()` API. The new
/// `CheckMenuItem` references replace the old ones in `MenuState`.
///
/// Frontend pushes a rebuild on pane focus change and on shortcut customization.
pub fn rebuild_view_mode_items<R: Runtime>(app: &AppHandle<R>, menu_state: &MenuState<R>) -> tauri::Result<()> {
    let left_submenu_guard = menu_state.view_left_pane_submenu.lock_ignore_poison();
    let right_submenu_guard = menu_state.view_right_pane_submenu.lock_ignore_poison();
    let left_submenu = left_submenu_guard
        .as_ref()
        .ok_or_else(|| tauri::Error::InvalidWindowHandle)?;
    let right_submenu = right_submenu_guard
        .as_ref()
        .ok_or_else(|| tauri::Error::InvalidWindowHandle)?;

    let active_pane = menu_state.view_mode_active_pane.lock_ignore_poison().clone();
    let left_mode = *menu_state.view_mode_left.lock_ignore_poison();
    let right_mode = *menu_state.view_mode_right.lock_ignore_poison();
    let full_accel = menu_state.view_mode_full_accel.lock_ignore_poison().clone();
    let brief_accel = menu_state.view_mode_brief_accel.lock_ignore_poison().clone();

    let left_active = active_pane == "left";
    // Each pane submenu holds only these two items, so one allocation serves
    // both panes and reproduces exactly what `build_view_mode_items` assigned.
    let mut mnemonics = Mnemonics::new();
    let full_label = mnemonics.assign(&menu_t("menu.view.fullView"));
    let brief_label = mnemonics.assign(&menu_t("menu.view.briefView"));

    // Helper: replace one CheckMenuItem inside its pane submenu, preserving its position.
    let swap = |slot: &Mutex<Option<CheckMenuItem<R>>>,
                parent: &Submenu<R>,
                position: usize,
                id: &str,
                label: &str,
                checked: bool,
                accel: Option<&str>|
     -> tauri::Result<()> {
        let mut guard = slot.lock_ignore_poison();
        if let Some(old) = guard.as_ref() {
            parent.remove(old)?;
        }
        let new_item = CheckMenuItem::with_id(app, id, label, true, checked, accel)?;
        parent.insert(&new_item, position)?;
        *guard = Some(new_item);
        Ok(())
    };

    swap(
        &menu_state.view_mode_full_left,
        left_submenu,
        0,
        VIEW_MODE_FULL_LEFT_ID,
        &full_label,
        left_mode == ViewMode::Full,
        if left_active { full_accel.as_deref() } else { None },
    )?;
    swap(
        &menu_state.view_mode_brief_left,
        left_submenu,
        1,
        VIEW_MODE_BRIEF_LEFT_ID,
        &brief_label,
        left_mode == ViewMode::Brief,
        if left_active { brief_accel.as_deref() } else { None },
    )?;
    swap(
        &menu_state.view_mode_full_right,
        right_submenu,
        0,
        VIEW_MODE_FULL_RIGHT_ID,
        &full_label,
        right_mode == ViewMode::Full,
        if !left_active { full_accel.as_deref() } else { None },
    )?;
    swap(
        &menu_state.view_mode_brief_right,
        right_submenu,
        1,
        VIEW_MODE_BRIEF_RIGHT_ID,
        &brief_label,
        right_mode == ViewMode::Brief,
        if !left_active { brief_accel.as_deref() } else { None },
    )?;

    Ok(())
}

/// Sets only the checked state on the existing per-pane view-mode items,
/// without touching accelerators. Used for in-place updates (a click in
/// the same pane, palette toggle) where active pane and shortcuts are
/// unchanged.
pub fn sync_view_mode_check_states<R: Runtime>(menu_state: &MenuState<R>) -> tauri::Result<()> {
    let left_mode = *menu_state.view_mode_left.lock_ignore_poison();
    let right_mode = *menu_state.view_mode_right.lock_ignore_poison();

    if let Some(item) = menu_state.view_mode_full_left.lock_ignore_poison().as_ref() {
        item.set_checked(left_mode == ViewMode::Full)?;
    }
    if let Some(item) = menu_state.view_mode_brief_left.lock_ignore_poison().as_ref() {
        item.set_checked(left_mode == ViewMode::Brief)?;
    }
    if let Some(item) = menu_state.view_mode_full_right.lock_ignore_poison().as_ref() {
        item.set_checked(right_mode == ViewMode::Full)?;
    }
    if let Some(item) = menu_state.view_mode_brief_right.lock_ignore_poison().as_ref() {
        item.set_checked(right_mode == ViewMode::Brief)?;
    }
    Ok(())
}
