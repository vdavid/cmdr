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

## Decisions

- **fern over a custom logger**: a from-scratch `log::Log` impl is ~150 LOC plus ~100 of tests; fern gives battle-tested
  record routing and the formatter abstraction for ~free, with a tiny dep (`log` only).
- **file-rotate over a hand-rolled rotating writer**: ~50 LOC plus a rotation step, but then we own the corner cases
  (rename-during-write on macOS, rotation atomicity on power loss, count-suffix vs timestamp-suffix collation).
  file-rotate has covered all of these for years.
- **AtomicU8 for the verbose toggle, not a dispatch rebuild**: rebuilding briefly drops records (old logger gone, new
  one not yet installed), the wrong tradeoff for a user-facing toggle. The atomic costs one Relaxed load per record.
- **Stdout chain chains stderr, not stdout**: devs who pipe stdout for parsing don't get logs in their pipe.
