# Load files super fast: specification

## Summary

Make the file explorer feel blazing fast when loading large directories (20k–100k files) by:

1. Returning file count immediately (~16ms) to show progress
2. Sending "core" file data (name, size, dates, permissions) first for fast display
3. Loading extended metadata (lastOpenedDate, etc.) in parallel
4. Using a non-reactive `FileDataStore` to avoid Svelte reactivity freezes (see
   [ADR-009](../adr/009-non-reactive-file-store.md))

Target: Match Commander One performance (~3s for 50k files to full load).

---

## Context

See [2025-12-28-dir-load-bench-findings.md](./2025-12-28-dir-load-bench-findings.md) for detailed benchmarks.

**Current bottlenecks:**

| Step                   | Time (50k files) |
| ---------------------- | ---------------- |
| Rust list_directory    | 308ms            |
| JSON serialize         | 18ms             |
| IPC transfer (17.4 MB) | ~4,100ms         |
| JSON.parse             | 67ms             |
| Svelte reactivity      | ~9,500ms         |
| **Total**              | **~14s**         |

The user experiences a frozen UI during the Svelte reactivity step because assigning large arrays to reactive state
triggers expensive internal tracking.

---

## Problem

Three goals for large directory loading:

1. **Get all files eventually** — user can scroll through the full list
2. **Show first files fast** — feels snappy, user sees content immediately
3. **Don't freeze the UI** — app remains responsive during loading

The current implementation fails goals #2 and #3 for directories with 20k+ files.

---

## Solution

### Architecture overview

```
┌──────────────────────────────────────────────────────────────────────┐
│                           RUST BACKEND                                │
├──────────────────────────────────────────────────────────────────────┤
│  1. readdir() → count files (~16ms for 20k)                          │
│  2. stat() per file → core data (name, size, dates, perms, uid/gid)  │
│  3. Extended metadata (macOS: lastOpenedDate, addedAt) in parallel   │
│  4. // TODO: Apply sort criteria here (future)                       │
└──────────────────────────────────────────────────────────────────────┘
                              │
                              ▼ IPC (chunked)
┌──────────────────────────────────────────────────────────────────────┐
│                          FRONTEND                                     │
├──────────────────────────────────────────────────────────────────────┤
│  FileDataStore (plain JS, per pane)                                  │
│  ├── files: FileEntry[]                                              │
│  ├── totalCount: number                                              │
│  ├── maxFilenameWidth: number                                        │
│  ├── getRange(start, end): FileEntry[]                               │
│  └── mergeExtendedData(entries): void                                │
└──────────────────────────────────────────────────────────────────────┘
                              │
                              ▼ visible range only (~50-100 items)
┌──────────────────────────────────────────────────────────────────────┐
│                     SVELTE COMPONENTS                                 │
│  BriefList / FullList with virtual scrolling                         │
│  Only reactive state: visibleItems[], totalCount, maxWidth           │
└──────────────────────────────────────────────────────────────────────┘
```

### Loading flow

```
Time →
────────────────────────────────────────────────────────────────────────

1. User navigates to folder
   │
   ▼
2. Rust: readdir().count() → 20000
   │
   ▼ IPC (~16ms)
3. Frontend: FileDataStore.totalCount = 20000
   UI shows: "Loading 20000 files..."
   │
   ▼
4. Rust: stat() first 1000 files → core data
   │
   ▼ IPC (~30-50ms for 350KB)
5. Frontend: FileDataStore receives first chunk
   UI shows first files immediately!
   │
   │ ┌────────────────────────────────┐
   │ │ IN PARALLEL:                   │
   │ │ - Rust sends remaining chunks  │
   │ │ - Rust loads extended metadata │
   │ └────────────────────────────────┘
   ▼
6. Extended data arrives → FileDataStore.mergeExtendedData()
   UI updates with full metadata (lastOpenedDate, etc.)
   │
   ▼
7. All chunks received → loading complete
```

### Key design decisions

**FileDataStore per pane:** Each pane has its own store because:

- Panes may show the same folder but with different sorting/filtering
- Simplifies state management

**Item ID = path (not filename):** Path is guaranteed unique and handles future cross-directory caching.

**extendedMetadataLoaded flag:** Each FileEntry has a flag indicating whether extended metadata is loaded. UI can show
placeholder or partial data until extended data arrives.

**Hidden files filtering:**  
Cannot know exact visible count until all files are loaded (some may be hidden). Handle gracefully—show what we have,
update count as data arrives.

**Chunk size:** 5000 files per chunk balances IPC overhead vs. responsiveness. May tune later.

**Sorting placeholders:** Backend sorting (when implemented) happens before chunking so first chunk contains the "best"
files for the current sort order.

---

## Cancellation

**Current state:**

- ✅ Frontend: `loadGeneration` counter discards stale results
- ❌ Backend: Rust `list_directory()` runs to completion even if user navigates away

**Future optimization:** Add cancellation flag checked periodically in Rust loop. Not critical for UX since UI doesn't
freeze.

---

## Key clues

Files and patterns to understand before implementing:

| File                                      | Why it matters                                                                                                                       |
| ----------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------ |
| `src/lib/file-explorer/FilePane.svelte`   | Current loading logic lives here. Look for `loadDirectory()`, `allFilesRaw`, `filesVersion`. This is what you're replacing.          |
| `src/lib/file-explorer/BriefList.svelte`  | Virtual scroll for Brief mode. Already calculates `startIndex`/`endIndex`. You'll modify it to request visible range from the store. |
| `src/lib/file-explorer/FullList.svelte`   | Virtual scroll for Full mode. Same pattern as BriefList.                                                                             |
| `src-tauri/src/file_system/operations.rs` | Rust session management (`list_directory_start`, `list_directory_next`, etc.). This is where streaming logic lives.                  |
| `src-tauri/src/commands/file_system.rs`   | Tauri command wrappers that call `operations.rs`. New commands must be added here.                                                   |
| `src-tauri/src/lib.rs`                    | Command registration. New Rust commands must be registered in the `invoke_handler`.                                                  |
| `src/lib/tauri-commands.ts`               | Frontend TypeScript wrappers for Tauri commands. Add new command wrappers here.                                                      |
| `src/lib/file-explorer/types.ts`          | `FileEntry` type definition. Add `extendedMetadataLoaded` flag here.                                                                 |

**Where to put `FileDataStore`:** Create it at `src/lib/file-explorer/FileDataStore.ts`.

**Verification:** Run `./scripts/check.sh` after each phase to ensure nothing is broken (runs rustfmt, clippy, tests,
eslint, svelte-check, etc.).

---

## Related documents

- [ADR-009: Non-reactive file data store](../adr/009-non-reactive-file-store.md) — why we need FileDataStore
- [2025-12-28-dir-load-bench-findings.md](./2025-12-28-dir-load-bench-findings.md) — benchmark data
