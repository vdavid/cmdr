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
            -> fern Dispatch tree (src-tauri/src/logging/DETAILS.md has its shape)
```

## Decisions

- **LogTape on the frontend**: it gives the `getAppLogger()` API, hierarchical categories, and per-feature debug
  toggles (`debugCategories`); only the sink is ours.
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
- **Rotation, the log-size cap, and the restart-to-apply transitions are the Rust side's**, single-sourced in
  `apps/desktop/src-tauri/src/logging/DETAILS.md`. Nothing on the frontend reads or reconfigures them.
