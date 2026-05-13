//! Per-column text-width computation for Brief mode.
//!
//! Pure-logic module: reads from `LISTING_CACHE`, iterates entries in column-major
//! order, and returns the widest filename's text-only width per column. Chrome
//! (icon, padding, gap) and clamping (`MIN_COLUMN_WIDTH`, `MAX_BRIEF_COLUMN_WIDTH`)
//! are FE concerns and are added there.
//!
//! Backend is the natural home for this because:
//! - it already holds every filename (no IPC round-trip per column needed),
//! - it already holds cached font metrics keyed by font ID,
//! - widths must agree with the virtual-scroll math, which lives FE-side and
//!   consumes these widths via a single IPC call per layout change.
//!
//! Column-major layout: with `has_parent = true`, column 0 displays the `".."`
//! literal followed by the first `items_per_column - 1` real entries; subsequent
//! columns shift by `items_per_column - 1`. With `has_parent = false`, columns
//! contain `items_per_column` entries each.

use std::time::Instant;

use crate::file_system::listing::caching::LISTING_CACHE;
use crate::file_system::listing::metadata::FileEntry;

/// Errors from `compute_brief_column_text_widths`. Internal to the backend —
/// the IPC command wrapper maps these to `IpcError` for the wire.
#[derive(Debug, Clone, PartialEq)]
pub enum BriefColumnsError {
    /// `calculate_max_width` returned `None` for at least one column —
    /// the font metrics cache doesn't yet hold the requested `font_id`.
    /// Callers retry after `ensureFontMetricsLoaded` resolves.
    FontMetricsNotReady,
    /// `items_per_column == 0` — would divide by zero. FE clamps to >= 1.
    InvalidItemsPerColumn,
    /// The listing ID isn't in `LISTING_CACHE` (already ended, or never started).
    ListingNotFound(String),
    /// Catch-all for cache-lock poisoning etc.
    Other(String),
}

/// Returns true if the entry is not a hidden dotfile.
fn is_visible(entry: &FileEntry) -> bool {
    !entry.name.starts_with('.')
}

/// Computes the widest filename's text-only width per Brief-mode column.
///
/// Returns a `Vec<f32>` of length equal to the number of columns required to
/// display all visible entries (plus the `".."` parent literal when
/// `has_parent`). Values are guaranteed finite — no NaN, no Infinity — so the
/// FE's `Float64Array` prefix sums stay valid.
///
/// Reads `LISTING_CACHE` with a read lock. Caller is responsible for wrapping
/// the call in a timeout if `LISTING_CACHE` could be write-locked.
pub fn compute_brief_column_text_widths(
    listing_id: &str,
    items_per_column: usize,
    has_parent: bool,
    font_id: &str,
    include_hidden: bool,
) -> Result<Vec<f32>, BriefColumnsError> {
    if items_per_column == 0 {
        return Err(BriefColumnsError::InvalidItemsPerColumn);
    }

    let start = Instant::now();

    let cache = LISTING_CACHE
        .read()
        .map_err(|e| BriefColumnsError::Other(format!("Failed to acquire cache lock: {}", e)))?;

    let listing = cache
        .get(listing_id)
        .ok_or_else(|| BriefColumnsError::ListingNotFound(listing_id.to_string()))?;

    // Materialize visible entries into a Vec so we can index by position cheaply.
    let visible: Vec<&FileEntry> = if include_hidden {
        listing.entries.iter().collect()
    } else {
        listing.entries.iter().filter(|e| is_visible(e)).collect()
    };

    // Total cells (display slots): visible entries + ".." if has_parent.
    let total_cells = visible.len() + usize::from(has_parent);
    if total_cells == 0 {
        return Ok(Vec::new());
    }

    let total_columns = total_cells.div_ceil(items_per_column);
    let mut widths = Vec::with_capacity(total_columns);

    for col in 0..total_columns {
        // Compute the slice of `visible` covered by this column. The math
        // differs depending on whether the parent literal occupies cell (0,0).
        let (start_idx, end_idx, include_parent_literal) = if has_parent {
            if col == 0 {
                // Column 0: ".." literal + entries [0 .. items_per_column - 1).
                let end = (items_per_column - 1).min(visible.len());
                (0, end, true)
            } else {
                // Column c (c >= 1): entries [c * items_per_column - 1 .. (c + 1) * items_per_column - 1).
                let start = col * items_per_column - 1;
                let end = ((col + 1) * items_per_column - 1).min(visible.len());
                (start.min(visible.len()), end, false)
            }
        } else {
            // No parent: column c covers entries [c * items_per_column .. (c + 1) * items_per_column).
            let start = col * items_per_column;
            let end = ((col + 1) * items_per_column).min(visible.len());
            (start.min(visible.len()), end, false)
        };

        // Build the slice of names for this column. We allocate per column;
        // typical column count is < 1000 even for huge directories, so this is
        // negligible compared to the text-width computation itself.
        let mut names: Vec<&str> = Vec::with_capacity(end_idx.saturating_sub(start_idx) + 1);
        if include_parent_literal {
            names.push("..");
        }
        for entry in &visible[start_idx..end_idx] {
            names.push(entry.name.as_str());
        }

        let width = crate::font_metrics::calculate_max_width(&names, font_id).ok_or_else(|| {
            log::warn!(
                target: "brief_columns",
                "Font metrics not ready for font_id='{}' (listing={}, col={})",
                font_id,
                listing_id,
                col,
            );
            BriefColumnsError::FontMetricsNotReady
        })?;

        // Guarantee finite values so FE prefix-sums (Float64Array) stay valid.
        // `calculate_max_width` returns sums over per-char widths from the cached
        // HashMap<u32, f32>; in practice all stored widths are finite, but a
        // belt-and-braces clamp here is cheap insurance and documents intent.
        let width = if width.is_finite() { width.max(0.0) } else { 0.0 };
        widths.push(width);
    }

    let elapsed = start.elapsed();
    if elapsed.as_millis() > 5 {
        log::debug!(
            target: "brief_columns",
            "Computed {} widths for listing {} in {}μs",
            widths.len(),
            listing_id,
            elapsed.as_micros(),
        );
    }

    Ok(widths)
}
