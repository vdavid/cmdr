# Font metrics: details

Depth and decision rationale. `CLAUDE.md` holds the must-knows.

## Disk format

bincode2 binary (~426 KB for a full Latin character set), at
`~/Library/Application Support/…/font-metrics/{font_id}.bin`. Only Rust reads it (never human-edited).

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
**Why**: `calculate_max_width` runs on every Brief-mode render for every visible column, and multiple Tauri command
threads may read metrics concurrently. `RwLock` allows unlimited parallel reads; a `Mutex` would serialize all column
width calculations and add latency to listing renders.

**Decision**: average-width fallback for unmeasured code points instead of returning an error or zero.
**Why**: returning zero would collapse unknown characters to invisible width and break column alignment. The average
keeps Brief-mode columns roughly sized even for scripts the frontend didn't explicitly measure, at the cost of slight
mis-measurement for CJK / complex-script filenames. Expanding the measured set is a follow-up.
