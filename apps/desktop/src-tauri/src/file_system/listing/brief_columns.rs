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
//! - widths must agree with the virtual-scroll math, which lives FE-side and consumes these widths
//!   via a single IPC call per layout change.
//!
//! Column-major layout: with `has_parent = true`, column 0 displays the `".."`
//! literal followed by the first `items_per_column - 1` real entries; subsequent
//! columns shift by `items_per_column - 1`. With `has_parent = false`, columns
//! contain `items_per_column` entries each.

use std::collections::BTreeSet;
use std::time::Instant;

use crate::file_system::listing::caching::LISTING_CACHE;
use crate::file_system::listing::metadata::FileEntry;

/// Per-column widths plus the code points that had to be estimated.
///
/// `missing_code_points` is empty in the steady state. When it isn't, the
/// widths are still usable (unmeasured characters counted at the font's average
/// width), and the caller measures those code points and asks again. Ascending
/// and deduplicated, from the `BTreeSet` they're gathered in.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BriefColumnWidths {
    pub widths: Vec<f32>,
    pub missing_code_points: Vec<u32>,
}

/// Errors from `compute_brief_column_text_widths`. Internal to the backend;
/// the IPC command wrapper converts these into `BriefColumnsIpcError` for the wire.
#[derive(Debug, Clone, PartialEq)]
pub enum BriefColumnsError {
    /// `calculate_max_width_with_suffixes` returned `None` for at least one column:
    /// the font metrics cache doesn't yet hold the requested `font_id`.
    /// Callers retry after `ensureFontMetricsLoaded` resolves.
    FontMetricsNotReady,
    /// `items_per_column == 0` (would divide by zero). FE clamps to >= 1.
    InvalidItemsPerColumn,
    /// The listing ID isn't in `LISTING_CACHE` (already ended, or never started).
    ListingNotFound(String),
    /// Catch-all for cache-lock poisoning etc.
    Other(String),
}

/// What went wrong, as a value the frontend can branch on.
///
/// The frontend decides "recover, retry, or give up" from this alone. Keep every
/// case that leads to a DIFFERENT decision as its own variant: `Other` means
/// "unclassified", so folding a real case into it costs the FE its recovery.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BriefColumnsErrorKind {
    /// The font metrics cache has no entry for the requested font yet. Recoverable:
    /// the FE measures the font, then asks again.
    FontMetricsNotReady,
    /// `items_per_column == 0`. A caller bug, never transient: retrying can't help.
    InvalidItemsPerColumn,
    /// The listing ended or hasn't started. Transient across a navigation.
    ListingNotFound,
    /// The IPC deadline expired before the computation finished, typically because
    /// a large listing held `LISTING_CACHE`'s write lock. Transient.
    Timeout,
    /// Unclassified (lock poisoning and the like). Treated as transient.
    Other,
}

/// The wire form of a failed `get_brief_column_text_widths`.
///
/// `kind` is the classifier; `message` is diagnostic text for logs and error
/// reports. ❌ Nothing may branch on `message`:
/// it carries listing IDs and OS text that change without notice.
#[derive(Debug, Clone, PartialEq, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BriefColumnsIpcError {
    pub kind: BriefColumnsErrorKind,
    pub message: String,
}

impl BriefColumnsIpcError {
    /// The command's own 2 s deadline expired.
    pub fn timeout() -> Self {
        Self {
            kind: BriefColumnsErrorKind::Timeout,
            message: "Timed out measuring Brief column widths".to_string(),
        }
    }
}

impl From<BriefColumnsError> for BriefColumnsIpcError {
    fn from(err: BriefColumnsError) -> Self {
        match err {
            BriefColumnsError::FontMetricsNotReady => Self {
                kind: BriefColumnsErrorKind::FontMetricsNotReady,
                message: "Font metrics are not loaded for the requested font".to_string(),
            },
            BriefColumnsError::InvalidItemsPerColumn => Self {
                kind: BriefColumnsErrorKind::InvalidItemsPerColumn,
                message: "items_per_column must be at least 1".to_string(),
            },
            BriefColumnsError::ListingNotFound(id) => Self {
                kind: BriefColumnsErrorKind::ListingNotFound,
                message: format!("Listing {} is no longer cached", id),
            },
            BriefColumnsError::Other(message) => Self {
                kind: BriefColumnsErrorKind::Other,
                message,
            },
        }
    }
}

/// How many more `compute_brief_column_text_widths` calls the E2E harness wants to fail.
///
/// The frontend's recovery (bounded retries, provisional widths, a cursor that stays
/// visible) only runs when measurement DOESN'T arrive, and nothing reachable from a spec
/// can make the real computation fail on demand: the listing is healthy and the font
/// metrics are loaded. So the failure is injected here.
#[cfg(feature = "playwright-e2e")]
pub static FAIL_NEXT_WIDTH_CALLS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

/// Consumes one injected failure, if any are armed. Compiled out of production builds.
#[cfg(feature = "playwright-e2e")]
fn take_injected_failure() -> bool {
    use std::sync::atomic::Ordering;
    FAIL_NEXT_WIDTH_CALLS
        .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
        .is_ok()
}

#[cfg(not(feature = "playwright-e2e"))]
fn take_injected_failure() -> bool {
    false
}

/// Returns true if the entry is not a hidden dotfile.
fn is_visible(entry: &FileEntry) -> bool {
    !entry.name.starts_with('.')
}

/// Colored tags (color index 1-7) on an entry; color 0 (colourless) is dotless.
fn colored_tag_count(entry: &FileEntry) -> usize {
    entry.tags.iter().filter(|t| (1..=7).contains(&t.color)).count()
}

/// Pixel width the Finder tag-dot cluster reserves to the right of a filename,
/// as a pure function of the colored-tag count. Mirrors `tagClusterWidthPx` in
/// `src/lib/file-explorer/selection/tag-dots-utils.ts` (gap + overlapping dot
/// slots + optional `+N` chip); keep the constants in lockstep or a reserved
/// Brief column clips or over-pads the cluster. Returns 0 for an untagged row,
/// so a listing whose tags were never enriched (feature off) reserves nothing.
fn tag_cluster_width(colored_count: usize) -> f32 {
    if colored_count == 0 {
        return 0.0;
    }
    const DOT_SIZE: f32 = 10.0;
    const OVERLAP_OFFSET: f32 = 5.0;
    const CHIP_EXTRA: f32 = 8.0;
    const CLUSTER_GAP: f32 = 5.0;
    const MAX_DOTS: usize = 3;
    let slots = colored_count.min(MAX_DOTS);
    let has_chip = colored_count > MAX_DOTS;
    let base = DOT_SIZE + (slots - 1) as f32 * OVERLAP_OFFSET + if has_chip { CHIP_EXTRA } else { 0.0 };
    CLUSTER_GAP + base
}

/// Computes the widest filename's text-only width per Brief-mode column.
///
/// Returns one width per column required to display all visible entries (plus
/// the `".."` parent literal when `has_parent`). Values are guaranteed finite
/// (no NaN, no Infinity), so the FE's `Float64Array` prefix sums stay valid.
///
/// Never blocks on measurement: a filename containing a code point the font
/// cache has no width for is costed at the font's average width and that code
/// point is reported in `missing_code_points`, so the caller can measure it and
/// come back for exact widths.
///
/// Reads `LISTING_CACHE` with a read lock. Caller is responsible for wrapping
/// the call in a timeout if `LISTING_CACHE` could be write-locked.
pub fn compute_brief_column_text_widths(
    listing_id: &str,
    items_per_column: usize,
    has_parent: bool,
    font_id: &str,
    include_hidden: bool,
) -> Result<BriefColumnWidths, BriefColumnsError> {
    if items_per_column == 0 {
        return Err(BriefColumnsError::InvalidItemsPerColumn);
    }

    // Additive E2E hook: `take_injected_failure` is a `const false` in production builds,
    // so this branch compiles away entirely.
    if take_injected_failure() {
        return Err(BriefColumnsError::Other("injected width failure (E2E)".to_string()));
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
        return Ok(BriefColumnWidths {
            widths: Vec::new(),
            missing_code_points: Vec::new(),
        });
    }

    let total_columns = total_cells.div_ceil(items_per_column);
    let mut widths = Vec::with_capacity(total_columns);
    // Accumulated across every column so one report covers the whole listing.
    let mut missing = BTreeSet::new();

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

        // Build (name, tag-cluster-suffix) pairs for this column. The suffix
        // reserves room for the trailing Finder tag dots so a short-named but
        // tagged row doesn't get its dots clipped by the next column. We
        // allocate per column; typical column count is < 1000 even for huge
        // directories, so this is negligible next to the width computation.
        let mut items: Vec<(&str, f32)> = Vec::with_capacity(end_idx.saturating_sub(start_idx) + 1);
        if include_parent_literal {
            // The ".." literal carries no tags.
            items.push(("..", 0.0));
        }
        for entry in &visible[start_idx..end_idx] {
            items.push((entry.name.as_str(), tag_cluster_width(colored_tag_count(entry))));
        }

        let width =
            crate::font_metrics::calculate_max_width_with_suffixes(&items, font_id, &mut missing).ok_or_else(|| {
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
        // `calculate_max_width_with_suffixes` returns sums over per-char widths from the cached
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
    if !missing.is_empty() {
        log::debug!(
            target: "brief_columns",
            "{} code point(s) not yet measured for font_id='{}'; widths are estimated until they're filled in",
            missing.len(),
            font_id,
        );
    }

    Ok(BriefColumnWidths {
        widths,
        missing_code_points: missing.into_iter().collect(),
    })
}

#[cfg(test)]
mod tests {
    use super::{BriefColumnsError, BriefColumnsErrorKind, BriefColumnsIpcError, tag_cluster_width};

    // The frontend decides retry-or-give-up from `kind` alone. If a variant ever loses
    // its own kind and collapses into `Other`, the FE silently stops retrying (or starts
    // retrying a caller bug forever) with nothing to notice it by.
    #[test]
    fn every_backend_variant_keeps_its_own_wire_kind() {
        assert_eq!(
            BriefColumnsIpcError::from(BriefColumnsError::FontMetricsNotReady).kind,
            BriefColumnsErrorKind::FontMetricsNotReady
        );
        assert_eq!(
            BriefColumnsIpcError::from(BriefColumnsError::InvalidItemsPerColumn).kind,
            BriefColumnsErrorKind::InvalidItemsPerColumn
        );
        assert_eq!(
            BriefColumnsIpcError::from(BriefColumnsError::ListingNotFound("listing-7".to_string())).kind,
            BriefColumnsErrorKind::ListingNotFound
        );
        assert_eq!(
            BriefColumnsIpcError::from(BriefColumnsError::Other("lock poisoned".to_string())).kind,
            BriefColumnsErrorKind::Other
        );
        assert_eq!(BriefColumnsIpcError::timeout().kind, BriefColumnsErrorKind::Timeout);
    }

    // Mirrors `tagClusterWidthPx` in `tag-dots-utils.ts`; keep the two in sync.
    #[test]
    fn cluster_width_matches_fe_geometry() {
        // No colored tags reserves nothing.
        assert_eq!(tag_cluster_width(0), 0.0);
        // gap(5) + dot(10) + (slots-1)*overlap(5).
        assert_eq!(tag_cluster_width(1), 15.0);
        assert_eq!(tag_cluster_width(2), 20.0);
        assert_eq!(tag_cluster_width(3), 25.0);
        // 4+ overflows: 3 slots + chip extra(8).
        assert_eq!(tag_cluster_width(4), 33.0);
        // Plateaus past the cap regardless of count.
        assert_eq!(tag_cluster_width(42), tag_cluster_width(4));
    }
}
