# Logging support module

Owns the log pipeline via a hand-rolled `fern` Dispatch tree, chosen over `tauri-plugin-log` for **per-output level
filtering**: the file target is locked at Debug, the terminal defaults to Info.

## Module map

- **`mod.rs`**: the resolved-log-dir and keep-count state plus the log-file listing and pruning helpers (DETAILS §
  "What lives in `mod.rs`")
- **`dispatch.rs`**: `init` (builds + installs the fern tree), `set_stdout_threshold` / `stdout_threshold` (the verbose
  toggle knob, a single AtomicU8)
- **`coalesce.rs`**: `CoalescingWriter`, the file chain's terminal writer; collapses identical-line floods so a runaway
  loop can't peg a core through the log file (DETAILS § "Duplicate coalescing")
- **`ram_gauge.rs`**: optional per-line RAM prefix (`CMDR_LOG_RAM_USE=1` → `… DEBUG (374 MB) target …`), sampled every
  100 ms into a lock-free atomic (DETAILS § "RAM gauge")
- **`tests.rs`**: pruner / listing helper unit tests

Dispatch-tree shape, why fern + file-rotate, timestamp formats, and decisions: `DETAILS.md`.

## Adding log calls

- **Use `log::*!` macros only.** `eprintln!` / `println!` / `dbg!` bypass the pipeline (no level filter, no file output,
  not in error-report bundles); clippy denies them crate-wide.
- **Always pass a scoped `target:`** so logs filter via `RUST_LOG` (`log::debug!(target: "open_with", …)`, then
  `RUST_LOG=open_with=debug pnpm dev`). Without one the log gets the file's module path: workable but noisier. Give a
  new subsystem one short stable `target:` and reuse it.
- **A per-event line with a varying payload (`attempt=27`, a name, a count) is a flood the coalescer can't catch**: its
  key is the exact line, and the file chain is always Debug. Log per episode, or only when the result changes. DETAILS
  § "Duplicate coalescing".

## Must-knows

- **Two independent chains; don't collapse them into a shared level.** The stdout chain (chains `stderr`, so piped
  stdout stays clean) defaults to Info under `RUST_LOG` and noise overrides. The file chain is always Debug when log
  storage is enabled, whatever the dev set, because error-report bundles need debug context.
- **The verbose toggle is an AtomicU8 read per record, NOT a dispatch rebuild** (a rebuild drops records during the
  swap). It gates the stdout chain only; `RUST_LOG` sets the startup threshold, the toggle takes over at runtime.
  DETAILS § "Verbose toggle".
- **The file chain's ISO timestamp is prepended by `CoalescingWriter`, NOT the fern `.format()` closure.** The dedup key
  must stay timestamp-free or identical lines a millisecond apart never coalesce. Don't move `file_timestamp()` back into
  the file-chain format. DETAILS § "Duplicate coalescing".
- **The RAM gauge is off unless `CMDR_LOG_RAM_USE` is truthy** (`ram_gauge::tag()` returns `""`, no alloc). When on, its
  ever-changing number lands in the file dedup key, so floods coalesce less: accepted debug-mode tradeoff.
- **Cap = 0 disables the file chain entirely** (`init` skips it). stdout and the verbose toggle still work; the error
  bundle just ships an empty `logs/`.
- **`file-rotate` bakes keep-N at startup; it can't be reconfigured live.** `set_keep_count` / `eager_prune` update the
  in-RAM count and delete excess files now, but restart-to-apply stands (DETAILS § "Cap changes at runtime").
- **Trust mtime, not the filename, for log ordering**: `file-rotate` uses `.1`, `.2`, … suffixes, not timestamps. The
  active-file pattern is `^cmdr\.log(\.\d+)?$` (case-insensitive); anything else, including legacy
  `Cmdr_<timestamp>.log`, is rejected and swept at startup.
- **`eager_prune(dir, 0)` wipes everything including the live file** (file-rotate re-creates it on the next write).
  That's correct for "the user just disabled logging": stop capturing now, not at the next restart.
- **Log-dir resolution must stay in sync across `lib.rs` and the settings early-load helpers.** `lib.rs` resolves it in
  `setup()`; `early_load_max_log_storage_mb` / `early_load_verbose_logging` (`settings::loader`) mirror the
  `CMDR_DATA_DIR` fallback but use `dirs::data_dir` + bundle id, not Tauri's `app_data_dir`. A bundle-id change touches
  both.
