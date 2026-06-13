# Font metrics module

Measures character pixel widths via the Canvas API and ships the data to Rust for use in Brief mode column sizing.

## Key files

- **`measure.ts`**: Canvas-based measurement over Unicode ranges, returns `Record<codePoint, pixelWidth>`
- **`index.ts`**: Orchestrates caching check, idle-time scheduling, and IPC to Rust

## Data flow

```
ensureFontMetricsLoaded()
  │
  ├─ hasFontMetrics(fontId) ──► cached ──► return immediately
  │
  └─ not cached
       │
       └─ requestIdleCallback (setTimeout(0) fallback)
            │
            └─ measureCharWidths(family, size, weight)   [Canvas API, ~100–300ms]
                 │
                 └─ storeFontMetrics(fontId, widths)     [IPC to Rust, ~426KB bincode2 (approximate)]
```

`measureCharWidths` creates an `OffscreenCanvas`, sets the font, then iterates over explicit Unicode ranges covering BMP
printable characters plus common emoji (U+1F300–U+1FAFF). The Private Use Area (U+E000–U+F8FF) is listed in the ranges
array but is skipped via a `skipRanges` set.

## Font ID

Font IDs encode `fontFamily-fontWeight-fontSize`, e.g. `system-400-12` (the default at scale 1). The size component
follows the effective text scale: `getCurrentFontId()` reads `getEffectiveScale()` from `$lib/text-size` and rounds
`12 * scale` to the nearest pixel. Any new size produces a fresh cache miss → re-measure → IPC to Rust → new
`{font_id}.bin` on disk. Multiple sizes coexist in the in-memory cache and on disk; `load_all_metrics_from_disk`
preloads them all at app startup.

For measurement, `'system'` resolves to `'-apple-system, BlinkMacSystemFont, system-ui, sans-serif'`.

## Key decisions

**Decision**: Measure on the frontend (Canvas API) and send to Rust, rather than measuring in Rust. **Why**: Rust has no
access to the actual rendered font metrics; the browser's text shaping engine (CoreText on macOS) determines how wide
each glyph is at a given font/size/weight. The Canvas API's `measureText()` uses the same engine that renders the UI, so
the widths match exactly. Measuring in Rust would require linking a font shaping library and still might not match the
browser's output.

**Decision**: Explicit Unicode range list instead of iterating all code points 0x0000-0xFFFF. **Why**: Many code points
in the BMP are unassigned or control characters that have zero width. Measuring them wastes time and bloats the width
map sent to Rust. The explicit ranges cover all printable blocks. The Private Use Area is listed in the ranges array but
skipped via `skipRanges`: this makes the intentional exclusion visible rather than silently omitting the range.

**Decision**: `requestIdleCallback` scheduling with `setTimeout(0)` fallback. **Why**: Measurement takes 100-300ms (tens
of thousands of `measureText` calls). Running it synchronously during app boot would delay the first meaningful paint.
`requestIdleCallback` defers it until the browser is idle. The `setTimeout(0)` fallback handles environments where
`requestIdleCallback` is unavailable (some WebView configurations).

**Decision**: Font ID is now derived from the effective text scale. **Why**: Adding the `appearance.textSize` setting
(compounded with the macOS Accessibility text size) means Brief mode renders text at different pixel sizes per user. The
Rust width cache is keyed by exact font ID, so the only correct fix is to vary the ID with size. The ID format
(`family-weight-size`) was already designed as a cache key; flipping the size component naturally invalidates and
triggers a re-measure. Multiple sizes coexist in the cache; the Rust side never evicts.

## Key patterns and gotchas

- Measurement uses the Canvas API (`OffscreenCanvas`); cannot run in Node.js or Vitest. Mock it in tests.
- The Rust module that consumes this data is at `src-tauri/src/font_metrics/mod.rs` (a sibling subsystem, not nested
  here).
- Rust uses the average width for any code point not present in the stored map.
- `requestIdleCallback` is used so measurement does not block the initial render.
- No TypeScript tests; the module is mocked in `DualPaneExplorer` tests.

## Dependencies

## Exported functions

- `ensureFontMetricsLoaded()`: main entry point; checks cache, schedules measurement if needed. `lib/text-size.ts` calls
  this on a 1 s debounce after each scale change.
- `getCurrentFontId()`: returns the font ID for the current effective scale (e.g. `system-400-12`, `system-400-15`).

## Dependencies

- `$lib/tauri-commands`: `storeFontMetrics`, `hasFontMetrics`
- `$lib/logging/logger`: `getAppLogger`
- Rust counterpart: `apps/desktop/src-tauri/src/font_metrics/mod.rs`
