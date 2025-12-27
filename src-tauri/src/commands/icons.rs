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
