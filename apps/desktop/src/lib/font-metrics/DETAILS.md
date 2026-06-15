# Font metrics details

Depth for the frontend font-metrics module. `CLAUDE.md` holds the must-knows; this file holds the rationale and flow.

## Data flow

```
ensureFontMetricsLoaded()
  ├─ hasFontMetrics(fontId) ──► cached ──► return immediately
  └─ not cached
       └─ requestIdleCallback (setTimeout(0) fallback)
            └─ measureCharWidths(family, size, weight)   [Canvas API, ~100–300ms]
                 └─ storeFontMetrics(fontId, widths)     [IPC to Rust, ~426KB bincode2, approximate]
```

`measureCharWidths` creates an `OffscreenCanvas`, sets the font, then iterates over explicit Unicode ranges covering BMP
printable characters plus common emoji (U+1F300–U+1FAFF). The Private Use Area (U+E000–U+F8FF) is in the ranges array
but skipped via a `skipRanges` set.

## Decisions

- **Explicit Unicode range list instead of iterating all of 0x0000–0xFFFF**: many BMP code points are unassigned or
  zero-width control characters. Measuring them wastes time and bloats the map sent to Rust. The explicit ranges cover
  all printable blocks; listing the Private Use Area but skipping it makes the exclusion visible rather than silent.
- **`requestIdleCallback` scheduling with a `setTimeout(0)` fallback**: measurement takes 100–300ms (tens of thousands
  of `measureText` calls). Running it synchronously at boot would delay first meaningful paint. The fallback covers
  WebView configurations where `requestIdleCallback` is unavailable.
- **Font ID derived from the effective text scale**: the `appearance.textSize` setting compounded with the macOS
  Accessibility text size means Brief mode renders at different pixel sizes per user. The Rust width cache is keyed by
  exact font ID, so varying the size component naturally invalidates and re-measures. Multiple sizes coexist; Rust never
  evicts.
