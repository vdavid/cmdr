# Logging details

Depth and rationale for the frontend logging bridge. `CLAUDE.md` holds the must-knows. The Rust dispatch tree is
documented in `src-tauri/src/logging/CLAUDE.md` (canonical home for the fern/per-output mechanism).

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

## Decisions

- **LogTape kept on the frontend**: preserves the `getAppLogger()` API, hierarchical categories, and per-feature debug
  toggles (`debugCategories`). Only the sink changed.
- **Custom batch IPC instead of the plugin JS API**: the bridge batches into one IPC call per 100 ms, with dedup and
  throttle (critical for infinite-loop protection).
- **Hand-rolled fern dispatch instead of `tauri-plugin-log`**: the plugin routes everything through one shared level. We
  need per-output filtering (file at Debug for error reports, terminal at Info for clean dev output). fern's tree of
  `Dispatch` chains makes this trivial. Full mechanism in `src-tauri/src/logging/CLAUDE.md`.
- **`RUST_LOG` parsed into `level_for()` on the stdout chain only**: the file chain stays Debug regardless, so
  `RUST_LOG=cmdr_lib::network=debug,smb=warn,info` controls the terminal without affecting error-report bundle content.
- **`developer.verboseLogging`**: with per-output filtering, the toggle bumps the stdout chain from Info to Debug at
  runtime via an `AtomicU8` (no dispatch rebuild, no records lost). The file target stays Debug, so error-report content
  is unchanged.
- **`file-rotate` for size+count rotation**: a small crate exposing rotation behind a `Write` impl. Keep-N is
  `ceil(cap_mb / 50)`. The eager prune on cap-lowered events still runs so the user sees excess files vanish
  immediately.
- **Restart-required for 0 ↔ non-zero cap transitions**: `file-rotate` is constructed once at startup with its keep-N
  value, and the file chain is either present or absent from the dispatch tree, so toggling the cap between `0` and any
  non-zero value needs an app restart. The settings UI shows a "Restart Cmdr to apply" toast for these transitions.
