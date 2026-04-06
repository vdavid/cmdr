//! Configuration constants and path helpers.
//!
//! These can be extracted to environment variables or a config file in the future.

use std::path::PathBuf;
use tauri::{AppHandle, Manager, Runtime};

/// Icon size in pixels (32x32 for retina display)
pub const ICON_SIZE: u32 = 32;

/// When true (macOS only): Show the associated app's icon for document types that don't
/// have custom document icons bundled. This results in colorful app icons, and they stay
/// up to date immediately when file associations change (for example, via Finder → Get Info).
///
/// When false: Fall back to system-generated document icons (Finder-style, with a small
/// app badge). These look more consistent with Finder, but may be stale until the next system
/// restart when file associations change (due to macOS Launch Services icon cache).
/// TODO: Move this to a setting once we have a settings window in place
pub const USE_APP_ICONS_AS_DOCUMENT_ICONS: bool = true;

/// Returns the app data directory. Priority:
/// 1. `CMDR_DATA_DIR` env var (set by `tauri-wrapper.js` for dev, by test harness for E2E)
/// 2. Tauri default (production)
///
/// Creates the directory if needed.
pub fn resolved_app_data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String> {
    let dir = if let Ok(custom) = std::env::var("CMDR_DATA_DIR") {
        PathBuf::from(custom)
    } else {
        app.path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {e}"))?
    };

    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create app data dir: {e}"))?;

    Ok(dir)
}

/// Logs the resolved data directory once at startup.
pub fn log_app_data_dir<R: Runtime>(app: &AppHandle<R>) {
    if let Ok(dir) = resolved_app_data_dir(app) {
        if std::env::var("CMDR_DATA_DIR").is_ok() {
            log::info!("Using CMDR_DATA_DIR: {}", dir.display());
        } else {
            log::debug!("Using default app data dir: {}", dir.display());
        }
    }
}

// MCP Server Security Design:
// --------------------------
// The MCP (Model Context Protocol) bridge allows AI assistants to control the app.
// This requires `withGlobalTauri: true` which exposes `window.__TAURI__` to the frontend.
//
// To prevent this security risk in production:
// 1. The MCP plugin is only registered in debug builds (see lib.rs: #[cfg(debug_assertions)])
// 2. `withGlobalTauri` is only enabled via tauri.dev.json (merged in dev only by tauri-wrapper.js)
// 3. Production builds use tauri.conf.json which has `withGlobalTauri: false`
//
// See CONTRIBUTING.md for setup instructions.
