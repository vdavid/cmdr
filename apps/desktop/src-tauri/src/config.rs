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
    let dir = match data_dir_from_env(std::env::var("CMDR_DATA_DIR").ok().as_deref()) {
        Some(path) => path,
        None => app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to get app data dir: {e}"))?,
    };

    std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create app data dir: {e}"))?;

    Ok(dir)
}

/// Pure helper: pull a data dir out of the env-var value, treating empty as unset.
/// Kept private + unit-tested so the env-precedence branch of `resolved_app_data_dir`
/// is exercised without needing a Tauri mock runtime.
fn data_dir_from_env(env_value: Option<&str>) -> Option<PathBuf> {
    match env_value {
        Some(s) if !s.is_empty() => Some(PathBuf::from(s)),
        _ => None,
    }
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
// 2. `withGlobalTauri` is only flipped to `true` by the generated `tauri.instance.json` that
//    `apps/desktop/scripts/tauri-wrapper.js` writes for non-prod instances and merges via `-c`.
// 3. Production builds skip the wrapper's instance composition, so canonical
//    `tauri.conf.json` (with `withGlobalTauri: false`) governs the bundle.
//
// See CONTRIBUTING.md for setup instructions.

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn data_dir_from_env_honors_set_value() {
        let got = data_dir_from_env(Some("/tmp/cmdr-p1-test"));
        assert_eq!(got, Some(PathBuf::from("/tmp/cmdr-p1-test")));
    }

    #[test]
    fn data_dir_from_env_returns_none_when_unset() {
        assert_eq!(data_dir_from_env(None), None);
    }

    #[test]
    fn data_dir_from_env_treats_empty_as_unset() {
        // An empty CMDR_DATA_DIR should not silently land us in cwd-equivalent paths.
        // Falling through to the Tauri default is the documented behavior.
        assert_eq!(data_dir_from_env(Some("")), None);
    }
}
