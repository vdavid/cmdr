//! Tauri commands for icon retrieval.

use crate::icons;
use std::collections::HashMap;

/// Gets icon data URLs for the requested icon IDs.
/// Returns a map of icon_id -> base64 WebP data URL.
/// Only fetches icons not already cached; clients should cache returned icons.
#[tauri::command]
pub fn get_icons(icon_ids: Vec<String>) -> HashMap<String, String> {
    icons::get_icons(icon_ids)
}

/// Refreshes icons for a directory listing.
/// Fetches icons in parallel for all directories and extensions.
/// Returns all fetched icons (frontend can compare with cache to detect changes).
#[tauri::command]
pub fn refresh_directory_icons(directory_paths: Vec<String>, extensions: Vec<String>) -> HashMap<String, String> {
    icons::refresh_icons_for_directory(directory_paths, extensions)
}
