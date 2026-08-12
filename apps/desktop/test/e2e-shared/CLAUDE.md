# E2E shared helpers

Modules used by both the macOS Playwright suite and the Linux Docker E2E suite. Anything that must run BEFORE the Tauri
binary launches (fixture creation, port-file reads, MCP client setup) lives here.

## Files

- **`fixtures.ts`**: builds the on-disk fixture tree the app opens at startup. macOS Playwright path: per-instance root
  at `/tmp/cmdr-e2e-fixtures-<instance>-<ts>/` with bulk `.dat` files hardlinked from a shared cache at
  `/tmp/cmdr-e2e-fixtures-cache/`. Linux Docker path: shared `/tmp/cmdr-e2e-<ts>/`.
- **`fixture-manifest.ts`**: the shared-fixture drift guard. `diffFixtureTree` compares `left/` + `right/` against the
  declared layout, `restoreFixtureTree` repairs only what drifted; the Playwright `afterEach` runs both.
- **`port-file.ts`**: reads `<data_dir>/mcp.port` and `<data_dir>/tauri-mcp.port`. `resolveMcpPort(dataDir)` follows the
  canonical precedence and never falls back to legacy ports.
- **`mcp-client.ts`**: lightweight MCP client wrapping `fetch` to the Cmdr MCP server (tool calls, resource reads).
- **`mtp-fixtures.ts`**: virtual MTP backing-dir composition. Backed by one shared `/tmp/cmdr-mtp-e2e-fixtures/` (MTP
  shard runs serialized for this reason).
- **`smb-fixtures.ts`**: SMB virtual-host fixtures, injected into the running Tauri process via the `smb-e2e` feature.

## Must-knows

- **`createFixtures(instanceId)` is the only fixture API.** Pass an instance ID on macOS for the per-shard path +
  hardlink cache; pass `undefined` (or omit) on Linux Docker for the legacy shared path. Both return the fixture root
  for `CMDR_E2E_START_PATH`.
- **Port-file read NEVER falls back to a hardcoded default.** Strict precedence: `CMDR_MCP_PORT` env (manual pin, set by
  the Go checker per shard) → `<data_dir>/mcp.port` (Cmdr MCP HTTP server) or `tauri-mcp.port` (Tauri MCP bridge) →
  throw `PortDiscoveryError`. A silent fallback hides bugs (the test "works" against the wrong instance). The Rust side
  writes `mcp.port` after `bind()` via tempfile + fsync + rename; the wrapper writes `tauri-mcp.port` BEFORE Tauri
  launches (the plugin has no public bound-port accessor).
- **`mcp-client.ts` and `mtp-fixtures.ts` predate instance isolation: they don't read the port file.** They're invoked
  from inside the running app via Tauri IPC, where the in-process `MCP_ACTUAL_PORT` atomic is the source of truth.
  Out-of-process callers use `port-file.ts`.
- **Bulk `.dat` files are zero-fill ASCII, hardlinked, and treated as read-only.** Tests needing real binary patterns
  must add their own fixtures or write to text files; the cache check is size-based + content-hash sampled at a few
  offsets, so arbitrary content wouldn't survive the deterministic-cache contract.
- **❌ Never write to a bulk `.dat` IN PLACE.** They're hardlinks into the cache, so one `truncateSync` shortens it in
  every live fixture tree on the machine at once. The cache self-heals on the next build; those trees do NOT, and the
  leak guard then blames a random innocent spec. Remove and rewrite, as `restoreBulkFile` does.
- **Text files are full copies, recreated per test; bulk files are NOT.** `recreateFixtures()` (called in `beforeEach`
  of mutating specs) restores the text files and re-copies the committed `media-fixtures/` (`sample.png` 2×2 RGBA,
  `sample.pdf` 1 page) into `left/`; bulk files survive across tests since they're read-only.

The cache's build protocol (why a torn cache is structurally impossible, and the `EXDEV` fallback): `DETAILS.md` §
"Hardlink cache protocol".

## Fixture layout

```
left/                         right/  (empty)
  file-a.txt, file-b.txt
  sample.png, sample.pdf
  sub-dir/nested-file.txt
  .hidden-file
  bulk/  (3 x 50 MB + 20 x 1 MB .dat files)
```

## Related docs

- `../CLAUDE.md`: E2E suite overview.
- `../e2e-playwright/CLAUDE.md`: Playwright conventions (incl. the clipboard-mock gotcha).
- `../e2e-linux/CLAUDE.md`: Linux Docker single-shard contract.
- `docs/tooling/instance-isolation.md`: canonical per-instance reference.
- `apps/desktop/src-tauri/src/mcp/port_file.rs`: the Rust side of the port-file protocol.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
