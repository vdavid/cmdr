# Logging

Both frontend and backend logs appear in a single unified stream (terminal + log file). The Rust side runs a hand-rolled
`fern` dispatch tree with **per-output level filtering**: file always at Debug, terminal defaults to Info.

## Frontend (Svelte/TypeScript)

Uses [LogTape](https://logtape.org/) with a batching bridge to the Rust backend.

### Usage

```typescript
import { getAppLogger } from '$lib/logging/logger'

const log = getAppLogger('fileExplorer')
log.debug('Loading directory {path}', { path })
log.info('Loaded {count} files', { count: files.length })
log.warn('Large directory: {count} items', { count })
log.error('Failed to load: {error}', { error: err.message })
```

### How it works

Logs are sent to the Rust backend via a batching bridge (`log-bridge.ts`):

- **Batching**: Collects entries for 100ms, sends in a single IPC call
- **Deduplication**: Consecutive identical `(level, category, message)` tuples collapse into one with a
  `(xN, deduplicated)` suffix
- **Throttle**: Max 200 entries/second; excess entries are dropped with a warning

Logs appear in both the **browser console** (via LogTape's console sink) AND the **terminal/log file** (via the bridge).

### Log levels

From lowest to highest: `debug` < `info` < `warning` < `error` < `fatal`

### Default behavior

- **Dev mode (terminal)**: Shows `info` and above by default. `RUST_LOG` overrides per-module. The verbose toggle bumps
  the whole terminal to `debug` at runtime.
- **Prod mode (terminal)**: Terminal isn't visible in shipped builds.
- **File target**: Always `debug` when log storage is enabled (cap > 0). Absent when the cap is `0`. The file target is
  independent: it ignores `RUST_LOG` and the verbose toggle, so error report bundles always carry the full debug
  context.

## Log levels

- **ERROR**: Something broke. Needs developer or user action.
- **WARN**: Unexpected but handled, or a degraded state worth noticing.
- **INFO**: Noteworthy lifecycle events. App started, operation canceled, errors that are part of normal functioning.
  Only a few selected interesting events, without noise.
- **DEBUG**: Routine operational details. Individual file copies, per-item MTP operations, intermediate steps. Disabled
  by default. Enable per scope with `RUST_LOG=module=debug` when investigating.
- **TRACE**: Protocol-level internals, full data structures, per-iteration details

### Enabling debug logs for a feature

Use `RUST_LOG` to enable FE debug logs in the terminal (no code changes needed):

```bash
RUST_LOG=FE:fileExplorer=debug,info pnpm dev
```

To also see debug logs in browser devtools, add the feature to `debugCategories` in
`apps/desktop/src/lib/logging/logger.ts`:

```typescript
const debugCategories: string[] = [
  'fileExplorer', // Now shows debug logs in browser devtools too
]
```

## Backend (Rust)

Hand-rolled `fern` dispatch tree (see `apps/desktop/src-tauri/src/logging/CLAUDE.md`) with `RUST_LOG` parsed into
per-module overrides on the stdout chain only. Same `log` facade API.

### Usage in Rust

```rust
use log::{debug, info, warn, error};

debug!("Loading path: {:?}", path);
info!("Loaded {} files", count);
warn!("Slow operation: {}ms", elapsed);
error!("Failed: {}", err);
```

### Enable debug for specific modules

**Important**: The crate name in `Cargo.toml` is `cmdr`, but the Rust library target compiles as `cmdr_lib`. Always use
`cmdr_lib::` as the module prefix in `RUST_LOG`, not `cmdr::`. Using `cmdr::` silently matches nothing.

```bash
# Debug for network module only
RUST_LOG=cmdr_lib::network=debug pnpm dev

# Debug + suppress noisy SMB logs
RUST_LOG=cmdr_lib::network=debug,smb=warn,sspi=warn,info pnpm dev

# Trace everything (very verbose)
RUST_LOG=trace pnpm dev
```

## Log file

- **Location**: Prod: `~/Library/Logs/com.veszelovszki.cmdr/`, Dev:
  `~/Library/Application Support/com.veszelovszki.cmdr-dev/logs/`
- Contains both Rust and frontend logs
- **Level**: Always Debug for the file target. The dispatch tree filters per output, so the file's Debug level is
  independent of `RUST_LOG` and the verbose toggle (those only affect the terminal). Error reports always get full debug
  context.
- **Rotation**: 50 MB per file, keep-N where `N = ceil(cap_mb / 50)`. Backed by the `file-rotate` crate.
- **Cap**: `Advanced > Maximum disk space for log files (MB)`, default 200 MB, range 0–5000. Set to `0` to disable log
  storage entirely. Error reports cannot be sent without logs. Lowering the cap at runtime eagerly prunes excess files.
  `0 ↔ non-zero` transitions (and raising the cap beyond its baked-in value) require an app restart.
- Accessible from **Settings > Logging > "Open log file"**, and bundled into error reports sent via **Help > Send error
  report…** (passes through the shared redactor first)

## Log format

```
10:19:34.908 DEBUG indexing::writer  Starting flush
10:19:34.912 INFO  FE:fileExplorer   Loaded 1,204 files
```

Format: `HH:MM:SS.mmm LEVEL target  message`. Frontend logs appear with an `FE:` prefix followed by the LogTape category
name.

## RUST_LOG recipes

Copy-paste commands for common debugging scenarios. All include `info` as the base level.

- **Network/SMB**: `RUST_LOG=cmdr_lib::network=debug,mdns_sd=debug,smb=warn,sspi=warn,info pnpm dev`
- **Drive indexing**: `RUST_LOG=cmdr_lib::indexing=debug,info pnpm dev`
- **Indexing scanner only**: `RUST_LOG=cmdr_lib::indexing::scanner=debug,info pnpm dev`
- **Indexing FSEvents**: `RUST_LOG=cmdr_lib::indexing::watch::watcher=debug,info pnpm dev`
- **Per-subtree churn** (needs `CMDR_CHURN_SPIKE`, see below): `RUST_LOG=cmdr_lib::indexing::churn=debug,info pnpm dev`
- **File operations (copy/move/delete)**: `RUST_LOG=cmdr_lib::file_system::write_operations=debug,info pnpm dev`
- **Directory listing**: `RUST_LOG=cmdr_lib::file_system::listing=debug,info pnpm dev`
- **File viewer**: `RUST_LOG=cmdr_lib::file_viewer=debug,FE:viewer=debug,info pnpm dev`
- **MTP (Android devices)**: `RUST_LOG=cmdr_lib::mtp=debug,FE:mtp=debug,info pnpm dev`
- **Volume discovery + broadcast**:
  `RUST_LOG=cmdr_lib::volume_broadcast=debug,cmdr_lib::volumes::watcher=debug,info pnpm dev`
- **AI/LLM**: `RUST_LOG=cmdr_lib::ai=debug,info pnpm dev`
- **Licensing**: `RUST_LOG=cmdr_lib::licensing=debug,info pnpm dev`
- **MCP server**: `RUST_LOG=cmdr_lib::mcp=debug,info pnpm dev`
- **All frontend logs**: `RUST_LOG=FE:=debug,info pnpm dev`
- **Specific FE feature**: `RUST_LOG=FE:fileExplorer=debug,info pnpm dev`
- **Everything (noisy deps suppressed)**: `RUST_LOG=debug,smb=warn,sspi=warn,mdns_sd=warn,hyper=warn pnpm dev`

Frontend log targets use `FE:{category}` where category matches the `getAppLogger('category')` name.

## RAM gauge (`CMDR_LOG_RAM_USE`)

Set `CMDR_LOG_RAM_USE=1` to prefix every log line with the backend's current memory use, right after the level:

```
2026-07-16T22:56:21.829+02:00 DEBUG (374 MB) smb2::client::tree  tree: fs_info done, total=...
```

```bash
CMDR_LOG_RAM_USE=1 pnpm dev
# combine with RUST_LOG to zoom in on a suspect subsystem:
CMDR_LOG_RAM_USE=1 RUST_LOG=cmdr_lib::indexing=debug,info pnpm dev
```

Use it to keep RAM at bay: the inline number turns the log into a memory timeline, so you can see which operation
coincided with a jump. It answers _when and near what_, not _what allocated_ (reach for Instruments or a heap profiler
for that). Works in dev, E2E, and prod builds. Accepts `1`/`true`/`yes`/`on`.

The number is `phys_footprint` (Activity Monitor's "Memory" metric, not RSS) of the Rust backend process only, sampled
every 100 ms. Cmdr's WebView runs in separate processes, so it's not included. Mechanism and the file-dedup caveat:
`apps/desktop/src-tauri/src/logging/DETAILS.md` § "RAM gauge".

## Churn instrumentation (`CMDR_CHURN_SPIKE`)

Set `CMDR_CHURN_SPIKE=1` to make the live FSEvents loop log per-subtree churn, rolled up the ancestor chain, on the
`indexing::churn` target at Debug. It's read-only: it writes no index state and changes no behaviour, and it costs
nothing when the variable is unset.

```bash
CMDR_CHURN_SPIKE=1 pnpm dev
```

Two knobs: `CMDR_CHURN_SPIKE_PERIOD_S` (rollup period, default `30`) and `CMDR_CHURN_SPIKE_TOP_N` (directories logged
per period, default `40`). Output is `1 + top_n` lines per period per volume.

**It only records while a live event loop runs**, so it's silent during a scan or rescan. That's the instrument being
honest, not broken. Line format, the offline analyser, and how to read a collection:
[`/docs/notes/churn-observability-spike.md`](../notes/churn-observability-spike.md).

## Verbose logging

Toggle in **Settings > Advanced > Logging > "Verbose console output (developer)"**:

- Flips frontend (LogTape) debug gating for the browser devtools console.
- Bumps the Rust **stdout chain** from Info to Debug (and back). The file chain stays at Debug regardless, so error
  reports are unaffected by the toggle.
- Implemented via an `AtomicU8` consulted on every record: the toggle takes effect mid-stream without rebuilding the
  logger, so no records are lost during the swap.
- `RUST_LOG` always wins at startup. The toggle takes over at runtime if the user clicks it (it overwrites the atomic
  directly).
