//! The desktop's font metrics: the one `FontMetricsStore` the app holds, and its
//! on-disk cache (`<data-dir>/font-metrics/{font_id}.bin`, bincode2).
//!
//! The width math and the Brief column computation live in `cmdr-brief-columns`
//! (Tauri-free, shared with the browser demo); this module is only what needs the
//! app: the process-wide store and the files that warm it at startup.

use std::collections::HashMap;
use std::fs;
use std::sync::LazyLock;

use cmdr_brief_columns::{FontMetrics, FontMetricsStore};

pub(crate) mod brief_columns;

/// The app's font metrics, keyed by font ID (like "system-400-12").
static STORE: LazyLock<FontMetricsStore> = LazyLock::new(FontMetricsStore::new);

/// The app's one store, for the width queries.
pub(crate) fn store() -> &'static FontMetricsStore {
    &STORE
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

    STORE.insert(font_id.clone(), metrics);

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
    let bytes = STORE.extend_with(font_id, widths, serialize)??;

    write_metrics_file(app, font_id, &bytes)?;
    log::debug!(target: "font_metrics", "Filled {added} code point(s) for font: {font_id}");
    Ok(())
}

/// Checks if metrics are available for a font ID
pub fn has_metrics(font_id: &str) -> bool {
    STORE.contains(font_id)
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
        STORE.insert(font_id.to_string(), metrics);
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
        STORE.insert(font_id.to_string(), metrics);
        loaded += 1;
    }
    if loaded > 0 {
        log::debug!(target: "font_metrics", "Loaded {loaded} cached size(s) from disk");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Every install's `{font_id}.bin` was written in this layout; the struct's field
    // order in `cmdr-brief-columns` is the format. A change here strands every cached
    // size (silently: a file that doesn't decode is skipped and re-measured).
    #[test]
    fn the_disk_format_is_pinned() {
        let metrics = FontMetrics::new("f".to_string(), HashMap::from([(97, 7.0)]));
        let bytes = serialize(&metrics).expect("serializes");
        assert_eq!(
            bytes,
            [
                1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0, 0, 102, 1, 0, 0, 0, 0, 0, 0, 0, 97, 0, 0, 0, 0, 0, 224, 64, 0, 0, 224,
                64
            ]
        );
        let decoded: FontMetrics = bincode2::deserialize(&bytes).expect("decodes");
        assert_eq!(serialize(&decoded).expect("re-serializes"), bytes);
    }
}
