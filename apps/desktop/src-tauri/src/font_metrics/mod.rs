//! Font metrics storage and calculation for accurate text width measurement.
//!
//! This module manages character width metrics for fonts used in the file explorer.
//! It stores width mappings in memory and on disk, and provides functions to calculate
//! text widths and find maximum widths across multiple strings.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::{LazyLock, RwLock};
use tauri::Manager;

/// Cache for font metrics, keyed by font ID (e.g., "system-400-12")
static METRICS_CACHE: LazyLock<RwLock<HashMap<String, FontMetrics>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Font metrics for a specific font configuration.
/// Stores character widths and an average width for fallback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontMetrics {
    /// Version for future format changes
    version: u32,
    /// Font identifier (e.g., "system-400-12")
    font_id: String,
    /// Map of code point → width in pixels
    widths: HashMap<u32, f32>,
    /// Average width for unmeasured characters
    average_width: f32,
}

impl FontMetrics {
    /// Creates new font metrics from measured widths
    pub fn new(font_id: String, widths: HashMap<u32, f32>) -> Self {
        let average_width = if widths.is_empty() {
            0.0
        } else {
            widths.values().sum::<f32>() / widths.len() as f32
        };

        Self {
            version: 1,
            font_id,
            widths,
            average_width,
        }
    }

    /// Gets the width of a character, falling back to average if not found
    fn get_char_width(&self, code_point: u32) -> f32 {
        self.widths.get(&code_point).copied().unwrap_or(self.average_width)
    }

    /// Calculates the total width of a text string
    pub fn calculate_text_width(&self, text: &str) -> f32 {
        text.chars().map(|c| self.get_char_width(c as u32)).sum()
    }
}

/// Stores font metrics in memory cache
pub fn store_metrics(font_id: String, widths: HashMap<u32, f32>) -> Result<(), String> {
    let metrics = FontMetrics::new(font_id.clone(), widths);

    let mut cache = METRICS_CACHE
        .write()
        .map_err(|e| format!("Failed to acquire cache lock: {}", e))?;
    cache.insert(font_id, metrics);
    Ok(())
}

/// Checks if metrics are available for a font ID
pub fn has_metrics(font_id: &str) -> bool {
    METRICS_CACHE
        .read()
        .map(|cache| cache.contains_key(font_id))
        .unwrap_or(false)
}

/// Calculates the width of a text string using cached metrics
#[allow(dead_code)] // Public API for future use
pub fn calculate_text_width(text: &str, font_id: &str) -> Option<f32> {
    let cache = METRICS_CACHE.read().ok()?;
    let metrics = cache.get(font_id)?;
    Some(metrics.calculate_text_width(text))
}

/// Calculates the maximum width among multiple text strings
pub fn calculate_max_width(texts: &[&str], font_id: &str) -> Option<f32> {
    let cache = METRICS_CACHE.read().ok()?;
    let metrics = cache.get(font_id)?;

    texts
        .iter()
        .map(|text| metrics.calculate_text_width(text))
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
}

/// Loads font metrics from disk
pub fn load_from_disk<R: tauri::Runtime>(app: &tauri::AppHandle<R>, font_id: &str) -> Option<FontMetrics> {
    let data_dir = app.path().app_data_dir().ok()?;
    let metrics_dir = data_dir.join("font-metrics");
    let file_path = metrics_dir.join(format!("{}.bin", font_id));

    let bytes = fs::read(file_path).ok()?;
    bincode2::deserialize(&bytes).ok()
}

/// Saves font metrics to disk
pub fn save_to_disk<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    font_id: &str,
    widths: &HashMap<u32, f32>,
) -> Result<(), String> {
    let data_dir = app
        .path()
        .app_data_dir()
        .map_err(|e| format!("Failed to get app data dir: {}", e))?;
    let metrics_dir = data_dir.join("font-metrics");

    // Create directory if it doesn't exist
    fs::create_dir_all(&metrics_dir).map_err(|e| format!("Failed to create metrics dir: {}", e))?;

    let metrics = FontMetrics::new(font_id.to_string(), widths.clone());
    let bytes = bincode2::serialize(&metrics).map_err(|e| format!("Failed to serialize metrics: {}", e))?;

    let file_path = metrics_dir.join(format!("{}.bin", font_id));
    fs::write(file_path, bytes).map_err(|e| format!("Failed to write metrics file: {}", e))?;

    Ok(())
}

/// Initializes font metrics by loading from disk if available
pub fn init_font_metrics<R: tauri::Runtime>(app: &tauri::AppHandle<R>, font_id: &str) {
    if let Some(metrics) = load_from_disk(app, font_id)
        && let Ok(mut cache) = METRICS_CACHE.write()
    {
        cache.insert(font_id.to_string(), metrics);
        log::debug!("Font metrics: Loaded from disk for font: {}", font_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_text_width_logic() {
        // Create test metrics with known widths
        let mut widths = HashMap::new();
        widths.insert('b' as u32, 7.37);
        widths.insert('e' as u32, 6.86);
        widths.insert('n' as u32, 7.00);
        widths.insert('c' as u32, 6.71);
        widths.insert('h' as u32, 7.06);
        widths.insert('_' as u32, 7.00);
        widths.insert('a' as u32, 6.62);
        widths.insert('f' as u32, 4.34);
        widths.insert('t' as u32, 4.36);
        widths.insert('r' as u32, 4.57);
        widths.insert('1' as u32, 5.57);
        widths.insert('.' as u32, 3.56);
        widths.insert('p' as u32, 7.32);
        widths.insert('g' as u32, 7.31);

        let metrics = FontMetrics::new("test".to_string(), widths);
        let width = metrics.calculate_text_width("bench_after_1.png");

        // Expected from Python: 106.52px
        println!("Calculated width: {}", width);
        assert!((width - 106.52).abs() < 0.1, "Expected ~106.52px, got {}", width);
    }
}
