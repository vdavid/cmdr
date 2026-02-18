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
  └─ Start parallel scan (jwalk)
      ├─ Emit progress events (entries scanned, dirs found)
      ├─ Write entries to SQLite in batches (1000-5000 per transaction)
      └─ On complete:
          ├─ Replay buffered watcher events (reconcile)
          ├─ Compute dir_stats bottom-up
          ├─ Emit index-ready event
          └─ Switch watcher to live mode
```

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
| Index not available for this dir | `<dir>` (unchanged) | — |
| Index scanning (scan in progress) | `⏳` (static hourglass) | "Scanning..." |
| Index ready, size = 0 | `0` (formatted) | "0 bytes, 0 files" |
| Index ready, size > 0 | Formatted size (same triads as files) | "1.23 GB, 4,521 files, 312 folders" |

**Brief mode** (BriefList.svelte): No size column exists. New behavior:
- When cursor is on a directory and index data is available, show tooltip with size info
- When cursor is on a directory and scan in progress, tooltip shows "Scanning..."

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
| `index-scan-started` | `{ volumeId }` | Scan begins |
| `index-scan-progress` | `{ volumeId, entriesScanned, dirsFound }` | Every 500ms during scan |
| `index-scan-complete` | `{ volumeId, totalEntries, totalDirs, durationMs }` | Scan + aggregation done |
| `index-dir-updated` | `{ paths: string[] }` | Watcher updated dir_stats for these dirs |

### IPC commands (Frontend → Rust)

| Command | Args | Returns | Purpose |
|---|---|---|---|
| `start_drive_index` | `volumeId` | `Result<(), String>` | Trigger manual scan |
| `stop_drive_index` | `volumeId` | `()` | Cancel running scan |
| `get_index_status` | `volumeId?` | `IndexStatus` | Status for debug UI |
| `get_dir_stats` | `path` | `Option<DirStats>` | Single dir lookup |
| `get_dir_stats_batch` | `paths: Vec<String>` | `Vec<Option<DirStats>>` | Batch lookup for listing |

### Module structure

```
src-tauri/src/indexing/
├── mod.rs          -- Public API: init(), start_scan(), get_dir_stats(), etc.
├── scanner.rs      -- jwalk-based parallel directory walker
├── store.rs        -- SQLite operations: insert, query, aggregate computation
├── watcher.rs      -- Drive-level FSEvents watcher (root "/" recursive)
├── reconciler.rs   -- Buffer events during scan, replay after scan completes
└── aggregator.rs   -- Dir stats computation (bottom-up + incremental propagation)

src-tauri/src/commands/indexing.rs -- Tauri IPC command definitions

src/lib/indexing/
├── index-state.svelte.ts  -- Svelte 5 reactive state ($state) for index status
└── index-events.ts        -- Event listeners for index progress/completion
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
- Scanner: walk a volume, write entries to SQLite in batches
- Aggregator: bottom-up dir_stats computation after scan
- Progress events during scan
- IPC commands: `start_drive_index`, `stop_drive_index`, `get_index_status`, `get_dir_stats_batch`
- Scan cancellation via `CancellationToken`
- Default exclusion paths
- Rust tests for store, aggregator, and scanner (with temp dirs)

### Milestone 2: Frontend display — directory sizes

- Add `recursiveSize`, `recursiveFileCount`, `recursiveDirCount` to FileEntry (Rust + TypeScript)
- Enrich directory entries with index data during listing
- FullList.svelte: show hourglass during scan, formatted size after
- FullList.svelte: tooltip with "X files, Y folders" detail
- BriefList.svelte: tooltip on directory hover with size info
- Reactive state: listen to `index-scan-complete` and `index-dir-updated` events to refresh
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
