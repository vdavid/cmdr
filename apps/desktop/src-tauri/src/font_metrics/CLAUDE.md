# Font metrics

Binary font metrics cache and text width calculation for Brief mode column sizing. Rust can't access system fonts, so
the frontend measures character widths via the Canvas API and ships them to Rust over IPC.

The whole module is `mod.rs` (plus `mod_test.rs`). `calculate_max_width_with_suffixes` is the basis for per-column text widths in
Brief mode via `file_system::listing::brief_columns` (which powers the `get_brief_column_text_widths` IPC).

## Public API

- **`store_and_persist(app, font_id, widths)`**: replace a font's entry (cache + `{font_id}.bin`) from the eager
  measurement. Takes ownership of the map.
- **`extend_and_persist(app, font_id, widths)`**: merge on-demand measured widths into an existing entry and rewrite
  its file. Errors if the font isn't cached.
- **`has_metrics(font_id)`**: is this font ID cached?
- **`calculate_max_width_with_suffixes(items, font_id, missing)`**: widest of `(text, trailing-px-suffix)` pairs
  (suffix `0.0` is the plain widest-string case; the Brief tag-dot reservation passes a per-row cluster width); `None`
  if the font ID isn't cached. Records every unmeasured code point into `missing`. Primary width entry point
  (`FontMetrics::calculate_text_width` is the per-string method used internally).
- **`load_from_disk` / `init_font_metrics(app, font_id)`**: read `{font_id}.bin` (bincode2) under `~/…/font-metrics/`;
  the latter is an idempotent startup load of one ID.
- **`load_all_metrics_from_disk(app)`**: startup scan that pre-loads every `*.bin`, so user-customized text sizes are
  warm on first paint.

Cache: `METRICS_CACHE: LazyLock<RwLock<HashMap<String, FontMetrics>>>`. `FontMetrics` holds `version`, `font_id`,
`widths`, `average_width`.

## Must-knows

- **`average_width` is a one-paint stand-in, never a resting state.** An unmeasured code point is costed at the average
  AND reported through `missing`, so the frontend measures it and calls `extend_font_metrics`; from the next query on
  the width is exact. ❌ Don't drop the `missing` out-param to "simplify" a width call — that silently reinstates
  permanently-approximate columns for every non-Latin filename. The frontend measures only a small eager set (Latin,
  punctuation, symbols, common emoji), so this path is normal, not exceptional.
- **Cache key is `"{family}-{weight}-{size}"`** (for example `"system-400-12"`) and MUST match the frontend's
  `getCurrentFontId()`. No validation: a mismatch just returns `None`. Size varies with `appearance.textSize` × system
  Accessibility text size, so several sizes can coexist in cache. If `getCurrentFontId()`'s format changes, width
  calculation silently breaks. The Brief-column path surfaces a missing key as `BriefColumnsError::FontMetricsNotReady`,
  which crosses the wire as `BriefColumnsIpcError { kind: fontMetricsNotReady, … }`; the frontend branches on `kind`,
  calls `ensureFontMetricsLoaded()`, and retries once, leaving the columns at their provisional width until widths
  arrive.
- **Both writes serialize under the lock, then write the file after releasing it.** That's what lets `extend` merge
  into the cached map without cloning it out; a few thousand pairs is not a map you want to copy per call, and this
  path used to clone it twice.

## Dependencies

External: `bincode2`. Internal: `crate::config::resolved_app_data_dir`, `crate::ignore_poison`.

Full details (decisions: Canvas-measure over Rust fonts, binary-over-JSON format, `RwLock`, the fill-in contract):
`DETAILS.md`.
