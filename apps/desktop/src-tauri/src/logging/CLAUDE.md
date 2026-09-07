# Logging support module

Owns the log pipeline: a hand-rolled `fern` Dispatch tree, chosen over `tauri-plugin-log` for **per-output level
filtering**: the file is locked at Debug, the terminal defaults to Info.

## Module map

- **`mod.rs`**: log-dir and keep-count state, log-file listing and pruning (DETAILS § "What lives in `mod.rs`")
- **`startup.rs`**: `init`, the one call `lib.rs` makes at startup (DETAILS § "Startup sequence")
- **`dispatch.rs`**: `init` (builds + installs the fern tree), `set_stdout_threshold` / `stdout_threshold`, and
  `write_terminal_line` (the terminal line format)
- **`coalesce.rs`**: `CoalescingWriter`, the file chain's terminal writer; collapses identical-line floods (DETAILS §
  "Duplicate coalescing")
- **`target_style.rs`**: the terminal target column: head width, per-head color, ANSI on/off (DETAILS § "Terminal
  target column")
- **`ram_gauge.rs`**: optional per-line RAM prefix, `CMDR_LOG_RAM_USE=1` (DETAILS § "RAM gauge")
- **`tests.rs`**: pruner / listing helper unit tests

Dispatch-tree shape, why fern + file-rotate, timestamp formats, and decisions: `DETAILS.md`.

## Adding log calls

- **Use `log::*!` macros only.** `eprintln!` / `println!` / `dbg!` bypass the pipeline (no level filter, no file output,
  not in error-report bundles); clippy denies them crate-wide.
- **Always pass a scoped `target:`** so logs filter via `RUST_LOG` (`log::debug!(target: "open_with", …)` →
  `RUST_LOG=open_with=debug pnpm dev`) and earn their own terminal color. Without one the log gets the file's module
  path: workable but noisier. One short stable `target:` per subsystem, reused.
- **A per-event line with a varying payload (`attempt=27`, a name, a count) is a flood the coalescer can't catch**: its
  key is the exact line. Log per episode, or when the result changes. DETAILS § "Duplicate coalescing".

## Must-knows

- **Two independent chains; don't collapse them into a shared level.** The terminal chain (writes to `stderr`, so piped
  stdout stays clean) defaults to Info under `RUST_LOG` and noise overrides. The file chain is always Debug whatever the
  dev set, because error-report bundles need debug context.
- **The verbose toggle is an AtomicU8 read per record, NOT a dispatch rebuild** (a rebuild drops records mid-swap). It
  gates the terminal chain only; `RUST_LOG` sets the startup threshold. DETAILS § "Verbose toggle".
- **The file chain's ISO timestamp is prepended by `CoalescingWriter`, NOT the fern `.format()` closure.** ❌ Don't move
  `file_timestamp()` back into that format: the dedup key must stay timestamp-free or identical lines a millisecond
  apart never coalesce. DETAILS § "Duplicate coalescing".
- **ANSI is terminal-only, resolved once at startup.** ❌ `is_terminal()` can't decide it alone: under `pnpm dev` the
  Tauri CLI hands us a piped stderr, so `tauri-wrapper.ts` sets `CLICOLOR_FORCE`; `NO_COLOR` outranks both. DETAILS §
  "Terminal target column".
- **The RAM gauge is off unless `CMDR_LOG_RAM_USE` is truthy** (`ram_gauge::tag()` returns `""`, no alloc). When on, its
  ever-changing number lands in the file dedup key, so floods coalesce less: accepted debug-mode tradeoff.
- **Cap = 0 disables the file chain entirely** (`init` skips it). The terminal and the verbose toggle still work; the
  error bundle ships an empty `logs/`.
- **`file-rotate` bakes keep-N at startup; it can't be reconfigured live.** `set_keep_count` / `eager_prune` update the
  in-RAM count and delete excess files now, but restart-to-apply stands (DETAILS § "Cap changes at runtime").
- **Trust mtime, not the filename, for log ordering**: `file-rotate` uses `.1`, `.2`, … suffixes, not timestamps.
  Anything off the active-file pattern, legacy `Cmdr_<timestamp>.log` included, is swept at startup (DETAILS § "What
  lives in `mod.rs`").
- **`eager_prune(dir, 0)` wipes everything including the live file** (file-rotate re-creates it on the next write).
  That's correct when the user just disabled logging: stop capturing now, not at the next restart.
- **Log-dir resolution is mirrored in `settings::loader`'s `early_load_*` helpers**; a bundle-id change touches both
  (DETAILS § "Startup sequence").
