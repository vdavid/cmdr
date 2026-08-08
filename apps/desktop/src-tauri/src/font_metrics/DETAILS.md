# Font metrics: details

Depth and decision rationale. `CLAUDE.md` holds the must-knows.

## Disk format

bincode2 binary at `~/Library/Application Support/…/font-metrics/{font_id}.bin`, one file per font size. Only Rust
reads it (never human-edited). A file starts at the frontend's eager set (~5,000 code points) and grows as the fill-in
merges in what filenames actually need, so its size tracks the scripts a user's files are named in.

## Decisions

**Decision**: the frontend measures character widths via the Canvas API and ships them to Rust over IPC, rather than
Rust measuring fonts directly.
**Why**: Rust has no access to the system's font rendering stack. The browser's Canvas API uses the exact font
rasterizer the user sees, so measurements match pixel-perfectly. Any Rust-side font library would need to load font
files, resolve system fonts, and might produce slightly different widths than the browser actually renders.

**Decision**: binary format (bincode2, a maintained fork of bincode) on disk instead of JSON.
**Why**: a full Latin character set produces ~4,000 code-point-to-width entries, ~100 KB as JSON with key-quoting
overhead. bincode compresses this to ~26 KB and deserializes in microseconds vs. milliseconds for JSON parsing. Read
only by Rust, so readability doesn't matter.

**Decision**: `RwLock` for the metrics cache instead of `Mutex`.
**Why**: `calculate_max_width_with_suffixes` runs on every Brief-mode render for every visible column, and multiple Tauri command
threads may read metrics concurrently. `RwLock` allows unlimited parallel reads; a `Mutex` would serialize all column
width calculations and add latency to listing renders.

**Decision**: an unmeasured code point is costed at `average_width` AND reported, rather than erroring, returning zero,
or blocking the query until it can be measured.
**Why**: zero would collapse unknown characters to invisible width and break column alignment, and blocking would put
the frontend's measuring latency inside a synchronous layout query. Costing at the average lets the columns paint at
once; reporting is what stops the estimate becoming permanent. The frontend measures those code points and calls
`extend_and_persist`, and the re-query is exact. The full contract, including the frontend's dedup and its
self-healing on failure, is in `../../../src/lib/font-metrics/DETAILS.md` § On-demand fill-in.

**Decision**: `calculate_max_width_with_suffixes` takes a `&mut BTreeSet<u32>` out-param instead of returning the
missing set or recording it in a global.
**Why**: one listing spans many columns and the caller wants one report for all of them, so an accumulator avoids a
`Vec` allocation per column. A global would make `brief_columns` impure, which is the property that keeps it testable
without a Tauri app handle. `BTreeSet` rather than `HashSet` so the report is ascending and deduplicated for free,
which the frontend's dedup and the tests both rely on.
