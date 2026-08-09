# Font metrics details

Depth for the frontend font-metrics module. `CLAUDE.md` holds the must-knows; this file holds the rationale and flow.

## Data flow

```
ensureFontMetricsLoaded()            ← main window only (setMeasuresFontMetrics)
  ├─ in-flight for this font ID? ──► share that promise
  ├─ hasFontMetrics(fontId) ──────► cached ──► return
  └─ not cached
       └─ eagerCodePoints()                   [~5k code points]
            └─ measureOffMainThread()         [worker; chunked main thread if unavailable]
                 └─ storeFontMetrics(fontId, codePoints, widths)

BriefList.doFetchColumnWidths()
  └─ getBriefColumnTextWidths(…) ──► { widths, missingCodePoints }
       ├─ paint widths now                    [unmeasured chars costed at average_width]
       └─ missingCodePoints non-empty?
            └─ fillMissingFontMetrics()       [measure just those, off the main thread]
                 └─ extendFontMetrics(…) ──► re-fetch once ──► exact widths
```

## Why a worker

Measuring is tens of thousands of `measureText` calls, and most of the expensive ones are font-fallback misses (a code
point the system font has no glyph for sends CoreText down the fallback cascade). Run synchronously on the main thread
that blocks everything: input, paint, and the log bridge.

It was scheduled through `requestIdleCallback`, which reads as "non-blocking" and isn't: `requestIdleCallback` defers
only the START of the callback. Once a synchronous loop begins it runs to completion, deadline or no deadline.

Measured on 2026-08-08 (dev build, macOS 15, `system-400-11`, from the app log): 2,884 ms with the machine idle, and
49,322 ms on a machine also running an SMB copy and a heavy index pass. During the second one the UI took no input at
all and the queued keystrokes replayed in a single batch when it ended.

Two properties fix it, and both are needed:

- **The worker** takes the loop off the main thread entirely. `OffscreenCanvas` is transferable and measures with the
  same rasterizer, so widths are identical to what the main thread would have produced.
- **The chunked fallback** covers a WebView with no `Worker` or no `OffscreenCanvas` (WebKitGTK on the Linux E2E image
  is the realistic case). It measures against a plain `<canvas>` and yields every 8 ms. Slower end to end than the
  worker, because each yield costs a task hop, but it never holds the thread. The budget is checked against the clock
  rather than a character count, so a machine where each call is 20× slower yields just as promptly, and the loop always
  measures at least one code point per slice so a pathologically slow call can't stall it at index 0.

`measureCodePointsChunked` takes its `yieldToEventLoop` as a parameter purely so the test can observe the yields without
waiting on real timers.

## On-demand fill-in

The eager set covers what a Latin-script filename actually contains. Everything else is measured only once something
needs it, which keeps the up-front cost roughly a tenth of what it was and makes the result MORE accurate than the old
full sweep: a code point outside the old range list (a script it never listed, an emoji added after the build) used to
render at the average width forever, and now gets measured like any other.

The loop deliberately has no new event channel. `get_brief_column_text_widths` already round-trips per layout change, so
it carries the report back on its own response:

1. Rust costs an unmeasured code point at `average_width`, so the query answers immediately and the columns paint.
2. It returns those code points in `missingCodePoints` (ascending, deduplicated — they're gathered in a `BTreeSet`).
3. The frontend measures exactly those, off the main thread, and calls `extend_font_metrics`.
4. It re-fetches once. From here the font's entry has real widths, so every later query is exact and reports nothing.

**The average width is used for one paint, never as a resting state.** That's the contract; a "good enough" estimate
that persists is the thing this design exists to avoid.

Dedup lives on the frontend, in a per-font-ID set of code points already sent. Repeated reports (several listings, a
re-fetch mid-flight) measure once. On failure the entries are removed again, so a transient worker error doesn't strand
those code points on the average forever — the next listing reports them and the fill retries. That self-healing is why
Rust doesn't need to track what it has already announced.

`BriefList`'s `WidthFetchAttempt.afterFill` bounds the round-trip: the re-fetch never triggers a second fill, so a code
point the font genuinely can't measure (it comes back reported again) can't loop.

Measured end to end on 2026-08-09 (dev build, macOS 15, machine mid-index-scan): the eager set is 5,199 code points and
took 11,010 ms to measure with the machine loaded, during which the main thread's longest stall was **16 ms** — one
frame. The same work on the old code held the main thread for its entire duration. Storing took 8 ms, the fill-in for
four CJK/Hangul/Ethiopic/Myanmar code points took 1 ms, and `system-400-12.bin` landed at 41 KB against the ~426 KB the
old full sweep produced.

## Decisions

**Decision**: an explicit `initTextSize({ measuresFontMetrics })` opt-in rather than a window-label check. **Why**: a
label check (`getCurrentWindow().label === 'main'`) is one line but invisible from the call site, and it silently
decides behaviour for any window added later. The flag defaults to `false`, so a new window opts out until someone
deliberately opts it in, and the reason sits next to the call.

**Decision**: the eager set covers Latin, punctuation, symbols, box drawing, and common emoji; not CJK or Hangul.
**Why**: CJK Unified Ideographs alone was 20,992 of the old ~54,500 code points, and Hangul Syllables another 11,184.
Together with Ethiopic, Myanmar, Yi and the Indic blocks they were most of the cost and almost none of the use. The
fill-in makes leaving them out lossless: a CJK filename is exact one round-trip after it first appears, and the widths
persist to disk, so it happens once per font size. Combining Diacritical Marks stay in the eager set despite looking
exotic — macOS stores filenames NFD, so they're in ordinary accented names.

**Decision**: parallel `codePoints` / `widths` arrays over a `Record<codePoint, width>` on the wire. **Why**: the object
form spends a quoted numeric key per entry. Same data, a fraction of the JSON, and it maps straight onto the
`Uint32Array` / `Float32Array` the worker already produces.

**Decision**: report missing code points on the width response instead of emitting an event. **Why**: the caller that
needs to act on them is exactly the caller that asked, and it's already awaiting a response. An event would need an
`AppHandle` on the command, a second listener, and its own correlation back to the listing that triggered it.

**Decision**: `store_font_metrics` and `extend_font_metrics` are `async` + timeout-wrapped. **Why**: they serialize
thousands of width pairs and write a file. As a sync `#[tauri::command]` that ran on the IPC handler thread, against the
project rule in `src-tauri/CLAUDE.md`.
