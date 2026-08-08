//! Font metrics storage and calculation for accurate text width measurement.
//!
//! This module manages character width metrics for fonts used in the file explorer.
//! It stores width mappings in memory and on disk, and provides functions to calculate
//! text widths and find maximum widths across multiple strings.
//!
//! The frontend measures only a small eager set of code points up front (Latin,
//! punctuation, symbols, common emoji). Anything else — CJK, Hangul, the Indic
//! blocks, an emoji added after this build — is absent from the map on first
//! sight. Width queries therefore answer immediately with `average_width` for
//! those and REPORT them to the caller, which measures them and calls
//! `extend_metrics`. From that point the real widths are used. See
//! `../../../src/lib/font-metrics/DETAILS.md` § On-demand fill-in.

use std::collections::{BTreeSet, HashMap};
use std::fs;
use std::sync::{LazyLock, RwLock};

use serde::{Deserialize, Serialize};

use crate::ignore_poison::RwLockIgnorePoison;

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
    /// Stand-in width for a code point not yet measured. Only ever used for the
    /// one query that discovers the gap: the caller fills it in right after.
    average_width: f32,
}

impl FontMetrics {
    /// Creates new font metrics from measured widths
    pub fn new(font_id: String, widths: HashMap<u32, f32>) -> Self {
        let average_width = mean_width(&widths);
        Self {
            version: 1,
            font_id,
            widths,
            average_width,
        }
    }

    /// Merges freshly measured widths in and refreshes the average.
    fn extend(&mut self, widths: HashMap<u32, f32>) {
        self.widths.extend(widths);
        self.average_width = mean_width(&self.widths);
    }

    /// Width of one code point, or `None` when it has never been measured.
    fn char_width(&self, code_point: u32) -> Option<f32> {
        self.widths.get(&code_point).copied()
    }

    /// Total width of a string, recording every code point it had no width for
    /// into `missing` so the caller can have them measured.
    pub fn calculate_text_width(&self, text: &str, missing: &mut BTreeSet<u32>) -> f32 {
        text.chars()
            .map(|c| {
                self.char_width(c as u32).unwrap_or_else(|| {
                    missing.insert(c as u32);
                    self.average_width
                })
            })
            .sum()
    }
}

/// Mean of the measured widths; `0.0` for an empty map.
fn mean_width(widths: &HashMap<u32, f32>) -> f32 {
    if widths.is_empty() {
        0.0
    } else {
        widths.values().sum::<f32>() / widths.len() as f32
    }
}

/// Replaces the cached metrics for `font_id` and writes them to disk.
///
/// Takes ownership of `widths` and builds the entry once: the map holds
/// thousands of pairs and this used to clone it twice per call.
pub fn store_and_persist<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    font_id: String,
    widths: HashMap<u32, f32>,
) -> Result<(), String> {
    let metrics = FontMetrics::new(font_id.clone(), widths);
    let bytes = serialize(&metrics)?;

    METRICS_CACHE.write_ignore_poison().insert(font_id.clone(), metrics);

    write_metrics_file(app, &font_id, &bytes)
}

/// Merges newly measured widths into an existing entry and rewrites its file.
///
/// Serializes while holding the write lock (a short CPU-only step) so the
/// merged map never has to be cloned out; the file write happens after the
/// lock is released.
pub fn extend_and_persist<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    font_id: &str,
    widths: HashMap<u32, f32>,
) -> Result<(), String> {
    let added = widths.len();
    let bytes = {
        let mut cache = METRICS_CACHE.write_ignore_poison();
        let Some(metrics) = cache.get_mut(font_id) else {
            // The font was evicted or never stored; the caller's next
            // `ensure_font_metrics_loaded` will measure the eager set afresh.
            return Err(format!("No metrics cached for font_id='{font_id}'"));
        };
        metrics.extend(widths);
        serialize(metrics)?
    };

    write_metrics_file(app, font_id, &bytes)?;
    log::debug!(target: "font_metrics", "Filled {added} code point(s) for font: {font_id}");
    Ok(())
}

/// Checks if metrics are available for a font ID
pub fn has_metrics(font_id: &str) -> bool {
    METRICS_CACHE.read_ignore_poison().contains_key(font_id)
}

/// Calculates the maximum width among text strings, each carrying a trailing
/// pixel suffix (a non-text decoration rendered after it, e.g. the Finder
/// tag-dot cluster) added to that text's own width before taking the max. A
/// suffix of `0.0` is the plain widest-string case. Lets a single Brief column
/// reserve room for a wide-name row and a tagged-but-short-name row
/// independently. `None` when the font ID isn't cached.
///
/// Code points with no measured width are counted at `average_width` and
/// collected into `missing`; the caller reports them so they get measured.
pub fn calculate_max_width_with_suffixes(
    items: &[(&str, f32)],
    font_id: &str,
    missing: &mut BTreeSet<u32>,
) -> Option<f32> {
    let cache = METRICS_CACHE.read_ignore_poison();
    let metrics = cache.get(font_id)?;

    items
        .iter()
        .map(|(text, suffix)| metrics.calculate_text_width(text, missing) + suffix)
        .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
}

/// Serializes metrics to the on-disk representation.
fn serialize(metrics: &FontMetrics) -> Result<Vec<u8>, String> {
    bincode2::serialize(metrics).map_err(|e| format!("Failed to serialize metrics: {e}"))
}

/// Writes already-serialized metrics to `<data-dir>/font-metrics/{font_id}.bin`.
fn write_metrics_file<R: tauri::Runtime>(app: &tauri::AppHandle<R>, font_id: &str, bytes: &[u8]) -> Result<(), String> {
    let data_dir = crate::config::resolved_app_data_dir(app)?;
    let metrics_dir = data_dir.join("font-metrics");
    fs::create_dir_all(&metrics_dir).map_err(|e| format!("Failed to create metrics dir: {e}"))?;
    fs::write(metrics_dir.join(format!("{font_id}.bin")), bytes)
        .map_err(|e| format!("Failed to write metrics file: {e}"))
}

/// Loads font metrics from disk
pub fn load_from_disk<R: tauri::Runtime>(app: &tauri::AppHandle<R>, font_id: &str) -> Option<FontMetrics> {
    let data_dir = crate::config::resolved_app_data_dir(app).ok()?;
    let metrics_dir = data_dir.join("font-metrics");
    let file_path = metrics_dir.join(format!("{font_id}.bin"));

    let bytes = fs::read(file_path).ok()?;
    bincode2::deserialize(&bytes).ok()
}

/// Initializes font metrics by loading from disk if available
pub fn init_font_metrics<R: tauri::Runtime>(app: &tauri::AppHandle<R>, font_id: &str) {
    if let Some(metrics) = load_from_disk(app, font_id) {
        METRICS_CACHE.write_ignore_poison().insert(font_id.to_string(), metrics);
        log::debug!(target: "font_metrics", "Loaded from disk for font: {font_id}");
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
        METRICS_CACHE.write_ignore_poison().insert(font_id.to_string(), metrics);
        loaded += 1;
    }
    if loaded > 0 {
        log::debug!(target: "font_metrics", "Loaded {loaded} cached size(s) from disk");
    }
}

/// Stores metrics in the in-memory cache only. Test-facing: production paths go
/// through `store_and_persist`, which also writes the file.
#[cfg(test)]
pub fn store_metrics(font_id: String, widths: HashMap<u32, f32>) -> Result<(), String> {
    let metrics = FontMetrics::new(font_id.clone(), widths);
    METRICS_CACHE.write_ignore_poison().insert(font_id, metrics);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::file_system::listing::caching_test_support::unique_test_id;

    fn seed(widths: &[(u32, f32)]) -> String {
        let font_id = unique_test_id("font-metrics");
        let map: HashMap<u32, f32> = widths.iter().copied().collect();
        store_metrics(font_id.clone(), map).expect("seed metrics");
        font_id
    }

    #[test]
    fn measured_code_points_report_nothing_missing() {
        let font_id = seed(&[('a' as u32, 7.0), ('b' as u32, 3.0)]);
        let mut missing = BTreeSet::new();

        let width = calculate_max_width_with_suffixes(&[("ab", 0.0)], &font_id, &mut missing);

        assert_eq!(width, Some(10.0));
        assert!(missing.is_empty(), "every code point was measured");
    }

    #[test]
    fn unmeasured_code_points_use_the_average_and_are_reported() {
        // Average of 7 and 3 is 5, so the unmeasured '猫' counts as 5.
        let font_id = seed(&[('a' as u32, 7.0), ('b' as u32, 3.0)]);
        let mut missing = BTreeSet::new();

        let width = calculate_max_width_with_suffixes(&[("a猫", 0.0)], &font_id, &mut missing);

        assert_eq!(width, Some(12.0), "7 for 'a' + the 5.0 average for '猫'");
        assert_eq!(missing.into_iter().collect::<Vec<_>>(), vec!['猫' as u32]);
    }

    #[test]
    fn extending_replaces_the_average_with_the_real_width() {
        let font_id = seed(&[('a' as u32, 7.0), ('b' as u32, 3.0)]);
        let mut missing = BTreeSet::new();
        calculate_max_width_with_suffixes(&[("a猫", 0.0)], &font_id, &mut missing);
        assert!(!missing.is_empty(), "the fill-in request starts non-empty");

        // Merge straight into the cache; the persisting wrapper needs an AppHandle.
        {
            let mut cache = METRICS_CACHE.write_ignore_poison();
            let metrics = cache.get_mut(&font_id).expect("seeded font is cached");
            metrics.extend(HashMap::from([('猫' as u32, 20.0)]));
        }

        let mut still_missing = BTreeSet::new();
        let width = calculate_max_width_with_suffixes(&[("a猫", 0.0)], &font_id, &mut still_missing);

        assert_eq!(width, Some(27.0), "7 for 'a' + the measured 20 for '猫'");
        assert!(still_missing.is_empty(), "nothing is missing once filled");
    }

    #[test]
    fn unknown_font_id_reports_no_width() {
        let mut missing = BTreeSet::new();
        assert_eq!(
            calculate_max_width_with_suffixes(&[("a", 0.0)], "never-measured", &mut missing),
            None
        );
    }
}
