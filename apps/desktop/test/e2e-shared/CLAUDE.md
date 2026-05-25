# E2E shared helpers

Modules used by both the macOS Playwright suite and the Linux Docker E2E suite. Anything that needs to run BEFORE the
Tauri binary launches (fixture creation, port-file reads, MCP client setup) lives here.

## Files

| File               | Purpose                                                                                                                                                                                                                                                                                   |
| ------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `fixtures.ts`      | Builds the on-disk fixture tree the app opens at startup. macOS Playwright path: per-instance root at `/tmp/cmdr-e2e-fixtures-<instance>-<ts>/` with bulk `.dat` files hardlinked from a shared cache at `/tmp/cmdr-e2e-fixtures-cache/`. Linux Docker path: shared `/tmp/cmdr-e2e-<ts>/` |
| `fixtures.test.ts` | Vitest suite for the fixture builder. Covers cache population race, hardlink cross-shard sharing, `EXDEV` fallback, recreate-text-files contract, legacy single-shard path                                                                                                                |
| `port-file.ts`     | Reads `<data_dir>/mcp.port` and `<data_dir>/tauri-mcp.port` written by the Rust side and the wrapper. Exports `resolveMcpPort(dataDir)` with the canonical precedence: `CMDR_MCP_PORT` env → port file → throw `PortDiscoveryError`. Never falls back to the legacy 19224/19225           |
| `mcp-client.ts`    | Lightweight MCP client wrapping `fetch` to the Cmdr MCP server. Spec files use this for tool calls and resource reads without re-implementing JSON-RPC                                                                                                                                    |
| `mtp-fixtures.ts`  | Virtual MTP backing-dir composition for the MTP shard. Backed by `/tmp/cmdr-mtp-e2e-fixtures/` (one shared dir; MTP shard runs serialized for this reason)                                                                                                                                |
| `smb-fixtures.ts`  | SMB virtual-host fixtures for the SMB E2E feature. Injects into the running Tauri process via the `smb-e2e` Cargo feature                                                                                                                                                                 |

## Fixture builder contract

The on-disk layout `fixtures.ts` produces (relative to the per-instance fixture root):

```
left/                         right/  (empty)
  file-a.txt, file-b.txt
  sub-dir/nested-file.txt
  .hidden-file
  bulk/  (3 x 50 MB + 20 x 1 MB .dat files)
```

Text files are full copies because tests mutate them. Bulk `.dat` files are zero-filled, deterministic, and hardlinked
from the cache. `recreateFixtures()` (called in `beforeEach` of mutating specs) restores the text files; bulk files
survive across tests since they're read-only.

## Hardlink cache protocol

The cache is built once at `/tmp/cmdr-e2e-fixtures-cache/`. Two concurrent E2E runs from two worktrees both finding the
cache missing: each writes to its own `/tmp/cmdr-e2e-fixtures-cache-tmp-<pid>/`, populates the .dat files (deterministic
zero-fill), verifies via size + content check, then atomically `renameSync` to `/tmp/cmdr-e2e-fixtures-cache/`. The
loser of the rename race cleans up its tmp dir. The cache's existence means "populated and verified," so torn writes are
structurally impossible. On `EXDEV` (cross-filesystem hardlink), falls back to copy with a warning.

Source of truth: `populateCacheIfMissing()` in [`fixtures.ts`](fixtures.ts). The Vitest suite covers the race scenarios
deterministically.

## Port discovery contract

For external readers (CLI tools, test fixtures), port files are the canonical discovery channel:

1. `CMDR_MCP_PORT` env (manual pin, set by the Go checker per shard) takes precedence.
2. Otherwise, read `<data_dir>/mcp.port` for the Cmdr MCP HTTP server, or `<data_dir>/tauri-mcp.port` for the Tauri MCP
   bridge plugin.
3. Never silently fall back to a hardcoded default. Throw `PortDiscoveryError`.

The Rust side writes `mcp.port` after `bind()` via tempfile + fsync + rename. The wrapper writes `tauri-mcp.port` BEFORE
Tauri launches (the plugin has no public bound-port accessor; the wrapper reserves the port and tells the world). See
`docs/tooling/instance-isolation.md` § "Per-resource breakdown" for the full design.

## Key decisions

**Decision**: per-instance fixture root with hardlinks instead of full copies. **Why**: copying 170 MB × N shards × M
concurrent runs blows past `/tmp` quotas and adds seconds to every E2E launch. Hardlinks are zero-cost after the first
populate; tests treat the files as read-only.

**Decision**: text files are NOT cached: full copies per shard. **Why**: `file-operations.spec.ts` and similar mutate
them. Recreating from a small in-memory template costs less than tracking which files got mutated and re-syncing from
the cache.

**Decision**: port-file read NEVER falls back to legacy ports silently. **Why**: a silent fallback hides bugs (the test
"works" but against the wrong instance). The strict precedence ladder (env → file → typed error) makes
mis-configurations loud.

## Gotchas

- **`createFixtures(instanceId)` is the only API.** Pass an instance ID on macOS for the per-shard path + hardlink
  cache. Pass `undefined` (or omit) on Linux Docker for the legacy shared path. Both paths return the fixture root for
  `CMDR_E2E_START_PATH`.
- **Bulk file content is zero-fill ASCII.** Tests that need real binary patterns must add their own fixtures or write to
  text files. The cache check is size-based + content-hash sampled at a few offsets; arbitrary content wouldn't survive
  the deterministic-cache contract.
- **`mcp-client.ts` and `mtp-fixtures.ts` predate instance isolation.** They don't read the port file; they're invoked
  from inside the running app via Tauri IPC where the in-process `MCP_ACTUAL_PORT` atomic is the source of truth.
  Out-of-process callers use `port-file.ts`.

## Related docs

- [`apps/desktop/test/CLAUDE.md`](../CLAUDE.md): E2E suite overview.
- [`apps/desktop/test/e2e-playwright/CLAUDE.md`](../e2e-playwright/CLAUDE.md): Playwright-specific conventions,
  including the clipboard-mock gotcha.
- [`apps/desktop/test/e2e-linux/CLAUDE.md`](../e2e-linux/CLAUDE.md): Linux Docker single-shard contract.
- [`docs/tooling/instance-isolation.md`](../../../../docs/tooling/instance-isolation.md): canonical reference for the
  per-instance primitive.
- [`apps/desktop/src-tauri/src/mcp/port_file.rs`](../../src-tauri/src/mcp/port_file.rs): the Rust side of the port-file
  protocol.
