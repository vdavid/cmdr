# Logging

Unified logging: frontend (Svelte/TS) and backend (Rust) logs share one terminal stream and log file with unified
timestamps. The Rust side runs a fern dispatch tree with per-output level filtering (the file target stays at Debug
regardless of `RUST_LOG` or the verbose toggle; terminal defaults to Info).

## Module map

- **`logger.ts`**: LogTape config, `getAppLogger()` entry point, verbose toggle, `debugCategories`.
- **`log-bridge.ts`**: batching sink (collects FE logs for 100 ms, dedups, throttles at 200/s, sends to Rust via IPC).
- Rust side: `src-tauri/src/commands/logging.rs` (batch IPC receiver + runtime level control); the dispatch tree is in
  `src-tauri/src/logging/CLAUDE.md`.

Full architecture and decisions: [DETAILS.md](DETAILS.md). Usage (adding logging, `RUST_LOG` recipes, the verbose
toggle): [docs/tooling/logging.md](../../../docs/tooling/logging.md).

## Must-knows

- **FE logs use `FE:{category}` as their log target**, so `RUST_LOG=FE:viewer=debug,info` filters them on the Rust side.
- **`debugCategories` only affects the console sink** (browser devtools). The tauriBridge sink always sends debug+ to
  Rust in dev, so `RUST_LOG=FE:fileExplorer=debug,info` works without touching `debugCategories`. The verbose-logging
  toggle enables debug for both sinks.
- **The bridge dedups and throttles to protect against infinite-loop log floods.** Consecutive identical messages get
  ` (×N, deduplicated)` appended. Above 200 FE logs/s the excess is dropped with an "Excessive frontend logging
  detected" warning naming the top three dropped-from categories. Don't remove these guards; an unthrottled FE loop
  floods the IPC.
- **`beforeunload` flush is best-effort (async)**, so logs right before a page unload may not all reach Rust.
- **Error-report bundles (Help > Send error report…) include the file target's recent debug logs**, the same logs the
  cap setting governs. Keep the file chain at Debug.
