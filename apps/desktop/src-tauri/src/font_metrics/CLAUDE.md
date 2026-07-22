# Font metrics

Binary font metrics cache and text width calculation for Brief mode column sizing. Rust can't access system fonts, so
the frontend measures character widths via the Canvas API and ships them to Rust over IPC.

The whole module is `mod.rs` (plus `mod_test.rs`). `calculate_max_width_with_suffixes` is the basis for per-column text widths in
Brief mode via `file_system::listing::brief_columns` (which powers the `get_brief_column_text_widths` IPC).

## Public API

- **`store_metrics(font_id, widths)`**: store a `HashMap<u32 code point, f32 px width>` into the in-memory cache.
- **`has_metrics(font_id)`**: is this font ID cached?
- **`calculate_max_width_with_suffixes(items, font_id)`**: widest of `(text, trailing-px-suffix)` pairs (suffix `0.0`
  is the plain widest-string case; the Brief tag-dot reservation passes a per-row cluster width); `None` if the font ID
  isn't cached. Primary
  width entry point (`FontMetrics::calculate_text_width` is the per-string method used internally).
- **`load_from_disk` / `save_to_disk`**: read/write `{font_id}.bin` (bincode2) under `~/…/font-metrics/`.
- **`init_font_metrics(app, font_id)`**: startup load of one font ID from disk if its file exists. Idempotent.
- **`load_all_metrics_from_disk(app)`**: startup scan that pre-loads every `*.bin`, so user-customized text sizes are
  warm on first paint.

Cache: `METRICS_CACHE: LazyLock<RwLock<HashMap<String, FontMetrics>>>`. `FontMetrics` holds `version`, `font_id`,
`widths`, `average_width`.

## Must-knows

- **Cache key is `"{family}-{weight}-{size}"`** (for example `"system-400-12"`) and MUST match the frontend's
  `getCurrentFontId()`. No validation: a mismatch just returns `None`. Size varies with `appearance.textSize` × system
  Accessibility text size, so several sizes can coexist in cache. If `getCurrentFontId()`'s format changes, width
  calculation silently breaks. The Brief-column path surfaces a missing key as `BriefColumnsError::FontMetricsNotReady`
  → `IpcError { message: "font_metrics_not_ready" }`; the frontend catches that, calls `ensureFontMetricsLoaded()`, and
  retries once, rendering at `MAX_BRIEF_COLUMN_WIDTH` until widths arrive. The same race fires on a scale flip
  (~100-300 ms uncached).
- **Unmeasured code points fall back to `average_width`** (mean of measured widths), never zero: zero would collapse
  unknown characters to invisible width and break alignment. The frontend measures only Latin, BMP-printable, and
  common emoji (U+1F300-U+1FAFF), so CJK / Arabic / complex scripts are approximate; Latin and emoji are pixel-accurate.

## Dependencies

External: `bincode2`. Internal: `crate::config::resolved_app_data_dir`.

Full details (decisions: Canvas-measure over Rust fonts, binary-over-JSON format, `RwLock`, average fallback):
`DETAILS.md`.
