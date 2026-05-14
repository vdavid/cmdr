//! Menu event handlers and live-update helpers.
//!
//! Functions here mutate the menu after construction: rebuilding the per-pane
//! view-mode items when focus or shortcuts change, syncing check states,
//! swapping a tracked menu item's accelerator, translating frontend shortcut
//! strings to Tauri accelerator strings, and the macOS post-construction
//! cleanup / SF Symbol icon pass.

use std::sync::Mutex;

use tauri::{
    AppHandle, Runtime,
    menu::{CheckMenuItem, MenuItem, Submenu},
};

use crate::ignore_poison::IgnorePoison;

use super::menu_items::{brief_view_label, full_view_label};
use super::{
    MenuItemEntry, MenuState, VIEW_MODE_BRIEF_LEFT_ID, VIEW_MODE_BRIEF_RIGHT_ID, VIEW_MODE_FULL_LEFT_ID,
    VIEW_MODE_FULL_RIGHT_ID, ViewMode,
};

/// Removes macOS system-injected items from the Edit menu and registers the Help menu.
///
/// macOS AppKit automatically injects "Writing Tools", "AutoFill", "Start Dictation...",
/// and "Emoji & Symbols" into any menu named "Edit". It also only shows the Help menu
/// search field when a menu is registered via `NSApplication.setHelpMenu:`. Both of these
/// happen at the AppKit level regardless of how the menu is constructed, so we fix them
/// post-construction via native API calls.
#[cfg(target_os = "macos")]
pub fn cleanup_macos_menus() {
    super::macos::cleanup_macos_menus();
}

/// Sets SF Symbol icons on menu items post-construction via native AppKit API.
///
/// Tauri's menu API doesn't support SF Symbols, so we walk the NSMenu hierarchy after
/// construction and call `NSImage(systemSymbolName:accessibilityDescription:)` + `setImage:`
/// on each item, matching by title within each known submenu.
#[cfg(target_os = "macos")]
pub fn set_macos_menu_icons() {
    super::macos::set_macos_menu_icons();
}

/// Convert frontend shortcut format (⌘2) to Tauri accelerator format (Cmd+2).
/// Returns None if the shortcut is empty or invalid.
pub fn frontend_shortcut_to_accelerator(shortcut: &str) -> Option<String> {
    if shortcut.is_empty() {
        return None;
    }

    let mut result = String::new();
    let mut chars = shortcut.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '⌘' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Cmd");
            }
            '⌃' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Ctrl");
            }
            '⌥' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Opt");
            }
            '⇧' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Shift");
            }
            '↑' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Up");
            }
            '↓' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Down");
            }
            '←' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Left");
            }
            '→' => {
                if !result.is_empty() {
                    result.push('+');
                }
                result.push_str("Right");
            }
            _ => {
                // Regular character (letter, number, etc.)
                if !result.is_empty() {
                    result.push('+');
                }
                // Handle special key names
                let remaining: String = std::iter::once(c).chain(chars.by_ref()).collect();
                if remaining.eq_ignore_ascii_case("enter") {
                    result.push_str("Enter");
                } else if remaining.eq_ignore_ascii_case("space") {
                    result.push_str("Space");
                } else if remaining.eq_ignore_ascii_case("tab") {
                    result.push_str("Tab");
                } else if remaining.eq_ignore_ascii_case("escape") {
                    result.push_str("Escape");
                } else if remaining.eq_ignore_ascii_case("backspace") {
                    result.push_str("Backspace");
                } else if remaining.starts_with('F') || remaining.starts_with('f') {
                    // Function keys like F1, F4
                    result.push_str(&remaining.to_uppercase());
                } else if remaining.eq_ignore_ascii_case("pageup") {
                    result.push_str("PageUp");
                } else if remaining.eq_ignore_ascii_case("pagedown") {
                    result.push_str("PageDown");
                } else if remaining.eq_ignore_ascii_case("home") {
                    result.push_str("Home");
                } else if remaining.eq_ignore_ascii_case("end") {
                    result.push_str("End");
                } else {
                    // Single character or unknown - use as-is (uppercase for letters)
                    result.push_str(&remaining.to_uppercase());
                }
                break;
            }
        }
    }

    if result.is_empty() { None } else { Some(result) }
}

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
    let full_label = full_view_label();
    let brief_label = brief_view_label();

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
        full_label,
        left_mode == ViewMode::Full,
        if left_active { full_accel.as_deref() } else { None },
    )?;
    swap(
        &menu_state.view_mode_brief_left,
        left_submenu,
        1,
        VIEW_MODE_BRIEF_LEFT_ID,
        brief_label,
        left_mode == ViewMode::Brief,
        if left_active { brief_accel.as_deref() } else { None },
    )?;
    swap(
        &menu_state.view_mode_full_right,
        right_submenu,
        0,
        VIEW_MODE_FULL_RIGHT_ID,
        full_label,
        right_mode == ViewMode::Full,
        if !left_active { full_accel.as_deref() } else { None },
    )?;
    swap(
        &menu_state.view_mode_brief_right,
        right_submenu,
        1,
        VIEW_MODE_BRIEF_RIGHT_ID,
        brief_label,
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

/// Update the accelerator for any menu item tracked in the items HashMap.
/// Removes the old item, creates a new one with the same ID/label/enabled state
/// but a new accelerator, and reinserts at the same position.
pub fn update_menu_item_accelerator<R: Runtime>(
    app: &AppHandle<R>,
    menu_state: &MenuState<R>,
    menu_item_id: &str,
    new_accelerator: Option<&str>,
) -> tauri::Result<()> {
    let mut items_guard = menu_state.items.lock_ignore_poison();
    let entry = items_guard
        .get(menu_item_id)
        .ok_or_else(|| tauri::Error::InvalidWindowHandle)?;

    let label = entry.item.text()?;
    let enabled = entry.item.is_enabled()?;
    let submenu = entry.submenu.clone();
    let position = entry.position;

    // Remove old item, create replacement with new accelerator, reinsert
    submenu.remove(&entry.item)?;
    let new_item = MenuItem::with_id(app, menu_item_id, &label, enabled, new_accelerator)?;
    submenu.insert(&new_item, position)?;

    // Update the HashMap entry
    items_guard.insert(
        menu_item_id.to_string(),
        MenuItemEntry {
            item: new_item,
            submenu,
            position,
        },
    );

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_frontend_shortcut_to_accelerator_simple() {
        // Basic modifier + key combinations
        assert_eq!(frontend_shortcut_to_accelerator("⌘1"), Some("Cmd+1".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌘2"), Some("Cmd+2".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌘⇧P"), Some("Cmd+Shift+P".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌥⌘O"), Some("Opt+Cmd+O".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌃⌘C"), Some("Ctrl+Cmd+C".to_string()));
    }

    #[test]
    fn test_frontend_shortcut_to_accelerator_arrows() {
        assert_eq!(frontend_shortcut_to_accelerator("⌘↑"), Some("Cmd+Up".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌘↓"), Some("Cmd+Down".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌘["), Some("Cmd+[".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("⌘]"), Some("Cmd+]".to_string()));
    }

    #[test]
    fn test_frontend_shortcut_to_accelerator_special_keys() {
        assert_eq!(frontend_shortcut_to_accelerator("Tab"), Some("Tab".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("Enter"), Some("Enter".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("Space"), Some("Space".to_string()));
        assert_eq!(frontend_shortcut_to_accelerator("F4"), Some("F4".to_string()));
        assert_eq!(
            frontend_shortcut_to_accelerator("Backspace"),
            Some("Backspace".to_string())
        );
    }

    #[test]
    fn test_frontend_shortcut_to_accelerator_empty() {
        assert_eq!(frontend_shortcut_to_accelerator(""), None);
    }
}
