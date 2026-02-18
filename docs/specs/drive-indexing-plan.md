# Drive indexing plan

Implementation plan for full-drive background indexing with directory size display.
See also: [drive-indexing-research.md](drive-indexing-research.md)

Date: 2026-02-18

## Overview

Index every file on local volumes at app startup (background, non-blocking). Track recursive size, file count, and
directory count per folder. Display directory sizes in file listings. Keep the index updated via FSEvents watching.

Symlink strategy: no follow. Symlinks are stored but not traversed. This prevents double-counting and infinite loops.

## Architecture

### Data flow

```
App start
  │
  ├─ Load existing SQLite index (if any)
  │   └─ Mark as stale, show sizes from it immediately
  │
  ├─ Start FSEvents watcher on volume root (buffer events)
  │
  └─ Start full background scan (jwalk)
      ├─ Emit progress events (entries scanned, dirs found)
      ├─ Write entries to SQLite in batches (1000-5000 per transaction)
      └─ On complete:
          ├─ Replay buffered watcher events (reconcile)
          ├─ Compute dir_stats bottom-up
          ├─ Emit index-ready event
          └─ Switch watcher to live mode

Concurrently, on-demand micro-scans run for prioritized directories:

User navigates to /Users/foo/
  └─ Frontend sends prioritize_dir("/Users/foo/")
      └─ Backend spawns micro-scan of /Users/foo/* subtrees
          └─ Stores dir_stats for each child dir as it completes
              └─ Emits index-dir-updated { paths } → frontend refreshes sizes

User presses Space on "Documents"
  └─ Frontend sends prioritize_dir("/Users/foo/Documents")
      └─ Backend spawns micro-scan of that single subtree
          └─ Stores dir_stats, emits update → size appears in seconds
```

### Priority scanning (on-demand micro-scans)

The full background scan takes 1-3 minutes. Users shouldn't have to wait. Instead, we run targeted subtree scans
concurrently with the background scan, so sizes appear within seconds for directories the user cares about.

**Priority levels** (highest first):
1. **Space-selected directories** — user explicitly pressed Space on a dir. Even if later unselected, the scan
   continues. These are single-subtree scans.
2. **Current directory's children** — when the user navigates to a dir, all its subdirectories get micro-scanned.
   Auto-triggered, auto-cancelled when navigating away.
3. **Full background scan** — everything else, lowest priority.

**Implementation**: A `MicroScanManager` with a bounded task pool (for example, 2-4 concurrent micro-scans):

```rust
struct MicroScanManager {
    /// Active micro-scan tasks, keyed by path
    active: HashMap<PathBuf, JoinHandle<()>>,
    /// Pending requests, ordered by priority
    queue: VecDeque<(ScanPriority, PathBuf)>,
    /// Paths that have completed micro-scans (skip if full scan hasn't overwritten yet)
    completed: HashSet<PathBuf>,
    /// Cancellation tokens for current-directory scans (cancelled on navigate-away)
    nav_tokens: HashMap<PathBuf, CancellationToken>,
}

enum ScanPriority {
    UserSelected,    // Space key — never auto-cancelled
    CurrentDir,      // Navigation — cancelled when leaving
}
```

**Conflict with full scan**: Micro-scan results are written to the same `dir_stats` table. When the full scan
completes and computes bottom-up aggregates, it overwrites everything with authoritative data. This is fine — by that
point, all sizes are accurate anyway.

**Deduplication**: If a micro-scan is already running or completed for a path, skip it. If the full scan has already
computed stats for a path (post-completion), skip micro-scans for it.

### Single-writer architecture

All SQLite writes go through a dedicated writer thread. This eliminates contention between the full scan, micro-scans,
and watcher updates. Reads happen on separate connections (WAL mode allows concurrent reads).

```
                          ┌─────────────────────────────────┐
Full scan ──WriteBatch──► │                                 │
                          │  Writer thread (owns connection) │──► SQLite DB
Micro-scans ─WriteDirStats│  Processes messages in order     │      (WAL mode)
              (priority)  │  Prioritizes DirStats over Batch │
                          │                                 │
Watcher ───WriteDeltas──► │                                 │
                          └─────────────────────────────────┘

Read connections (any thread):
  Listing enrichment ──► SELECT dir_stats ──► SQLite DB (WAL: concurrent reads OK)
  Debug UI status    ──► SELECT meta      ──►
```

**Write message types** (mpsc channel):

```rust
enum WriteMessage {
    /// Full scan: batch of entries. Lowest priority.
    InsertEntries(Vec<ScannedEntry>),
    /// Micro-scan or watcher: dir_stats updates. Highest priority — processed before pending batches.
    UpdateDirStats(Vec<DirStatsUpdate>),
    /// Full scan complete: trigger bottom-up aggregation.
    ComputeAllAggregates,
    /// Watcher: incremental delta propagation for a single file change.
    PropagateDelta { path: PathBuf, size_delta: i64, file_count_delta: i32, dir_count_delta: i32 },
    /// Shutdown.
    Shutdown,
}
```

**Priority handling**: The writer thread checks for `UpdateDirStats` messages first (via `try_recv` loop draining
those), then processes one `InsertEntries` batch, then checks again. This ensures micro-scan results are written
promptly even while the full scan is pushing large batches.

### SQLite schema

```sql
-- One DB file per volume: ~/Library/Application Support/com.veszelovszki.cmdr/index-{volume_id}.db

PRAGMA journal_mode = WAL;
PRAGMA synchronous = NORMAL;
PRAGMA cache_size = -65536;  -- 64 MB page cache

CREATE TABLE entries (
    path TEXT PRIMARY KEY,
    parent_path TEXT NOT NULL,
    name TEXT NOT NULL,
    is_directory INTEGER NOT NULL DEFAULT 0,
    is_symlink INTEGER NOT NULL DEFAULT 0,
    size INTEGER,              -- file size in bytes (NULL for dirs)
    modified_at INTEGER        -- unix timestamp seconds
) WITHOUT ROWID;               -- path is the key, skip rowid overhead

CREATE INDEX idx_parent ON entries(parent_path);

-- Pre-computed recursive aggregates per directory
CREATE TABLE dir_stats (
    path TEXT PRIMARY KEY,
    recursive_size INTEGER NOT NULL DEFAULT 0,
    recursive_file_count INTEGER NOT NULL DEFAULT 0,
    recursive_dir_count INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;

-- Index metadata
CREATE TABLE meta (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;
-- Keys: 'volume_path', 'scan_completed_at', 'scan_duration_ms', 'total_entries'
```

### Dir stats computation

**After scan** (bottom-up): Sort all directory paths by depth (deepest first). For each dir, sum its children:
- `recursive_size` = sum of child file sizes + sum of child dir `recursive_size`
- `recursive_file_count` = count of child files + sum of child dir `recursive_file_count`
- `recursive_dir_count` = count of child dirs + sum of child dir `recursive_dir_count`

This is a single-pass O(N) traversal with N = number of directories.

**On watcher event** (incremental): When a file changes, propagate delta up:
- File added (size X): add X bytes and 1 file to all ancestor dir_stats
- File removed (size X): subtract X bytes and 1 file from all ancestor dir_stats
- File modified (size delta D): add D bytes to all ancestor dir_stats
- Dir added: add 1 dir to all ancestor dir_stats
- Dir removed: subtract its full recursive stats from all ancestors

Ancestor propagation is O(depth), typically <20 levels.

### Integration with FileEntry

Add three optional fields to `FileEntry` (Rust + TypeScript):

```rust
// In metadata.rs
pub struct FileEntry {
    // ... existing fields ...
    /// Recursive size in bytes (from drive index, None if not indexed)
    pub recursive_size: Option<u64>,
    /// Recursive file count (from drive index, None if not indexed)
    pub recursive_file_count: Option<u64>,
    /// Recursive dir count (from drive index, None if not indexed)
    pub recursive_dir_count: Option<u64>,
}
```

**Enrichment**: After `list_directory_core()` returns entries and before caching, enrich directory entries:

```rust
fn enrich_with_index_data(entries: &mut [FileEntry]) {
    for entry in entries.iter_mut().filter(|e| e.is_directory && !e.is_symlink) {
        if let Some(stats) = indexing::get_dir_stats(&entry.path) {
            entry.recursive_size = Some(stats.recursive_size);
            entry.recursive_file_count = Some(stats.recursive_file_count);
            entry.recursive_dir_count = Some(stats.recursive_dir_count);
        }
    }
}
```

### Frontend display

**Full mode** (FullList.svelte): Size column currently shows `<dir>` for directories. New behavior:

| State | Display | Tooltip |
|---|---|---|
| Indexing disabled / no index | `<dir>` (unchanged) | — |
| Full scan running, no micro-scan yet | `⏳` (static hourglass) | "Scanning..." |
| Micro-scan completed for this dir | Formatted size (triads) | "1.23 GB, 4,521 files, 312 folders" |
| Full scan complete | Formatted size (triads) | "1.23 GB, 4,521 files, 312 folders" |
| Size = 0 (empty dir) | `0` (formatted) | "0 bytes, 0 files" |

When the user presses Space on a directory in the listing, a micro-scan is triggered for that directory. The hourglass
is replaced by the actual size within seconds. Pressing Space on more directories queues them for scanning. The
current directory's child dirs are also auto-prioritized on navigation.

**Brief mode** (BriefList.svelte): No size column exists. New behavior:
- When cursor is on a directory and index data is available, show tooltip with size info
- When cursor is on a directory and scan in progress, tooltip shows "Scanning..."
- Pressing Space on a directory triggers a micro-scan (same as Full mode), tooltip updates when done

### Dev mode

- **Env var**: `CMDR_DRIVE_INDEX=1` — required to start scanning in dev mode. Without it, no scan runs.
- **Debug window**: Add "Drive index" section with:
  - Status display: "Scanning... 423,891 / ? entries" or "Ready: 1,823,456 entries, 287,392 dirs"
  - "Start scan" button (triggers manual scan)
  - "Clear index" button (drops SQLite DB, resets state)
  - Volume selector (if multiple volumes)
  - Last scan timestamp and duration

### Events (Rust → Frontend)

| Event | Payload | When |
|---|---|---|
| `index-scan-started` | `{ volumeId }` | Full background scan begins |
| `index-scan-progress` | `{ volumeId, entriesScanned, dirsFound }` | Every 500ms during scan |
| `index-scan-complete` | `{ volumeId, totalEntries, totalDirs, durationMs }` | Scan + aggregation done |
| `index-dir-updated` | `{ paths: string[] }` | Micro-scan or watcher updated dir_stats for these dirs |

### IPC commands (Frontend → Rust)

| Command | Args | Returns | Purpose |
|---|---|---|---|
| `start_drive_index` | `volumeId` | `Result<(), String>` | Trigger full background scan |
| `stop_drive_index` | `volumeId` | `()` | Cancel running scan |
| `get_index_status` | `volumeId?` | `IndexStatus` | Status for debug UI |
| `get_dir_stats` | `path` | `Option<DirStats>` | Single dir lookup |
| `get_dir_stats_batch` | `paths: Vec<String>` | `Vec<Option<DirStats>>` | Batch lookup for listing |
| `prioritize_dir` | `path, priority` | `()` | Queue on-demand micro-scan (Space or navigation) |
| `cancel_nav_priority` | `path` | `()` | Cancel current-dir micro-scans on navigate-away |

### Module structure

```
src-tauri/src/indexing/
├── mod.rs              -- Public API: init(), start_scan(), get_dir_stats(), prioritize_dir(), etc.
├── scanner.rs          -- jwalk-based parallel directory walker (full scan + subtree scan)
├── micro_scan.rs       -- MicroScanManager: priority queue, task pool, dedup, cancellation
├── writer.rs           -- Single writer thread: owns DB write connection, processes WriteMessage channel
├── store.rs            -- SQLite schema, read queries (get_dir_stats, get_status), DB open/migrate
├── watcher.rs          -- Drive-level FSEvents watcher (root "/" recursive)
├── reconciler.rs       -- Buffer events during scan, replay after scan completes
└── aggregator.rs       -- Dir stats computation (bottom-up + incremental propagation), runs on writer thread

src-tauri/src/commands/indexing.rs -- Tauri IPC command definitions

src/lib/indexing/
├── index-state.svelte.ts  -- Svelte 5 reactive state ($state) for index status per volume
├── index-events.ts        -- Event listeners for index progress/completion/dir-updated
└── index-priority.ts      -- Calls prioritize_dir on Space/navigate, cancel_nav_priority on leave
```

## Scan exclusions

Skip these paths by default (configurable later):
- `/System/` — immutable system volume
- `/private/var/` — system temp/cache
- `/Library/Caches/` — system caches
- `/.Spotlight-V100/` — Spotlight index
- `/.fseventsd/` — FSEvents log
- `/dev/`, `/proc/` — virtual filesystems
- `node_modules/` — (maybe, configurable)
- `.git/objects/` — git internals (maybe, configurable)

## Milestones

### Milestone 1: Core index infrastructure

- New `indexing` module with SQLite store (`rusqlite` with `bundled` feature)
- `jwalk` dependency for parallel scanning
- Scanner: `scan_volume()` for full walk + `scan_subtree()` for targeted subtree (shared core)
- Aggregator: bottom-up dir_stats computation after full scan, and per-subtree after micro-scan
- `MicroScanManager`: priority queue with bounded task pool (2-4 concurrent), deduplication, cancellation
- Progress events during scan
- IPC commands: `start_drive_index`, `stop_drive_index`, `get_index_status`, `get_dir_stats_batch`,
  `prioritize_dir`, `cancel_nav_priority`
- Scan cancellation via `CancellationToken` (both full scan and micro-scans)
- Default exclusion paths
- Rust tests for store, aggregator, scanner, and micro-scan manager (with temp dirs)

### Milestone 2: Frontend display — directory sizes

- Add `recursiveSize`, `recursiveFileCount`, `recursiveDirCount` to FileEntry (Rust + TypeScript)
- Enrich directory entries with index data during listing
- FullList.svelte: show hourglass during scan, formatted size after
- FullList.svelte: tooltip with "X files, Y folders" detail
- BriefList.svelte: tooltip on directory hover with size info
- **Priority triggers**: on Space for a directory → call `prioritize_dir(path, "user_selected")`;
  on navigation → call `prioritize_dir(parentPath, "current_dir")` for child dirs;
  on navigate-away → call `cancel_nav_priority(oldPath)`
- Reactive state: listen to `index-scan-complete` and `index-dir-updated` events to refresh sizes
  in the current listing (re-fetch affected entries or patch in-place)
- Svelte tests for new display logic

### Milestone 3: Dev mode + debug window

- `CMDR_DRIVE_INDEX=1` env var gating in production auto-start
- Debug window: "Drive index" section with status, start/clear buttons
- Debug window: emit/listen to index events

### Milestone 4: Drive-level FSEvents watcher

- Recursive watcher on volume root (separate from per-directory watcher)
- Event buffering during scan
- Reconciliation after scan completes
- Incremental aggregate propagation on file changes
- Update enriched FileEntry data when dir_stats change for visible directories
- Periodic full rescan (every 60 min or on wake from sleep)

### Milestone 5: Persistence + cold start

- Load existing SQLite index on startup, mark as stale
- Show stale sizes immediately with age indicator
- Background rescan to refresh
- Store scan metadata (timestamp, duration, entry count) in `meta` table
- Handle index from a different macOS version / drive layout (detect and rebuild)

### Milestone 6: Polish + checks

- Run all checks: clippy, rustfmt, eslint, prettier, svelte-check, knip, stylelint
- Run Rust tests and Svelte tests
- Update CLAUDE.md for the indexing module
- Update architecture.md with new module
