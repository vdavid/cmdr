# Font metrics module

Measures character pixel widths via the Canvas API and ships the data to Rust for use in Brief mode column sizing.

## Key files

| File         | Purpose                                                                               |
| ------------ | ------------------------------------------------------------------------------------- |
| `measure.ts` | Canvas-based measurement over Unicode ranges, returns `Record<codePoint, pixelWidth>` |
| `index.ts`   | Orchestrates caching check, idle-time scheduling, and IPC to Rust                     |

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

The font ID `'system-400-12'` is **hardcoded in both TypeScript and Rust** and must stay in sync. It encodes
`fontFamily-fontWeight-fontSize`. When font settings become user-configurable this will read from settings instead.

For measurement, `'system'` resolves to `'-apple-system, BlinkMacSystemFont, system-ui, sans-serif'`.

## Key patterns and gotchas

- Measurement uses the Canvas API (`OffscreenCanvas`) — cannot run in Node.js or Vitest. Mock it in tests.
- The Rust module that consumes this data is at `src-tauri/src/font_metrics/mod.rs` (a sibling subsystem, not nested
  here).
- Rust uses the average width for any code point not present in the stored map.
- `requestIdleCallback` is used so measurement does not block the initial render.
- No TypeScript tests; the module is mocked in `DualPaneExplorer` tests.

## Dependencies

## Exported functions

- `ensureFontMetricsLoaded()` — main entry point; checks cache, schedules measurement if needed
- `getCurrentFontId()` — returns the current hardcoded font ID (`'system-400-12'`)

## Dependencies

- `$lib/tauri-commands` — `storeFontMetrics`, `hasFontMetrics`
- `$lib/logger` — `getAppLogger`
- Rust counterpart: `apps/desktop/src-tauri/src/font_metrics/mod.rs`
