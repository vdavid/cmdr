# Logging

Unified logging: frontend (Svelte/TS) and backend (Rust) logs share one terminal stream and log file with unified
timestamps. The Rust side's fern dispatch tree, with its per-output level filtering, is documented in
`apps/desktop/src-tauri/src/logging/CLAUDE.md`.

## Module map

- **`logger.ts`**: LogTape config, `getAppLogger()` entry point, verbose toggle, `debugCategories`.
- **`log-bridge.ts`**: batching sink (collects FE logs for 100 ms, dedups, throttles at 200/s, sends to Rust via IPC).
- **`uncaught-errors.ts`**: forwards `window` `error` / `unhandledrejection` to `log.error` under the `uncaught`
  category. Registered from `routes/+layout.ts`.
- Rust side: `src-tauri/src/commands/logging.rs` (batch IPC receiver + runtime level control); the dispatch tree is in
  `src-tauri/src/logging/CLAUDE.md`.

Usage (adding logging, `RUST_LOG` recipes, the verbose toggle): `docs/tooling/logging.md`.

## Must-knows

- **FE logs use `FE:{category}` as their log target**, so `RUST_LOG=FE:viewer=debug,info` filters them on the Rust side.
- **`debugCategories` only affects the console sink** (browser devtools). The tauriBridge sink always sends debug+ to
  Rust in dev, so `RUST_LOG=FE:fileExplorer=debug,info` works without touching `debugCategories`. The verbose-logging
  toggle enables debug for both sinks.
- **The bridge forwards the rendered message and nothing else, so every property must appear in the template.**
  `FrontendLogEntry` is level + category + message; nothing reads `record.properties`. A field the message never names
  is discarded at the IPC boundary and reaches neither the log file nor a bundle. `cmdr/no-unrendered-log-fields`
  enforces it at every level (`debug` and `info` drop properties the same way). Note `String(obj)` renders
  `[object Object]`, so interpolate a field you can read: `{error.message}`, or a pre-stringified value.
- **The bridge dedups and throttles to protect against infinite-loop log floods.** Consecutive identical messages get
  ` (×N, deduplicated)` appended. Above 200 FE logs/s the excess is dropped with an "Excessive frontend logging
  detected" warning naming the top three dropped-from categories. Don't remove these guards; an unthrottled FE loop
  floods the IPC.
- **An uncaught frontend throw is only visible because `uncaught-errors.ts` forwards it.** Nothing else does: the
  console is unread under Tauri, so without those two listeners a crash leaves no line in the log file, nothing in an
  error-report bundle, and nothing in a CI E2E run. The listeners deliberately don't `preventDefault`: they observe,
  they never swallow (which would also blind `hmr-recovery`). ❗ WebKit's `error.stack` carries FRAMES ONLY, so the
  message is put back in front of it; ❌ never log a stack verbatim here.
- **`beforeunload` flush is best-effort (async)**, so logs right before a page unload may not all reach Rust.
- **Error-report bundles (Help > Send error report…) include the file target's recent debug logs**, the same logs the
  cap setting governs. Keep the file chain at Debug.

Architecture and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or
advising.
