use crate::ignore_poison::IgnorePoison;
use crate::menu::{
    CLOSE_TAB_ID, CommandScope, FileContextInfo, MenuState, REOPEN_CLOSED_TAB_ID, ViewMode,
    build_breadcrumb_context_menu, build_context_menu, build_network_host_context_menu, build_tab_context_menu,
    frontend_shortcut_to_accelerator, menu_id_to_command, rebuild_view_mode_items, sync_view_mode_check_states,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;
use tauri::menu::ContextMenu;
use tauri::{AppHandle, Emitter, Manager, Runtime, Window};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[tauri::command]
#[specta::specta]
pub fn update_menu_context<R: Runtime>(app: AppHandle<R>, path: String, filename: String) {
    let state = app.state::<MenuState<R>>();
    let mut context = state.context.lock_ignore_poison();
    context.path = path;
    context.filename = filename;
}

#[tauri::command]
#[specta::specta]
pub fn show_file_context_menu<R: Runtime>(
    window: Window<R>,
    path: String,
    filename: String,
    is_directory: bool,
    paths: Vec<String>,
) -> Result<(), String> {
    let app = window.app_handle();

    // The "primary" path drives single-file actions like "Copy 'filename'", Get info,
    // Quick look. `paths` carries the full selection that "Open with" and cloud actions
    // should apply to: it equals `[path]` when the right-clicked file isn't part of a
    // multi-selection, or the entire selection otherwise.
    let context_paths = if paths.is_empty() { vec![path.clone()] } else { paths };

    // Compute per-file context (sync status, FP-domain membership, candidate "Open with"
    // apps). The LaunchServices query for candidates can take 50-200 ms on a cold cache,
    // which delays the popup; the cache (in `file_system::open_with`) keeps later
    // right-clicks fast.
    #[cfg(target_os = "macos")]
    let info = build_file_context_info(&path, &context_paths);
    #[cfg(not(target_os = "macos"))]
    let info = FileContextInfo;

    // Update menu context so on_menu_event has paths + bundle map for the new items.
    {
        let state = app.state::<MenuState<R>>();
        let mut context = state.context.lock_ignore_poison();
        context.path = path.clone();
        context.filename = filename.clone();
        context.paths = context_paths;
        #[cfg(target_os = "macos")]
        {
            // Filled in from build_context_menu's return value below.
            context.open_with_apps.clear();
        }
    }

    let result = build_context_menu(app, &filename, is_directory, &info).map_err(|e| e.to_string())?;

    // Stash the bundle_id → app_path map so on_menu_event can resolve clicks on
    // `open-with:<bundle-id>` items back to a real app URL.
    #[cfg(target_os = "macos")]
    {
        let state = app.state::<MenuState<R>>();
        let mut context = state.context.lock_ignore_poison();
        context.open_with_apps = result.open_with_apps;
    }

    result.menu.popup(window).map_err(|e| e.to_string())?;

    Ok(())
}

#[cfg(target_os = "macos")]
fn build_file_context_info(primary_path: &str, all_paths: &[String]) -> FileContextInfo {
    use crate::file_system::cloud_actions::is_in_icloud_drive;
    use crate::file_system::open_with::compute_open_with_choices;
    use crate::file_system::sync_status::get_sync_statuses;
    use std::path::PathBuf;

    let path_buf = PathBuf::from(primary_path);
    let is_icloud_drive = is_in_icloud_drive(&path_buf);

    // Sync status of the primary path only (drives the cloud-action label).
    let sync_status = if is_icloud_drive {
        let mut statuses = get_sync_statuses(vec![primary_path.to_string()]);
        statuses.remove(primary_path).unwrap_or_default()
    } else {
        Default::default()
    };

    let open_with = compute_open_with_choices(all_paths.iter().map(PathBuf::from).collect());

    FileContextInfo {
        sync_status,
        is_icloud_drive,
        open_with,
    }
}

/// Shows a native context menu for the breadcrumb path bar.
/// The `shortcut` is the user's configured shortcut in frontend format (e.g. "⌃⌘C"),
/// or empty string if no shortcut is configured.
#[tauri::command]
#[specta::specta]
pub fn show_breadcrumb_context_menu<R: Runtime>(window: Window<R>, shortcut: String) -> Result<(), String> {
    let app = window.app_handle();
    let accelerator = frontend_shortcut_to_accelerator(&shortcut).unwrap_or_default();
    let menu = build_breadcrumb_context_menu(app, &accelerator).map_err(|e| e.to_string())?;
    menu.popup(window).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn show_main_window<R: Runtime>(window: Window<R>) -> Result<(), String> {
    // E2E: on macOS, use `orderFront:` instead of `makeKeyAndOrderFront:` so the
    // window appears without stealing focus from whatever the user is currently
    // working in. `window.show()` calls the latter on macOS, which always grabs
    // OS focus. Linux/Windows test runs happen in headless containers, so the
    // standard show is fine there.
    #[cfg(target_os = "macos")]
    if crate::test_mode::is_e2e_mode() {
        use objc2::msg_send;
        use objc2::runtime::AnyObject;
        let ns_window = window.ns_window().map_err(|e| e.to_string())? as *mut AnyObject;
        if ns_window.is_null() {
            return Err("NSWindow pointer is null".into());
        }
        unsafe {
            let _: () = msg_send![ns_window, orderFront: std::ptr::null_mut::<AnyObject>()];
        }
        return Ok(());
    }
    window.show().map_err(|e| e.to_string())
}

/// Toggle hidden files visibility - updates menu checkbox and emits event.
///
/// This is the "external trigger" path: MCP tool calls and any other Rust-side
/// caller that needs to flip the setting from outside the explorer. It updates
/// the macOS `CheckMenuItem` and emits `settings-changed` so the explorer
/// listener picks up the change.
///
/// **The keyboard-shortcut / command-palette path does NOT use this.** That
/// path mutates the explorer's FE state directly (synchronous, no Rust round-
/// trip) and uses [`sync_menu_show_hidden`] to push the new check state to the
/// native menu. Routing the FE-driven toggle through here would create an
/// IPC → event → effect → DOM-update chain that the e2e test against `⌘⇧.`
/// flaked on (~1/25) when the slow lane was under load.
#[tauri::command]
#[specta::specta]
pub fn toggle_hidden_files<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    let menu_state = app.state::<MenuState<R>>();
    let guard = menu_state.show_hidden_files.lock_ignore_poison();
    let Some(check_item) = guard.as_ref() else {
        return Err("Menu not initialized".to_string());
    };

    // Get current state and toggle it
    let current = check_item.is_checked().unwrap_or(false);
    let new_state = !current;
    check_item.set_checked(new_state).map_err(|e| e.to_string())?;

    // Emit event to frontend with the new state
    app.emit("settings-changed", serde_json::json!({ "showHiddenFiles": new_state }))
        .map_err(|e| e.to_string())?;

    Ok(new_state)
}

/// One-way sync of the native "Show hidden files" `CheckMenuItem` checked
/// state from the frontend. Does NOT emit `settings-changed`: the FE is the
/// caller, it already knows the new state and has already updated its own
/// view. Idempotent — safe to call with the current state.
#[tauri::command]
#[specta::specta]
pub fn sync_menu_show_hidden<R: Runtime>(app: AppHandle<R>, checked: bool) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    let guard = menu_state.show_hidden_files.lock_ignore_poison();
    let Some(check_item) = guard.as_ref() else {
        // Menu not yet initialized (very early in startup). The next menu
        // build will pick up the persisted setting, so a no-op here is fine.
        return Ok(());
    };
    check_item.set_checked(checked).map_err(|e| e.to_string())?;
    Ok(())
}

/// Pushes the full View menu state from the frontend: which pane is active and
/// the per-pane view modes. The menu's check states are updated for both pane
/// pairs, and if the active pane changed since the last call the keyboard
/// accelerators are migrated to the newly-active pair via
/// `rebuild_view_mode_items`. Called on initial mount, focus change, swap, and
/// after any view-mode change (palette, MCP, menu click round-trip).
#[tauri::command]
#[specta::specta]
pub fn update_view_mode_menu<R: Runtime>(
    app: AppHandle<R>,
    active_pane: String,
    left_mode: String,
    right_mode: String,
) -> Result<(), String> {
    if active_pane != "left" && active_pane != "right" {
        return Err(format!("Invalid active_pane: {active_pane}"));
    }
    let parse_mode = |s: &str| match s {
        "full" => Ok(ViewMode::Full),
        "brief" => Ok(ViewMode::Brief),
        other => Err(format!("Invalid view mode: {other}")),
    };
    let left = parse_mode(&left_mode)?;
    let right = parse_mode(&right_mode)?;

    let menu_state = app.state::<MenuState<R>>();

    // Stash new state, then decide whether a full rebuild is needed.
    let active_changed = {
        let mut guard = menu_state.view_mode_active_pane.lock_ignore_poison();
        let changed = *guard != active_pane;
        *guard = active_pane;
        changed
    };
    *menu_state.view_mode_left.lock_ignore_poison() = left;
    *menu_state.view_mode_right.lock_ignore_poison() = right;

    if active_changed {
        rebuild_view_mode_items(&app, &menu_state).map_err(|e| e.to_string())?;
    } else {
        sync_view_mode_check_states(&menu_state).map_err(|e| e.to_string())?;
    }

    Ok(())
}

// ============================================================================
// Direct file action commands (for command palette and other invocations)
// ============================================================================

/// Show a file in Finder (reveal in parent folder)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub fn show_in_finder(path: String) -> Result<(), String> {
    Command::new("open")
        .arg("-R")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Show a file in the default file manager (open parent folder via xdg-open)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "linux")]
pub fn show_in_finder(path: String) -> Result<(), String> {
    let parent = std::path::Path::new(&path)
        .parent()
        .unwrap_or(std::path::Path::new("/"));
    Command::new("xdg-open")
        .arg(parent)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn show_in_finder(_path: String) -> Result<(), String> {
    Err("Show in file manager is not available on this platform".to_string())
}

/// Copy text to clipboard
#[tauri::command]
#[specta::specta]
pub fn copy_to_clipboard<R: Runtime>(app: AppHandle<R>, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// Quick Look preview (macOS only)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub fn quick_look(path: String) -> Result<(), String> {
    Command::new("qlmanage")
        .arg("-p")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(target_os = "macos"))]
pub fn quick_look(_path: String) -> Result<(), String> {
    Ok(())
}

/// Open the Get Info window for a file (macOS only, no-op on other platforms)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub fn get_info(path: String) -> Result<(), String> {
    // Pass the path as a positional argument via `on run argv` to avoid AppleScript injection.
    let script = r#"on run argv
        tell application "Finder"
            activate
            open information window of (POSIX file (item 1 of argv) as alias)
        end tell
    end run"#;

    Command::new("osascript")
        .arg("-e")
        .arg(script)
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(target_os = "macos"))]
pub fn get_info(_path: String) -> Result<(), String> {
    Ok(())
}

/// Open file in the system's default text editor (macOS only)
#[tauri::command]
#[specta::specta]
#[cfg(target_os = "macos")]
pub fn open_in_editor(path: String) -> Result<(), String> {
    Command::new("open")
        .arg("-t")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

/// Make a cloud-managed file available offline (download it). On macOS, talks to the
/// File Provider extension responsible for the file (iCloud Drive, Dropbox, GDrive,
/// OneDrive, Box, etc.).
#[tauri::command]
#[specta::specta]
pub async fn cloud_make_available_offline(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || {
        crate::file_system::cloud_actions::request_download(std::path::Path::new(&path))
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Evict a cloud-managed file's local copy, leaving a placeholder. Counterpart to
/// `cloud_make_available_offline`.
#[tauri::command]
#[specta::specta]
pub async fn cloud_remove_download(path: String) -> Result<(), String> {
    tokio::task::spawn_blocking(move || crate::file_system::cloud_actions::evict_item(std::path::Path::new(&path)))
        .await
        .map_err(|e| e.to_string())?
}

#[tauri::command]
#[specta::specta]
#[cfg(target_os = "linux")]
pub fn open_in_editor(path: String) -> Result<(), String> {
    Command::new("xdg-open").arg(&path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn open_in_editor(_path: String) -> Result<(), String> {
    Err("Open in editor is not available on this platform".to_string())
}

/// Shows a native context menu for a tab (fire-and-forget).
/// The selected action is delivered asynchronously via a `tab-context-action` Tauri event
/// from `on_menu_event`, because `popup()` returns before the event loop processes the
/// `MenuEvent` from muda. A synchronous channel approach doesn't work here: the wakeup
/// signal posted during the popup's NSEvent tracking loop gets consumed, so `recv` always
/// times out.
#[tauri::command]
#[specta::specta]
pub fn show_tab_context_menu(
    window: Window<tauri::Wry>,
    is_pinned: bool,
    can_close: bool,
    has_other_unpinned_tabs: bool,
) -> Result<(), String> {
    let app = window.app_handle().clone();

    let menu =
        build_tab_context_menu(&app, is_pinned, can_close, has_other_unpinned_tabs).map_err(|e| e.to_string())?;
    menu.popup(window).map_err(|e| e.to_string())?;

    Ok(())
}

/// Shows a native context menu for a network host (fire-and-forget).
/// The selected action is delivered asynchronously via a `network-host-context-action` Tauri event
/// from `on_menu_event`.
#[tauri::command]
#[specta::specta]
pub fn show_network_host_context_menu(
    window: Window<tauri::Wry>,
    host_id: String,
    host_name: String,
    is_manual: bool,
    has_credentials: bool,
) -> Result<(), String> {
    let app = window.app_handle().clone();

    let menu = build_network_host_context_menu(&app, is_manual, has_credentials).map_err(|e| e.to_string())?;

    // Store context so on_menu_event can include host info in the emitted event
    {
        let state = app.state::<MenuState<tauri::Wry>>();
        let mut ctx = state.network_host_context.lock_ignore_poison();
        ctx.host_id = host_id;
        ctx.host_name = host_name;
    }

    menu.popup(window).map_err(|e| e.to_string())?;

    Ok(())
}

/// Updates the File menu "Pin tab" / "Unpin tab" label based on the active tab's pin state.
#[tauri::command]
#[specta::specta]
pub fn update_pin_tab_menu<R: Runtime>(app: AppHandle<R>, is_pinned: bool) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    let guard = menu_state.pin_tab.lock_ignore_poison();
    let Some(item) = guard.as_ref() else {
        return Err("Menu not initialized".to_string());
    };
    let label = if is_pinned { "Unpin tab" } else { "Pin tab" };
    item.set_text(label).map_err(|e| e.to_string())
}

/// Enables or disables the Tab menu "Reopen closed tab" item based on whether the
/// focused pane's closed-tab stack has entries. Mirrors the dynamic-label pattern
/// used by `update_pin_tab_menu`.
#[tauri::command]
#[specta::specta]
pub fn set_reopen_closed_tab_enabled<R: Runtime>(app: AppHandle<R>, enabled: bool) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    let guard = menu_state.reopen_closed_tab.lock_ignore_poison();
    let Some(item) = guard.as_ref() else {
        return Err("Menu not initialized".to_string());
    };
    item.set_enabled(enabled).map_err(|e| e.to_string())
}

/// Enables or disables explorer-scoped menu items based on the current context.
/// - `"explorer"`: all menu items enabled (main file explorer has focus)
/// - `"other"`: all non-App items disabled except Close tab (⌘W), which doubles as
///   "close the focused window" (standard macOS behavior)
#[tauri::command]
#[specta::specta]
pub fn set_menu_context<R: Runtime>(app: AppHandle<R>, context: String) -> Result<(), String> {
    let enabled = context == "explorer";
    let menu_state = app.state::<MenuState<R>>();

    for (id, entry) in menu_state.items.lock_ignore_poison().iter() {
        // Close tab stays enabled: on_menu_event has special logic to close the focused
        // non-main window when main isn't focused (standard ⌘W behavior on macOS).
        if id == CLOSE_TAB_ID {
            continue;
        }
        // Reopen closed tab is managed exclusively by `set_reopen_closed_tab_enabled`:
        // skip it here so an "explorer" context switch doesn't enable it while the
        // focused pane's closed-tab stack is empty.
        if id == REOPEN_CLOSED_TAB_ID {
            continue;
        }
        let is_app = matches!(menu_id_to_command(id), Some((_, CommandScope::App)));
        if !is_app {
            let _ = entry.item.set_enabled(enabled);
        }
    }

    // Items stored in separate MenuState fields (not in the HashMap)
    if let Some(ref item) = *menu_state.pin_tab.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    if let Some(ref item) = *menu_state.show_hidden_files.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    if let Some(ref item) = *menu_state.view_mode_full_left.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    if let Some(ref item) = *menu_state.view_mode_brief_left.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    if let Some(ref item) = *menu_state.view_mode_full_right.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    if let Some(ref item) = *menu_state.view_mode_brief_right.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    // Disable the parent "Left pane" / "Right pane" submenus too, so they appear
    // greyed out instead of opening to reveal disabled items.
    if let Some(ref submenu) = *menu_state.view_left_pane_submenu.lock_ignore_poison() {
        let _ = submenu.set_enabled(enabled);
    }
    if let Some(ref submenu) = *menu_state.view_right_pane_submenu.lock_ignore_poison() {
        let _ = submenu.set_enabled(enabled);
    }
    if let Some(ref submenu) = *menu_state.sort_submenu.lock_ignore_poison() {
        let _ = submenu.set_enabled(enabled);
    }

    Ok(())
}
