//! The two memory controls in the Ask Cmdr settings section.
//!
//! Both exist because the folder is the agent's while the notes in it are about the user: they
//! get to see what is being said about them, and to throw it away. Everything either control
//! decides lives in the pure `MemoryStore`; these resolve the root and hand the answer over.
//!
//! ⚠️ **The path has to come from Rust.** It is `<data-dir>/ai/memory/`, and the data dir moves
//! with `CMDR_DATA_DIR` (prod, plain dev, and every worktree slug are separate). A frontend
//! that built the path itself would walk someone into the production folder from a dev build.

use tauri::AppHandle;

use crate::agent::memory;

/// Where the agent's notes live, ready for the main window to open.
///
/// Creates the folder when it isn't there yet, so the click lands somewhere real even before
/// the agent has written its first note. The settings window can't navigate a pane itself, so
/// it takes this path and emits `reveal-path` at the main window.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_memory_folder(app: AppHandle) -> Result<String, String> {
    let root = memory::store_for(&app)
        .ok_or_else(|| "the app data folder is not available".to_string())?
        .root()
        .to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        Ok(root.to_string_lossy().into_owned())
    })
    .await
    .map_err(|e| e.to_string())?
}

/// Throw away every note the agent has written about the user, and say how many went.
///
/// Chats and files are untouched: this reaches the memory folder and nothing else, which is
/// what the confirmation dialog promises.
#[tauri::command]
#[specta::specta]
pub async fn ask_cmdr_forget_memory(app: AppHandle) -> Result<u32, String> {
    let store = memory::store_for(&app).ok_or_else(|| "the app data folder is not available".to_string())?;
    tauri::async_runtime::spawn_blocking(move || {
        store
            .forget_all()
            .map(|forgotten| forgotten as u32)
            // The token, never a parsed sentence: the frontend logs this and shows its own copy.
            .map_err(|refusal| refusal.token().to_string())
    })
    .await
    .map_err(|e| e.to_string())?
}
