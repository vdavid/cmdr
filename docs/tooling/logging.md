# Logging

Both frontend and backend use structured logging with configurable levels per feature.

## Frontend (Svelte/TypeScript)

Uses [LogTape](https://logtape.org/) - zero dependencies, browser-native, TypeScript-first.

### Usage

```typescript
import { getAppLogger } from '$lib/logger'

const log = getAppLogger('fileExplorer')
log.debug('Loading directory {path}', { path })
log.info('Loaded {count} files', { count: files.length })
log.warn('Large directory: {count} items', { count })
log.error('Failed to load: {error}', { error: err.message })
```

### Log levels

From lowest to highest: `debug` < `info` < `warning` < `error` < `fatal`

### Default behavior

- **Dev mode**: Shows `info` and above
- **Prod mode**: Shows `error` and above

### Enabling debug logs for a feature

Edit `apps/desktop/src/lib/logger.ts` and add the feature name to `debugCategories`:

```typescript
const debugCategories: string[] = [
    'fileExplorer',  // Now shows debug logs for this feature
]
```

## Backend (Rust)

Uses `env_logger` with `RUST_LOG` environment variable.

### Default

Info level: `RUST_LOG=info` (implicit default)

### Enable debug for specific modules

```bash
# Debug for network module only
RUST_LOG=cmdr::network=debug pnpm dev

# Debug for multiple modules
RUST_LOG=cmdr::network=debug,cmdr::file_system=debug pnpm dev

# Trace everything (very verbose)
RUST_LOG=trace pnpm dev
```

### Usage in Rust

```rust
use log::{debug, info, warn, error};

debug!("Loading path: {:?}", path);
info!("Loaded {} files", count);
warn!("Slow operation: {}ms", elapsed);
error!("Failed: {}", err);
```
