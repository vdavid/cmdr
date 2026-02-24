# Unified logging

Replace the split terminal/browser-console logging with a single stream via `tauri-plugin-log`.

## Problem

Rust logs go to the terminal (`env_logger`), frontend logs go to the browser dev tools (`LogTape` console sink).
Debugging anything that crosses the boundary means watching two windows and mentally correlating timestamps.
The ad-hoc `feLog()` IPC bridge (used by the viewer and updater) works but is manual and unstructured.

## Solution

1. **Rust side**: Replace `env_logger` with `tauri-plugin-log`. All `log::info!()` etc. calls stay unchanged (same `log`
   facade). Outputs to terminal (colored) + log file (rotated).
2. **Frontend side**: Keep LogTape for category-based filtering and the `getAppLogger()` API. Add a new LogTape sink
   that batches, deduplicates, throttles, and sends logs to Rust via a custom `batch_fe_logs` Tauri command.
3. **Result**: Both Rust and frontend logs appear in the same terminal stream and the same log file, with unified
   timestamps and a shorter format.

## Design

### Log line format

Before:

```
[2026-02-24T10:19:34.908Z DEBUG cmdr_lib::indexing::writer] Starting flush
```

After:

```
10:19:34.908 DEBUG indexing::writer  Starting flush
10:19:34.912 INFO  FE:fileExplorer   Loaded 1,204 files
```

Changes: drop full date (keep `HH:MM:SS.mmm`), drop `Z`, drop `cmdr_lib::` prefix, colored level.
Frontend logs get an `FE:` prefix with the LogTape category name.

Implementation: `tauri-plugin-log`'s `.format()` closure with `chrono::Local::now()` for the timestamp and
`.target().strip_prefix("cmdr_lib::")` for the module path.

### Terminal colors

Raw ANSI escape codes in the format closure (red = error, yellow = warn, green = info, cyan = debug, magenta = trace).
Note: `.with_colors()` overrides `.format()` in `tauri-plugin-log`, so colors must be handled inside the format closure.

### Log file

- **Location**: `~/Library/Logs/com.veszelovszki.cmdr/` (macOS default via `LogDir` target)
- **Rotation**: 50 MB max via `.max_file_size(50_000_000)` + `RotationStrategy::KeepAll`
- **Level**: Same as the global level (Info+ by default, Debug+ when verbose logging is toggled on)
- **Note on debug-always-to-file**: `tauri-plugin-log` doesn't support per-target level filtering. Having Debug+ always
  in the file would fill 50 MB fast during normal use and make it harder to find relevant info in bug reports. Info+ is
  the right default; users can toggle verbose logging when debugging, which switches both terminal and file to Debug+.
  Since the terminal isn't visible in production builds anyway, there's no downside to matching levels.

### Frontend batching layer (`log-bridge.ts`)

A LogTape sink that sits between `getAppLogger()` and the Rust IPC. Handles three concerns:

1. **Batching**: Collects log entries for 100 ms, then sends them in a single `batch_fe_logs` IPC call. Avoids
   per-message IPC overhead.
2. **Deduplication**: Within each batch window, if the same `(level, category, message)` tuple appears N times,
   collapse to a single entry with suffix ` (repeated Nx, deduplicated when batching)`.
3. **Throttle + warning**: Max 200 log entries per second (across all categories). When the cap is hit, drop excess
   entries and emit a single warning: `"Excessive frontend logging detected: {dropped} entries dropped in the last
   second. This may indicate a bug (infinite loop, runaway effect)."` This warning itself goes through the bridge so
   it appears in the terminal.

Flush also happens on `beforeunload` (sync flush of remaining buffer).

### Rust-side batch command

```rust
#[derive(Deserialize)]
struct FrontendLogEntry {
    level: String,     // "debug" | "info" | "warn" | "error"
    category: String,  // LogTape category, for example "fileExplorer"
    message: String,
}

#[tauri::command]
fn batch_fe_logs(entries: Vec<FrontendLogEntry>) {
    for entry in entries {
        let target = format!("FE:{}", entry.category);
        match entry.level.as_str() {
            "debug" => log::debug!(target: &target, "{}", entry.message),
            "info"  => log::info!(target: &target, "{}", entry.message),
            "warn"  => log::warn!(target: &target, "{}", entry.message),
            "error" => log::error!(target: &target, "{}", entry.message),
            _       => log::info!(target: &target, "{}", entry.message),
        }
    }
}
```

This re-emits each frontend log through the Rust `log` facade, so `tauri-plugin-log` handles it like any other Rust log
(same format, same targets, same file).

### Runtime level control

The existing "Verbose logging" toggle in settings calls `setVerboseLogging()`. Extend this to also call a new Tauri
command `set_log_level(level: String)` that calls `log::set_max_level(LevelFilter::Debug)` or `::Info`. This changes the
Rust-side filtering at runtime without restarting.

Frontend-side filtering continues to work via LogTape's own level config (the existing `applyLoggerConfig` logic).

`RUST_LOG` still works for dev-time startup override.

### What happens to `feLog()`

The ~25 call sites in `viewer/+page.svelte` and `updater.svelte.ts` get migrated to use `getAppLogger('viewer')` and
`getAppLogger('updater')` respectively. These then flow through the batching sink automatically. The `feLog` function,
the `fe_log` Tauri command, and the network.rs handler are all deleted.

### What happens to `attachConsole()`

We do **not** use `tauri-plugin-log`'s `attachConsole()` (which forwards Rust logs into the browser console). That would
make the browser console noisy with Rust internals. Frontend logs already appear in the browser console via LogTape's
console sink. Rust logs live in the terminal. Both appear in the log file.

### Settings UI update

`LoggingSection.svelte`'s "Open log file" button currently shows a placeholder alert. Update it to open the actual log
directory (`~/Library/Logs/com.veszelovszki.cmdr/`) via `tauri-plugin-opener`.

## Dependencies

### Add

- `tauri-plugin-log = "2"` (Rust)
- `chrono = "0.4"` (already a macOS dep, made unconditional for the format closure)

### Remove

- `env_logger = "0.11.8"` (Rust)
- `env_filter` (Rust, was added then removed — `RUST_LOG` parsing is done manually instead)

### Keep

- `log = "0.4"` (Rust, still the facade)
- `@logtape/logtape` (frontend, still the logger framework)

### Not needed

- `@tauri-apps/plugin-log` (npm) - we use a custom batch command instead of the plugin's JS API

## Tasks

### Milestone 1: Rust-side migration

- [x] Add `tauri-plugin-log` with `colored` feature to `Cargo.toml`
- [x] Make `chrono` unconditional (move from `[target.'cfg(target_os = "macos")'.dependencies]` to `[dependencies]`)
- [x] Replace `env_logger` init in `lib.rs` with `tauri-plugin-log` builder (format, colors, targets, rotation)
- [x] Remove `env_logger` from `Cargo.toml`
- [x] Verify all existing `log::` calls work unchanged (build + run)
- [x] Run `./scripts/check.sh --check clippy --check rustfmt --check cargo-deny`

### Milestone 2: Batch command + bridge

- [x] Add `batch_fe_logs` Tauri command and `set_log_level` command in a new `commands/logging.rs`
- [x] Register both commands in `lib.rs` invoke handler
- [x] Create `apps/desktop/src/lib/log-bridge.ts` with batching, dedup, and throttle logic
- [x] Add the bridge as a LogTape sink in `logger.ts` alongside the existing console sink
- [x] Write Vitest tests for the batching/dedup/throttle logic
- [x] Run `./scripts/check.sh --svelte`

### Milestone 3: Migrate `feLog` call sites

- [x] Replace `feLog()` calls in `viewer/+page.svelte` with `getAppLogger('viewer')`
- [x] Replace `feLog()` calls in `updater.svelte.ts` with `getAppLogger('updater')`
- [x] Delete `feLog` from `tauri-commands/networking.ts` and `fe_log` from `commands/network.rs`
- [x] Remove `fe_log` from `lib.rs` invoke handler registration
- [x] Run `./scripts/check.sh --check knip --check svelte-check`

### Milestone 4: Settings UI and runtime control

- [x] Wire `setVerboseLogging()` to also call `set_log_level` Tauri command
- [x] Update "Open log file" button to open `~/Library/Logs/com.veszelovszki.cmdr/` via `tauri-plugin-opener`
- [x] Test the toggle: info → debug → info, verify both terminal and file change levels

### Milestone 5: Docs and cleanup

- [x] Update `docs/tooling/logging.md` (new architecture, log file location, batch behavior, verbose toggle)
- [x] Update `AGENTS.md` debugging section (log file path, no more `feLog`)
- [x] Add `log-bridge.ts` to `coverage-allowlist.json` if it depends on Tauri IPC (can't run in Vitest without mocks)
- [x] Run full `./scripts/check.sh`
