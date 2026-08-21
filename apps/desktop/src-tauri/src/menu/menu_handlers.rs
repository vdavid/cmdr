//! What happens when someone picks a menu item.
//!
//! `handle_menu_event` is the `.on_menu_event` dispatcher wired into the Tauri
//! builder: it maps a clicked item's ID to a command, a settings toggle, or a
//! native responder-chain action. The macOS post-construction passes live here
//! too (`cleanup_macos_menus`, `set_macos_menu_icons`), since they share this
//! file's platform seam onto AppKit.
//!
//! The other two live-update lanes moved out: `accelerators.rs` for shortcut
//! strings, `view_mode_items.rs` for the per-pane view-mode items.

use tauri::{AppHandle, Manager, Runtime};

use crate::ignore_poison::IgnorePoison;

use super::{
    CLOSE_TAB_ID, CommandScope, EDIT_COPY_ID, EDIT_CUT_ID, EDIT_PASTE_ID, EJECT_VOLUME_ID, FAVORITE_REMOVE_ID,
    FAVORITE_RENAME_ID, FAVORITES_ADD_CONTEXT_ID, MEDIA_INDEX_ADD_FOLDER_ID, MEDIA_INDEX_EXCLUDE_FOLDER_ID,
    MEDIA_INDEX_INCLUDE_FOLDER_ID, MEDIA_INDEX_REMOVE_FOLDER_ID, MediaIndexFolderChoice, MediaIndexFolderExclusion,
    MenuSort, MenuState, NETWORK_HOST_DISCONNECT_ID, NETWORK_HOST_FORGET_PASSWORD_ID, NETWORK_HOST_FORGET_SERVER_ID,
    SELECT_ALL_ID, SHOW_HIDDEN_FILES_ID, SORT_ASCENDING_ID, SORT_BY_CREATED_ID, SORT_BY_EXTENSION_ID,
    SORT_BY_MODIFIED_ID, SORT_BY_NAME_ID, SORT_BY_SIZE_ID, SORT_DESCENDING_ID, SettingsChanged, TAB_CLOSE_ID,
    TAB_CLOSE_OTHERS_ID, TAB_PIN_ID, VIEW_MODE_BRIEF_LEFT_ID, VIEW_MODE_BRIEF_RIGHT_ID, VIEW_MODE_FULL_LEFT_ID,
    VIEW_MODE_FULL_RIGHT_ID, VIEWER_WORD_WRAP_ID, ViewMode, ViewModeChanged, menu_id_to_command,
};

/// Removes macOS system-injected items from the Edit menu and registers the Help menu.
///
/// macOS AppKit automatically injects Writing Tools, AutoFill, Start Dictation, and Emoji & Symbols
/// into any menu it takes for an Edit menu. It also only shows the Help menu search field when a
/// menu is registered via `NSApplication.setHelpMenu:`. Both of these happen at the AppKit level
/// regardless of how the menu is constructed, so we fix them post-construction via native API
/// calls. Acts on whichever menu bar is installed (`app.menu()`), finding both menus by ID.
#[cfg(target_os = "macos")]
pub fn cleanup_macos_menus<R: Runtime>(app: &AppHandle<R>) {
    super::macos_appkit::cleanup_macos_menus(app);
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
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || cleanup_macos_menus(&handle)) {
        log::warn!(target: "menu", "Failed to dispatch macOS menu cleanup to the main thread: {e}");
    }
}

/// Sets SF Symbol icons on menu items post-construction via native AppKit API.
///
/// Tauri's menu API doesn't support SF Symbols, so we walk the NSMenu hierarchy after
/// construction and call `NSImage(systemSymbolName:accessibilityDescription:)` + `setImage:`
/// on each item. Which item gets which symbol is keyed by menu item ID; the ID is resolved to the
/// item's current title only to find it on the AppKit side, which knows no other index.
#[cfg(target_os = "macos")]
pub fn set_macos_menu_icons<R: Runtime>(app: &AppHandle<R>) {
    super::macos_appkit::set_macos_menu_icons(app);
}

/// Runs [`set_macos_menu_icons`] on the main thread, for callers running on a Tauri command thread.
///
/// Same fire-and-forget contract as [`cleanup_macos_menus_from_command`]: a failed hop costs icons,
/// never correctness.
#[cfg(target_os = "macos")]
pub fn set_macos_menu_icons_from_command<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    if let Err(e) = app.run_on_main_thread(move || set_macos_menu_icons(&handle)) {
        log::warn!(target: "menu", "Failed to dispatch macOS menu icons to the main thread: {e}");
    }
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
        let _ = super::view_mode_items::sync_view_mode_check_states(&menu_state);
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

    // === Image-search chosen-folder membership (media_index "Folders to index") ===
    // Same shape as the exclusion above: acts on the RIGHT-CLICKED folder and emits the
    // target membership to the FE, which persists `mediaIndex.alwaysIndexFolders` and
    // calls `media_index_set_always_index_folder` (adding kicks a pass backend-side).
    if id == MEDIA_INDEX_ADD_FOLDER_ID || id == MEDIA_INDEX_REMOVE_FOLDER_ID {
        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let folder = menu_state.context.lock_ignore_poison().path.clone();
        if folder.is_empty() {
            log::warn!(target: "media_index", "folder choice clicked with no context path, ignoring");
            return;
        }
        use tauri_specta::Event as _;
        let _ = MediaIndexFolderChoice {
            folder,
            chosen: id == MEDIA_INDEX_ADD_FOLDER_ID,
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

    // === Open with → Other… : show NSOpenPanel, then launch ===
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
            log::warn!("Open with (Other…) failed: {e}");
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
