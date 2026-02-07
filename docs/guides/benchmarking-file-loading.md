# Benchmarking file loading performance

How to measure and analyze directory loading performance in Cmdr.

## Overview

The app has built-in performance instrumentation that creates a **unified timeline** across Rust backend and TypeScript
frontend. When enabled, both sides emit timestamped events to stderr that appear **interleaved in chronological order**.

FE events are sent to Rust via IPC to appear in the same output stream as Rust events.

## Quick start

```bash
# Run the app with benchmarking enabled
RUSTY_COMMANDER_BENCHMARK=1 VITE_BENCHMARK=1 pnpm tauri dev 2>&1 | tee benchmark.log
```

Then navigate to a test directory in the app. The **unified** timeline events will appear in the terminal with both `FE`
and `RUST` events interleaved.

## Test data setup

Use the test data generator (see `docs/workflows/generating-test-files.md`):

```bash
# Creates folders with 1000, 5000, 20000, and 50000 files
go run scripts/test-data-generator/main.go
```

Test folders are created at `_ignored/test-data/folder with XXXXX files`.

## Running a benchmark

### Step 1: Start the app with benchmarking enabled

```bash
cd /path/to/cmdr

# Kill any existing dev server
pkill -f "cmdr" || true

# Start with benchmarking enabled
RUSTY_COMMANDER_BENCHMARK=1 VITE_BENCHMARK=1 pnpm tauri dev 2>&1 | tee benchmark.log
```

### Step 2: Navigate to the test directory

In the app, navigate to your test data folder (like `_ignored/test-data/20k-files`).

### Step 3: Extract and analyze the timeline

```bash
# Extract timeline events from the log
grep '\[TIMELINE\]' benchmark.log | sort -t'|' -k1 -n > timeline.txt
```

## Understanding the timeline

The timeline shows events from both Rust (RUST) and Frontend (FE) with microsecond timestamps:

```
[TIMELINE]          0μs | FE   | EPOCH_RESET
[TIMELINE]        123μs | FE   | loadDirectory CALLED = /path/to/folder
[TIMELINE]        456μs | FE   | IPC listDirectoryStartSession CALL
[TIMELINE]        500μs | RUST | EPOCH_RESET
[TIMELINE]        502μs | RUST | list_directory_start CALLED = /path/to/folder
[TIMELINE]        510μs | RUST | list_directory_core START
[TIMELINE]        520μs | RUST | readdir START
[TIMELINE]       5000μs | RUST | readdir END, count = 20000
[TIMELINE]       5100μs | RUST | stat_loop START
[TIMELINE]     150000μs | RUST | stat_loop END, entries = 20000
[TIMELINE]     150100μs | RUST | sort START
[TIMELINE]     180000μs | RUST | sort END
[TIMELINE]     180500μs | RUST | list_directory_start RETURNING
[TIMELINE]     181000μs | FE   | IPC listDirectoryStartSession RETURNED, totalCount = 20000
[TIMELINE]     181500μs | FE   | fileStore.setFiles START
[TIMELINE]     182000μs | FE   | fileStore.setFiles END, count = 5001
[TIMELINE]     182500μs | FE   | loading = false (UI can render)
```

## Key metrics to watch

| Event                         | Meaning                             |
| ----------------------------- | ----------------------------------- |
| `readdir START/END`           | Time to enumerate directory entries |
| `stat_loop START/END`         | Time to stat() all files            |
| `sort START/END`              | Time to sort entries                |
| `IPC ... CALL/RETURNED`       | Round-trip time for Tauri IPC       |
| `fileStore.setFiles`          | Time to populate the store          |
| `loading = false`             | When UI can first render            |
| `get_extended_metadata_batch` | Time for macOS metadata (Phase 2)   |

## Typical performance breakdown (measured 2024-12-30)

### 20,000 files (warm filesystem cache)

| Phase                               | Time       |
| ----------------------------------- | ---------- |
| readdir                             | ~15ms      |
| stat loop                           | ~110ms     |
| sort                                | ~42ms      |
| IPC serialization + return          | ~200ms     |
| **Total to first render**           | **~365ms** |
| Extended metadata (5k batch, async) | ~68ms      |

### 50,000 files (warm filesystem cache)

| Phase                               | Time       |
| ----------------------------------- | ---------- |
| readdir                             | ~35ms      |
| stat loop                           | ~285ms     |
| sort                                | ~115ms     |
| IPC serialization + return          | ~460ms     |
| **Total to first render**           | **~900ms** |
| Extended metadata (5k batch, async) | ~70ms      |

Note: Cold cache can be 3-4x slower due to disk I/O.

## Environment variables

| Variable                      | Side     | Purpose                                 |
| ----------------------------- | -------- | --------------------------------------- |
| `RUSTY_COMMANDER_BENCHMARK=1` | Rust     | Enable Rust-side timeline logging       |
| `VITE_BENCHMARK=1`            | Frontend | Enable TypeScript-side timeline logging |

Both must be set for a complete timeline. They're independent — you can enable just one side if needed.

## Runtime toggle (TypeScript only)

You can also enable benchmarking at runtime from the browser console:

```javascript
window.__BENCHMARK__ = true
```

Then navigate to a directory to see the timeline in the console.

## Analyzing results

### Quick summary script

```bash
# Show timeline with relative timestamps
grep '\[TIMELINE\]' benchmark.log | head -50

# Find total time to first render
grep 'loading = false' benchmark.log

# Show extended metadata timing
grep 'extended_metadata' benchmark.log
```

### Key questions to answer

1. **How long until the user sees files?** → Look for `loading = false`
2. **Where is the bottleneck?** → Compare `readdir`, `stat_loop`, `sort`, IPC times
3. **Is extended metadata blocking?** → Check if it appears before or after `loading = false`

## Troubleshooting

### No timeline output

- Ensure both env vars are set: `RUSTY_COMMANDER_BENCHMARK=1 VITE_BENCHMARK=1`
- Check that you're capturing stderr: `2>&1 | tee benchmark.log`
- The Rust side only logs on macOS (for extended metadata)

### Timestamps don't align

- FE and Rust have independent epochs that reset on each navigation
- Compare relative times within each side, not absolute values
- Both should reset to ~0 when navigating to a new folder

### Missing frontend events

- Open browser DevTools console (F12) to see FE logs
- Or check the Tauri dev server output

## Code locations

- Rust: `src-tauri/src/benchmark.rs`
- TypeScript: `src/lib/benchmark.ts`
- File loading: `src-tauri/src/file_system/operations.rs`
- Frontend: `src/lib/file-explorer/FilePane.svelte`
