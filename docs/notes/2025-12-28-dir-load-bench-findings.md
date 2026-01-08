# Directory loading benchmark findings

Work-in-progress document tracking performance investigation.

## Goal

Match Commander One performance (~3s for 50k files to full load).

---

## Current bottleneck summary (updated)

**ROOT CAUSE: Huge JSON payload over IPC!**

| Files | Rust backend | JSON size   | Total user experience |
| ----- | ------------ | ----------- | --------------------- |
| 5000  | 131ms        | 1.7 MB      | ~2s (after DOM fix)   |
| 20000 | ~500ms       | ~7 MB       | ~3s                   |
| 50000 | **308ms**    | **17.4 MB** | **14s** ❌            |

Commander One: 50k files in **3 seconds**. We're at **14 seconds**.

**Why Commander One is faster:**

- Likely sends minimal data per entry (name + type only)
- Lazy-loads metadata on scroll
- May use binary protocol instead of JSON

**Our overhead for 50k files (detailed):**

```
Rust list_directory:     308ms  ✅ fast
JSON serialize (Rust):    18ms  ✅ fast
IPC TRANSFER (17.4 MB): ~4100ms  ❌ BOTTLENECK!
JSON.parse (JS):          67ms  ✅ fast
Svelte reactivity:      ~9.5s   ❌ also slow
────────────────────────────────────────────
Total:                    14s
```

**Key insight:** Even with DOM limited to 100 items, 50k items in state triggers expensive Svelte reactivity (~9.5s).
Plus IPC transfer (~4.1s).

---

## Fix 1: Svelte reactivity optimization ✅

**Change:** Store files in plain JS variable, use version counter to trigger updates.

| Metric              | Before | After      | Improvement     |
| ------------------- | ------ | ---------- | --------------- |
| IPC roundtrip (50k) | 4507ms | **1435ms** | **3.1x faster** |

**Code change:**

```diff
- let allFiles = $state<FileEntry[]>([])
+ let allFilesRaw: FileEntry[] = []
+ let filesVersion = $state(0)
```

## Step-by-step breakdown (what happens when loading a directory)

```
User navigates to folder
    ↓
1. [Frontend] loadDirectory() called
    ↓
2. [Frontend] Tauri IPC invoke('list_directory_contents')
    ↓
3. [Rust] list_directory() starts
    ├── 3a. fs::read_dir() - enumerate directory
    ├── 3b. For each entry: file_type(), metadata()
    ├── 3c. For each entry: get_owner_name(uid), get_group_name(gid)
    ├── 3d. For each entry: create FileEntry struct
    └── 3e. Sort entries (dirs first, then alpha)
    ↓
4. [Rust→Frontend] Serialize to JSON
    ↓
5. [IPC] Transfer JSON to webview
    ↓
6. [Frontend] Deserialize JSON
    ↓
7. [Frontend] First 500 entries rendered to DOM
    ↓
8. [Frontend] Remaining entries rendered in RAF chunks
    ↓
9. [Frontend] Icons loaded in background (async)
```

---

## Benchmarks collected

### Native Rust (optimized standalone, no IPC)

Standalone Rust binary compiled with `-O` (for reference):

| Folder     | read_dir | collect+meta | sort  | **Total** |
| ---------- | -------- | ------------ | ----- | --------- |
| 1000 files | 0.9ms    | 4.4ms        | 0.9ms | **6.1ms** |
| 5000 files | 3.2ms    | 25.2ms       | 4.7ms | **33ms**  |

### Debug build timing (from /tmp/cmdr_timing.log)

| Folder     | list_directory | serialize | JSON size | **Total Rust** |
| ---------- | -------------- | --------- | --------- | -------------- |
| 1000 files | 26ms           | 9ms       | 347 KB    | **35ms**       |
| 5000 files | 131ms          | 47ms      | 1.7 MB    | **178ms**      |

### Full IPC roundtrip (measured from JavaScript invoke())

| Folder     | IPC total | Calculated overhead |
| ---------- | --------- | ------------------- |
| 1000 files | ~66ms     | 31ms (IPC+deser)    |
| 5000 files | ~255ms    | 77ms (IPC+deser)    |

### Overhead breakdown for 5000 files (debug build)

```
list_directory():     131ms (51%)
  └── Rust operations: read_dir, metadata, owner lookup, sort
serialize (JSON):      47ms (18%)
  └── serde_json::to_string (1.7MB output)
IPC + deserialize:     77ms (31%)
  └── WebView message passing + JSON.parse
────────────────────────────────
TOTAL (debug):        255ms
```

### Comparison: Native optimized vs Debug IPC

| Step           | Native (release) | Debug IPC | Ratio    |
| -------------- | ---------------- | --------- | -------- |
| list_directory | 33ms             | 131ms     | 4x       |
| Serialization  | ~7ms (est)       | 47ms      | 7x       |
| IPC overhead   | 0                | 77ms      | n/a      |
| **Total**      | **33ms**         | **255ms** | **7.7x** |

---

## Ideas to investigate (checklist)

### Not yet tested

- [ ] Release build IPC performance (can't use MCP, need file logging)
- [ ] Why does later chunk rendering slow down for larger folders? (DOM re-renders?)
- [ ] Reduce FileEntry size - skip owner/group initially?
- [ ] Two-phase loading: names first, metadata in background
- [ ] Skip sort on backend, sort on frontend for first chunk only?

### Tested/ruled out

- [x] O(n²) in backend chunking - FIXED (now single call)
- [x] O(n²) in frontend array append - NOT an issue (pure JS is fast)
- [x] Owner/group lookup - already cached
- [x] Native read_dir performance - very fast (3ms for 5k files)
- [x] JSON serialization timing - measured: 47ms for 5k files (1.7MB)
- [x] Cancellation behavior - **frontend ignores stale, but backend runs to completion**

---

## Cancellation behavior

**Current state:**

1. Each `loadDirectory()` increments `loadGeneration` counter
2. After awaits, code checks if generation matches
3. If stale (user navigated away), result is discarded

**What this means:**

- ✅ Frontend correctly ignores stale results
- ❌ Backend Rust call runs to completion (SSD still accessed)
- ❌ Large directories still consume backend resources even if user navigates away

**Possible fix:** Convert to async Rust with cancellation token (complex).

---

## Next steps

1. ~~Add JSON serialization timing in Rust~~ DONE
2. Test release build performance via file logging
3. Profile DOM rendering for "later chunks slow" issue
4. Consider two-phase loading optimization
