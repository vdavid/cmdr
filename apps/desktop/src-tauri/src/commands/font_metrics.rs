//! Tauri commands for font metrics operations.
//!
//! Both writes are `async` + timeout-wrapped: they serialize a few thousand
//! width pairs and write a file, and a sync command would do that on the IPC
//! handler thread.
//!
//! Widths cross the wire as two parallel arrays
//! (`cmdr_brief_columns::widths_from_parallel`), and a write that doesn't land
//! answers `cmdr_brief_columns::FontMetricsError`.

use std::time::Duration;

use crate::font_metrics;
use cmdr_brief_columns::{FontMetricsError, widths_from_parallel};
use cmdr_fs::deadline::blocking_typed_result_with_timeout;

/// Writing the metrics file is a small serialize plus one `fs::write` to the
/// app data dir; 5 s matches the write tier used elsewhere in `commands/`.
const WRITE_TIMEOUT: Duration = Duration::from_secs(5);

/// Runs one metrics write under the shared deadline, in this family's vocabulary.
async fn persist_metrics(f: impl FnOnce() -> Result<(), String> + Send + 'static) -> Result<(), FontMetricsError> {
    blocking_typed_result_with_timeout(
        WRITE_TIMEOUT,
        || FontMetricsError::TimedOut,
        |detail| FontMetricsError::Unexpected { detail },
        move || f().map_err(|detail| FontMetricsError::CouldntPersist { detail }),
    )
    .await
}

/// Stores the eagerly measured widths for a font, replacing any existing entry.
///
/// # Arguments
/// * `font_id` - Font identifier (like "system-400-12")
/// * `code_points` - Code points, parallel to `widths`
/// * `widths` - Width in pixels for each code point
#[tauri::command]
#[specta::specta]
pub async fn store_font_metrics<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    font_id: String,
    code_points: Vec<u32>,
    widths: Vec<f32>,
) -> Result<(), FontMetricsError> {
    let widths = widths_from_parallel(code_points, widths)?;
    let count = widths.len();
    let logged_id = font_id.clone();

    persist_metrics(move || {
        font_metrics::store_and_persist(&app, font_id, widths)
            .map(|()| log::debug!(target: "font_metrics", "Stored {count} width(s) for font: {logged_id}"))
    })
    .await
}

/// Merges on-demand measured widths into a font's existing entry.
///
/// Serves the fill-in loop: a width query reports the code points it had no
/// width for, the frontend measures exactly those, and this folds them in so
/// every later query is exact.
#[tauri::command]
#[specta::specta]
pub async fn extend_font_metrics<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    font_id: String,
    code_points: Vec<u32>,
    widths: Vec<f32>,
) -> Result<(), FontMetricsError> {
    let widths = widths_from_parallel(code_points, widths)?;

    persist_metrics(move || font_metrics::extend_and_persist(&app, &font_id, widths)).await
}

/// Checks if font metrics are available for a font ID.
///
/// # Arguments
/// * `font_id` - Font identifier to check
#[tauri::command]
#[specta::specta]
pub fn has_font_metrics(font_id: String) -> bool {
    font_metrics::has_metrics(&font_id)
}
