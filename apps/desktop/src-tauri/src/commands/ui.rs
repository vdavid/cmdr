use crate::menu::{MenuState, build_context_menu};
#[cfg(target_os = "macos")]
use std::process::Command;
use tauri::menu::ContextMenu;
use tauri::{AppHandle, Emitter, Manager, Runtime, Window};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_opener::OpenerExt;

#[tauri::command]
pub fn update_menu_context<R: Runtime>(app: AppHandle<R>, path: String, filename: String) {
    let state = app.state::<MenuState<R>>();
    let mut context = state.context.lock().unwrap();
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

#[tauri::command]
pub fn show_main_window<R: Runtime>(window: Window<R>) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())
}

/// Toggle hidden files visibility - updates menu checkbox and emits event.
/// This is used by the command palette to sync with menu state.
#[tauri::command]
pub fn toggle_hidden_files<R: Runtime>(app: AppHandle<R>) -> Result<bool, String> {
    let menu_state = app.state::<MenuState<R>>();
    let guard = menu_state.show_hidden_files.lock().unwrap();
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
    let menu_state = app.state::<MenuState<R>>();
    let full_guard = menu_state.view_mode_full.lock().unwrap();
    let brief_guard = menu_state.view_mode_brief.lock().unwrap();

    let (Some(full_item), Some(brief_item)) = (full_guard.as_ref(), brief_guard.as_ref()) else {
        return Err("Menu not initialized".to_string());
    };

    // Set the correct check state (radio behavior)
    let is_full = mode == "full";
    full_item.set_checked(is_full).map_err(|e| e.to_string())?;
    brief_item.set_checked(!is_full).map_err(|e| e.to_string())?;

    // Emit event to frontend
    app.emit("view-mode-changed", serde_json::json!({ "mode": mode }))
        .map_err(|e| e.to_string())?;

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

#[tauri::command]
#[cfg(not(target_os = "macos"))]
pub fn show_in_finder(_path: String) -> Result<(), String> {
    Err("Show in Finder is only available on macOS".to_string())
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
    Err("Quick Look is only available on macOS".to_string())
}

/// Open Get Info window in Finder (macOS only)
#[tauri::command]
#[cfg(target_os = "macos")]
pub fn get_info(path: String) -> Result<(), String> {
    // Use AppleScript to open the Get Info window
    // The path needs to be escaped for AppleScript
    let escaped_path = path.replace("\\", "\\\\").replace("\"", "\\\"");
    let script = format!(
        r#"tell application "Finder"
            activate
            open information window of (POSIX file "{}" as alias)
        end tell"#,
        escaped_path
    );

    Command::new("osascript")
        .arg("-e")
        .arg(&script)
        .spawn()
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
#[cfg(not(target_os = "macos"))]
pub fn get_info(_path: String) -> Result<(), String> {
    Err("Get Info is only available on macOS".to_string())
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
#[cfg(not(target_os = "macos"))]
pub fn open_in_editor(_path: String) -> Result<(), String> {
    Err("Open in editor is only available on macOS".to_string())
}

/// Executes a menu action for the current context.
pub fn execute_menu_action<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let state = app.state::<MenuState<R>>();
    let context = state.context.lock().unwrap().clone();

    if context.path.is_empty() {
        return;
    }

    match id {
        crate::menu::OPEN_ID => {
            let _ = app.opener().open_path(&context.path, None::<&str>);
        }
        crate::menu::EDIT_ID => {
            #[cfg(target_os = "macos")]
            {
                let _ = open_in_editor(context.path);
            }
        }
        crate::menu::SHOW_IN_FINDER_ID => {
            #[cfg(target_os = "macos")]
            {
                let _ = show_in_finder(context.path);
            }
        }
        crate::menu::COPY_PATH_ID => {
            let _ = app.clipboard().write_text(context.path);
        }
        crate::menu::COPY_FILENAME_ID => {
            let _ = app.clipboard().write_text(context.filename);
        }
        crate::menu::QUICK_LOOK_ID => {
            #[cfg(target_os = "macos")]
            {
                let _ = quick_look(context.path);
            }
        }
        crate::menu::GET_INFO_ID => {
            #[cfg(target_os = "macos")]
            {
                let _ = get_info(context.path);
            }
        }
        _ => {}
    }
}
