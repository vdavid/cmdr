# Logging support module

Owns the log pipeline: a hand-rolled `fern` Dispatch tree, chosen over `tauri-plugin-log` for **per-output level
filtering**: the file is locked at Debug, the terminal defaults to Info.

## Module map

- **`mod.rs`**: log-dir and keep-count state, debug-log path, log-file listing and pruning, `warn_once`
- **`startup.rs`**: `init`, the one call `lib.rs` makes at startup
- **`dispatch.rs`**: builds + installs the fern tree, the stdout threshold, both line formats
- **`provenance.rs`**: the `·` own-code tag on file lines, read back by error reports
- **`coalesce.rs`**: `CoalescingWriter`, the file chain's writer; collapses identical-line floods
- **`target_style.rs`**: the terminal target column. **`ram_gauge.rs`**: optional per-line RAM prefix
- **`rotating_file.rs`**: `RotatingFile`, the size rotation under `CoalescingWriter`

Dispatch-tree shape, why fern, timestamp formats, and decisions: `DETAILS.md`.

## Adding log calls

- **Use `log::*!` macros only.** `eprintln!` / `println!` / `dbg!` bypass the pipeline (no level filter, no file output,
  not in error-report bundles); clippy denies them crate-wide.
- **Always pass a scoped `target:`** so logs filter via `RUST_LOG` (`log::debug!(target: "open_with", …)` →
  `RUST_LOG=open_with=debug pnpm dev`) and earn their own terminal color. One short stable `target:` per subsystem.
- **Wrap each path, name, volume, host, or account in `cmdr_fs::log_private::LogPrivate`** so a report hashes it
  exactly (`docs/tooling/logging.md` § "Usage in Rust").
- **A per-event line with a varying payload (`attempt=27`, a name, a count) is a flood the coalescer can't catch**: its
  key is the exact line. Log per episode, or when the result changes. DETAILS § "Duplicate coalescing".

## Must-knows

- **Two independent chains; don't collapse them into a shared level.** The terminal chain (`stderr`, so piped stdout
  stays clean) defaults to Info under `RUST_LOG` and noise overrides. The file chain is Debug whatever the dev set,
  because error-report bundles need debug context; only `mdns_sd` is held at Info there (its dumps were most of it).
- **The file tags OUR lines (`·` before the target), from `record.module_path()`, ❌ never the target.** Error reports
  ship DEBUG/TRACE only from tagged lines, so an untagged line fails closed. DETAILS § "Line provenance".
- **The terminal hides log markers; the file keeps them.** `write_terminal_line` shows each marked value as itself
  (`crate::redact::display_line`), and so does Help > View debug log; the error reporter reads the markers from the
  file, so ❌ never strip them in the file chain. `../redact/DETAILS.md` § "Marked spans".
- **The verbose toggle is an AtomicU8 read per record, NOT a dispatch rebuild** (a rebuild drops records mid-swap). It
  gates the terminal chain only; `RUST_LOG` sets the startup threshold.
- **The file chain's ISO timestamp is prepended by `CoalescingWriter`, NOT the fern `.format()` closure.** ❌ Don't move
  `file_timestamp()` back: the dedup key must stay timestamp-free or identical lines a millisecond apart never coalesce.
- **ANSI is terminal-only, resolved once at startup.** ❌ `is_terminal()` can't decide it alone: under `pnpm dev` the
  Tauri CLI hands us a piped stderr, so `tauri-wrapper.ts` sets `CLICOLOR_FORCE`; `NO_COLOR` outranks both.
- **Cap = 0 disables the file chain entirely.** The terminal still works; the error bundle ships an empty `logs/`.
- **`RotatingFile` never panics or caches the directory's state**: logging runs on every thread, and other processes
  change the files (CRASH-EC4QS killed the index writer). It renames only the file its fd still holds
  (#812). ❌ `keep_count()` never means "this session writes a file": `debug_log_path()` does.
- **Trust mtime, not the filename, for log ordering.** `eager_prune(dir, 0)` wipes everything, the live file included.
- **`startup::init` runs after the instance-lock claim**: a refused instance never touches `cmdr.log` (#812).
- **The default log dir follows the build's identifier** (`config::app_log_dir`), ❌ never the hardcoded release one.
