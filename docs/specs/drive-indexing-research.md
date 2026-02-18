# Drive indexing research

Research notes for full-drive background indexing in Cmdr.

Date: 2026-02-18

## Goal

Index the entire local drive at app startup (background, non-blocking) and keep it updated via file watching.
This enables instant folder size display, fast file search, and other features no other Mac file manager offers.

## Feasibility summary

| Question | Answer |
|---|---|
| Scan time (2-5M files) | 1-3 min with metadata on SSD. `jwalk` is ~3-4x faster than current `walkdir`. |
| FSEvents reliability | ~99%. Advisory, not guaranteed. Coalescing, `MustScanSubDirs`, no network drive support. |
| Memory cost | 450-750 MB in-memory for 3M files. Use SQLite instead: 64-128 MB RAM + 600 MB-1.5 GB on disk. |
| CPU cost | Initial scan: one core for 1-3 min. Ongoing watch: ~0.2% CPU. |
| Disk/battery impact | Negligible on SSD. One-time burst at startup. |
| Concurrent scan+watch | Well-established pattern: watch first, scan second, reconcile via event IDs. |

## FSEvents: what to know

- Not 100% reliable. Apple calls it "advisory."
- Coalesces rapid create+delete into single ambiguous events. Must re-stat to know actual state.
- `MustScanSubDirs` flag fires when kernel event buffer overflows (heavy I/O). Requires subtree rescan.
- Events sometimes delayed until file close (~10 seconds).
- Does NOT work on SMB/NFS/AFP volumes. Polling-only fallback needed for network drives.
- `notify` crate wraps FSEvents on macOS. Known issue: drops events at scale (1500+ rapid modifications).
- Practical implication: need periodic full rescans (every 30-60 min or on wake from sleep) to catch misses.

## Scan performance

Benchmarks for ~1M files on macOS SSD:

| Method | Time |
|---|---|
| `readdir` only (no stat) | ~17s |
| `getattrlistbulk` (batched metadata) | ~36s |
| `enumeratorAtURL` (empty keys) | ~5s (357K files) |
| `jwalk` parallel walk (Rust, rayon) | ~2-4x faster than `walkdir` |

For 3M files, expect 30-90 seconds with `jwalk` + metadata.

Alternative: `searchfs()` syscall queries APFS catalog B-tree directly, ~100x faster for name lookup.
But doesn't return sizes. Useful for search, not enumeration.

## Concurrent scan + watch pattern

1. Start FSEvents stream on `/` with latency=0, buffer all events
2. Begin parallel recursive scan with `jwalk`, populating SQLite index
3. After scan completes, replay buffered event queue:
   - Event for path scanned before event timestamp: apply (filesystem changed after scan read it)
   - Event for path scanned after event timestamp: ignore (scan data is newer)
   - Event for path not yet scanned: apply
4. Switch to live event processing

FSEvents provides monotonically increasing `eventId` for ordering. Same pattern VSCode uses.

## Storage recommendation: SQLite with WAL mode

Why SQLite over in-memory or key-value stores:

- Path-prefix queries for folder size: `SELECT SUM(size) FROM files WHERE path >= '/foo/' AND path < '/foo0'`
- Concurrent read/write: watcher updates while UI reads (WAL mode)
- ACID: no corruption on crash
- Bounded RAM: 64-128 MB page cache, rest on disk
- Rich querying: secondary indexes by size, date, extension

Alternatives considered:
- `redb` (pure Rust KV store): no SQL, manual aggregation. Simpler but less powerful.
- `rkyv` (zero-copy mmap): fastest reads, but no ACID, corruption on crash requires rebuild.
  Could be a secondary cache layer on top of SQLite for hot data.
- `sled`: stalled development, not recommended.

## Why other file managers don't do this

- Windows `Everything` reads NTFS Master File Table directly (~1s for 1M files). APFS has no equivalent.
- Spotlight exists but doesn't index hidden files, `.app` bundles, or system dirs. Designed for document search.
- Memory cost (300 MB-2 GB) is steep without a disk-backed store.
- FSEvents being advisory makes the architecture feel impure (need rescans).
- TCC permissions since Mojave complicate full-drive access.

macOS apps that attempt it: Cardinal (Rust+Tauri), Cling (in-memory, 300 MB-2 GB), KatSearch (uses `searchfs`).

## Permissions

- Full Disk Access needed for TCC-protected dirs (`~/Desktop`, `~/Documents`, `~/Downloads`, etc.)
- Without it: index what we can, show "Grant full disk access" prompt for protected directories
- FSEvents subscription itself doesn't need FDA. The permission issue is reading metadata for protected paths.
- Skip `/System`, `/private/var/folders`, and similar noise. Let users configure exclusions.
- Watch for APFS firmlinks to avoid double-counting (`/Users` vs `/System/Volumes/Data/Users`).

## Network drives

- FSEvents: zero support. Polling-only fallback.
- Should be opt-in ("Index this network drive"), not automatic.
- Separate index per volume, with its own staleness tracking.

## Cold start behavior

On quit and restart:
- Load previous index from SQLite immediately (fast: mmap)
- Show folder sizes from stale index with age indicator ("Last indexed 2 hours ago")
- Start background rescan, update index incrementally
- Progress display during rescan

## Recommended tech stack

| Component | Choice | Crate |
|---|---|---|
| Initial scan | Parallel directory walk | `jwalk = "0.8"` (replaces `walkdir` for indexing) |
| File watching | FSEvents via notify (already in use) | `notify = "8"` |
| Persistent index | SQLite, WAL mode, path index | `rusqlite` with `bundled` feature |
| Periodic rescan | Every 30-60 min + on wake from sleep | Built-in timer |
| File search | Hybrid: SQLite for sizes, `mdfind` for content search | `std::process::Command` |
| Network drives | Polling, opt-in | Custom polling loop |
| Parallelism | rayon (already in use) | `rayon = "1.11"` |

## Open questions

- Exact SQLite schema design (path as primary key? separate path components for tree queries?)
- How to handle APFS volume groups and firmlinks cleanly
- Whether to expose "indexing" as a user-visible feature or make it invisible
- Disk space budget: warn users? Let them configure max index size?
- Index granularity: every file, or directories-only for size aggregation?

## Sources

Key references used in this research:
- [FSEvents Programming Guide](https://developer.apple.com/library/archive/documentation/Darwin/Conceptual/FSEvents_ProgGuide/)
- [notify-rs/notify](https://github.com/notify-rs/notify) and issues #412, #267, #240, #465
- [jwalk benchmarks](https://github.com/Byron/jwalk)
- [Thomas Tempelmann - dir read performance](http://blog.tempel.org/2019/04/dir-read-performance.html)
- [searchfs](https://github.com/sveinbjornt/searchfs)
- [VSCode File Watcher Internals](https://github.com/microsoft/vscode/wiki/File-Watcher-Internals)
- [SQLite: 35% Faster Than The Filesystem](https://sqlite.org/fasterthanfs.html)
- [Cling](https://lowtechguys.com/cling/)
- [Everything by Voidtools](https://www.voidtools.com/faq/)
