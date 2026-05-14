# Logging support module

Lives under `src-tauri/src/logging/`. Owns the log pipeline end to end via a hand-rolled
`fern` Dispatch tree. Replaced `tauri-plugin-log` to get **per-output level filtering**:
file target locked at Debug, terminal defaults to Info.

## Rules for adding log calls

- **Use `log::*!` macros only.** `eprintln!` / `println!` / `dbg!` bypass this entire
  pipeline (no level filtering, no file output, no inclusion in error-report bundles)
  and are denied by clippy at the crate root.
- **Always pass a scoped `target:`** so logs are filterable via `RUST_LOG`:
  ```rust
  log::debug!(target: "open_with", "candidates intersected: {n}");
  log::warn!(target: "cloud_actions", "evict failed: {e}");
  ```
  Then dev sees just that subsystem with `RUST_LOG=open_with=debug pnpm dev`. Without a
  `target:`, the log gets the file's module path as its target, workable but noisier
  and harder to filter consistently. For new subsystems, pick a short stable `target:`
  string and use it across that module's log calls.
- The verbose toggle in Settings flips the stdout chain to Debug at runtime; `RUST_LOG`
  overrides it at startup. The file chain is always Debug when log storage is enabled.

## File map

| File          | Purpose                                                                                                                                       |
| ------------- | --------------------------------------------------------------------------------------------------------------------------------------------- |
| `mod.rs`      | `OnceLock<PathBuf>` for the resolved log dir, `AtomicUsize` for keep-count, `eager_prune`, `list_recent_log_files` |
| `dispatch.rs` | `init` (builds + installs the fern tree), `set_stdout_threshold` / `stdout_threshold` (verbose toggle knob)                                   |
| `tests.rs`    | Pruner / listing helper unit tests                                                                                                            |

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

Why per-output filtering matters: error report bundles need debug context regardless of
what the dev set in `RUST_LOG`. Previously we had to either bake terminal noise into the
file (annoying for dev) or skimp on file context (bad for error reports). Now both
chains are independent.

## What lives in `mod.rs`

| Function                             | Role                                                                                                                  |
| ------------------------------------ | --------------------------------------------------------------------------------------------------------------------- |
| `set_log_dir(path)` / `log_dir()`    | Cache the resolved dir once at logger-init time; the error-report bundle builder reads it back                        |
| `set_keep_count(n)` / `keep_count()` | Live view of the keep-N value the file chain was built with                                                           |
| `list_recent_log_files(dir)`         | Active log files (`cmdr.log` plus `cmdr.log.<digits>`) newest-first by mtime. Rejects legacy `Cmdr_*.log` files       |
| `eager_prune(dir, keep_n)`           | One-shot: delete everything beyond `keep_n` newest. Used after the user lowers the cap so they see files vanish now.  |
| `cleanup_legacy_log_files(dir)`      | One-shot startup sweep: remove `Cmdr_<timestamp>.log` files left over from the pre-`319d5d37` `tauri-plugin-log` setup |

## Why fern + file-rotate (and not tauri-plugin-log)

The plugin is one-shot, owns the global `log` facade, and routes everything through a
single shared level. Per-target levels would have required either patching the plugin or
shipping two loggers, neither great. fern gives us a tree of independent dispatches with
their own levels and filters; `file-rotate` is a small, focused crate exposing
size+count rotation behind a `Write` impl. Together they're ~250 LOC of glue plus two
~3000 LOC well-maintained MIT crates.

`file-rotate` wraps `cmdr.log` and produces siblings like `cmdr.log.1`, `cmdr.log.2`, ...
The `list_recent_log_files` helper sorts by mtime so the live file is always first.

## Manifest snapshot

`init` records the resolved stdout default + per-module overrides into
`error_reporter::log_level_overrides::record` so the error-report bundle's
`logLevels` field shows triagers what *could* have been logged. The file chain's
`Debug` is hard-coded in the manifest: the dispatch tree is the only source of
truth, and a triager seeing `fileChain: "debug"` should match what's in this file.

## Verbose toggle (`developer.verboseLogging`)

Wired through `commands/logging.rs::set_log_level`, which calls
`dispatch::set_stdout_threshold(Debug | Info)`. Implementation: a single AtomicU8 packed
with `log::LevelFilter`'s integer value. fern reads it via a `.filter(...)` closure on
every record, so the toggle takes effect mid-stream without rebuilding the dispatch (no
records lost during the swap).

The file chain ignores this knob entirely: it stays at Debug whenever log storage is
enabled. RUST_LOG sets the **startup** stdout threshold; the toggle takes over at
runtime if the user clicks it.

## Cap = 0 (log storage disabled)

`init` skips the file chain entirely when `keep_count == 0`. The stdout chain still
works. The verbose toggle still works. The error reporter sees `log_dir() == None` (well,
it sees the path but `keep_count == 0`) and produces a bundle with empty `logs/`; upload still goes through, just less useful. The settings UI documents this.

## Cap changes at runtime

`set_keep_count(n)` updates the in-RAM count. `eager_prune(dir, n)` is called by
`set_max_log_storage_mb` to delete excess archived files immediately so the user sees
the change. `file-rotate` itself doesn't get reconfigured: the keep-N value baked into
the rotator at startup stays. Restart-to-apply is documented in the settings UI for
`0 ↔ non-zero` transitions and for raising the cap above the previously baked-in value.

## Decision/Why

- **fern over a custom logger**: writing a `log::Log` impl from scratch is ~150 LOC and
  another 100 LOC of tests. fern gives us battle-tested record routing and the formatter
  abstraction for ~free. The dep is tiny (`log` only).
- **file-rotate over a hand-rolled rotating writer**: 50 LOC of `Write` plus a rotation
  step, but then we own the corner cases (rename-during-write on macOS, rotation atomicity
  on power loss, count-suffix vs timestamp-suffix collation). file-rotate has covered all
  of these for years.
- **AtomicU8 for the verbose toggle, not dispatch rebuild**: rebuilding would briefly
  drop log records (between the old logger going away and the new one being installed),
  which is the wrong tradeoff for a user-facing toggle. The atomic costs one Relaxed load
  per record, a rounding error.
- **Stdout chain chains stderr, not stdout**: matches the previous plugin behavior. Devs
  who pipe stdout for parsing don't get logs in their pipe.

## Timestamp formats

- **Stdout chain**: `HH:MM:SS.mmm` (terse; devs reading the live terminal know the date).
- **File chain**: `YYYY-MM-DDTHH:MM:SS.mmm±HH:MM` (ISO 8601 with millisecond precision
  and timezone offset). The file lives forever and gets shipped to triage; bare
  `HH:MM:SS.mmm` is impossible to correlate without context. The error reporter's
  Flow B bundle parses this stamp to line-trim by timestamp.

## Gotcha/Why

- `list_recent_log_files` returns `Vec<PathBuf>` in "newest-first by mtime" order. Trust
  mtime, not the filename: `file-rotate` uses `.1`, `.2`, ... suffixes, not timestamps.
- The active-file pattern is `^cmdr\.log(\.\d+)?$` (case-insensitive). Anything else
  (the legacy `Cmdr_<timestamp>.log` from the pre-`319d5d37` plugin setup, weird
  `cmdr.logsy` typos, unrelated `notes.log`s) is rejected. Legacy files are removed
  on startup by `cleanup_legacy_log_files`.
- `eager_prune(dir, 0)` wipes everything including the live file; `file-rotate` re-creates
  it on the next write. This is the correct behavior for the "user just disabled logging
  at runtime" path: we stop capturing immediately rather than waiting for the next
  restart.
- The log dir resolution in `lib.rs` and in `early_load_max_log_storage_mb` /
  `early_load_verbose_logging` must stay in sync. `lib.rs` resolves it inside `setup()`;
  the early-load helpers in `settings::loader` mirror the CMDR_DATA_DIR fallback but use
  `dirs::data_dir` + bundle id rather than Tauri's `app_data_dir`. If the bundle id ever
  changes, both places need updating.
- RUST_LOG always wins at startup. If the user has both `RUST_LOG=info` and the verbose
  toggle on, the toggle is ignored at startup but takes over the next time they flip it
  (it overwrites the AtomicU8 directly).
