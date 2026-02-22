# Font metrics

Binary font metrics cache and text width calculation for Brief mode column sizing. Rust cannot directly access system fonts, so the frontend measures character widths via the Canvas API and ships them to Rust over IPC.

## Key file

`mod.rs` — the entire module is one file (plus `mod_test.rs` for tests).

### Public API

| Function | Purpose |
|---|---|
| `store_metrics(font_id, widths)` | Store a `HashMap<u32 (code point), f32 (px width)>` into the in-memory cache |
| `has_metrics(font_id)` | Check if metrics for a font ID exist in cache |
| `calculate_text_width(text, font_id)` | Sum per-character widths; falls back to `average_width` for unmeasured code points |
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
- `calculate_text_width` is `#[allow(dead_code)]` — it's part of the public API kept for future use; `calculate_max_width` is the primary call site.
- `init_font_metrics` is idempotent — safe to call multiple times; it just overwrites the cache entry.

## Dependencies

External: `bincode2`, `tauri::Manager`
Internal: none
