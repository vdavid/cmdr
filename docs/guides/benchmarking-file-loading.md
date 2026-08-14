# Benchmarking file loading performance

How to measure where the time goes when Cmdr opens a directory.

The instrumentation is a **unified timeline**: both the Rust backend and the Svelte frontend emit timestamped events,
and the frontend's go to Rust over IPC (`benchmark_log`) so everything lands in ONE stderr stream in chronological
order. Each side resets its own epoch when a navigation starts, so timestamps are relative to that navigation.

## Running one

```bash
# From the repo root. Add `--worktree <slug>` when running from a worktree.
RUSTY_COMMANDER_BENCHMARK=1 VITE_BENCHMARK=1 pnpm dev 2>&1 | tee benchmark.log
```

Then navigate to the directory you want to measure and pull the timeline out:

```bash
grep '\[TIMELINE\]' benchmark.log > timeline.txt
```

⚠️ Use `pnpm dev` from the repo root, ❌ never `cargo tauri dev`: the wrapper is what picks the dev data dir and port
(`apps/desktop/CLAUDE.md`). It runs the app with `stdio: 'inherit'`, which is why the timeline reaches your terminal.

Before starting, make sure no dev session is already holding this worktree's data dir:

```bash
# ❌ Never widen this to `pkill -f cmdr`: every checkout lives under a path containing
# "cmdr", so that reaches every Cmdr on the machine, E2E shards included. See
# `apps/desktop/test/e2e-playwright/DETAILS.md` § "Running on macOS" for why no argv
# pattern can single one out.
pgrep -fl "$PWD/.*/Cmdr"   # look first; kill only the pid you recognize
```

Test directories big enough to measure: `docs/guides/generating-test-files.md`.

## The two enable flags

- `RUSTY_COMMANDER_BENCHMARK=1` turns on the Rust side (read in `benchmark.rs::init_benchmarking`, called from
  `lib.rs`'s setup hook). It also gates the `benchmark_log` command the frontend sends through.
- `VITE_BENCHMARK=1` turns on the frontend side (read through `import.meta.env`, so it's baked in at dev-server start).

They're independent, and you want both: with only one, half the timeline is missing. The frontend side also has a
runtime escape hatch, `window.__BENCHMARK__ = true` in the DevTools console, for when the dev server is already up.

## Reading the timeline

Events carry a side tag (`FE` / `RUST`) and microseconds since that side's epoch:

```
[TIMELINE]          0μs | FE   | EPOCH_RESET
[TIMELINE]        123μs | FE   | loadDirectory CALLED = /path/to/folder
[TIMELINE]        456μs | FE   | IPC listDirectoryStart CALL
[TIMELINE]        500μs | RUST | EPOCH_RESET
[TIMELINE]        502μs | RUST | list_directory_start_streaming CALLED = /path/to/folder
[TIMELINE]        900μs | RUST | list_directory_start_streaming RETURNING
[TIMELINE]       1000μs | FE   | IPC listDirectoryStart RETURNED = <listingId>
[TIMELINE]       1100μs | RUST | read_directory_with_progress START
[TIMELINE]      40000μs | RUST | read_dir COMPLETE, entries = 20000
[TIMELINE]      41000μs | RUST | sort START
[TIMELINE]      70000μs | RUST | sort END
[TIMELINE]      71000μs | RUST | read_directory_with_progress COMPLETE, read_dir_time_ms = 39
[TIMELINE]      75000μs | FE   | listing-complete received, totalCount = 20000
[TIMELINE]      75500μs | FE   | loading = false (UI can render)
```

The shape to expect: `list_directory_start_streaming` returns a `listingId` almost immediately, and the real work runs
behind it, so the interesting gap is between `RETURNING` and `listing-complete received`. A cancelled navigation shows
one of the `read_directory_with_progress CANCELLED (…)` events instead of `COMPLETE`, naming the point it bailed.

The non-streaming core (`list_directory_core START`, `readdir START/END`, `stat_loop START/END`, `sort START/END`,
`list_directory_core END`, in `listing/reading.rs`) emits its own events when that path runs. macOS extended metadata
appears separately as `get_extended_metadata_batch START/END, count`.

## What to look at

- **When does the user see files?** → `loading = false (UI can render)`, measured from the FE `EPOCH_RESET`.
- **Where is the time?** → compare `read_dir COMPLETE` (enumerate + stat), `sort END`, and the gap between the Rust
  `COMPLETE` and the FE `listing-complete received` (that gap is IPC and store population).
- **Is extended metadata in the way?** → check whether `get_extended_metadata_batch` lands before or after
  `loading = false`. It shouldn't block it.
- **Did the navigation even finish?** → a `CANCELLED` event means a second navigation superseded this one, and the
  numbers below it describe nothing.

## Nothing in the output?

- Both env vars set, and stderr captured (`2>&1 | tee benchmark.log`).
- Frontend events are ALSO `console.log`ed, so DevTools shows the FE half even when the Rust half is off. If DevTools
  has them and the terminal doesn't, `RUSTY_COMMANDER_BENCHMARK` isn't set on the app process.
- Timestamps that "don't align" across sides are expected: each side keeps its own epoch and resets it per navigation.
  Compare within a side, or compare the ORDER across them.

## Code locations

- `apps/desktop/src-tauri/src/benchmark.rs`: the Rust emitter (`log_event`, `log_event_value`, `reset_epoch`).
- `apps/desktop/src/lib/benchmark.ts`: the frontend emitter, forwarding through `benchmarkLog`.
- `apps/desktop/src-tauri/src/file_system/listing/streaming.rs` and `reading.rs`: the instrumented backend paths.
- `apps/desktop/src/lib/file-explorer/pane/listing-loader.ts`: the instrumented frontend path.

## Recording a result

A measurement worth keeping goes in `docs/notes/` (see its `README.md`), dated and with the machine and directory size
named. ❌ Don't leave numbers in this guide: a how-to outlives the shape of the pipeline it measured, and stale figures
here read as current.
