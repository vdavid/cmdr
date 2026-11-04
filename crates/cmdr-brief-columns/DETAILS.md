# cmdr-brief-columns details

Depth and decision rationale. Must-knows: `CLAUDE.md`. The frontend half (eager set, the fill-in loop, dedup):
`apps/desktop/src/lib/font-metrics/DETAILS.md` § On-demand fill-in. The desktop's disk cache:
`apps/desktop/src-tauri/src/font_metrics/DETAILS.md`.

## The flow

1. The frontend measures an eager code-point set for the current font ID and sends it (`store_font_metrics`, two
   parallel arrays zipped by `widths_from_parallel`); the host inserts a `FontMetrics` into its store.
2. Brief mode asks `get_brief_column_text_widths` (`BriefColumnsQuery`). `compute_brief_column_text_widths` reads the
   pane's visible rows through `Listings::with_rows`, lays them out column-major (`..` in cell (0, 0) when
   `has_parent`), and sizes each column to its widest `(name, tag-dot suffix)` pair.
3. Code points with no width are costed at `average_width` and returned in `missing_code_points` (ascending, deduped).
   The frontend measures them, calls `extend_font_metrics` (`FontMetricsStore::extend_with`), and re-queries.

## Decisions

**Decision**: the frontend measures character widths via the Canvas API and ships them over IPC, rather than Rust
measuring fonts directly. **Why**: Rust has no access to the system's font rendering stack. The Canvas API uses the
exact rasterizer the user sees, so measurements match pixel-perfectly. A Rust font library would need to load font
files, resolve system fonts, and might produce slightly different widths than the webview renders. In the browser demo
it's the same code path: the page measures with its own fonts and sends them through `store_font_metrics`.

**Decision**: Brief column widths are computed in Rust from a per-code-point table, not by measuring each filename in
the frontend. **Why**: measuring 50k filenames in JS is slow. The frontend measures each code point once; later width
queries are hash lookups, over rows the engine already holds, in one IPC call per layout change.

**Decision**: `FontMetricsStore` is a value each host owns, not a process global in this crate. **Why**: the desktop
keeps one in a `LazyLock` (`font_metrics::store()`), the demo one per `Demo`, and each test its own or a shared one
under unique font IDs. A global here would make the crate's tests share state and tie the demo's lifetime to the
module's. Persistence stays in the app: `FontMetrics` derives `Serialize`, and `extend_with` runs the app's serializer
under the write lock so the merged map (thousands of pairs) is never cloned out.

**Decision**: `RwLock` for the store instead of `Mutex`. **Why**: the width queries run on every Brief-mode layout for
every column, and several Tauri command threads may read at once. Writes are the rare measurement and fill-in.

**Decision**: an unmeasured code point is costed at `average_width` AND reported, rather than erroring, returning zero,
or blocking the query until it can be measured. **Why**: zero would collapse unknown characters to invisible width and
break column alignment, and blocking would put the frontend's measuring latency inside a synchronous layout query.
Costing at the average lets the columns paint at once; reporting is what stops the estimate becoming permanent.

**Decision**: the width math takes a `&mut BTreeSet<u32>` accumulator instead of returning a missing set per call.
**Why**: one listing spans many columns and the caller wants one report for all of them, so an accumulator avoids a
`Vec` per column. `BTreeSet` so the report is ascending and deduplicated for free, which the frontend's dedup and the
tests rely on.

**Decision**: an unmeasured size borrows the nearest stored size of the same family and weight (by ratio, so 12 → 24 is
as far as 6 → 12), scaled, and says `approximate: true`, reporting no missing code points. **Why**: after a text-size
change the exact size takes seconds to measure; text width tracks font size to within ~1%, so a neighbor stands in. A
fill-in can only extend a stored size, so an approximate answer asks for none. Only when no size of the family is stored
does it answer `FontMetricsNotReady`.

## Cost in the browser build

Adding the crate to `cmdr-listing-wasm` grew `cmdr_listing_wasm_bg.wasm` from 2,094,733 to 2,149,943 bytes (+54 KiB raw,
+12 KiB gzipped: 748,025 → 760,775) (verified on rustc 1.99.1, `wasm-release`, `pnpm build:listing-wasm`, 2026-11-04).
Most of it is likely `f32` parsing (`split_font_id`) and `ln` from libm, plus the `HashMap<u32, f32>` code; not
profiled.

## The public surface is capped

`index-crate-isolation` caps it at what the hosts use, item by item:

- `FontMetrics` (`new`): what the app builds, serializes, and reads back from disk.
- `FontMetricsStore` (`new`, `insert`, `extend_with`, `contains`): the handle. The width math on it is `pub(crate)`:
  only `compute_brief_column_text_widths` costs text.
- `widths_from_parallel` and `FontMetricsError`: the write commands' zip and error, identical on both hosts.
- `BriefColumnsQuery`, `compute_brief_column_text_widths`, `BriefColumnWidths`: the query, its args deserialize straight
  from the command's camelCase JSON in the demo.
- `BriefColumnsError` (what the computation returns), `BriefColumnsErrorKind` and `BriefColumnsIpcError` (the wire). The
  desktop adds `Timeout` and its E2E failure hook in the command; the demo has neither.
