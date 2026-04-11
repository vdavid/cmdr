use crate::ignore_poison::IgnorePoison;
use crate::menu::{
    CLOSE_TAB_ID, CommandScope, MenuState, build_breadcrumb_context_menu, build_context_menu,
    build_network_host_context_menu, build_tab_context_menu, frontend_shortcut_to_accelerator, menu_id_to_command,
};
#[cfg(any(target_os = "macos", target_os = "linux"))]
use std::process::Command;
use tauri::menu::ContextMenu;
use tauri::{AppHandle, Emitter, Manager, Runtime, Window};
use tauri_plugin_clipboard_manager::ClipboardExt;

#[tauri::command]
pub fn update_menu_context<R: Runtime>(app: AppHandle<R>, path: String, filename: String) {
    let state = app.state::<MenuState<R>>();
    let mut context = state.context.lock_ignore_poison();
    context.path = path;
    context.filename = filename;
}

#[tauri::command]
pub fn show_file_context_menu<R: Runtime>(
    window: Window<R>,
    path: String,
    filename: String,
    is_directory: bool,
) -> Result<(), String> {
    let app = window.app_handle();

    // Update context first so menu events have the right data
    update_menu_context(app.clone(), path, filename.clone());

    let menu = build_context_menu(app, &filename, is_directory).map_err(|e| e.to_string())?;
    menu.popup(window).map_err(|e| e.to_string())?;

    Ok(())
}

/// Shows a native context menu for the breadcrumb path bar.
/// The `shortcut` is the user's configured shortcut in frontend format (e.g. "⌃⌘C"),
/// or empty string if no shortcut is configured.
#[tauri::command]
pub fn show_breadcrumb_context_menu<R: Runtime>(window: Window<R>, shortcut: String) -> Result<(), String> {
    let app = window.app_handle();
    let accelerator = frontend_shortcut_to_accelerator(&shortcut).unwrap_or_default();
    let menu = build_breadcrumb_context_menu(app, &accelerator).map_err(|e| e.to_string())?;
    menu.popup(window).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
pub fn show_main_window<R: Runtime>(window: Window<R>) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())
}

/// Toggle hidden files visibility - updates menu checkbox and emits event.
/// This is used by the command palette to sync with menu state.
#[tauri::command]
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

/// Set view mode - updates menu radio buttons and emits event.
/// This is used by the command palette to sync with menu state.
#[tauri::command]
pub fn set_view_mode<R: Runtime>(app: AppHandle<R>, mode: String) -> Result<(), String> {
    sync_view_mode_menu_impl::<R>(&app, &mode)?;

    // Emit event to frontend
    app.emit("view-mode-changed", serde_json::json!({ "mode": mode }))
        .map_err(|e| e.to_string())?;

    Ok(())
}

/// Sync the View menu checkmarks to match the given mode, without emitting any event.
/// Called when the focused pane changes so the menu reflects the active pane's view mode.
#[tauri::command]
pub fn sync_view_mode_menu<R: Runtime>(app: AppHandle<R>, mode: String) -> Result<(), String> {
    sync_view_mode_menu_impl::<R>(&app, &mode)
}

fn sync_view_mode_menu_impl<R: Runtime>(app: &AppHandle<R>, mode: &str) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    let full_guard = menu_state.view_mode_full.lock_ignore_poison();
    let brief_guard = menu_state.view_mode_brief.lock_ignore_poison();

    let (Some(full_item), Some(brief_item)) = (full_guard.as_ref(), brief_guard.as_ref()) else {
        return Err("Menu not initialized".to_string());
    };

    let is_full = mode == "full";
    full_item.set_checked(is_full).map_err(|e| e.to_string())?;
    brief_item.set_checked(!is_full).map_err(|e| e.to_string())?;

    Ok(())
}

// ============================================================================
// Direct file action commands (for command palette and other invocations)
// ============================================================================

/// Show a file in Finder (reveal in parent folder)
#[tauri::command]
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
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn show_in_finder(_path: String) -> Result<(), String> {
    Err("Show in file manager is not available on this platform".to_string())
}

/// Copy text to clipboard
#[tauri::command]
pub fn copy_to_clipboard<R: Runtime>(app: AppHandle<R>, text: String) -> Result<(), String> {
    app.clipboard().write_text(text).map_err(|e| e.to_string())
}

/// Quick Look preview (macOS only)
#[tauri::command]
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
#[cfg(not(target_os = "macos"))]
pub fn quick_look(_path: String) -> Result<(), String> {
    Ok(())
}

/// Open the Get Info window for a file (macOS only, no-op on other platforms)
#[tauri::command]
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
#[cfg(not(target_os = "macos"))]
pub fn get_info(_path: String) -> Result<(), String> {
    Ok(())
}

/// Open file in the system's default text editor (macOS only)
#[tauri::command]
#[cfg(target_os = "macos")]
pub fn open_in_editor(path: String) -> Result<(), String> {
    Command::new("open")
        .arg("-t")
        .arg(&path)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[cfg(target_os = "linux")]
pub fn open_in_editor(path: String) -> Result<(), String> {
    Command::new("xdg-open").arg(&path).spawn().map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub fn open_in_editor(_path: String) -> Result<(), String> {
    Err("Open in editor is not available on this platform".to_string())
}

/// Shows a native context menu for a tab (fire-and-forget).
/// The selected action is delivered asynchronously via a `tab-context-action` Tauri event
/// from `on_menu_event`, because `popup()` returns before the event loop processes the
/// `MenuEvent` from muda. A synchronous channel approach doesn't work here — the wakeup
/// signal posted during the popup's NSEvent tracking loop gets consumed, so `recv` always
/// times out.
#[tauri::command]
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
pub fn update_pin_tab_menu<R: Runtime>(app: AppHandle<R>, is_pinned: bool) -> Result<(), String> {
    let menu_state = app.state::<MenuState<R>>();
    let guard = menu_state.pin_tab.lock_ignore_poison();
    let Some(item) = guard.as_ref() else {
        return Err("Menu not initialized".to_string());
    };
    let label = if is_pinned { "Unpin tab" } else { "Pin tab" };
    item.set_text(label).map_err(|e| e.to_string())
}

/// Enables or disables explorer-scoped menu items based on the current context.
/// - `"explorer"`: all menu items enabled (main file explorer has focus)
/// - `"other"`: all non-App items disabled except Close tab (⌘W), which doubles as
///   "close the focused window" — standard macOS behavior
#[tauri::command]
pub fn set_menu_context<R: Runtime>(app: AppHandle<R>, context: String) -> Result<(), String> {
    let enabled = context == "explorer";
    let menu_state = app.state::<MenuState<R>>();

    for (id, entry) in menu_state.items.lock_ignore_poison().iter() {
        // Close tab stays enabled: on_menu_event has special logic to close the focused
        // non-main window when main isn't focused (standard ⌘W behavior on macOS).
        if id == CLOSE_TAB_ID {
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
    if let Some(ref item) = *menu_state.view_mode_full.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    if let Some(ref item) = *menu_state.view_mode_brief.lock_ignore_poison() {
        let _ = item.set_enabled(enabled);
    }
    if let Some(ref submenu) = *menu_state.sort_submenu.lock_ignore_poison() {
        let _ = submenu.set_enabled(enabled);
    }

    Ok(())
}
