# Font metrics

Binary font metrics cache and text width calculation for Brief mode column sizing. Rust cannot directly access system fonts, so the frontend measures character widths via the Canvas API and ships them to Rust over IPC.

`calculate_max_width` is the basis for per-column text widths in Brief mode via the
`file_system::listing::brief_columns` module (which powers the `get_brief_column_text_widths` IPC). Each column's
widest filename is measured here, then the FE adds chrome and clamps.

## Key file

`mod.rs` — the entire module is one file (plus `mod_test.rs` for tests).

### Public API

| Function | Purpose |
|---|---|
| `store_metrics(font_id, widths)` | Store a `HashMap<u32 (code point), f32 (px width)>` into the in-memory cache |
| `has_metrics(font_id)` | Check if metrics for a font ID exist in cache |
| `calculate_max_width(texts, font_id)` | Find the widest string from a slice; returns `None` if font ID not in cache |
| `load_from_disk(app, font_id)` | Read `{font_id}.bin` (bincode2) from `~/…/font-metrics/` |
| `save_to_disk(app, font_id, widths)` | Serialize and write metrics to `{font_id}.bin` |
| `init_font_metrics(app, font_id)` | Called at app startup — loads a single specific font ID from disk into cache if its file exists |
| `load_all_metrics_from_disk(app)` | Called at app startup — scans `~/…/font-metrics/` and pre-loads every `*.bin` file. Used so user-customized text sizes are warm on first paint. |

### Internal state

```
METRICS_CACHE: LazyLock<RwLock<HashMap<String, FontMetrics>>>
```

`FontMetrics` holds: `version: u32`, `font_id: String`, `widths: HashMap<u32, f32>`, `average_width: f32`.

## Key patterns

- **Cache key format**: `"{family}-{weight}-{size}"`, e.g. `"system-400-12"`. Must match what the frontend's `getCurrentFontId()` returns — a mismatch means `calculate_max_width` returns `None`. The size component now varies with the user's `appearance.textSize` × system Accessibility text size, so several sizes can live in the cache simultaneously (e.g. `system-400-12` and `system-400-15`).
- **Disk format**: bincode2 binary (~426 KB for a full Latin character set). File path: `~/Library/Application Support/…/font-metrics/{font_id}.bin`.
- **Unmeasured code points** (e.g., rare Unicode) fall back to `average_width` computed as the mean of all measured widths.
- `calculate_max_width` is the primary public entry point for width calculation. `FontMetrics::calculate_text_width` is the per-string method used internally.
- `init_font_metrics` is idempotent — safe to call multiple times; it just overwrites the cache entry.

## Key decisions

**Decision**: Frontend measures character widths via Canvas API and ships them to Rust over IPC, rather than Rust measuring fonts directly.
**Why**: Rust has no access to the system's font rendering stack. The browser's Canvas API uses the exact same font rasterizer the user sees, so the measurements match pixel-perfectly. Any Rust-side font library would need to load font files, handle system font resolution, and might produce slightly different widths than what the browser actually renders.

**Decision**: Binary format (bincode2, a maintained fork of the original bincode) on disk instead of JSON.
**Why**: A full Latin character set produces ~4,000 code-point-to-width entries. As JSON that's ~100 KB with key quoting overhead. Bincode compresses this to ~26 KB and deserializes in microseconds vs. milliseconds for JSON parsing. Since this file is only read by Rust (never human-edited), readability doesn't matter.

**Decision**: `RwLock` for the metrics cache instead of `Mutex`.
**Why**: `calculate_max_width` is called on every Brief mode render for every visible column. Multiple Tauri command threads may need to read metrics concurrently. `RwLock` allows unlimited parallel reads; a `Mutex` would serialize all column width calculations, adding latency to directory listing renders.

**Decision**: Average-width fallback for unmeasured code points instead of returning an error or zero.
**Why**: The frontend only measures a known character set: Latin, BMP-printable characters, and common emoji
(U+1F300–U+1FAFF). Filenames can contain any Unicode — CJK, Arabic, complex scripts, rare symbols. Returning zero would
collapse unknown characters to invisible width, breaking column alignment. The average width keeps Brief-mode columns
roughly sized even for scripts the frontend didn't explicitly measure — at the cost of slight visual mis-measurement
for CJK / complex-script filenames. Emoji and Latin are pixel-accurate; everything else is approximate. Expanding the
measured set is a follow-up.

## Gotchas

**Gotcha**: If the frontend's `getCurrentFontId()` format changes, `calculate_max_width` silently returns `None`.
**Why**: The cache key is a string like `"system-400-12"` that must match exactly between frontend and backend. There's
no validation — a mismatch just means the key isn't found in the cache. The Brief-column path surfaces this via
`BriefColumnsError::FontMetricsNotReady`, which the IPC wrapper maps to `IpcError { message: "font_metrics_not_ready" }`.
The frontend catches that specific error, calls `ensureFontMetricsLoaded()`, and retries once. Until widths arrive,
columns render at `MAX_BRIEF_COLUMN_WIDTH` as a fallback. Same race fires on a scale flip — the new font ID isn't
cached for ~100–300 ms while metrics get re-measured.

## Dependencies

External: `bincode2`
Internal: `crate::config::resolved_app_data_dir`
Internal: none
