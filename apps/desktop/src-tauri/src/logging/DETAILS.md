# Logging details

Depth and rationale. `CLAUDE.md` holds the must-knows; the dispatch-tree shape, the why-fern narrative, timestamp
formats, and decisions live here.

## Dispatch tree

```
root Dispatch (level Trace: pure ceiling, per-chain filters do real gating)
├── stdout chain
│     .level(Info)                       // default; AtomicU8 below can bump to Debug
│     .level_for("nusb", Warn) ...        // noise overrides (stdout only)
│     .level_for(<from RUST_LOG>, ...)    // per-module overrides (stdout only)
│     .filter(stdout-threshold AtomicU8)  // verbose-toggle gate, no rebuild
│     .chain(io::stderr())
└── file chain (skipped when advanced.maxLogStorageMb = 0)
      .level(Debug)                      // always, regardless of RUST_LOG/verbose
      .chain(file-rotate(50 MB, KeepN))  // N = ceil(cap_mb / 50)
```

Per-output filtering matters because error-report bundles need debug context regardless of the dev's `RUST_LOG`. With a
single shared level you either bake terminal noise into the file (annoying for dev) or skimp on file context (bad for
error reports). Two independent chains avoid the tradeoff.

## What lives in `mod.rs`

- **`set_log_dir(path)` / `log_dir()`**: cache the resolved dir at logger-init; the error-report bundle builder reads it
  back.
- **`set_keep_count(n)` / `keep_count()`**: live view of the keep-N the file chain was built with.
- **`list_recent_log_files(dir)`**: active log files (`cmdr.log` plus `cmdr.log.<digits>`) newest-first by mtime.
  Rejects legacy `Cmdr_*.log`.
- **`eager_prune(dir, keep_n)`**: one-shot delete of everything beyond `keep_n` newest. Used after the user lowers the
  cap so files vanish now.
- **`cleanup_legacy_log_files(dir)`**: one-shot startup sweep removing `Cmdr_<timestamp>.log` files left from the
  earlier `tauri-plugin-log` setup.

## Duplicate coalescing (file chain)

The file chain's terminal writer is `coalesce::CoalescingWriter`, wrapping `FileRotate`. It exists so a runaway loop
logging the same line thousands of times a second can't peg a core through `write` syscalls or stall other threads on
the log mutex (an incident once logged 12,700 near-identical warnings; a thread sample caught ~900 samples inside
`write`). The loop's source is fixed elsewhere (fatal storage errors stop instead of retrying); this is the general
safety net.

How it works:

- fern hands a chain-target writer one record as a run of `write` calls followed by exactly one `flush`, all under
  fern's own `Mutex<Box<dyn Write + Send>>`. So the writer buffers bytes in `pending`, and `flush` sees exactly one
  complete record with no interior locking and no cross-record interleaving. (This also means the old `MutexWriter`
  wrapper was redundant and is gone.)
- The dedup key is the record's `LEVEL target  message` bytes. Within a 1 s window, the first `BURST_THRESHOLD` (3)
  identical lines pass verbatim; further identical lines are dropped and counted. At the next window the line reappears
  tagged `[+N identical suppressed]`, so triage sees the repetition rate. Distinct lines never coalesce, so normal
  traffic loses nothing; only a genuine flood is trimmed.
- The key set is bounded (`MAX_KEYS`, 4096): on the insert path, if full, idle keys (window elapsed) are dropped, then
  cleared if still full. At worst this re-emits a line that would have been coalesced.

Why this design and not an async drain thread: synchronous, unbuffered per-record writes (fern flushes after each one;
`file-rotate` writes straight to an unbuffered `File`) are exactly what makes the crash tail complete: every line a
crashing run logged is already on disk when the next launch bundles it. A bounded async channel would leave the most
recent lines in-process at crash time, and a flood-then-crash (the worst case for triage) would lose precisely the
lines that matter. Coalescing keeps the synchronous path and only removes redundant duplicate writes, so the crash tail
and error-report completeness are preserved while the CPU-burn lever is closed.

**Timestamp placement gotcha**: the ISO-8601 stamp is prepended by the writer at emit time, NOT in the fern `.format()`
closure. The dedup key must be timestamp-free, or two identical messages a millisecond apart would hash differently and
never coalesce. Don't move `file_timestamp()` back into the file-chain format. The on-disk line shape is unchanged
(`<iso-ts> LEVEL target  message`), so the error reporter's timestamp parsing is unaffected.

## Why fern + file-rotate

`tauri-plugin-log` is one-shot, owns the global `log` facade, and routes everything through a single shared level.
Per-target levels would have required patching the plugin or shipping two loggers. fern gives a tree of independent
dispatches with their own levels and filters; `file-rotate` is a small, focused crate exposing size+count rotation
behind a `Write` impl. Together ~250 LOC of glue plus two well-maintained MIT crates. `file-rotate` wraps `cmdr.log` and
produces siblings (`cmdr.log.1`, `cmdr.log.2`, …); `list_recent_log_files` sorts by mtime so the live file is first.

## Manifest snapshot

`init` records the resolved stdout default + per-module overrides into `error_reporter::log_level_overrides::record` so
the bundle's `logLevels` field shows triagers what could have been logged. The file chain's `Debug` is hard-coded in the
manifest: the dispatch tree is the single source of truth, and a triager seeing `fileChain: "debug"` should match this.

## Verbose toggle (`developer.verboseLogging`)

Wired through `commands/logging.rs::set_log_level`, which calls `dispatch::set_stdout_threshold(Debug | Info)`. A single
AtomicU8 packed with `log::LevelFilter`'s integer value; fern reads it via a `.filter(...)` closure on every record, so
the toggle takes effect mid-stream without rebuilding the dispatch (no records lost during the swap). The file chain
ignores it (stays Debug whenever log storage is enabled). `RUST_LOG` sets the startup stdout threshold; the toggle takes
over at runtime if clicked.

## Cap = 0 (log storage disabled)

`init` skips the file chain entirely when `keep_count == 0`. stdout and the verbose toggle still work. The error
reporter produces a bundle with empty `logs/`; upload still goes through, just less useful. The settings UI documents
this.

## Cap changes at runtime

`set_keep_count(n)` updates the in-RAM count. `eager_prune(dir, n)`, called by `set_max_log_storage_mb`, deletes excess
archived files immediately so the user sees the change. `file-rotate` itself isn't reconfigured: the keep-N baked in at
startup stays. Restart-to-apply is documented in the settings UI for `0 ↔ non-zero` transitions and for raising the cap
above the baked-in value.

## Timestamp formats

- **Stdout chain**: `HH:MM:SS.mmm` (terse; devs reading the live terminal know the date).
- **File chain**: `YYYY-MM-DDTHH:MM:SS.mmm±HH:MM` (ISO 8601 with millisecond precision and timezone offset). The file
  ships to triage, where bare `HH:MM:SS.mmm` is impossible to correlate; the error reporter's Flow B bundle parses this
  stamp to line-trim by timestamp.

## RAM gauge (`CMDR_LOG_RAM_USE`)

Opt-in debug aid in `ram_gauge.rs`: with `CMDR_LOG_RAM_USE` set to a truthy value (`1`/`true`/`yes`/`on`), every log
line carries the process's current memory use right after the level, on both chains:

```
2026-07-16T22:56:21.829+02:00 DEBUG (374 MB) smb2::client::tree  tree: fs_info done, total=...
```

The point is a memory timeline: the inline number tells you which operation coincided with a jump. It answers *when and
near what*, not *what allocated* (for that, Instruments or a heap profiler). It rides the logger, so it works in dev,
E2E, and prod builds identically.

- **Metric**: `phys_footprint` of the Rust process via `crate::process_memory` (the shared Mach-`task_info` reader the
  indexing watchdog also uses; see that module for why `phys_footprint`, not RSS). Cmdr is multi-process (Tauri), so
  this is the backend only, not the WebView helper processes: "backend RAM," not "total Cmdr RAM."
- **Cost**: a background OS thread (named `ram-gauge`, plain `std::thread` so it runs before the async runtime exists)
  samples every 100 ms into an `AtomicU64`. The format closures do one Relaxed load per line, no syscall on the log
  path. `init()` (called from `dispatch::init`) reads the env flag once and spawns the sampler only when enabled and
  only once (a `STARTED` swap-guard). When the flag is unset, `tag()` returns an empty `String` (no allocation).
- **Format**: `MB` below 1 GiB, else `GB` with two decimals (binary units, matching the watchdog's `gb()`/`mb()`).
  `?? MB` before the first sample lands (primed synchronously in `init`, so only a theoretical window).
- **Dedup interaction**: the file chain keys its flood-coalescing on the line text (`coalesce.rs`). With the gauge ON,
  the per-line RAM number varies, so identical messages no longer collapse to one key. Accepted because (a) the flag is
  an explicit debug opt-in, off in normal runs where dedup is fully intact, and (b) 100 ms sampling caps distinct values
  to ~10 per one-second window, so a tight loop still collapses from thousands of writes to a few dozen, not thousands.

## Decisions

- **fern over a custom logger**: a from-scratch `log::Log` impl is ~150 LOC plus ~100 of tests; fern gives battle-tested
  record routing and the formatter abstraction for ~free, with a tiny dep (`log` only).
- **file-rotate over a hand-rolled rotating writer**: ~50 LOC plus a rotation step, but then we own the corner cases
  (rename-during-write on macOS, rotation atomicity on power loss, count-suffix vs timestamp-suffix collation).
  file-rotate has covered all of these for years.
- **AtomicU8 for the verbose toggle, not a dispatch rebuild**: rebuilding briefly drops records (old logger gone, new
  one not yet installed), the wrong tradeoff for a user-facing toggle. The atomic costs one Relaxed load per record.
- **Stdout chain chains stderr, not stdout**: devs who pipe stdout for parsing don't get logs in their pipe.
