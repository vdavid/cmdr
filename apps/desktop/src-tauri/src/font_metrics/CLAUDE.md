# Font metrics

Binary font metrics cache and text width calculation for Brief mode column sizing. Rust cannot directly access system fonts, so the frontend measures character widths via the Canvas API and ships them to Rust over IPC.

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
| `init_font_metrics(app, font_id)` | Called at app startup — loads from disk into cache if file exists |

### Internal state

```
METRICS_CACHE: LazyLock<RwLock<HashMap<String, FontMetrics>>>
```

`FontMetrics` holds: `version: u32`, `font_id: String`, `widths: HashMap<u32, f32>`, `average_width: f32`.

## Key patterns

- **Cache key format**: `"{family}-{weight}-{size}"`, e.g. `"system-400-12"`. Must match what the frontend's `getCurrentFontId()` returns — a mismatch means `calculate_max_width` returns `None`.
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
**Why**: The frontend only measures a known character set (typically Latin + common symbols). Filenames can contain any Unicode — emoji, CJK, Arabic. Returning zero would collapse unknown characters to invisible width, breaking column alignment. The average width is a reasonable approximation that keeps columns roughly sized even for scripts the frontend didn't explicitly measure.

## Gotchas

**Gotcha**: If the frontend's `getCurrentFontId()` format changes, `calculate_max_width` silently returns `None`.
**Why**: The cache key is a string like `"system-400-12"` that must match exactly between frontend and backend. There's no validation — a mismatch just means the key isn't found in the cache. The frontend handles the `None` by falling back to its own width estimation.

## Dependencies

External: `bincode2`
Internal: `crate::config::resolved_app_data_dir`
Internal: none
