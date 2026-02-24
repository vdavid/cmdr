# Logging

Unified logging system. Both frontend (Svelte/TS) and backend (Rust) logs appear in the same terminal stream and log
file, with unified timestamps.

## File map

| File                 | Purpose                                                                                            |
| -------------------- | -------------------------------------------------------------------------------------------------- |
| `logger.ts`          | LogTape configuration, `getAppLogger()` entry point, verbose toggle, `debugCategories`             |
| `log-bridge.ts`      | Batching sink: collects FE logs for 100ms, deduplicates, throttles at 200/s, sends to Rust via IPC |
| `log-bridge.test.ts` | Vitest tests for bridge (batching, dedup, throttle)                                                |
| `index.ts`           | Barrel export                                                                                      |

Rust side lives in `src-tauri/src/commands/logging.rs` (batch IPC receiver + runtime level control).

## Architecture

```
getAppLogger('feature')
  -> LogTape (category filtering, level gates)
    -> console sink (browser dev tools)
    -> tauriBridge sink (log-bridge.ts)
        -> batch for 100ms, dedup consecutive identical entries, throttle 200/s
        -> invoke('batch_fe_logs', entries[])
          -> Rust: log::info!(target: "FE:feature", msg)
            -> tauri-plugin-log (fern)
              -> terminal (colored, custom short format)
              -> log file (~/Library/Logs/com.veszelovszki.cmdr/, 50 MB rotation)
            -> env_filter (RUST_LOG support, per-module filtering)
```

## Key decisions

- **LogTape kept on frontend**: Preserves the `getAppLogger()` API, hierarchical categories, and per-feature debug
  toggles (`debugCategories` array). Only the sink changed.
- **Custom batch IPC instead of plugin JS API**: `tauri-plugin-log`'s JS API sends one IPC per log. The bridge batches
  into one call per 100ms, with dedup and throttle -- critical for infinite-loop protection.
- **`env_filter` wraps `tauri-plugin-log`**: `tauri-plugin-log` doesn't support `RUST_LOG` natively. We use
  `skip_logger()` + `split()` to get the raw logger, wrap it with `env_filter::FilteredLog`, and register that as the
  global logger. This means `RUST_LOG=cmdr_lib::network=debug,info` works exactly like it did with `env_logger`.
- **Same level for terminal and file**: `tauri-plugin-log` doesn't support per-target level filtering. Both get the same
  level (Info by default, Debug when verbose toggle is on). Fine because the terminal isn't visible in production.

## Gotchas

- **FE logs are filterable via `RUST_LOG`**: Frontend logs use `FE:{category}` as the log target, so
  `RUST_LOG=FE:viewer=debug,info` works. See `docs/tooling/logging.md` for the full cheat sheet.
- **Dedup suffix**: Consecutive identical messages get ` (xN, deduplicated)` appended.
- **Throttle warning**: When >200 FE logs/s, excess is dropped and a warning is emitted: "Excessive frontend logging
  detected". This protects against infinite loops flooding the IPC.
- **`beforeunload` flush**: The bridge flushes remaining logs on page unload, but this is best-effort (async).
- **`debugCategories` is compile-time**: Adding a category requires editing `logger.ts` and restarting. The verbose
  logging toggle is runtime.

## Usage guide

See [docs/tooling/logging.md](../../../docs/tooling/logging.md) for how to add logging to your feature, `RUST_LOG`
recipes, and the verbose toggle.
