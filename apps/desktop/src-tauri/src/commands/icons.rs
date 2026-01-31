//! Tauri commands for icon retrieval.

use crate::icons;
use std::collections::HashMap;

/// Gets icon data URLs for the requested icon IDs.
/// Returns a map of icon_id -> base64 WebP data URL.
/// Only fetches icons not already cached; clients should cache returned icons.
///
/// When `use_app_icons_for_documents` is true and on macOS, extension-based icons
/// are fetched from app bundles (showing the app's icon as fallback). When false,
/// the system's default document icons are used (Finder-style with app badge).
#[tauri::command]
pub fn get_icons(icon_ids: Vec<String>, use_app_icons_for_documents: bool) -> HashMap<String, String> {
    icons::get_icons(icon_ids, use_app_icons_for_documents)
}

/// Refreshes icons for a directory listing.
/// Fetches icons in parallel for all directories and extensions.
/// Returns all fetched icons (frontend can compare with cache to detect changes).
///
/// When `use_app_icons_for_documents` is true, falls back to app icons for files without
/// document-specific icons. When false, uses Finder-style document icons.
#[tauri::command]
pub fn refresh_directory_icons(
    directory_paths: Vec<String>,
    extensions: Vec<String>,
    use_app_icons_for_documents: bool,
) -> HashMap<String, String> {
    icons::refresh_icons_for_directory(directory_paths, extensions, use_app_icons_for_documents)
}

/// Clears cached extension icons.
/// Called when the "use app icons for documents" setting changes.
#[tauri::command]
pub fn clear_extension_icon_cache() {
    icons::clear_extension_icon_cache();
}
