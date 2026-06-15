# Logging support module

Owns the log pipeline end to end via a hand-rolled `fern` Dispatch tree, chosen over `tauri-plugin-log` for **per-output
level filtering**: the file target is locked at Debug, the terminal defaults to Info.

## Module map

- **`mod.rs`**: resolved-log-dir `OnceLock<PathBuf>`, keep-count `AtomicUsize`, `set_log_dir` / `log_dir`,
  `set_keep_count` / `keep_count`, `list_recent_log_files`, `eager_prune`, `cleanup_legacy_log_files`
- **`dispatch.rs`**: `init` (builds + installs the fern tree), `set_stdout_threshold` / `stdout_threshold` (the verbose
  toggle knob, a single AtomicU8)
- **`tests.rs`**: pruner / listing helper unit tests

Dispatch-tree shape, why fern + file-rotate, timestamp formats, and decisions: [DETAILS.md](DETAILS.md).

## Adding log calls

- **Use `log::*!` macros only.** `eprintln!` / `println!` / `dbg!` bypass the pipeline (no level filtering, no file
  output, not in error-report bundles) and clippy denies them crate-wide.
- **Always pass a scoped `target:`** so logs filter via `RUST_LOG` (for example `log::debug!(target: "open_with", …)`,
  then `RUST_LOG=open_with=debug pnpm dev`). Without one, the log gets the file's module path: workable but noisier. For
  a new subsystem, pick a short stable `target:` and reuse it.

## Must-knows

- **Two independent chains.** The stdout chain (chains `stderr`, not stdout, so piped stdout stays clean) defaults to
  Info and is gated by the verbose-toggle AtomicU8 plus `RUST_LOG`/noise overrides. The file chain is always Debug when
  log storage is enabled, regardless of `RUST_LOG` or the verbose toggle, because error-report bundles need debug
  context independent of what the dev set. Don't collapse them into a shared level.
- **The verbose toggle is an AtomicU8 read per record, NOT a dispatch rebuild.** Rebuilding would drop records during
  the swap. `commands/logging.rs::set_log_level` calls `dispatch::set_stdout_threshold(Debug | Info)`; the file chain
  ignores the knob. `RUST_LOG` sets the startup stdout threshold and always wins at startup; the toggle takes over at
  runtime if the user flips it (it overwrites the AtomicU8 directly).
- **Cap = 0 disables the file chain entirely** (`init` skips it). stdout and the verbose toggle still work; the error
  reporter then produces a bundle with empty `logs/` (upload still goes through). The settings UI documents this.
- **`file-rotate` is configured once at startup; the keep-N value can't be reconfigured live.** `set_keep_count(n)`
  updates the in-RAM count and `eager_prune(dir, n)` deletes excess archived files now, but the rotator keeps its
  startup keep-N. Restart-to-apply is documented in the settings UI for `0 ↔ non-zero` and for raising the cap above the
  baked-in value.
- **Trust mtime, not the filename, for log ordering.** `list_recent_log_files` returns newest-first by mtime because
  `file-rotate` uses `.1`, `.2`, … suffixes (not timestamps). The active-file pattern is `^cmdr\.log(\.\d+)?$`
  (case-insensitive); anything else (legacy `Cmdr_<timestamp>.log`, typos, unrelated `.log`s) is rejected. Legacy files
  are removed on startup by `cleanup_legacy_log_files`.
- **`eager_prune(dir, 0)` wipes everything including the live file** (file-rotate re-creates it on the next write). This
  is correct for "user just disabled logging at runtime": stop capturing immediately rather than waiting for a restart.
- **Log-dir resolution must stay in sync across `lib.rs` and the settings early-load helpers.** `lib.rs` resolves it in
  `setup()`; `early_load_max_log_storage_mb` / `early_load_verbose_logging` (in `settings::loader`) mirror the
  `CMDR_DATA_DIR` fallback but use `dirs::data_dir` + bundle id rather than Tauri's `app_data_dir`. If the bundle id
  changes, update both.
