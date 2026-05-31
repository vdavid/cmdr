//! Tauri commands for icon retrieval.

use std::collections::HashMap;
use tokio::time::Duration;

use super::util::{TimedOut, blocking_with_timeout_flag};
use crate::icons;

const ICONS_TIMEOUT: Duration = Duration::from_secs(2);

/// Gets icon data URLs for the requested icon IDs.
/// Returns a map of icon_id -> base64 WebP data URL.
/// Only fetches icons not already cached; clients should cache returned icons.
///
/// When `use_app_icons_for_documents` is true and on macOS, extension-based icons
/// are fetched from app bundles (showing the app's icon as fallback). When false,
/// the system's default document icons are used (Finder-style with app badge).
///
/// Returns an empty map while `crate::fda_gate::is_fda_pending_runtime()` is true.
/// `fetch_fresh_extension_icon` walks UTType / LaunchServices, which on macOS
/// touches MediaLibrary, AppData, FileProvider, and Apple Events TCC services
/// for media/app/cloud-storage/scriptable extensions (exactly the popups we
/// must not stack on top of the in-app FDA modal). Frontend re-requests after
/// the gate clears.
#[tauri::command]
#[specta::specta]
pub async fn get_icons(icon_ids: Vec<String>, use_app_icons_for_documents: bool) -> TimedOut<HashMap<String, String>> {
    if crate::fda_gate::is_fda_pending_runtime() {
        return TimedOut {
            data: HashMap::new(),
            timed_out: false,
        };
    }
    blocking_with_timeout_flag(ICONS_TIMEOUT, HashMap::new(), move || {
        icons::get_icons(icon_ids, use_app_icons_for_documents)
    })
    .await
}

/// Refreshes icons for a directory listing.
/// Fetches icons in parallel for all directories and extensions.
/// Returns all fetched icons (frontend can compare with cache to detect changes).
///
/// When `use_app_icons_for_documents` is true, falls back to app icons for files without
/// document-specific icons. When false, uses Finder-style document icons.
///
/// Returns an empty map while the FDA gate is pending. Same reason as
/// `get_icons`. See `crate::fda_gate`.
#[tauri::command]
#[specta::specta]
pub async fn refresh_directory_icons(
    directory_paths: Vec<String>,
    extensions: Vec<String>,
    use_app_icons_for_documents: bool,
) -> TimedOut<HashMap<String, String>> {
    if crate::fda_gate::is_fda_pending_runtime() {
        return TimedOut {
            data: HashMap::new(),
            timed_out: false,
        };
    }
    blocking_with_timeout_flag(ICONS_TIMEOUT, HashMap::new(), move || {
        icons::refresh_icons_for_directory(directory_paths, extensions, use_app_icons_for_documents)
    })
    .await
}

/// Detects which of the given VISIBLE directory paths carry a Finder custom-icon
/// flag, returning the `path:{dir}` icon id for each. The frontend calls this for
/// visible directory rows, then feeds the returned ids into `get_icons` to fetch
/// the real icons. The `getxattr` check is cheap (no NSWorkspace, no TCC), but
/// still deferred off the bulk-listing hot path, so it's safe to run without the
/// FDA gate. Empty result while there's nothing to report.
#[tauri::command]
#[specta::specta]
pub async fn get_custom_folder_icon_ids(directory_paths: Vec<String>) -> TimedOut<Vec<String>> {
    blocking_with_timeout_flag(ICONS_TIMEOUT, Vec::new(), move || {
        icons::custom_folder_icon_ids(directory_paths)
    })
    .await
}

/// Clears cached extension icons.
/// Called when the "use app icons for documents" setting changes.
#[tauri::command]
#[specta::specta]
pub fn clear_extension_icon_cache() {
    icons::clear_extension_icon_cache();
}

/// Clears cached directory icons (`dir`, `symlink-dir`, `path:*`, `special:*`).
/// Called when the system theme or accent color changes.
#[tauri::command]
#[specta::specta]
pub fn clear_directory_icon_cache() {
    icons::clear_directory_icon_cache();
}
