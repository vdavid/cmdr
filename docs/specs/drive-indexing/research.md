# Drive indexing research

Research notes for full-drive background indexing in Cmdr.

Date: 2026-02-18. Updated: 2026-02-21.

## Goal

Index the entire local drive at app startup (background, non-blocking) and keep it updated via file watching.
This enables instant folder size display, fast file search, and other features no other Mac file manager offers.

## Feasibility summary

| Question               | Answer                                                                                        |
|------------------------|-----------------------------------------------------------------------------------------------|
| Scan time (2-5M files) | 1-3 min with metadata on SSD. `jwalk` is ~3-4x faster than current `walkdir`.                 |
| FSEvents reliability   | ~99%. Advisory, not guaranteed. Coalescing, `MustScanSubDirs`, no network drive support.      |
| Memory cost            | 450-750 MB in-memory for 3M files. Use SQLite instead: 64-128 MB RAM + 600 MB-1.5 GB on disk. |
| CPU cost               | Initial scan: one core for 1-3 min. Ongoing watch: ~0.2% CPU.                                 |
| Disk/battery impact    | Negligible on SSD. One-time burst at startup.                                                 |
| Concurrent scan+watch  | Well-established pattern: watch first, scan second, reconcile via event IDs.                  |

## FSEvents: what to know

- Not 100% reliable. Apple calls it "advisory" because external disk modifications (another computer, older macOS) can
  bypass the journal. In practice, ~95-98% per-file reliability, ~99%+ per-directory on local APFS under normal load.
- Coalesces rapid create+delete into single ambiguous events. Must re-stat to know actual state.
- `MustScanSubDirs` flag fires when kernel event buffer overflows (4,096 event queue, triggers at >75% capacity).
  Requires subtree rescan. Higher rate on Apple Silicon than Intel.
- Events fire on file `close()`, not on each `write()`. A file kept open and written to continuously won't generate
  events until closed.
- `chmod` (permission-only changes) may not generate events (reported by Syncthing, macOS-version-dependent).
- Does NOT work on SMB/NFS/AFP volumes. Polling-only fallback needed for network drives.
- Maintains on-disk journal in `.fseventsd/` per volume, typically covering days to weeks. Supports `sinceWhen` parameter
  to replay events from a stored event ID, avoiding full rescans on app restart.
- Practical implication: periodic full rescans are NOT needed. Instead: sinceWhen replay on startup/wake,
  MustScanSubDirs-triggered subtree rescans, and per-navigation filesystem verification cover all cases. Full rescan
  only when the journal is unavailable.

### How production apps handle FSEvents

- **VSCode**: Custom chokidar fork, MustScanSubDirs triggers rescan, event IDs for scan+watch reconciliation.
- **Spotlight/Time Machine**: FSEvents primary, fall back to deep timestamp scan when journal unavailable.
- **Syncthing/Dropbox**: FSEvents + periodic polling/reconciliation as safety net.
- **Watchexec**: Abandoned FSEvents entirely in favor of kqueue (different tradeoffs, not applicable to our scale).

## Scan performance

Benchmarks for ~1M files on macOS SSD:

| Method                               | Time                        |
|--------------------------------------|-----------------------------|
| `readdir` only (no stat)             | ~17s                        |
| `getattrlistbulk` (batched metadata) | ~36s                        |
| `enumeratorAtURL` (empty keys)       | ~5s (357K files)            |
| `jwalk` parallel walk (Rust, rayon)  | ~2-4x faster than `walkdir` |

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
- Bounded RAM: 64–128 MB page cache, rest on disk
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

- **FSEvents**: zero support on any network filesystem. No `.fseventsd` journal exists on remote volumes.
- **SMB**: has protocol-native change notifications (`SMB2 CHANGE_NOTIFY`). On macOS, the `smbfs` kernel extension
  translates these into `vnode_notify()` calls, so **kqueue works on SMB mounts**. This is why Finder and Cmdr already
  update instantly on SMB shares. Limitations: up to 15-second kernel thread latency, credit-based watch limits,
  `STATUS_NOTIFY_ENUM_DIR` overflow (requires re-enumeration), server quality varies.
- **NFS/AFP**: no equivalent of CHANGE_NOTIFY. Polling-only fallback needed.
- Indexing network drives should be opt-in ("Index this network drive"), not automatic.
- Separate index per volume, with its own staleness tracking.

## Cold start behavior

On quit and restart:

- Load previous index from SQLite immediately (fast: mmap) --- listings served from DB instantly
- Read `last_event_id` from meta table, start FSEvents watcher with `sinceWhen = last_event_id`
- FSEvents replays its on-disk journal since that event ID --- applies changes incrementally, cost: seconds
- If journal unavailable (app not opened in weeks, `.fseventsd/` cleaned): fall back to full background scan
- No "stale" indicator needed in the common case --- sinceWhen replay catches up within seconds

## Recommended tech stack

| Component       | Choice                                                | Crate                                             |
|-----------------|-------------------------------------------------------|---------------------------------------------------|
| Initial scan    | Parallel directory walk                               | `jwalk = "0.8"` (replaces `walkdir` for indexing) |
| File watching   | Raw FSEvents with event IDs, sinceWhen, file-level    | [`cmdr-fsevent-stream`](https://github.com/vdavid/cmdr-fsevent-stream) v0.3.0 (our fork, MIT) |
| Persistent index | SQLite, WAL mode, path index                         | `rusqlite` with `bundled` feature                 |
| Freshness       | sinceWhen replay + MustScanSubDirs rescan + per-nav verify | No periodic full rescans                     |
| File search     | Hybrid: SQLite for sizes, `mdfind` for content search | `std::process::Command`                           |
| Network drives  | Polling, opt-in                                       | Custom polling loop                               |
| Parallelism     | rayon (already in use)                                | `rayon = "1.11"`                                  |

Note: `notify` stays for existing per-directory watchers in `file_system/watcher.rs` (different use case).

## Open questions (resolved)

These were resolved during planning (2026-02-21). See [plan.md](plan.md) for details.

- ~~Exact SQLite schema design~~ --- path as primary key, `WITHOUT ROWID`, index on `parent_path` for DB-first listings
- ~~How to handle APFS firmlinks~~ --- scan from `/`, skip `/System/Volumes/Data/` entirely, firmlinks cover it
- ~~Whether to expose "indexing" as a user-visible feature or make it invisible~~ --- visible but subtle: animated
  spinner + "Scanning..." in size column during scan, ⚠️ on stale sizes, top-right overlay during full scan, setting
  to enable/disable under "Settings > General > Drive indexing"
- ~~Disk space budget: warn users? Let them configure max index size?~~ --- show current index size in Settings next to
  the enable/disable toggle. No max size limit; user can disable indexing if space is a concern.
- ~~Index granularity~~ --- every file (enables DB-first listings, not only size aggregation)
- ~~File watching library~~ --- `fsevent-stream` (event IDs, sinceWhen, MustScanSubDirs, async)
- ~~Periodic rescans~~ --- not needed; sinceWhen replay + per-navigation verification + MustScanSubDirs handling
- ~~Scanning/watching abstraction~~ --- separate `VolumeScanner` and `VolumeWatcher` traits

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
- [cmdr-fsevent-stream](https://github.com/vdavid/cmdr-fsevent-stream) --- our fork of fsevent-stream; bumped deps, fixed for Sequoia
- [XNU kernel source - vfs_fsevents.c](https://github.com/apple/darwin-xnu/blob/main/bsd/vfs/vfs_fsevents.c)
- [Watchexec FSEvents limitations](https://watchexec.github.io/docs/macos-fsevents.html)
