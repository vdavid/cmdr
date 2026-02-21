# Drive indexing plan

Implementation plan for full-drive background indexing with directory size display.
See also: [research.md](research.md), [benchmarks.md](benchmarks.md)

Date: 2026-02-18. Updated: 2026-02-21.

## Overview

Index every file on local volumes at app startup (background, non-blocking). Track recursive size, file count, and
directory count per folder. Display directory sizes in file listings. Keep the index updated via FSEvents watching.

Once the index is populated, use it to **enrich directory listings with recursive sizes** — the key UX differentiator.
Listings continue to come from `readdir` + `stat` (fast enough at 2-50ms). DB-first listings (sub-millisecond) are a
future optimization once the index is proven reliable (see "Future" milestone).

Symlink strategy: no follow. Symlinks are stored but not traversed. This prevents double-counting and infinite loops.

APFS firmlinks: scan from `/` only; skip `/System/Volumes/Data` entirely. The firmlinks at `/` (for example, `/Users`
pointing to `/System/Volumes/Data/Users`) ensure full coverage without double-counting. This matches what
`getattrlistbulk` does naturally in the benchmarks.

Firmlink path normalization: macOS has 18 fixed firmlinks defined in `/usr/share/firmlinks` (users cannot create custom
firmlinks). When the user navigates to a path under `/System/Volumes/Data/` that maps to a firmlinked path (for example,
`/System/Volumes/Data/Users/foo/` → `/Users/foo/`), normalize the path before any DB lookup. Implementation: parse
`/usr/share/firmlinks` at startup, build a prefix-replacement map, apply on every DB query and watcher event path. This
ensures the index works transparently regardless of how the user reached a directory. Since scanning from `/` naturally
follows firmlinks, all DB entries are stored under the firmlinked canonical paths (for example, `/Users/foo/`, not
`/System/Volumes/Data/Users/foo/`).

<details>
<summary>Complete firmlink list (macOS, from <code>/usr/share/firmlinks</code>)</summary>

All 18 entries. Format: root path → `/System/Volumes/Data/{relative path}`.

| Root path | Data volume relative path |
|---|---|
| `/Applications` | `Applications` |
| `/Library` | `Library` |
| `/System/Library/Caches` | `System/Library/Caches` |
| `/System/Library/Assets` | `System/Library/Assets` |
| `/System/Library/PreinstalledAssets` | `System/Library/PreinstalledAssets` |
| `/System/Library/AssetsV2` | `System/Library/AssetsV2` |
| `/System/Library/PreinstalledAssetsV2` | `System/Library/PreinstalledAssetsV2` |
| `/System/Library/CoreServices/CoreTypes.bundle/Contents/Library` | `System/Library/CoreServices/CoreTypes.bundle/Contents/Library` |
| `/System/Library/Speech` | `System/Library/Speech` |
| `/Users` | `Users` |
| `/Volumes` | `Volumes` |
| `/cores` | `cores` |
| `/opt` | `opt` |
| `/private` | `private` |
| `/usr/local` | `usr/local` |
| `/usr/libexec/cups` | `usr/libexec/cups` |
| `/usr/share/snmp` | `usr/share/snmp` |
| `/AppleInternal` | `AppleInternal` (Apple-internal machines only) |

Note: `/tmp`, `/var`, `/etc` are traditional **symlinks** to `/private/{tmp,var,etc}`, not firmlinks. But since
`/private` itself is a firmlink, they transitively resolve through the Data volume.

</details>

Size semantics: all file sizes stored in the index are **physical sizes** (`st_blocks * 512` from `stat()`, equivalent
to `DATAALLOCSIZE` from `getattrlistbulk`). Physical size reflects actual disk allocation and is more meaningful than
logical size for disk usage analysis. Note: per-file physical sizes may overcount by ~10-20% due to APFS clone
block-sharing (see benchmarks.md). For the volume usage bar, always use `statfs()` which reports true block-level usage.

## Architecture

### Data flow

```
First launch (no existing index):
  │
  ├─ Start FSEvents watcher on volume root via fsevent-stream (buffer events)
  │
  └─ Start full background scan (jwalk)
      ├─ Emit progress events (entries scanned, dirs found)
      ├─ Write entries to SQLite in batches (1000-5000 per transaction)
      └─ On complete:
          ├─ Replay buffered watcher events (reconcile via event IDs)
          ├─ Compute dir_stats bottom-up
          ├─ Store last processed FSEvents event ID in meta table
          ├─ Emit index-ready event
          └─ Switch watcher to live mode

Subsequent launches (existing index):
  │
  ├─ Load SQLite index immediately → listings served from DB instantly
  │
  ├─ Start FSEvents watcher with sinceWhen = last stored event ID
  │   └─ FSEvents replays all events from its on-disk journal since that ID
  │       └─ Apply events to DB (same as live mode)
  │
  └─ If journal unavailable (too old / truncated):
      └─ Fall back to full background scan (same as first launch)

Ongoing (index populated, watcher running):
  │
  ├─ FSEvents delivers live file change events → incremental DB updates
  │
  ├─ On user navigation to /Users/foo/:
  │   ├─ INSTANT: SELECT from entries WHERE parent_path = '/Users/foo/' → display
  │   └─ BACKGROUND: readdir + stat → diff against DB → update if needed
  │
  └─ On MustScanSubDirs event:
      └─ Queue subtree rescan for affected directory

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

### DB-first directory listings

Once indexed, the DB becomes the primary source for directory listings. This is the key UX differentiator: listings
appear in **sub-millisecond** instead of the 2-50ms a `readdir` + `stat` takes.

**Flow on navigation:**

1. Query `SELECT * FROM entries WHERE parent_path = ?` with index on `parent_path` → display instantly
2. Spawn background task: `readdir` + `stat` the same directory on the real filesystem
3. Diff the filesystem result against DB. If identical (common case), done.
4. If different, update DB and emit a diff to the frontend (listing updates seamlessly).

**What the DB stores vs. what needs lazy loading:**

- **From scan (instant):** path, name, is_directory, is_symlink, size, modified_at
- **Lazy-loaded (background):** owner, group, permissions, icon_id, created_at, added_at, opened_at

This matches the existing `extended_metadata_loaded` pattern — first paint shows name/size/date from DB, extended
metadata fills in asynchronously.

**When the index isn't ready yet** (first launch, scan in progress): fall back to the current `readdir` + `stat` path.
The DB-first path activates per-directory as entries become available.

### Keeping the index fresh (no periodic full rescans)

FSEvents is advisory but reliable enough (~95-98% per-file, ~99%+ per-directory) that periodic full rescans are
unnecessary and wasteful (2 min on SSD, 15-30+ min on HDD, continuous drive noise).

Instead, freshness comes from four mechanisms:

1. **Live FSEvents** via `fsevent-stream` with file-level granularity. Handles the vast majority of changes.
2. **sinceWhen replay** on startup/wake: FSEvents maintains an on-disk journal (`.fseventsd/`) covering days to weeks.
   On app restart or wake from sleep, replay events since the last stored event ID. Cost: seconds, not minutes.
3. **MustScanSubDirs** handling: when the kernel buffer overflows (4,096 event queue, triggers at >75% capacity), FSEvents
   sends this flag. Queue a subtree rescan for the affected directory via `scan_subtree()`. Throttled: max 1 concurrent
   MustScanSubDirs rescan at a time; additional events are queued and deduplicated by path. If a rescan takes >10s, emit
   a progress event so the UI can show status. This prevents a burst of MustScanSubDirs events from saturating I/O.
4. **Per-navigation verification**: every time the user navigates to a directory, the background `readdir` diff catches
   any silent misses for that specific directory. This covers the hot path — users only care about directories they visit.

**What this doesn't catch:** changes in directories the user never visits. These accumulate silently but are corrected
the moment the user navigates there (mechanism 4). For dir_stats accuracy, watcher events propagate deltas up the
ancestor chain, so parent sizes stay correct even if a deeply nested change was technically missed — as long as the
parent received an event (which happens at ~99%+ directory-level reliability).

**When a full rescan is needed:** only if the FSEvents journal doesn't cover the gap since the last stored event ID
(app not opened in weeks, or `.fseventsd/` was cleaned). Detected on startup: if `sinceWhen` replay returns
`kFSEventStreamEventIdSinceNow` or historical events aren't available, fall back to full scan.

### File watching: fsevent-stream

Use the [`fsevent-stream`](https://github.com/photonquantum/fsevent-stream) crate for the indexing watcher. This is a
safe Rust wrapper over the FSEvents C API that exposes what `notify` hides:

- **Event IDs** (`event.id`) — monotonically increasing, used for scan/watch reconciliation and sinceWhen replay
- **`sinceWhen` parameter** — replay events from a stored event ID on cold start (no full rescan needed)
- **`MustScanSubDirs` flag** — typed in `StreamFlags`, triggers immediate subtree rescan
- **File-level events** — via `kFSEventStreamCreateFlagFileEvents`
- **Async (tokio)** — returns a `Stream` of events

The existing `notify` crate stays for the per-directory file watchers in `file_system/watcher.rs` (different use case,
cross-platform, non-recursive). The indexing watcher is a separate, macOS-specific, volume-root recursive watcher.

### Volume abstraction: separate scanner and watcher traits

Scanning and watching are separate capabilities with different lifecycles, error modes, and per-volume-type support:

| Volume type | Can scan? | How?                            | Can watch? | How?                                              |
|-------------|-----------|---------------------------------|------------|----------------------------------------------------|
| Local POSIX | Yes       | jwalk (raw syscalls, parallel)  | Yes        | FSEvents (fsevent-stream)                          |
| SMB         | Yes       | Recursive list_directory (slow) | Yes        | kqueue (macOS smbfs translates SMB2 CHANGE_NOTIFY) |
| NFS/AFP     | Yes       | Recursive list_directory (slow) | No         | Polling only (no kernel change notification)       |
| FTP         | Yes       | Recursive LIST                  | No         | Polling                                            |
| S3          | Yes       | ListObjectsV2 (fast)            | Maybe      | EventBridge/SNS                                    |
| MTP         | Yes       | list_directory (already exists)  | Limited    | MTP event loop                                     |

**SMB watching on macOS**: The `smbfs` kernel extension sends `SMB2 CHANGE_NOTIFY` requests to the server and translates
responses into `vnode_notify()` calls, which kqueue picks up. This is already working in Cmdr's existing `notify`-based
per-directory watcher. Limitations: up to 15-second latency (kernel thread sleep interval), per-volume watch limits
based on SMB2 credits (~128-512), and `STATUS_NOTIFY_ENUM_DIR` when too many changes occur (equivalent of
`MustScanSubDirs` — requires directory re-enumeration). Server quality varies (Windows solid, some NAS/Samba have quirks).

**Filesystem type detection**: `statfs()` returns `f_fstypename` per path (`"apfs"`, `"smbfs"`, `"nfs"`, `"afpfs"`,
`"webdav"`, etc.). Already used in `chunked_copy.rs` for network filesystem detection. The `VolumeWatcher`
implementation can use this to select the right strategy automatically. For NFS/AFP/WebDAV where no native notifications
exist, adaptive background polling is a reasonable future fallback: poll interval = 20x the measured directory load time
(keeping overhead under 5% of network I/O), clamped to a 15-second floor and 5-minute ceiling. Not in scope for initial
milestones — network volume indexing is opt-in.

Two separate traits, accessed via optional methods on `Volume`:

```rust
/// Bulk enumeration for indexing. Each volume type implements its optimal strategy.
pub trait VolumeScanner: Send + Sync {
    fn scan(&self, root: &Path, cancel: CancellationToken, sender: Sender<Vec<ScannedEntry>>)
        -> Result<ScanSummary, VolumeError>;
    fn scan_subtree(&self, path: &Path, cancel: CancellationToken, sender: Sender<Vec<ScannedEntry>>)
        -> Result<ScanSummary, VolumeError>;
}

/// Real-time change notification. Each volume type implements its own mechanism.
pub trait VolumeWatcher: Send + Sync {
    fn watch(&self, root: &Path, cancel: CancellationToken)
        -> Result<Receiver<VolumeChangeEvent>, VolumeError>;
}

pub trait Volume: Send + Sync {
    // ... existing methods ...
    fn scanner(&self) -> Option<&dyn VolumeScanner> { None }
    fn watcher(&self) -> Option<&dyn VolumeWatcher> { None }
}
```

The indexing module checks `volume.scanner()` and `volume.watcher()` to decide what's available. `LocalPosixVolume`
returns `Some` for both. Future volume types return whatever they support.

### Priority scanning (on-demand micro-scans)

The full background scan takes 1-3 minutes. Users shouldn't have to wait. Instead, we run targeted subtree scans
concurrently with the background scan, so sizes appear within seconds for directories the user cares about.

**Priority levels** (highest first):

1. **Space-selected directories** — user explicitly pressed Space on a dir. Even if later unselected, the scan
   continues. These are single-subtree scans (one task per selected dir).
2. **Current directory's children** — when the user navigates to a dir, a single "scan children" task walks the parent
   directory depth-first, computing and writing `dir_stats` for each child directory as it completes. Results trickle in
   one at a time (emitting `index-dir-updated` per batch). One task to spawn, one task to cancel on navigate-away.
   Much simpler than queuing N individual micro-scans for N subdirectories.
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
                          +----------------------------------+
Full scan --WriteBatch--> |                                  |
                          |  Writer thread (owns connection)  |--> SQLite DB
Micro-scans -WriteDirStats|  Processes messages in order      |      (WAL mode)
              (priority)  |  Prioritizes DirStats over Batch  |
                          |                                  |
Watcher ---WriteDeltas--> |                                  |
                          +----------------------------------+

Read connections (any thread):
  DB-first listing     --> SELECT entries    --> SQLite DB (WAL: concurrent reads OK)
  Listing enrichment   --> SELECT dir_stats  -->
  Debug UI status      --> SELECT meta       -->
```

**Write message types** (bounded mpsc channel, capacity ~32 batches for backpressure):

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
    /// Store the last processed FSEvents event ID.
    UpdateLastEventId(u64),
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

CREATE TABLE entries
(
    path         TEXT PRIMARY KEY,
    parent_path  TEXT    NOT NULL,
    name         TEXT    NOT NULL,
    is_directory INTEGER NOT NULL DEFAULT 0,
    is_symlink   INTEGER NOT NULL DEFAULT 0,
    size         INTEGER, -- physical size in bytes, st_blocks * 512 (NULL for dirs)
    modified_at  INTEGER  -- unix timestamp seconds
) WITHOUT ROWID; -- path is the key, skip rowid overhead

CREATE INDEX idx_parent ON entries (parent_path);

-- Pre-computed recursive aggregates per directory
CREATE TABLE dir_stats
(
    path                 TEXT PRIMARY KEY,
    recursive_size       INTEGER NOT NULL DEFAULT 0,
    recursive_file_count INTEGER NOT NULL DEFAULT 0,
    recursive_dir_count  INTEGER NOT NULL DEFAULT 0
) WITHOUT ROWID;

-- Index metadata
CREATE TABLE meta
(
    key   TEXT PRIMARY KEY,
    value TEXT NOT NULL
) WITHOUT ROWID;
-- Keys: 'schema_version' (currently '1'), 'volume_path', 'scan_completed_at', 'scan_duration_ms',
--        'total_entries', 'last_event_id'
-- On startup, if schema_version doesn't match what the code expects, drop and rebuild the DB.
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

**Enrichment at read time, not cache time**: The `recursive_size`/`recursive_file_count`/`recursive_dir_count` fields
are NOT stored in `LISTING_CACHE`. Instead, they are populated on every `get_file_range` call by doing a batch SQLite
read from `dir_stats`. This avoids stale cache entries when micro-scans complete — the next `get_file_range` call
automatically picks up new data. The `index-dir-updated` event tells the frontend "re-fetch your visible range."

```rust
fn enrich_with_index_data(entries: &mut [FileEntry]) {
    let dir_paths: Vec<&str> = entries.iter()
        .filter(|e| e.is_directory && !e.is_symlink)
        .map(|e| e.path.as_str())
        .collect();
    let stats_map = indexing::get_dir_stats_batch(&dir_paths); // single batch query
    for entry in entries.iter_mut().filter(|e| e.is_directory && !e.is_symlink) {
        if let Some(stats) = stats_map.get(entry.path.as_str()) {
            entry.recursive_size = Some(stats.recursive_size);
            entry.recursive_file_count = Some(stats.recursive_file_count);
            entry.recursive_dir_count = Some(stats.recursive_dir_count);
        }
    }
}
```

This runs in `get_file_range`, not in `list_directory_core`. The cost is a batch SQLite read per page — microseconds on
a WAL connection, negligible.

### Frontend display

**Full mode** (FullList.svelte): Size column currently shows `<dir>` for directories. New behavior:

| State                                       | Size column display           | Tooltip                                             |
|---------------------------------------------|-------------------------------|-----------------------------------------------------|
| Indexing disabled / no index                 | `<dir>` (unchanged)           | ---                                                 |
| Scanning, no data yet for this dir           | {small spinner} Scanning...   | "Scanning..."                                       |
| Has stale data, currently rescanning this dir | Formatted size (triads) ⚠️   | "Might be outdated. Currently scanning..."          |
| Size available (fresh)                       | Formatted size (triads)       | "1.23 GB · 4,521 files · 312 folders"               |
| Size = 0 (empty dir)                         | `0` (formatted)               | "0 bytes, 0 files"                                  |

When the user presses Space on a directory in the listing, a micro-scan is triggered for that directory. The spinner
is replaced by the actual size within seconds. Pressing Space on more directories queues them for scanning. The
current directory's child dirs are also auto-prioritized on navigation.

**Brief mode** (BriefList.svelte): No size column exists. New behavior:

- When cursor is on a directory and index data is available, show size in tooltip
- When cursor is on a directory and scan in progress, tooltip shows "{spinner} Scanning..."
- When data is stale, tooltip shows size + "⚠️ Might be outdated. Currently scanning..."
- Pressing Space on a directory triggers a micro-scan (same as Full mode), tooltip updates when done

**SelectionInfo** (both modes, bottom bar): When the selection includes directories, reuse the same spinner/⚠️ logic:

- If any selected directory is being scanned with no data yet, show "{spinner} Scanning..." in the size area
- If any selected directory has stale data, show the summed size with ⚠️ and tooltip "Might be outdated"
- Otherwise, show the total size (files + recursive dir sizes) as normal

**Scan status overlay** (top-right corner of window, both panes): A small, non-intrusive notification:

| State                                 | Display                                              |
|---------------------------------------|------------------------------------------------------|
| Full scan running                     | {~32px animated spinner} "Scanning..."               |
| FSEvents replay on cold start         | {~32px animated spinner} "Catching up, ~12,345 events processed..." |
| Idle / scan complete                  | Hidden                                               |

The overlay is visible whenever a full background scan or a large sinceWhen replay is in progress. For FSEvents replay,
show the number of events processed (event count increments as events are applied). If the event ID gap is known
(current device event ID minus last stored event ID), show as "{processed} / ~{estimated total}" for a rough progress
indicator — the estimate is approximate because not all event IDs in the range belong to our volume.

### Settings

Add "File system sync" subsection under "Settings > General":

- **Toggle**: enable/disable file system sync (default: enabled). When disabled, no scanning or watching runs; directory
  sizes show `<dir>` as before.
- **Index size display**: show current index size (SQLite DB file size on disk), for example, "Index size: 1.2 GB".
  Updated live when visible.
- **Clear index** action: drops the SQLite DB and resets state. Next time sync is enabled, starts a fresh full scan.

### Dev mode

- **Env var**: `CMDR_DRIVE_INDEX=1` --- required to start scanning in dev mode. Without it, no scan runs.
- **Debug window**: Add "Drive index" section with:
    - Status display: "Scanning... 423,891 / ? entries" or "Ready: 1,823,456 entries, 287,392 dirs"
    - "Start scan" button (triggers manual scan)
    - "Clear index" button (drops SQLite DB, resets state)
    - Volume selector (if multiple volumes)
    - Last scan timestamp, duration, and last event ID

### Events (Rust -> Frontend)

| Event                 | Payload                                             | When                                                   |
|-----------------------|-----------------------------------------------------|--------------------------------------------------------|
| `index-scan-started`  | `{ volumeId }`                                      | Full background scan begins                            |
| `index-scan-progress` | `{ volumeId, entriesScanned, dirsFound }`           | Every 500ms during scan                                |
| `index-scan-complete` | `{ volumeId, totalEntries, totalDirs, durationMs }` | Scan + aggregation done                                |
| `index-dir-updated`   | `{ paths: string[] }`                               | Micro-scan or watcher updated dir_stats for these dirs |
| `index-replay-progress` | `{ volumeId, eventsProcessed, estimatedTotal? }`  | Every 500ms during sinceWhen replay                  |

### IPC commands (Frontend -> Rust)

| Command               | Args                 | Returns                 | Purpose                                          |
|-----------------------|----------------------|-------------------------|--------------------------------------------------|
| `start_drive_index`   | `volumeId`           | `Result<(), String>`    | Trigger full background scan                     |
| `stop_drive_index`    | `volumeId`           | `()`                    | Cancel running scan                              |
| `get_index_status`    | `volumeId?`          | `IndexStatus`           | Status for debug UI                              |
| `get_dir_stats`       | `path`               | `Option<DirStats>`      | Single dir lookup                                |
| `get_dir_stats_batch` | `paths: Vec<String>` | `Vec<Option<DirStats>>` | Batch lookup for listing                         |
| `prioritize_dir`      | `path, priority`     | `()`                    | Queue on-demand micro-scan (Space or navigation) |
| `cancel_nav_priority` | `path`               | `()`                    | Cancel current-dir micro-scans on navigate-away  |

### Module structure

```
src-tauri/src/indexing/
+-- mod.rs              -- Public API: init(), start_scan(), get_dir_stats(), prioritize_dir(), etc.
+-- scanner.rs          -- jwalk-based parallel directory walker (full scan + subtree scan)
+-- micro_scan.rs       -- MicroScanManager: priority queue, task pool, dedup, cancellation
+-- writer.rs           -- Single writer thread: owns DB write connection, processes WriteMessage channel
+-- store.rs            -- SQLite schema, read queries (get_dir_stats, get_status, list_entries), DB open/migrate
+-- watcher.rs          -- Drive-level FSEvents watcher via fsevent-stream (root "/" recursive)
+-- reconciler.rs       -- Buffer events during scan, replay after scan completes (using event IDs)
+-- aggregator.rs       -- Dir stats computation (bottom-up + incremental propagation), runs on writer thread
+-- firmlinks.rs        -- Parse /usr/share/firmlinks, build prefix map, normalize paths (macOS-specific)
+-- verifier.rs         -- Per-navigation background readdir diff against DB

src-tauri/src/commands/indexing.rs -- Tauri IPC command definitions

src/lib/indexing/
+-- index-state.svelte.ts  -- Svelte 5 reactive state ($state) for index status per volume
+-- index-events.ts        -- Event listeners for index progress/completion/dir-updated
+-- index-priority.ts      -- Calls prioritize_dir on Space/navigate, cancel_nav_priority on leave
```

**Dependency direction**: The rest of the codebase depends on `indexing` (one-way). The `indexing` module never imports
from `file_system::listing` or other modules. The single integration point is `enrich_with_index_data()` called from
`get_file_range` in the listing module.

## Permission handling

Without Full Disk Access, the scanner can't read TCC-protected directories (`~/Desktop`, `~/Documents`,
`~/Downloads`, etc.). Handling:

- During scan: `jwalk` will get `EPERM` for these directories. Log at debug level, skip silently, don't count as errors
  in progress stats.
- If major user-visible directories are missing from the index (for example, `~/Documents` has no entries), surface a
  non-intrusive prompt: "Grant Full Disk Access for a complete index" with a link to System Settings.
- The FSEvents subscription itself doesn't need FDA — only reading file metadata does.
- Never block the scan or show errors for individual permission-denied paths. The index works fine with partial coverage.

## Scan exclusions

Skip these paths by default (configurable later):

- `/System/Volumes/Data/` --- skip entirely; firmlinks from `/` provide coverage without double-counting
- `/System/Volumes/VM/` --- VM swap
- `/System/Volumes/Preboot/` --- boot volume
- `/System/Volumes/Update/` --- OS updates
- `/System/Volumes/xarts/` --- security
- `/System/Volumes/iSCPreboot/` --- security
- `/System/Volumes/Hardware/` --- hardware
- `/System/` (except paths reached via firmlinks) --- immutable system volume
- `/private/var/` --- system temp/cache
- `/Library/Caches/` --- system caches
- `/.Spotlight-V100/` --- Spotlight index
- `/.fseventsd/` --- FSEvents log
- `/dev/`, `/proc/` --- virtual filesystems
- `node_modules/` --- (maybe, configurable)
- `.git/objects/` --- git internals (maybe, configurable)

## Per-volume indexing

One SQLite DB file per indexed volume: `~/Library/Application Support/com.veszelovszki.cmdr/index-{volume_id}.db`.
Volume ID is derived from the mount point (for example, `root` for `/`, `naspi` for `/Volumes/naspi`).

On a typical macOS laptop, only the root volume is indexed automatically:

| Volume | Indexed? | DB file |
|---|---|---|
| `/` (APFS root + Data via firmlinks) | Yes, automatic | `index-root.db` |
| `/System/Volumes/{VM,Preboot,Update,...}` | No | Excluded (system internals) |
| `/Volumes/naspi` (SMB network share) | Future, opt-in | `index-naspi.db` |

On Windows (future): one DB per drive letter (`index-C.db`, `index-D.db`, etc.). The `Volume` trait already abstracts
mount points, so the indexing module doesn't need platform-specific logic — it receives a volume with a root path and
indexes from there.

## Key decisions

Decisions made during planning (2026-02-21):

1. **fsevent-stream over notify**: The indexing watcher uses `fsevent-stream` for direct FSEvents access (event IDs,
   sinceWhen, MustScanSubDirs). The existing per-directory watchers in `file_system/watcher.rs` keep using `notify`.

2. **No periodic full rescans**: Full drive rescans are expensive (2 min SSD, 15-30+ min HDD) and unnecessary. Instead:
   sinceWhen replay on startup/wake, live FSEvents during operation, MustScanSubDirs-triggered subtree rescans, and
   per-navigation filesystem verification. Full rescan only as a fallback when the FSEvents journal is unavailable.

3. **DB-first directory listings**: Once indexed, the SQLite DB is the primary listing source (sub-ms). Background
   `readdir` verification on each navigation catches any drift. Falls back to `readdir` when the index isn't ready.

4. **Separate VolumeScanner and VolumeWatcher traits**: Scanning and watching are independent capabilities with different
   lifecycles and per-volume-type support. Accessed via `volume.scanner()` and `volume.watcher()` optional methods.

5. **APFS firmlinks**: Scan from `/` only, skip `/System/Volumes/Data`. Firmlinks provide full coverage naturally.
   Normalize paths via `/usr/share/firmlinks` prefix map on every DB lookup and watcher event.

6. **Physical sizes**: Store `st_blocks * 512` (physical allocation), not logical size. More meaningful for disk usage.
   May overcount ~10-20% for APFS clones (shared blocks). Volume usage bar always uses `statfs()` for true totals.

7. **Scan cancellation**: Partial data left as-is in DB. `scan_completed_at` not set in meta table, so next startup
   detects an incomplete scan and runs a fresh full scan. No cleanup or rollback needed — the DB is a cache.

8. **Self-contained indexing module**: Narrow public API, one-way dependency (listing depends on indexing, never the
   reverse). Testable in isolation with temp dirs and in-memory SQLite.

## Performance targets

Rough targets for validation, not hard SLAs:

| Operation                                   | Target              | Notes                                      |
|---------------------------------------------|---------------------|--------------------------------------------|
| DB listing query (`parent_path` index)      | <1ms for <10K entries | SQLite WAL read, indexed                 |
| Full scan (5M files, SSD)                   | <3 min              | jwalk parallel, batched writes             |
| Micro-scan (single subtree, <100K files)    | <5s                 | Priority scan, immediate write             |
| Enrichment per `get_file_range` call        | <500µs              | Batch `dir_stats` read, ~50 dirs per page  |
| Watcher event processing (single file change) | <10ms            | Delta propagation up ancestor chain        |
| Cold start with sinceWhen replay            | <10s                | Depends on event gap; full scan if unavailable |
| Index DB size on disk (5M files)            | ~1-2 GB             | SQLite with WAL, physical sizes stored     |

## Milestones

### Milestone 1: Core index infrastructure

- New `indexing` module with SQLite store (`rusqlite` with `bundled` feature), schema versioning (drop + rebuild on
  mismatch)
- `jwalk` dependency for parallel scanning; collect physical sizes (`st_blocks * 512`)
- Scanner: `scan_volume()` for full walk + `scan_subtree()` for targeted subtree (shared core)
- Aggregator: bottom-up dir_stats computation after full scan, and per-subtree after micro-scan
- `MicroScanManager`: priority queue with bounded task pool (2-4 concurrent), deduplication, cancellation
- Firmlink normalization: parse `/usr/share/firmlinks` at startup, normalize paths on all DB queries and watcher events
- Progress events during scan
- IPC commands: `start_drive_index`, `stop_drive_index`, `get_index_status`, `get_dir_stats_batch`,
  `prioritize_dir`, `cancel_nav_priority`
- Scan cancellation via `CancellationToken` (both full scan and micro-scans). Partial data left as-is in DB;
  `scan_completed_at` not set, so next startup runs a fresh full scan.
- Default exclusion paths (including `/System/Volumes/Data/` for firmlink dedup)
- Rust tests for store, aggregator, scanner, firmlink normalization, and micro-scan manager (with temp dirs)

### Milestone 2: Dev mode + debug window

- `CMDR_DRIVE_INDEX=1` env var gating in production auto-start
- Debug window: "Drive index" section with status display, start/clear buttons, last event ID
- Debug window: volume selector (if multiple volumes), last scan timestamp, duration
- Debug window: listen to and display index events in real time
- Having this early provides visibility and debugging tools for all subsequent milestones

### Milestone 3: Frontend display — directory sizes

- Add `recursiveSize`, `recursiveFileCount`, `recursiveDirCount` to FileEntry (Rust + TypeScript)
- Enrich directory entries at read time (`get_file_range`), not cache time — batch `dir_stats` lookup per page
- FullList.svelte: animated spinner + "Scanning..." during scan, formatted size (triads) after
- FullList.svelte: ⚠️ indicator on stale sizes with tooltip "Might be outdated. Currently scanning..."
- FullList.svelte: tooltip with "1.23 GB · 4,521 files · 312 folders" detail
- BriefList.svelte: tooltip on directory hover/cursor with size info, spinner/⚠️ when scanning/stale
- SelectionInfo: reuse same spinner/⚠️ logic when selected directories are scanning or stale
- Scan status overlay: top-right corner, ~32px animated spinner + "Scanning..." during full scan;
  show event count during FSEvents replay on cold start
- **Priority triggers**: on Space for a directory → call `prioritize_dir(path, "user_selected")`;
  on navigation → call `prioritize_dir(parentPath, "current_dir")` for child dirs;
  on navigate-away → call `cancel_nav_priority(oldPath)`
- Reactive state: listen to `index-scan-complete` and `index-dir-updated` events to refresh sizes
  in the current listing (re-fetch affected entries or patch in-place)
- Svelte tests for new display logic

### Milestone 4: FSEvents watcher + reconciliation

- `fsevent-stream` dependency; recursive watcher on volume root with file-level events
- Event buffering during scan, reconciliation after scan completes (using event IDs)
- Incremental aggregate propagation (delta up ancestor chain) on file changes
- `MustScanSubDirs` handling: queue subtree rescan via `scan_subtree()`, max 1 concurrent rescan
  (additional events queued, deduplicated by path). Emit progress event if rescan takes >10s.
- Store last processed event ID in meta table on every write batch
- Update enriched FileEntry data when dir_stats change for visible directories
- Rust tests for reconciler and incremental propagation

### Milestone 5: Persistence + cold start (sinceWhen replay)

- On startup: read `last_event_id` from meta table, start FSEvents with `sinceWhen`
- FSEvents replays journal events since that ID → apply to DB (same as live mode)
- If journal unavailable (sinceWhen too old): fall back to full background scan
- Show existing index data immediately on startup (no "stale" indicator needed if sinceWhen replay works)
- Wake from sleep: FSEvents stream stays alive; kernel buffers events during sleep and delivers on wake.
  If buffer overflows (long sleep, many changes), `MustScanSubDirs` fires automatically (handled by Milestone 4).
- Store scan metadata (timestamp, duration, entry count) in `meta` table
- Handle index from a different macOS version / drive layout (detect schema mismatch and rebuild)

### Milestone 6: Settings + user controls

- Add "File system sync" subsection under "Settings > General"
- Toggle to enable/disable file system sync (default: enabled)
- Display current index size (read from SQLite DB file size on disk)
- "Clear index" action (drops DB, resets state)

### Milestone 7: Volume scanner/watcher traits

- Add `VolumeScanner` and `VolumeWatcher` traits
- Add `scanner()` and `watcher()` optional methods to `Volume` trait
- Implement `VolumeScanner` for `LocalPosixVolume` (wraps jwalk)
- Implement `VolumeWatcher` for `LocalPosixVolume` (wraps fsevent-stream)
- Refactor indexing module to use traits instead of direct jwalk/fsevent-stream calls
- Verify existing volume types still work (MTP, InMemory)

### Milestone 8: Polish + checks

- Run all checks: clippy, rustfmt, eslint, prettier, svelte-check, knip, stylelint
- Run Rust tests and Svelte tests
- Update CLAUDE.md for the indexing module
- Update architecture.md with new module

### Future: DB-first directory listings

Deferred. The current `readdir` + `stat` path is already fast enough (2-50ms), and the index enrichment approach
(Milestone 3) delivers the key UX win (directory sizes) without changing the listing pipeline. Once the index is mature
and proven reliable, it can become the primary listing source for sub-ms response times.

- `verifier.rs`: background `readdir` + diff against DB on each navigation
- Integrate DB-first path into `get_file_range`: if index has entries for this parent_path, serve from DB
- Lazy-load extended metadata (owner, group, permissions, icon_id) in background
- Fall back to `readdir` when index not yet populated for a directory
- Performance validation: benchmark DB listing vs. `readdir` for directories with 100, 1K, 10K, 100K files
