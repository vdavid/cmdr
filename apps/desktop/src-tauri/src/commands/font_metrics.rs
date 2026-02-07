//! Tauri commands for font metrics operations.

use crate::font_metrics;
use std::collections::HashMap;

/// Stores font metrics received from the frontend.
///
/// # Arguments
/// * `app` - Tauri app handle for accessing app data directory
/// * `font_id` - Font identifier (like "system-400-12")
/// * `widths` - Map of code point → width in pixels
#[tauri::command]
pub fn store_font_metrics<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    font_id: String,
    widths: HashMap<u32, f32>,
) -> Result<(), String> {
    // Store in memory
    font_metrics::store_metrics(font_id.clone(), widths.clone())?;

    // Save to disk
    font_metrics::save_to_disk(&app, &font_id, &widths)?;

    log::debug!("Font metrics: Stored for font: {}", font_id);
    Ok(())
}

/// Checks if font metrics are available for a font ID.
///
/// # Arguments
/// * `font_id` - Font identifier to check
#[tauri::command]
pub fn has_font_metrics(font_id: String) -> bool {
    font_metrics::has_metrics(&font_id)
}
