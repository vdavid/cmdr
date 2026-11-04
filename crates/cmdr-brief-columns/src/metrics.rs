//! Per-code-point font widths and the text widths they add up to.
//!
//! The frontend measures only a small eager set of code points up front (Latin,
//! punctuation, symbols, common emoji). Anything else (CJK, Hangul, the Indic
//! blocks, an emoji added after this build) is absent from the map on first
//! sight. Width queries therefore answer immediately with `average_width` for
//! those and REPORT them to the caller, which measures them and extends the
//! entry. From that point the real widths are used. See
//! `apps/desktop/src/lib/font-metrics/DETAILS.md` § On-demand fill-in.

use std::collections::{BTreeSet, HashMap};
use std::sync::RwLock;

use cmdr_fs::ignore_poison::RwLockIgnorePoison;
use serde::{Deserialize, Serialize};

/// Font metrics for a specific font configuration.
/// Stores character widths and an average width for fallback.
///
/// The derived `Serialize` is the desktop's on-disk format (bincode2, one file
/// per font ID), so ❌ never reorder, rename, or retype a field.
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
    pub(crate) fn calculate_text_width(&self, text: &str, missing: &mut BTreeSet<u32>) -> f32 {
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

/// Every font size's metrics a host holds, keyed by font ID (like
/// `"system-400-12"`, the frontend's `getCurrentFontId()`).
///
/// An `RwLock`: every Brief-mode layout reads it for every column, from several
/// command threads at once on the desktop, and writes are rare.
#[derive(Debug, Default)]
pub struct FontMetricsStore {
    fonts: RwLock<HashMap<String, FontMetrics>>,
}

impl FontMetricsStore {
    /// An empty store: every width query answers `None` until a font is inserted.
    pub fn new() -> Self {
        Self::default()
    }

    /// Replaces the entry for `font_id`.
    pub fn insert(&self, font_id: String, metrics: FontMetrics) {
        self.fonts.write_ignore_poison().insert(font_id, metrics);
    }

    /// Merges newly measured widths into `font_id`'s entry, then runs `then` on
    /// the merged entry while still holding the write lock (the desktop
    /// serializes it there, so the map, thousands of pairs, is never cloned out).
    /// Keep `then` CPU-only and short. `Err` when the font isn't stored: it was
    /// never measured, so there's nothing to extend.
    pub fn extend_with<T>(
        &self,
        font_id: &str,
        widths: HashMap<u32, f32>,
        then: impl FnOnce(&FontMetrics) -> T,
    ) -> Result<T, String> {
        let mut fonts = self.fonts.write_ignore_poison();
        let Some(metrics) = fonts.get_mut(font_id) else {
            // The caller's next `ensureFontMetricsLoaded` measures the eager set afresh.
            return Err(format!("No metrics cached for font_id='{font_id}'"));
        };
        metrics.extend(widths);
        Ok(then(metrics))
    }

    /// Whether `font_id` is stored.
    pub fn contains(&self, font_id: &str) -> bool {
        self.fonts.read_ignore_poison().contains_key(font_id)
    }

    /// Calculates the maximum width among text strings, each carrying a trailing
    /// pixel suffix (a non-text decoration rendered after it, like the Finder
    /// tag-dot cluster) added to that text's own width before taking the max. A
    /// suffix of `0.0` is the plain widest-string case. Lets a single Brief column
    /// reserve room for a wide-name row and a tagged-but-short-name row
    /// independently. `None` when the font ID isn't stored.
    ///
    /// Code points with no measured width are counted at `average_width` and
    /// collected into `missing`; the caller reports them so they get measured.
    pub(crate) fn max_width_with_suffixes(
        &self,
        items: &[(&str, f32)],
        font_id: &str,
        missing: &mut BTreeSet<u32>,
    ) -> Option<f32> {
        let fonts = self.fonts.read_ignore_poison();
        let metrics = fonts.get(font_id)?;

        items
            .iter()
            .map(|(text, suffix)| metrics.calculate_text_width(text, missing) + suffix)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Like [`Self::max_width_with_suffixes`] for a size that isn't measured yet:
    /// costs the text with the stored size of the same family and weight nearest
    /// `font_id`'s, scaled by the size ratio. Text width tracks font size to within
    /// about 1%, so this stands in while the exact size is measured, which takes
    /// seconds. `None` if no size of this family and weight is stored.
    ///
    /// Reports no missing code points: a fill-in can only extend a stored size, and
    /// the exact size's own query reports them once it's measured. Suffixes are fixed
    /// pixels, so they aren't scaled.
    pub(crate) fn max_width_scaled_from_nearest(&self, items: &[(&str, f32)], font_id: &str) -> Option<f32> {
        let (prefix, size) = split_font_id(font_id)?;
        let fonts = self.fonts.read_ignore_poison();
        // Nearest by ratio, not difference: 12 → 24 is as far as 6 → 12.
        let distance = |other: f32| (other / size).ln().abs();
        let (metrics, source_size) = fonts
            .iter()
            .filter_map(|(id, metrics)| {
                let (other_prefix, other_size) = split_font_id(id)?;
                (other_prefix == prefix).then_some((metrics, other_size))
            })
            .min_by(|(_, a), (_, b)| {
                distance(*a)
                    .partial_cmp(&distance(*b))
                    .unwrap_or(std::cmp::Ordering::Equal)
            })?;
        let ratio = size / source_size;
        let mut unreported = BTreeSet::new();
        items
            .iter()
            .map(|(text, suffix)| metrics.calculate_text_width(text, &mut unreported) * ratio + suffix)
            .max_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
    }
}

/// Splits a font ID into its `family-weight` prefix and its pixel size.
fn split_font_id(font_id: &str) -> Option<(&str, f32)> {
    let (prefix, size) = font_id.rsplit_once('-')?;
    let size: f32 = size.parse().ok()?;
    (size.is_finite() && size > 0.0).then_some((prefix, size))
}

/// Why a metrics write (`store_font_metrics`, `extend_font_metrics`) didn't land.
///
/// ❌ Not prose: the frontend logs the variant and re-measures next time. A
/// pane whose widths didn't persist renders at its provisional width, so
/// nothing here has ever needed words.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum FontMetricsError {
    /// The two parallel arrays didn't line up, which means a caller bug rather
    /// than anything about the user's data. Carries both lengths so the log
    /// says which side is short.
    MismatchedWidths {
        /// How many code points arrived.
        code_points: usize,
        /// How many widths arrived.
        widths: usize,
    },
    /// Serializing or writing the metrics file didn't work (or, for an extend,
    /// the font isn't stored).
    CouldntPersist {
        /// What the store reported, for the log.
        detail: String,
    },
    /// The write didn't finish inside the command's wait.
    TimedOut,
    /// The blocking task panicked, so no answer is coming.
    Unexpected {
        /// What the runtime reported, for the log.
        detail: String,
    },
}

/// Zips the wire's parallel arrays (code points, widths) into the map a store
/// holds. They cross as two arrays rather than a `Record<codePoint, width>`: the
/// object form spends a quoted key per entry.
///
/// A length mismatch means a caller bug, not user data, so it's rejected
/// rather than silently truncated.
pub fn widths_from_parallel(code_points: Vec<u32>, widths: Vec<f32>) -> Result<HashMap<u32, f32>, FontMetricsError> {
    if code_points.len() != widths.len() {
        return Err(FontMetricsError::MismatchedWidths {
            code_points: code_points.len(),
            widths: widths.len(),
        });
    }
    Ok(code_points.into_iter().zip(widths).collect())
}
