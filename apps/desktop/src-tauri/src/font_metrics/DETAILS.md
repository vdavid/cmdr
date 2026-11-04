# Font metrics: details

Depth and decision rationale. `CLAUDE.md` holds the must-knows. The width math, the fill-in contract, and every other
decision about the store: `crates/cmdr-brief-columns/DETAILS.md`.

## Disk format

bincode2 binary at `~/Library/Application Support/…/font-metrics/{font_id}.bin`, one file per font size. Only Rust
reads it (never human-edited). A file starts at the frontend's eager set (~5,000 code points) and grows as the fill-in
merges in what filenames actually need, so its size tracks the scripts a user's files are named in. The key a file
loads under is its file stem, not the `font_id` inside it.

## Decisions

**Decision**: binary format (bincode2, a maintained fork of bincode) on disk instead of JSON.
**Why**: a full Latin character set produces ~4,000 code-point-to-width entries, ~100 KB as JSON with key-quoting
overhead. bincode compresses this to ~26 KB and deserializes in microseconds vs. milliseconds for JSON parsing. Read
only by Rust, so readability doesn't matter.

**Decision**: persistence stays here, out of `cmdr-brief-columns`.
**Why**: the crate is shared with the browser demo, which has no disk; the app's data dir (`config::resolved_app_data_dir`)
needs an `AppHandle`. The crate exposes `FontMetrics`' serde form and `FontMetricsStore::extend_with` so the app can
serialize under the lock without the crate knowing about files.
