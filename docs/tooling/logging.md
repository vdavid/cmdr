# Logging

Both frontend and backend logs appear in a single unified stream (terminal + log file) via `tauri-plugin-log`.

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
- **Deduplication**: Consecutive identical `(level, category, message)` tuples collapse into one with a `(xN, deduplicated)` suffix
- **Throttle**: Max 200 entries/second; excess entries are dropped with a warning

Logs appear in both the **browser console** (via LogTape's console sink) AND the **terminal/log file** (via the bridge).

### Log levels

From lowest to highest: `debug` < `info` < `warning` < `error` < `fatal`

### Default behavior

- **Dev mode**: Shows `info` and above
- **Prod mode**: Shows `error` and above

### Enabling debug logs for a feature

Edit `apps/desktop/src/lib/logging/logger.ts` and add the feature name to `debugCategories`:

```typescript
const debugCategories: string[] = [
    'fileExplorer',  // Now shows debug logs for this feature
]
```

## Backend (Rust)

Uses `tauri-plugin-log` (replaces `env_logger`) with `RUST_LOG` support via parsed `level_for()` directives. Same `log` facade API.

### Usage in Rust

```rust
use log::{debug, info, warn, error};

debug!("Loading path: {:?}", path);
info!("Loaded {} files", count);
warn!("Slow operation: {}ms", elapsed);
error!("Failed: {}", err);
```

### Enable debug for specific modules

`RUST_LOG` works exactly as before:

```bash
# Debug for network module only
RUST_LOG=cmdr_lib::network=debug pnpm dev

# Debug + suppress noisy SMB logs
RUST_LOG=cmdr_lib::network=debug,smb=warn,sspi=warn,info pnpm dev

# Trace everything (very verbose)
RUST_LOG=trace pnpm dev
```

## Log file

- **Location**: `~/Library/Logs/com.veszelovszki.cmdr/`
- Contains both Rust and frontend logs
- **Rotation**: 50 MB max, old files kept
- Accessible from **Settings > Logging > "Open log file"**

## Log format

```
10:19:34.908 DEBUG indexing::writer  Starting flush
10:19:34.912 INFO  FE:fileExplorer   Loaded 1,204 files
```

Format: `HH:MM:SS.mmm LEVEL target  message`. Frontend logs appear with an `FE:` prefix followed by the LogTape category name.

## RUST_LOG recipes

Copy-paste commands for common debugging scenarios. All include `info` as the base level.

| Area | Command |
|------|---------|
| Network/SMB | `RUST_LOG=cmdr_lib::network=debug,mdns_sd=debug,smb=warn,sspi=warn,info pnpm dev` |
| Drive indexing | `RUST_LOG=cmdr_lib::indexing=debug,info pnpm dev` |
| Indexing scanner only | `RUST_LOG=cmdr_lib::indexing::scanner=debug,info pnpm dev` |
| Indexing FSEvents | `RUST_LOG=cmdr_lib::indexing::watcher=debug,info pnpm dev` |
| File operations (copy/move/delete) | `RUST_LOG=cmdr_lib::file_system::write_operations=debug,info pnpm dev` |
| Directory listing | `RUST_LOG=cmdr_lib::file_system::listing=debug,info pnpm dev` |
| File viewer | `RUST_LOG=cmdr_lib::file_viewer=debug,FE:viewer=debug,info pnpm dev` |
| MTP (Android devices) | `RUST_LOG=cmdr_lib::mtp=debug,FE:mtp=debug,info pnpm dev` |
| AI/LLM | `RUST_LOG=cmdr_lib::ai=debug,info pnpm dev` |
| Licensing | `RUST_LOG=cmdr_lib::licensing=debug,info pnpm dev` |
| MCP server | `RUST_LOG=cmdr_lib::mcp=debug,info pnpm dev` |
| All frontend logs | `RUST_LOG=FE:=debug,info pnpm dev` |
| Specific FE feature | `RUST_LOG=FE:fileExplorer=debug,info pnpm dev` |
| Everything (noisy deps suppressed) | `RUST_LOG=debug,smb=warn,sspi=warn,mdns_sd=warn,hyper=warn pnpm dev` |

Frontend log targets use `FE:{category}` where category matches the `getAppLogger('category')` name.

## Verbose logging

Toggle in **Settings > Logging > "Verbose logging"**:

- Switches both frontend (LogTape) and backend (`log::set_max_level`) to debug level at runtime
- `RUST_LOG` env var overrides at startup for dev
