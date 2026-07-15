//! Menu event handlers and live-update helpers.
//!
//! Functions here mutate the menu after construction: rebuilding the per-pane
//! view-mode items when focus or shortcuts change, syncing check states,
//! swapping a tracked menu item's accelerator, translating frontend shortcut
//! strings to Tauri accelerator strings, and the macOS post-construction
//! cleanup / SF Symbol icon pass.

use std::sync::Mutex;

use tauri::{
    AppHandle, Manager, Runtime,
    menu::{CheckMenuItem, MenuItem, Submenu},
};

use crate::ignore_poison::IgnorePoison;

use super::menu_items::{brief_view_label, full_view_label};
use super::{
    CLOSE_TAB_ID, CommandScope, EDIT_COPY_ID, EDIT_CUT_ID, EDIT_PASTE_ID, EJECT_VOLUME_ID, FAVORITE_REMOVE_ID,
    FAVORITE_RENAME_ID, FAVORITES_ADD_CONTEXT_ID, MEDIA_INDEX_EXCLUDE_FOLDER_ID, MEDIA_INDEX_INCLUDE_FOLDER_ID,
    MediaIndexFolderExclusion, MenuItemEntry, MenuSort, MenuState, NETWORK_HOST_DISCONNECT_ID,
    NETWORK_HOST_FORGET_PASSWORD_ID, NETWORK_HOST_FORGET_SERVER_ID, SELECT_ALL_ID, SHOW_HIDDEN_FILES_ID,
    SORT_ASCENDING_ID, SORT_BY_CREATED_ID, SORT_BY_EXTENSION_ID, SORT_BY_MODIFIED_ID, SORT_BY_NAME_ID, SORT_BY_SIZE_ID,
    SORT_DESCENDING_ID, SettingsChanged, TAB_CLOSE_ID, TAB_CLOSE_OTHERS_ID, TAB_PIN_ID, VIEW_MODE_BRIEF_LEFT_ID,
    VIEW_MODE_BRIEF_RIGHT_ID, VIEW_MODE_FULL_LEFT_ID, VIEW_MODE_FULL_RIGHT_ID, VIEWER_WORD_WRAP_ID, ViewMode,
    ViewModeChanged, menu_id_to_command,
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

/// Runs [`cleanup_macos_menus`] on the main thread, for callers running on a Tauri command thread.
///
/// `cleanup_macos_menus` (and `set_macos_menu_icons`) touch AppKit and must run on the main thread.
/// At startup `lib.rs` already runs in the `setup` hook on the main thread, so it calls them
/// directly; Tauri command handlers run on a worker thread, so they hop via `run_on_main_thread`.
/// Fire-and-forget: the cleanup is a UI tidy-up, so a failed hop only leaves the OS-injected Edit
/// items in place, never a broken state.
#[cfg(target_os = "macos")]
pub fn cleanup_macos_menus_from_command<R: Runtime>(app: &AppHandle<R>) {
    if let Err(e) = app.run_on_main_thread(cleanup_macos_menus) {
        log::warn!(target: "menu", "Failed to dispatch macOS menu cleanup to the main thread: {e}");
    }
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

/// Sends a native edit action (copy:/cut:/paste:/selectAll:) through the responder chain.
///
/// Used when a non-main window is focused: the custom Edit/Select menu items can't use the
/// native responder chain like PredefinedMenuItems do, so we replicate it manually via
/// `NSApplication.sendAction:to:from:` with nil target (routes to the first responder).
#[cfg(target_os = "macos")]
fn send_native_edit_action(menu_id: &str) {
    use objc2::sel;
    use objc2_app_kit::NSApplication;

    let selector = match menu_id {
        EDIT_CUT_ID => sel!(cut:),
        EDIT_COPY_ID => sel!(copy:),
        EDIT_PASTE_ID => sel!(paste:),
        SELECT_ALL_ID => sel!(selectAll:),
        _ => return,
    };

    let mtm = objc2::MainThreadMarker::new().expect("send_native_edit_action must be called from the main thread");
    let ns_app = NSApplication::sharedApplication(mtm);

    // sendAction:to:from: with nil `to` sends to the first responder, exactly like
    // PredefinedMenuItems do internally. This lets WKWebView handle text clipboard natively.
    // SAFETY: `ns_app` is the live `sharedApplication` singleton; `sendAction:to:from:` takes
    // `(SEL, id, id)` — `selector` is one of the responder-chain editing selectors matched above, and
    // both `to`/`from` are nil (routes to the first responder). Returns `BOOL`, decoded as `bool`. On
    // the main thread (the `MainThreadMarker` above asserts it), as AppKit requires.
    unsafe {
        let _: bool = objc2::msg_send![
            &ns_app,
            sendAction: selector,
            to: std::ptr::null::<objc2::runtime::AnyObject>(),
            from: std::ptr::null::<objc2::runtime::AnyObject>(),
        ];
    }
}

/// Dispatches a global-menu click to the right window or frontend command.
///
/// Wired into the Tauri builder as `.on_menu_event(menu::handle_menu_event)`. Most items flow
/// through the unified `menu_id_to_command` mapping at the bottom and emit `execute-command` to
/// the main window; the blocks above it are the exceptions that need direct emits, per-pane
/// state syncing, focus-routed clipboard handling, or native macOS panels.
pub fn handle_menu_event(app: &AppHandle<tauri::Wry>, event: tauri::menu::MenuEvent) {
    let id = event.id().as_ref();

    // === CheckMenuItem exceptions: sync checked state and emit directly ===
    // These must NOT go through "execute-command", as that would double-toggle.
    if id == SHOW_HIDDEN_FILES_ID {
        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let guard = menu_state.show_hidden_files.lock_ignore_poison();
        if let Some(check_item) = guard.as_ref() {
            let new_state = check_item.is_checked().unwrap_or(true);
            use tauri_specta::Event as _;
            let _ = SettingsChanged {
                show_hidden_files: new_state,
            }
            .emit_to(app, "main");
        }
        return;
    }
    if id == VIEW_MODE_FULL_LEFT_ID
        || id == VIEW_MODE_BRIEF_LEFT_ID
        || id == VIEW_MODE_FULL_RIGHT_ID
        || id == VIEW_MODE_BRIEF_RIGHT_ID
    {
        // Per-pane view mode click. Sync the affected pane's pair (the muda click
        // already toggled the clicked item, so unchecking the sibling is enough),
        // store the new mode in MenuState, and notify the frontend with the target
        // pane so it can update without changing focus.
        let (pane, mode_str) = match id {
            VIEW_MODE_FULL_LEFT_ID => ("left", "full"),
            VIEW_MODE_BRIEF_LEFT_ID => ("left", "brief"),
            VIEW_MODE_FULL_RIGHT_ID => ("right", "full"),
            VIEW_MODE_BRIEF_RIGHT_ID => ("right", "brief"),
            _ => unreachable!(),
        };
        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let new_mode = if mode_str == "full" {
            ViewMode::Full
        } else {
            ViewMode::Brief
        };
        if pane == "left" {
            *menu_state.view_mode_left.lock_ignore_poison() = new_mode;
        } else {
            *menu_state.view_mode_right.lock_ignore_poison() = new_mode;
        }
        let _ = sync_view_mode_check_states(&menu_state);
        use tauri_specta::Event as _;
        let _ = ViewModeChanged {
            mode: mode_str.to_string(),
            pane: pane.to_string(),
        }
        .emit_to(app, "main");
        return;
    }

    // === Close-tab exception: close focused non-main window, or emit tab.close ===
    if id == CLOSE_TAB_ID {
        if let Some(main_window) = app.get_webview_window("main")
            && main_window.is_focused().unwrap_or(false)
        {
            use tauri_specta::Event as _;
            let _ = crate::window_events::ExecuteCommand {
                command_id: "tab.close".to_string(),
            }
            .emit_to(app, "main");
        } else {
            for (_label, window) in app.webview_windows() {
                if window.is_focused().unwrap_or(false) {
                    let _ = window.close();
                    break;
                }
            }
        }
        return;
    }

    // === Add to favorites (folder-row + parent-row context menus) ===
    // Favorites the right-clicked path stashed in `MenuState.context.path` (the folder for a folder
    // row, the parent dir for `..`). Intercepted here so it never routes through `favorites.add`
    // (which favorites the focused-pane dir instead). The store write touches the filesystem, so it
    // runs on the blocking pool, never on this menu thread; the command re-emits `volumes-changed`.
    if id == FAVORITES_ADD_CONTEXT_ID {
        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let path = menu_state.context.lock_ignore_poison().path.clone();
        if path.is_empty() {
            log::warn!(target: "favorites", "Add to favorites: empty context path, ignoring");
            return;
        }
        tauri::async_runtime::spawn(async move {
            let write = tauri::async_runtime::spawn_blocking(move || crate::favorites::store::add(&path, None)).await;
            if let Err(e) = write {
                log::warn!(target: "favorites", "Add to favorites: store write failed: {e}");
                return;
            }
            crate::volume_broadcast::emit_volumes_changed();
        });
        return;
    }

    // === Image-search folder exclusion (media_index privacy veto) ===
    // Acts on the RIGHT-CLICKED folder in `MenuState.context.path` (not the focused-pane
    // selection), so it can't route through `execute-command`. Emit the target folder +
    // state to the FE, which persists `mediaIndex.excludedFolders` and calls
    // `media_index_set_excluded_folder` (the native menu can't write the FE store).
    if id == MEDIA_INDEX_EXCLUDE_FOLDER_ID || id == MEDIA_INDEX_INCLUDE_FOLDER_ID {
        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let folder = menu_state.context.lock_ignore_poison().path.clone();
        if folder.is_empty() {
            log::warn!(target: "media_index", "folder exclusion clicked with no context path, ignoring");
            return;
        }
        use tauri_specta::Event as _;
        let _ = MediaIndexFolderExclusion {
            folder,
            excluded: id == MEDIA_INDEX_EXCLUDE_FOLDER_ID,
        }
        .emit_to(app, "main");
        return;
    }

    // === Viewer word wrap: emit to the focused viewer window ===
    if id == VIEWER_WORD_WRAP_ID {
        for (label, window) in app.webview_windows() {
            if label.starts_with("viewer-") && window.is_focused().unwrap_or(false) {
                use tauri_specta::Event as _;
                let _ = crate::window_events::ViewerWordWrapToggled.emit_to(app, &label);
                break;
            }
        }
        return;
    }

    // === Sort items: emit menu-sort directly (frontend has a dedicated listener) ===
    if id == SORT_BY_NAME_ID
        || id == SORT_BY_EXTENSION_ID
        || id == SORT_BY_SIZE_ID
        || id == SORT_BY_MODIFIED_ID
        || id == SORT_BY_CREATED_ID
    {
        let column = match id {
            SORT_BY_NAME_ID => "name",
            SORT_BY_EXTENSION_ID => "extension",
            SORT_BY_SIZE_ID => "size",
            SORT_BY_MODIFIED_ID => "modified",
            _ => "created",
        };
        use tauri_specta::Event as _;
        let _ = MenuSort {
            action: "sortBy".to_string(),
            value: column.to_string(),
        }
        .emit_to(app, "main");
        return;
    }
    if id == SORT_ASCENDING_ID || id == SORT_DESCENDING_ID {
        let order = if id == SORT_ASCENDING_ID { "asc" } else { "desc" };
        use tauri_specta::Event as _;
        let _ = MenuSort {
            action: "sortOrder".to_string(),
            value: order.to_string(),
        }
        .emit_to(app, "main");
        return;
    }

    // === Tab context menu actions: emit tab-context-action directly ===
    if id == TAB_PIN_ID || id == TAB_CLOSE_OTHERS_ID || id == TAB_CLOSE_ID {
        use tauri_specta::Event as _;
        let _ = crate::window_events::TabContextAction { action: id.to_string() }.emit_to(app, "main");
        return;
    }

    // === Eject volume / favorite rename / favorite remove (volume-selector row menus) ===
    // All three are routed back to the frontend through the same `volume-context-action`
    // event with the target stashed in `volume_row_context`; the action string disambiguates.
    if id == EJECT_VOLUME_ID || id == FAVORITE_RENAME_ID || id == FAVORITE_REMOVE_ID {
        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let ctx = menu_state.volume_row_context.lock_ignore_poison();
        if ctx.volume_id.is_empty() {
            log::warn!(target: "menu", "Volume row menu item {id} clicked with no volume_id stashed");
            return;
        }
        let action = if id == FAVORITE_RENAME_ID {
            "rename-favorite"
        } else if id == FAVORITE_REMOVE_ID {
            "remove-favorite"
        } else {
            "eject"
        };
        use tauri_specta::Event as _;
        let payload = crate::volume_broadcast::VolumeContextAction {
            action: action.to_string(),
            volume_id: ctx.volume_id.clone(),
            volume_name: ctx.volume_name.clone(),
        };
        let _ = payload.emit_to(app, "main");
        return;
    }

    // === Network host context menu actions ===
    if id == NETWORK_HOST_FORGET_SERVER_ID || id == NETWORK_HOST_FORGET_PASSWORD_ID || id == NETWORK_HOST_DISCONNECT_ID
    {
        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let ctx = menu_state.network_host_context.lock_ignore_poison();
        let action = if id == NETWORK_HOST_FORGET_SERVER_ID {
            "forget-server"
        } else if id == NETWORK_HOST_FORGET_PASSWORD_ID {
            "forget-password"
        } else {
            "disconnect"
        };
        use tauri_specta::Event as _;
        let payload = crate::network::NetworkHostContextAction {
            action: action.to_string(),
            host_id: ctx.host_id.clone(),
            host_name: ctx.host_name.clone(),
        };
        let _ = payload.emit_to(app, "main");
        return;
    }

    // === Edit-action exception: file semantics in main window, native text semantics elsewhere ===
    // Custom MenuItems for Cut/Copy/Paste/Select all route through execute-command in the main
    // window so the frontend can decide between file and text semantics. In non-main windows
    // (viewer, settings), we send the native action through the responder chain so WKWebView
    // handles text clipboard / text select-all natively, just like PredefinedMenuItems would.
    // Without the Select-all branch, ⌘A is dead in settings text fields: the accelerator fires
    // before the webview ever sees the key, and the FileScoped focus guard would drop it.
    if id == EDIT_CUT_ID || id == EDIT_COPY_ID || id == EDIT_PASTE_ID || id == SELECT_ALL_ID {
        let main_focused = app
            .get_webview_window("main")
            .is_some_and(|w| w.is_focused().unwrap_or(false));
        if main_focused {
            let command_id = match id {
                EDIT_CUT_ID => "edit.cut",
                EDIT_COPY_ID => "edit.copy",
                EDIT_PASTE_ID => "edit.paste",
                _ => "selection.selectAll",
            };
            use tauri_specta::Event as _;
            let _ = crate::window_events::ExecuteCommand {
                command_id: command_id.to_string(),
            }
            .emit_to(app, "main");
        } else {
            // Send the native action to the first responder chain
            #[cfg(target_os = "macos")]
            send_native_edit_action(id);
        }
        return;
    }

    // === Open with submenu: dynamic IDs prefix-routed before unified dispatch ===
    // Items have IDs like `open-with:com.apple.Xcode`, too dynamic to enumerate
    // in `menu_id_to_command`. We resolve the bundle ID back to an app path via
    // `MenuState.context.open_with_apps` and call the launch helper directly.
    #[cfg(target_os = "macos")]
    if let Some(bundle_id) = id.strip_prefix(super::open_with::OPEN_WITH_ID_PREFIX) {
        use crate::file_system::open_with::open_paths_with;
        use std::path::PathBuf;

        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let ctx = menu_state.context.lock_ignore_poison();
        let app_path = ctx.open_with_apps.get(bundle_id).cloned();
        let paths: Vec<PathBuf> = ctx.paths.iter().map(PathBuf::from).collect();
        drop(ctx);

        if let Some(app_path) = app_path
            && !paths.is_empty()
        {
            if let Err(e) = open_paths_with(&paths, &app_path) {
                log::warn!("Open with failed for {bundle_id}: {e}");
            }
        } else {
            log::warn!("Open with: missing app or paths for {bundle_id}");
        }
        return;
    }

    // === Open with → Other... : show NSOpenPanel, then launch ===
    #[cfg(target_os = "macos")]
    if id == super::open_with::OPEN_WITH_OTHER_ID {
        use crate::file_system::open_with::{open_paths_with, pick_app_via_open_panel};
        use std::path::PathBuf;

        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let paths: Vec<PathBuf> = menu_state
            .context
            .lock_ignore_poison()
            .paths
            .iter()
            .map(PathBuf::from)
            .collect();

        // NSOpenPanel must run on the main thread. on_menu_event is invoked on
        // the main thread by Tauri/muda, so this is safe.
        if let Some(app_path) = pick_app_via_open_panel()
            && !paths.is_empty()
            && let Err(e) = open_paths_with(&paths, &app_path)
        {
            log::warn!("Open with (Other...) failed: {e}");
        }
        return;
    }

    // === Tag color items: prefix-routed straight to the tag write (like open-with) ===
    // `tag-color:<index>` toggles that system color on the RIGHT-CLICKED selection
    // (`MenuState.context.paths`), then refreshes the stashed listing's cache. It acts on
    // the right-clicked set, not the focused-pane selection, so it can't route through
    // `execute-command` + a frontend command (those read the focused selection — wrong
    // when the right-click landed on an unselected row). The keyboard `tags.toggle*`
    // commands handle the focused-selection case separately.
    #[cfg(target_os = "macos")]
    if let Some(rest) = id.strip_prefix(super::TAG_COLOR_ID_PREFIX) {
        if let Ok(color) = rest.parse::<u8>() {
            let menu_state = app.state::<MenuState<tauri::Wry>>();
            let ctx = menu_state.context.lock_ignore_poison();
            let paths = ctx.paths.clone();
            let listing_id = ctx.tags_listing_id.clone();
            drop(ctx);
            if !paths.is_empty() {
                // `setxattr` is blocking I/O; keep it off the main (menu) thread.
                tauri::async_runtime::spawn_blocking(move || {
                    match crate::file_system::tags::toggle_color(&paths, color) {
                        Ok(updates) if !updates.is_empty() => {
                            crate::file_system::listing::caching::apply_tags_to_listing(&listing_id, updates);
                        }
                        Ok(_) => {}
                        Err(e) => log::warn!(target: "tags", "context-menu tag toggle failed (color={color}): {e}"),
                    }
                });
            }
        }
        return;
    }

    // === Unified dispatch: look up command ID from the mapping ===
    if let Some((command_id, scope)) = menu_id_to_command(id) {
        if scope == CommandScope::FileScoped {
            // Focus guard: only emit if main window has focus
            let focused = app
                .get_webview_window("main")
                .is_some_and(|w| w.is_focused().unwrap_or(false));
            if !focused {
                return;
            }
        }
        use tauri_specta::Event as _;
        let _ = crate::window_events::ExecuteCommand {
            command_id: command_id.to_string(),
        }
        .emit_to(app, "main");
    }

    // Unknown menu ID: no-op (all known IDs are handled above)
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
