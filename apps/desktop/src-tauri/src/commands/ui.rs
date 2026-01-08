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
        crate::menu::SHOW_IN_FINDER_ID => {
            #[cfg(target_os = "macos")]
            {
                let _ = Command::new("open").arg("-R").arg(&context.path).spawn();
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
                let _ = Command::new("qlmanage").arg("-p").arg(&context.path).spawn();
            }
        }
        crate::menu::GET_INFO_ID => {
            let _ = app.emit(
                "menu-action",
                serde_json::json!({ "action": "get-info", "path": context.path }),
            );
        }
        _ => {}
    }
}
