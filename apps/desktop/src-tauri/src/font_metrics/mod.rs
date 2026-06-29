//! Font metrics storage and calculation for accurate text width measurement.
//!
//! This module manages character width metrics for fonts used in the file explorer.
//! It stores width mappings in memory and on disk, and provides functions to calculate
//! text widths and find maximum widths across multiple strings.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::sync::{LazyLock, RwLock};

/// Cache for font metrics, keyed by font ID (like "system-400-12")
static METRICS_CACHE: LazyLock<RwLock<HashMap<String, FontMetrics>>> = LazyLock::new(|| RwLock::new(HashMap::new()));

/// Font metrics for a specific font configuration.
/// Stores character widths and an average width for fallback.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FontMetrics {
    /// Version for future format changes
    version: u32,
    /// Font identifier (like "system-400-12")
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

/// Calculates the maximum width among text strings, each carrying a trailing
/// pixel suffix (a non-text decoration rendered after it, e.g. the Finder
/// tag-dot cluster) added to that text's own width before taking the max. A
/// suffix of `0.0` is the plain widest-string case. Lets a single Brief column
/// reserve room for a wide-name row and a tagged-but-short-name row
/// independently. `None` when the font ID isn't cached.
pub fn calculate_max_width_with_suffixes(items: &[(&str, f32)], font_id: &str) -> Option<f32> {
    let cache = METRICS_CACHE.read().ok()?;
    let metrics = cache.get(font_id)?;

    items
        .iter()
        .map(|(text, suffix)| metrics.calculate_text_width(text) + suffix)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
}

/// Loads font metrics from disk
pub fn load_from_disk<R: tauri::Runtime>(app: &tauri::AppHandle<R>, font_id: &str) -> Option<FontMetrics> {
    let data_dir = crate::config::resolved_app_data_dir(app).ok()?;
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
    let data_dir = crate::config::resolved_app_data_dir(app)?;
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

/// Loads every `*.bin` file from the on-disk font-metrics directory into the
/// in-memory cache.
///
/// With user-controlled text scaling, the same install can have measurements
/// for several font sizes side-by-side (`system-400-12`, `system-400-15`, …).
/// Pre-loading them all at startup avoids a re-measure burst on first paint
/// when the user has previously chosen a non-default size.
pub fn load_all_metrics_from_disk<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Ok(data_dir) = crate::config::resolved_app_data_dir(app) else {
        return;
    };
    let metrics_dir = data_dir.join("font-metrics");
    let Ok(entries) = fs::read_dir(&metrics_dir) else {
        return;
    };

    let mut loaded = 0usize;
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("bin") {
            continue;
        }
        let Some(font_id) = path.file_stem().and_then(|s| s.to_str()) else {
            continue;
        };
        let Ok(bytes) = fs::read(&path) else { continue };
        let Ok(metrics): Result<FontMetrics, _> = bincode2::deserialize(&bytes) else {
            continue;
        };
        if let Ok(mut cache) = METRICS_CACHE.write() {
            cache.insert(font_id.to_string(), metrics);
            loaded += 1;
        }
    }
    if loaded > 0 {
        log::debug!("Font metrics: Loaded {loaded} cached size(s) from disk");
    }
}
