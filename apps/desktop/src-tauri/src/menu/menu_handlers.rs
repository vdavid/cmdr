//! What happens when someone picks a menu item.
//!
//! `handle_menu_event` is the `.on_menu_event` dispatcher wired into the Tauri
//! builder: it maps a clicked item's ID to a command, a settings toggle, or a
//! native responder-chain action. The macOS post-construction passes live here
//! too (`cleanup_macos_menus`, `set_macos_menu_icons`, `set_display_accelerators`),
//! since they share this file's platform seam onto AppKit.
//!
//! The other two live-update lanes moved out: `accelerators.rs` for shortcut
//! strings, `view_mode_items.rs` for the per-pane view-mode items.

#[cfg(target_os = "macos")]
use tauri::Runtime;
use tauri::{AppHandle, Manager};

use crate::ignore_poison::IgnorePoison;
use crate::volume_broadcast::VolumeContextActionKind;
use crate::window_events::ViewerEditActionKind;

use super::{
    CLOSE_TAB_ID, CommandScope, EDIT_COPY_ID, EDIT_CUT_ID, EDIT_PASTE_ID, EJECT_VOLUME_ID, FAVORITES_ADD_CONTEXT_ID,
    FUNCTION_KEY_BAR_HIDE_ID, MEDIA_INDEX_ADD_FOLDER_ID, MEDIA_INDEX_EXCLUDE_FOLDER_ID, MEDIA_INDEX_INCLUDE_FOLDER_ID,
    MEDIA_INDEX_REMOVE_FOLDER_ID, MediaIndexFolderChoice, MediaIndexFolderExclusion, MenuSort, MenuState,
    NETWORK_HOST_DISCONNECT_ID, NETWORK_HOST_EDIT_ID, NETWORK_HOST_FORGET_SECRET_ID, NETWORK_HOST_FORGET_SERVER_ID,
    SELECT_ALL_ID, SHOW_HIDDEN_FILES_ID, SHOW_SEARCH_RESULT_IN_FOLDER_ID, SORT_ASCENDING_ID, SORT_BY_CREATED_ID,
    SORT_BY_EXTENSION_ID, SORT_BY_MODIFIED_ID, SORT_BY_NAME_ID, SORT_BY_SIZE_ID, SORT_DESCENDING_ID, SettingsChanged,
    TAB_CLOSE_ID, TAB_CLOSE_OTHERS_ID, TAB_PIN_ID, VIEW_MODE_BRIEF_LEFT_ID, VIEW_MODE_BRIEF_RIGHT_ID,
    VIEW_MODE_FULL_LEFT_ID, VIEW_MODE_FULL_RIGHT_ID, VIEW_SET_MODE_COMMAND_ID, VIEW_SHOW_HIDDEN_COMMAND_ID,
    VIEWER_CONTEXT_COPY_ID, VIEWER_CONTEXT_SELECT_ALL_ID, VIEWER_EDIT_COPY_ID, VIEWER_EDIT_CUT_ID,
    VIEWER_EDIT_PASTE_ID, VIEWER_SELECT_ALL_ID, VIEWER_WORD_WRAP_ID, ViewMode, ViewModeChanged, menu_id_to_command,
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

/// Draws the display-only accelerators on the installed menu bar, post-construction via AppKit.
///
/// The counterpart of [`set_macos_menu_icons`] for the shortcuts a menu item can only SHOW
/// (`menu_spec::ItemSpec::display_accelerator`), and it has to run everywhere that one does:
/// neither an attributed title nor an image survives a fresh `NSMenuItem`.
#[cfg(target_os = "macos")]
pub fn set_display_accelerators<R: Runtime>(app: &AppHandle<R>, menu_state: &MenuState<R>) {
    super::display_accelerators::set_display_accelerators(app, menu_state);
}

/// Runs [`set_display_accelerators`] on the main thread, for callers on a Tauri command thread.
///
/// Same fire-and-forget contract as [`set_macos_menu_icons_from_command`]: a failed hop costs a
/// glyph until the next menu-bar swap, never correctness. Reads `MenuState` inside the hop, which
/// is safe because every caller of this variant runs long after `app.manage`.
#[cfg(target_os = "macos")]
pub fn set_display_accelerators_from_command<R: Runtime>(app: &AppHandle<R>) {
    let handle = app.clone();
    let dispatched = app.run_on_main_thread(move || {
        let menu_state = handle.state::<MenuState<R>>();
        set_display_accelerators(&handle, &menu_state);
    });
    if let Err(e) = dispatched {
        log::warn!(target: "menu", "Failed to dispatch the menu bar's display accelerators to the main thread: {e}");
    }
}

/// A native text-editing selector a Custom menu item forwards to the first responder.
///
/// A typed variant rather than the selector's name as a string, so the id → selector decision
/// can be tested on every platform while `sel!` stays macOS-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NativeEditSelector {
    Cut,
    Copy,
    Paste,
    SelectAll,
}

/// The native selector menu item `menu_id` forwards, or `None` when it forwards none.
///
/// Both bars are in here. The main bar's Edit / Select items take this lane only outside the
/// main window (`handle_menu_event`), while the viewer bar's Cut and Paste always do: the
/// viewer's search box is the only editable field in that window. ❗ The viewer bar's Copy and
/// Select all are deliberately absent — they act on the viewer's own offset-based selection,
/// which no native selector can reach (`viewer_edit_action_for`).
#[cfg_attr(
    not(target_os = "macos"),
    allow(
        dead_code,
        reason = "the responder chain is macOS-only; the mapping is still pinned by tests everywhere"
    )
)]
pub(crate) fn native_edit_selector_for(menu_id: &str) -> Option<NativeEditSelector> {
    match menu_id {
        EDIT_CUT_ID | VIEWER_EDIT_CUT_ID => Some(NativeEditSelector::Cut),
        EDIT_COPY_ID => Some(NativeEditSelector::Copy),
        EDIT_PASTE_ID | VIEWER_EDIT_PASTE_ID => Some(NativeEditSelector::Paste),
        SELECT_ALL_ID => Some(NativeEditSelector::SelectAll),
        _ => None,
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

    let selector = match native_edit_selector_for(menu_id) {
        Some(NativeEditSelector::Cut) => sel!(cut:),
        Some(NativeEditSelector::Copy) => sel!(copy:),
        Some(NativeEditSelector::Paste) => sel!(paste:),
        Some(NativeEditSelector::SelectAll) => sel!(selectAll:),
        None => return,
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

/// The label of the viewer window that currently has focus, if one does.
///
/// The viewer bar is app-level on macOS and shared by every open viewer, so an item in it names
/// no window of its own: the focused one is the only honest answer. Viewer windows are labeled
/// `viewer-<n>` (`commands/file_viewer.rs`, `capabilities/viewer.json`).
fn focused_viewer_label(app: &AppHandle<tauri::Wry>) -> Option<String> {
    app.webview_windows()
        .into_iter()
        .find(|(label, window)| label.starts_with("viewer-") && window.is_focused().unwrap_or(false))
        .map(|(label, _)| label)
}

/// The viewer Edit-menu action a clicked item id names, or `None` when the id isn't one of them.
fn viewer_edit_action_for(menu_id: &str) -> Option<ViewerEditActionKind> {
    match menu_id {
        VIEWER_EDIT_COPY_ID => Some(ViewerEditActionKind::Copy),
        VIEWER_SELECT_ALL_ID => Some(ViewerEditActionKind::SelectAll),
        _ => None,
    }
}

/// The viewer right-click menu's action a clicked item id names, or `None` when the id isn't one
/// of them.
fn viewer_context_action_for(menu_id: &str) -> Option<ViewerEditActionKind> {
    match menu_id {
        VIEWER_CONTEXT_COPY_ID => Some(ViewerEditActionKind::Copy),
        VIEWER_CONTEXT_SELECT_ALL_ID => Some(ViewerEditActionKind::SelectAll),
        _ => None,
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

    if id == SHOW_SEARCH_RESULT_IN_FOLDER_ID {
        let state = app.state::<MenuState<tauri::Wry>>();
        let path = {
            let context = state.context.lock_ignore_poison();
            search_result_reveal_path(id, &context)
        };
        if let Some(path) = path {
            use tauri_specta::Event as _;
            let _ = crate::window_events::ShowSearchResultInFolder { path }.emit_to(app, "main");
        }
        return;
    }

    // === CheckMenuItem exceptions: sync checked state and emit directly ===
    // These must NOT go through "execute-command", as that would double-toggle.
    //
    // ❗ They're also the one place the main window's dialog gate is applied in Rust. The item has
    // already toggled itself by the time we hear of the click, and the frontend can't untoggle it:
    // show hidden never reaches the dispatch core at all, and a refused `view.setMode` would leave
    // the check on a mode the pane isn't in. So a refused click puts the check back and stops here.
    // A disabled item's accelerator still fires, so the greying alone doesn't cover it.
    if id == SHOW_HIDDEN_FILES_ID {
        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let guard = menu_state.show_hidden_files.lock_ignore_poison();
        if let Some(check_item) = guard.as_ref() {
            let new_state = check_item.is_checked().unwrap_or(true);
            if menu_state.refuses_over_dialog(VIEW_SHOW_HIDDEN_COMMAND_ID) {
                let _ = check_item.set_checked(!new_state);
                return;
            }
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
        if menu_state.refuses_over_dialog(VIEW_SET_MODE_COMMAND_ID) {
            // Re-sync every check from the stored modes, which undoes muda's toggle.
            let _ = super::view_mode_items::sync_view_mode_check_states(&menu_state);
            return;
        }
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
    // (which favorites the focused-pane dir instead). ❗ It goes through the `add_favorite` COMMAND
    // rather than `favorites::store::add`, so this surface meets the same add gate as the palette
    // and the MCP tool; a `store::add` here would let a right-click inside an archive or on a phone
    // store a favorite nothing can ever show. The command also owns the blocking pool, the timeout,
    // and the `volumes-changed` re-emit.
    if id == FAVORITES_ADD_CONTEXT_ID {
        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let path = menu_state.context.lock_ignore_poison().path.clone();
        if path.is_empty() {
            log::warn!(target: "favorites", "Add to favorites: empty context path, ignoring");
            return;
        }
        tauri::async_runtime::spawn(async move {
            if let Err(e) = crate::commands::favorites::add_favorite(path, None).await {
                log::warn!(target: "favorites", "Add to favorites: {e}");
            }
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
        if let Some(label) = focused_viewer_label(app) {
            use tauri_specta::Event as _;
            let _ = crate::window_events::ViewerWordWrapToggled.emit_to(app, &label);
        }
        return;
    }

    // === Viewer Edit > Copy / Select all: emit to the focused viewer window ===
    // The viewer runs both itself, over its own offset-based selection; see
    // `window_events::ViewerEditAction` for why these can't be native selectors.
    if let Some(action) = viewer_edit_action_for(id) {
        let Some(label) = focused_viewer_label(app) else {
            log::warn!(target: "menu", "Viewer Edit item {id} clicked with no viewer focused, ignoring");
            return;
        };
        use tauri_specta::Event as _;
        let _ = crate::window_events::ViewerEditAction { action }.emit_to(app, &label);
        return;
    }

    // === The viewer's right-click menu: emit to the viewer it was popped over ===
    // `show_viewer_context_menu` focused that window before the popup, so the focused one is it.
    if let Some(action) = viewer_context_action_for(id) {
        let Some(label) = focused_viewer_label(app) else {
            log::warn!(target: "menu", "Viewer context item {id} clicked with no viewer focused, ignoring");
            return;
        };
        use tauri_specta::Event as _;
        let _ = crate::window_events::ViewerContextMenuAction { action }.emit_to(app, &label);
        return;
    }

    // === Viewer Edit > Cut / Paste: forward the native selector to the focused text field ===
    // The viewer's search box, the only editable thing in that window, which is also why
    // `apply_menu_item_states` greys these two out while it doesn't have focus. ❗ Never the
    // main bar's lane below: that one asks whether the MAIN window is focused, and a viewer
    // click would answer no and land in the same place by accident rather than by rule.
    if id == VIEWER_EDIT_CUT_ID || id == VIEWER_EDIT_PASTE_ID {
        #[cfg(target_os = "macos")]
        send_native_edit_action(id);
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

    // === Function key bar context menu: emit function-key-bar-hide-requested ===
    // No context to stash: the frontend owns both the setting write and the toast.
    if id == FUNCTION_KEY_BAR_HIDE_ID {
        use tauri_specta::Event as _;
        let _ = crate::window_events::FunctionKeyBarHideRequested.emit_to(app, "main");
        return;
    }

    // === The breadcrumb menu's Eject ===
    // Routed back to the frontend through the `volume-context-action` event with the target
    // stashed in `volume_row_context`. (A switcher row's and a favorite's actions never come
    // through here: they're the in-app `Menu`'s, picked in the frontend.)
    if let Some(action) = volume_row_action(id) {
        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let ctx = menu_state.volume_row_context.lock_ignore_poison();
        if ctx.volume_id.is_empty() {
            log::warn!(target: "menu", "Volume row menu item {id} clicked with no volume_id stashed");
            return;
        }
        use tauri_specta::Event as _;
        let payload = crate::volume_broadcast::VolumeContextAction {
            action,
            volume_id: ctx.volume_id.clone(),
            volume_name: ctx.volume_name.clone(),
        };
        let _ = payload.emit_to(app, "main");
        return;
    }

    // === Network host context menu actions ===
    if id == NETWORK_HOST_FORGET_SERVER_ID
        || id == NETWORK_HOST_FORGET_SECRET_ID
        || id == NETWORK_HOST_DISCONNECT_ID
        || id == NETWORK_HOST_EDIT_ID
    {
        use crate::network::NetworkHostContextActionKind;
        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let ctx = menu_state.network_host_context.lock_ignore_poison();
        let action = if id == NETWORK_HOST_FORGET_SERVER_ID {
            NetworkHostContextActionKind::ForgetServer
        } else if id == NETWORK_HOST_FORGET_SECRET_ID {
            NetworkHostContextActionKind::ForgetSecret
        } else if id == NETWORK_HOST_EDIT_ID {
            NetworkHostContextActionKind::Edit
        } else {
            NetworkHostContextActionKind::Disconnect
        };
        use tauri_specta::Event as _;
        let payload = crate::network::NetworkHostContextAction {
            action,
            row_id: ctx.row_id.clone(),
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
        use std::path::PathBuf;

        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let ctx = menu_state.context.lock_ignore_poison();
        let app_path = ctx.open_with_apps.get(bundle_id).cloned();
        let paths: Vec<PathBuf> = ctx.paths.iter().map(PathBuf::from).collect();
        drop(ctx);

        if let Some(app_path) = app_path
            && !paths.is_empty()
        {
            super::open_with::launch_with(app, paths, app_path);
        } else {
            log::warn!("Open with: missing app or paths for {bundle_id}");
        }
        return;
    }

    // === Open with → Other… : show NSOpenPanel, then launch ===
    #[cfg(target_os = "macos")]
    if id == super::open_with::OPEN_WITH_OTHER_ID {
        use crate::file_system::open_with::pick_app_via_open_panel;
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
        {
            super::open_with::launch_with(app, paths, app_path);
        }
        return;
    }

    // === Share → one service: run it on the rows the submenu was built from ===
    // The ID carries an index into the offer `show_file_context_menu` enumerated, so
    // this needs no path lookup at all: the service performs on the very `NSURL`s
    // macOS vetted, which is also what keeps it on the RIGHT-CLICKED rows rather than
    // the focused-pane selection.
    //
    // Deferred one main-thread turn instead of performed inline: we're inside muda's
    // action while the menu's own tracking loop is still unwinding, and a service
    // usually puts a window or sheet up, which that unwind can dismiss.
    #[cfg(target_os = "macos")]
    if let Some(index) = super::share_submenu::share_service_index(id) {
        use crate::file_system::share::{ShareError, perform_offered};

        if let Err(e) = app.run_on_main_thread(move || {
            let Some(mtm) = objc2::MainThreadMarker::new() else {
                log::warn!(target: "menu", "Share: the main-thread hop didn't land on the main thread");
                return;
            };
            if let Err(reason) = perform_offered(mtm, index) {
                // A refusal is a share that never happened, which the user sees as
                // nothing happening; naming the state is what makes that debuggable.
                let state = match reason {
                    ShareError::NoSuchService => "no service at that index",
                };
                log::warn!(target: "menu", "Share didn't start: {state}");
            }
        }) {
            log::warn!(target: "menu", "Share: couldn't reach the main thread: {e}");
        }
        return;
    }

    // === A File Provider action: the provider's own ===
    // The ID carries an index into the offer the menu was built from, so the action runs on
    // exactly the rows and domain that offer was evaluated for. Not through
    // `menu_id_to_command`: the items exist only once File Provider vouched for the rows,
    // which a palette entry or shortcut couldn't check first. The provider shows its own
    // windows, and the File Provider calls run on a thread of their own.
    #[cfg(target_os = "macos")]
    if let Some(index) = super::file_provider_items::file_provider_action_index(id) {
        let menu_state = app.state::<MenuState<tauri::Wry>>();
        let offer = menu_state.context.lock_ignore_poison().file_provider_offer.clone();
        match offer {
            Some(offer) => {
                if let Err(reason) = crate::file_system::file_provider_actions::perform(&offer, index) {
                    log::warn!(target: "menu", "File Provider action didn't start: {reason:?}");
                }
            }
            None => log::warn!(target: "menu", "File Provider action clicked with no offer armed"),
        }
        return;
    }

    // === Share → Edit extensions: System Settings' Extensions pane ===
    // Handled here rather than through `menu_id_to_command` because it isn't a file
    // command: there's nothing to bind a shortcut to and nothing for the palette to
    // offer, and the whole `Share` submenu is built and routed in the backend anyway.
    // `open_system_settings_url` is the house helper for these deep links (the Tauri
    // opener plugin's allowlist drops the `x-apple.systempreferences:` scheme silently).
    #[cfg(target_os = "macos")]
    if id == super::share_submenu::SHARE_EDIT_EXTENSIONS_ID {
        if let Err(e) =
            crate::permissions::open_system_settings_url(super::share_submenu::EXTENSIONS_SETTINGS_URL.to_string())
        {
            log::warn!(target: "menu", "Share: couldn't open the Extensions settings pane: {e}");
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
            // Focus guard: a file command acts on the main window's pane, so it must not
            // fire while the user is in Settings or the viewer — an accelerator reaches
            // this handler whatever has focus, and a greyed-looking item is only a hint.
            let focused = app
                .get_webview_window("main")
                .is_some_and(|w| w.is_focused().unwrap_or(false));
            if !focused {
                // ❗ Say so. This refuses a click the user just made and shows them
                // nothing, so with no line here the symptom is "the menu did nothing"
                // and the log holds no trace of it at all: the context menu had already
                // drawn the item as enabled, and the drop happens after that. Cost one
                // debug line rather than another blind bug report.
                log::debug!(
                    target: "menu",
                    "dropped `{command_id}`: file-scoped, and the main window doesn't have focus"
                );
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

/// The primary row path carried by the snapshot-only context action.
fn search_result_reveal_path(menu_id: &str, context: &super::MenuContext) -> Option<String> {
    (menu_id == SHOW_SEARCH_RESULT_IN_FOLDER_ID && !context.path.is_empty()).then(|| context.path.clone())
}

/// The action a volume menu id stands for, or `None` when the id belongs to some
/// other menu.
///
/// ❗ One table, so the ids the handler recognizes and the actions it emits can't
/// drift apart: every id that reaches the branch above answers here, and every
/// answer is a typed variant rather than a string the frontend has to guess.
fn volume_row_action(id: &str) -> Option<VolumeContextActionKind> {
    match id {
        EJECT_VOLUME_ID => Some(VolumeContextActionKind::Eject),
        _ => None,
    }
}

#[cfg(test)]
mod volume_row_action_tests {
    use super::*;

    #[test]
    fn the_breadcrumb_eject_id_maps_to_eject() {
        assert_eq!(volume_row_action(EJECT_VOLUME_ID), Some(VolumeContextActionKind::Eject));
    }

    /// ❗ The SMB hub's host menu rides its OWN event with a host id, so an id
    /// from it must not fall into the volume-row branch and emit a volume action
    /// against whatever id happened to be stashed.
    #[test]
    fn a_network_host_menu_id_is_not_a_volume_row_action() {
        assert_eq!(volume_row_action(NETWORK_HOST_DISCONNECT_ID), None);
        assert_eq!(volume_row_action(NETWORK_HOST_FORGET_SERVER_ID), None);
        assert_eq!(volume_row_action(NETWORK_HOST_FORGET_SECRET_ID), None);
        assert_eq!(volume_row_action("tab_close"), None);
    }
}

#[cfg(test)]
mod viewer_edit_action_tests {
    use super::*;
    use crate::menu::{EDIT_COPY_ID, SELECT_ALL_ID};

    #[test]
    fn the_viewer_bars_edit_items_name_their_action() {
        assert_eq!(
            viewer_edit_action_for(VIEWER_EDIT_COPY_ID),
            Some(ViewerEditActionKind::Copy)
        );
        assert_eq!(
            viewer_edit_action_for(VIEWER_SELECT_ALL_ID),
            Some(ViewerEditActionKind::SelectAll)
        );
    }

    /// ❗ Both bars share `EDIT_MENU_ID`, and the main bar's Copy / Select all route
    /// somewhere else entirely (the main window, or the native responder chain). A main-bar
    /// id falling into this branch would copy the wrong thing from the wrong window.
    #[test]
    fn a_main_bar_edit_id_is_not_a_viewer_edit_action() {
        assert_eq!(viewer_edit_action_for(EDIT_COPY_ID), None);
        assert_eq!(viewer_edit_action_for(SELECT_ALL_ID), None);
        assert_eq!(viewer_edit_action_for(VIEWER_WORD_WRAP_ID), None);
        assert_eq!(viewer_edit_action_for("unknown_id"), None);
    }

    #[test]
    fn the_viewer_context_menus_items_name_their_action() {
        assert_eq!(
            viewer_context_action_for(VIEWER_CONTEXT_COPY_ID),
            Some(ViewerEditActionKind::Copy)
        );
        assert_eq!(
            viewer_context_action_for(VIEWER_CONTEXT_SELECT_ALL_ID),
            Some(ViewerEditActionKind::SelectAll)
        );
    }

    /// ❗ The bar's pair defers to the search box when it has focus; the right-click pair
    /// always acts on the file. Crossing the two would copy the query from a menu opened over
    /// the text, or the reverse.
    #[test]
    fn the_bar_and_the_context_menu_never_share_an_item() {
        assert_eq!(viewer_context_action_for(VIEWER_EDIT_COPY_ID), None);
        assert_eq!(viewer_context_action_for(VIEWER_SELECT_ALL_ID), None);
        assert_eq!(viewer_edit_action_for(VIEWER_CONTEXT_COPY_ID), None);
        assert_eq!(viewer_edit_action_for(VIEWER_CONTEXT_SELECT_ALL_ID), None);
    }
}

#[cfg(test)]
mod native_edit_selector_tests {
    use super::*;

    /// The guarantee the viewer's Cut and Paste exist for: whatever else changes about them,
    /// the click still ends as `cut:` / `paste:` on the first responder, which is the search
    /// box. Trimming these two once left ⌘X / ⌘V dead in the viewer's search field.
    #[test]
    fn the_viewer_bars_cut_and_paste_forward_the_native_selector() {
        assert_eq!(
            native_edit_selector_for(VIEWER_EDIT_CUT_ID),
            Some(NativeEditSelector::Cut)
        );
        assert_eq!(
            native_edit_selector_for(VIEWER_EDIT_PASTE_ID),
            Some(NativeEditSelector::Paste)
        );
    }

    #[test]
    fn the_main_bars_four_edit_items_keep_their_selectors() {
        assert_eq!(native_edit_selector_for(EDIT_CUT_ID), Some(NativeEditSelector::Cut));
        assert_eq!(native_edit_selector_for(EDIT_COPY_ID), Some(NativeEditSelector::Copy));
        assert_eq!(native_edit_selector_for(EDIT_PASTE_ID), Some(NativeEditSelector::Paste));
        assert_eq!(
            native_edit_selector_for(SELECT_ALL_ID),
            Some(NativeEditSelector::SelectAll)
        );
    }

    /// ❗ The viewer's Copy and Select all act on its own offset-based selection, which lives
    /// outside the DOM the responder chain reaches. A native selector would land on the status
    /// bar instead, which is the bug `viewer_edit_action_for` exists to avoid.
    #[test]
    fn the_viewer_bars_copy_and_select_all_forward_nothing() {
        assert_eq!(native_edit_selector_for(VIEWER_EDIT_COPY_ID), None);
        assert_eq!(native_edit_selector_for(VIEWER_SELECT_ALL_ID), None);
        assert_eq!(native_edit_selector_for(VIEWER_WORD_WRAP_ID), None);
        assert_eq!(native_edit_selector_for("unknown_id"), None);
    }
}

#[cfg(test)]
mod search_result_reveal_tests {
    use super::*;
    use crate::menu::{MenuContext, SHOW_IN_FINDER_ID};

    #[test]
    fn show_in_folder_carries_the_primary_right_clicked_path_not_the_selection() {
        let context = MenuContext {
            path: "/share/B.txt".to_string(),
            paths: vec!["/share/A.txt".to_string(), "/share/C.txt".to_string()],
            ..MenuContext::default()
        };

        assert_eq!(
            search_result_reveal_path(SHOW_SEARCH_RESULT_IN_FOLDER_ID, &context),
            Some("/share/B.txt".to_string())
        );
        assert_eq!(search_result_reveal_path(SHOW_IN_FINDER_ID, &context), None);
    }
}
