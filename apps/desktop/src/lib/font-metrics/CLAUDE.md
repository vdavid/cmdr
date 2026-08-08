# Font metrics module

Measures character pixel widths via the Canvas API and ships them to Rust for Brief mode column sizing. The Rust
consumer is a sibling subsystem at `src-tauri/src/font_metrics/mod.rs` (not nested here).

## Files

- **`ranges.ts`**: which code points get measured eagerly, and `isMeasurable` (rejects lone surrogates).
- **`measure.ts`**: the pure measuring core plus font-ID parsing. Takes a context, so it's testable without Canvas.
- **`measure-worker.ts`**: the worker shell that owns an `OffscreenCanvas`. Thin by design.
- **`worker-client.ts`**: runs jobs in the worker, with a chunked main-thread fallback.
- **`index.ts`**: lifecycle and IPC. Exports `ensureFontMetricsLoaded()`, `fillMissingFontMetrics()`,
  `setMeasuresFontMetrics()`, and `getCurrentFontId()`.

## Must-knows

- **Measuring NEVER runs on the main thread when a worker is available**, and the fallback yields on a time budget. A
  straight loop over the code points froze the UI for seconds at rest and ~46 s under load, because
  `requestIdleCallback` defers only the START of a synchronous loop. ❌ Don't "simplify" `worker-client.ts` back into a
  direct call. `DETAILS.md` § Why a worker.
- **Only the window that opted in measures.** Every window runs `initTextSize`, but only the main one renders Brief
  mode; `initTextSize({ measuresFontMetrics: true })` in `routes/(main)/+layout.svelte` is the single opt-in, and
  `setMeasuresFontMetrics` gates both entry points. Without it, a text-size change made every window measure the same
  font at once on the thread they share.
- **`ensureFontMetricsLoaded` must register its in-flight entry in the same tick as the lookup.** It's deliberately not
  `async`: an `await` before the `inFlight.set` lets every concurrent caller past the check, which is the duplicate work
  the map exists to prevent (three callers can land at once: `DualPaneExplorer` mount, the text-size debounce, and
  `BriefList`'s not-ready retry). Pinned by `font-metrics.test.ts`.
- **The eager set is small on purpose (~5k code points, Latin + symbols + common emoji).** Everything else — CJK,
  Hangul, Ethiopic, Myanmar, the Indic blocks — arrives through the on-demand fill-in instead. Adding a bulk block back
  to `EAGER_RANGES` restores the freeze; `ranges.test.ts` fails if the set grows past 8,000.
- **Widths cross IPC as two parallel arrays**, never a `Record<codePoint, width>`: the object form spends a quoted key
  per entry. `store_font_metrics` replaces a font's entry; `extend_font_metrics` merges the fill-in.
- **Font ID is `family-weight-size`** (for example `system-400-12`): the size component tracks the effective text scale
  (`round(12 * scale)`). A new size is a fresh cache miss → re-measure → IPC → new `{font_id}.bin` on disk. Multiple
  sizes coexist; the Rust side never evicts and preloads all on startup via `load_all_metrics_from_disk`. Don't change
  the ID format without keeping it a stable, size-varying cache key, or the Rust cache silently serves stale widths.
- **Measure on the frontend, not in Rust**: Canvas `measureText()` uses the same shaping engine (CoreText on macOS) that
  renders the UI, so widths match exactly. Rust has no access to rendered font metrics.
- For measurement, `'system'` resolves to `'-apple-system, BlinkMacSystemFont, system-ui, sans-serif'`.

## Dependencies

- `$lib/tauri-commands`: `storeFontMetrics`, `extendFontMetrics`, `hasFontMetrics`.
- `$lib/logging/logger`: `getAppLogger` (main thread only; a worker has no bridge to post to).
- Rust counterpart: `apps/desktop/src-tauri/src/font_metrics/mod.rs`.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
