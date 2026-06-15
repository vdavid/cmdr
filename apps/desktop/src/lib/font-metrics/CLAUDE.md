# Font metrics module

Measures character pixel widths via the Canvas API and ships them to Rust for Brief mode column sizing. The Rust
consumer is a sibling subsystem at `src-tauri/src/font_metrics/mod.rs` (not nested here).

## Files

- **`measure.ts`**: Canvas-based measurement over explicit Unicode ranges, returns `Record<codePoint, pixelWidth>`.
- **`index.ts`**: Caching check, idle-time scheduling, IPC to Rust. Exports `ensureFontMetricsLoaded()` (entry point;
  `lib/text-size.ts` calls it on a 1 s debounce after each scale change) and `getCurrentFontId()`.

## Must-knows

- **Measurement needs the Canvas API (`OffscreenCanvas`)**: it can't run in Node.js or Vitest, so mock it in tests (the
  module is mocked in `DualPaneExplorer` tests). There are no TS tests here.
- **Font ID is `family-weight-size`** (for example `system-400-12`): the size component tracks the effective text scale
  (`getEffectiveScale()` from `$lib/text-size`, `round(12 * scale)`). A new size is a fresh cache miss → re-measure →
  IPC → new `{font_id}.bin` on disk. Multiple sizes coexist; the Rust side never evicts and preloads all on startup via
  `load_all_metrics_from_disk`. Don't change the ID format without keeping it a stable, size-varying cache key, or the
  Rust cache silently serves stale widths.
- **Measure on the frontend, not in Rust**: Canvas `measureText()` uses the same shaping engine (CoreText on macOS) that
  renders the UI, so widths match exactly. Rust has no access to rendered font metrics.
- **The Private Use Area (U+E000–U+F8FF) is listed in the ranges array but skipped via `skipRanges`**: intentional, to
  keep the exclusion visible. Rust falls back to the average width for any code point absent from the map.
- For measurement, `'system'` resolves to `'-apple-system, BlinkMacSystemFont, system-ui, sans-serif'`.

## Dependencies

- `$lib/tauri-commands`: `storeFontMetrics`, `hasFontMetrics`.
- `$lib/logging/logger`: `getAppLogger`.
- Rust counterpart: `apps/desktop/src-tauri/src/font_metrics/mod.rs`.

Full details: [DETAILS.md](DETAILS.md).
