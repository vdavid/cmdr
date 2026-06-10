# Logging

Unified logging system. Both frontend (Svelte/TS) and backend (Rust) logs appear in the same terminal stream and log
file, with unified timestamps. The Rust side runs a hand-rolled `fern` dispatch tree with **per-output level filtering**
: file target stays at Debug regardless of `RUST_LOG` or the verbose toggle, terminal defaults to Info.

## File map

| File                 | Purpose                                                                                            |
| -------------------- | -------------------------------------------------------------------------------------------------- |
| `logger.ts`          | LogTape configuration, `getAppLogger()` entry point, verbose toggle, `debugCategories`             |
| `log-bridge.ts`      | Batching sink: collects FE logs for 100ms, deduplicates, throttles at 200/s, sends to Rust via IPC |
| `log-bridge.test.ts` | Vitest tests for bridge (batching, dedup, throttle)                                                |

Rust side lives in `src-tauri/src/commands/logging.rs` (batch IPC receiver + runtime level control).

## Architecture

```
getAppLogger('feature')
  -> LogTape (separate level gates per sink)
    -> console sink (browser devtools): info+ default, debug for debugCategories
    -> tauriBridge sink: debug+ in dev (RUST_LOG filters on Rust side), error+ in prod
        -> batch for 100ms, dedup consecutive identical entries, throttle 200/s
        -> invoke('batch_fe_logs', entries[])
          -> Rust: log::info!(target: "FE:feature", msg)
            -> fern Dispatch tree (logging::dispatch)
              ├── stdout chain (Info default; RUST_LOG / verbose toggle bump it)
              │     -> stderr (colored, HH:MM:SS.mmm LEVEL target  message)
              └── file chain (Debug always; absent when cap = 0)
                    -> file-rotate (50 MB per file, keep-N rotation, plain text)
```

## Key decisions

- **LogTape kept on frontend**: Preserves the `getAppLogger()` API, hierarchical categories, and per-feature debug
  toggles (`debugCategories` array). Only the sink changed.
- **Custom batch IPC instead of plugin JS API**: The bridge batches into one IPC call per 100ms, with dedup and throttle
  (critical for infinite-loop protection).
- **Hand-rolled fern dispatch instead of `tauri-plugin-log`**: The plugin routes everything through one shared level. We
  needed per-output filtering: file at Debug for error reports, terminal at Info for clean dev output. fern's tree of
  `Dispatch` chains makes this trivial; the plugin made it impossible. See
  `apps/desktop/src-tauri/src/logging/CLAUDE.md`.
- **RUST_LOG parsed into `level_for()` calls on the stdout chain only**: same parsing as before, but applied only to
  stdout. The file chain stays Debug regardless of RUST_LOG. This means `RUST_LOG=cmdr_lib::network=debug,smb=warn,info`
  controls what the dev sees in the terminal without affecting what error report bundles capture.
- **`developer.verboseLogging` is meaningful again**: With per-output filtering in place, the toggle now bumps the
  stdout chain from Info to Debug at runtime via an `AtomicU8` (no dispatch rebuild, no records lost). The file target
  stays Debug regardless, so error report content is unchanged. Frontend LogTape gating works as before.
- **file-rotate for size+count rotation**: small, focused crate exposing rotation behind a `Write` impl. Replaced the
  plugin's built-in rotation. Keep-N is `ceil(cap_mb / 50)`, same math as before. The eager-prune on cap-lowered events
  still runs so the user sees excess files vanish immediately.
- **Restart-required for 0 ↔ non-zero transitions**: `file-rotate` is constructed once at startup with its keep-N value.
  Changing the cap between `0` and any non-zero value requires an app restart (the file chain is either present or
  absent from the dispatch tree). The settings UI shows a "Restart Cmdr to apply" toast for these transitions.

## Gotchas

- **FE logs are filterable via `RUST_LOG`**: Frontend logs use `FE:{category}` as the log target, so
  `RUST_LOG=FE:viewer=debug,info` works. See `docs/tooling/logging.md` for the full cheat sheet.
- **Dedup suffix**: Consecutive identical messages get ` (xN, deduplicated)` appended.
- **Throttle warning**: When >200 FE logs/s, excess is dropped and a warning is emitted: "Excessive frontend logging
  detected", including the top three dropped-from categories (for example `top: fileExplorer ×30, search ×10`) so the
  offending feature is identifiable from the log alone. This protects against infinite loops flooding the IPC.
- **`beforeunload` flush**: The bridge flushes remaining logs on page unload, but this is best-effort (async).
- **`debugCategories` only affects the console sink**: The tauriBridge sink always sends debug+ to Rust in dev mode, so
  `RUST_LOG=FE:fileExplorer=debug,info` works without touching `debugCategories`. `debugCategories` controls which
  features get debug in browser devtools. The verbose logging toggle enables debug for both sinks.

## Usage guide

See [docs/tooling/logging.md](../../../docs/tooling/logging.md) for how to add logging to your feature, `RUST_LOG`
recipes, and the verbose toggle.

When debugging issues, the error report bundle from the Help menu (**Help > Send error report…**) includes recent
debug-level logs from the file target, the same logs the cap setting governs.
