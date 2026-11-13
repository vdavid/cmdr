# E2E shared helpers

Modules used by both the macOS Playwright suite and the Linux Docker E2E suite. Anything that must run BEFORE the Tauri
binary launches (fixture creation, port-file reads, MCP client setup) lives here.

## Files

- **`fixtures.ts`**: builds the on-disk fixture tree the app opens at startup. macOS Playwright path: per-instance root
  at `/tmp/cmdr-e2e-fixtures-<instance>-<ts>/`, bulk `.dat` files hardlinked from a shared cache at
  `/tmp/cmdr-e2e-fixtures-cache/`. Linux Docker: shared `/tmp/cmdr-e2e-<ts>/`.
- **`fixture-manifest.ts`**: the shared-fixture drift guard. `diffFixtureTree` compares `left/` + `right/` against the
  declared layout, `restoreFixtureTree` repairs what drifted; the Playwright `afterEach` runs both.
- **`port-file.ts`**: reads `<data_dir>/mcp.port` and `<data_dir>/tauri-mcp.port`. `resolveMcpPort(dataDir)` follows the
  canonical precedence, never a legacy port.
- **`mcp-client.ts`**: a `fetch` client for the Cmdr MCP server (tool calls, resource reads).
- **`mtp-fixtures.ts`**: virtual MTP backing-dir composition, at the run's `MTP_FIXTURE_ROOT` (`CMDR_MTP_FIXTURE_ROOT`,
  matching the app's `CMDR_VIRTUAL_MTP`). One root per run, shared by its shards, so the MTP shard is serialized.
- **`smb-fixtures.ts`**: SMB virtual-host fixtures, injected into the running Tauri process via the `smb-e2e` feature.
  ❗ Its mount is found by source, ❌ never by path (`/Volumes/public` may be someone else's). Specs read
  `guestMountPoint()`. `DETAILS.md` § "Finding the SMB fixture's mount".
- **`server-fixtures.ts`**: where the app dials the SFTP / WebDAV fixtures, plus an `ssh` / `curl` side door to their
  exports. `../e2e-playwright/DETAILS.md` § "Real SFTP and WebDAV servers".
- **`about-fixtures.ts`**: the "Tell me about this" QA tree (header).
- **`pin-locale.ts`**: `pinUiLanguage(dataDir)` pins the language in `settings.json`; `EN_US_LOCALE_ARGS` and
  `OVERLAY_SCROLLBAR_ARGS` are macOS launch args. `../e2e-playwright/DETAILS.md` § "The locale pin", § "The scrollbar
  pin".

## Must-knows

- **`createFixtures(instanceId)` is the only fixture API.** Pass an instance ID on macOS (per-shard path + hardlink
  cache), omit it on Linux Docker (shared path). Both return the root for `CMDR_E2E_START_PATH`.
- **Port-file read NEVER falls back to a hardcoded default.** Strict precedence: `CMDR_MCP_PORT` env (the Go checker's
  per-shard pin) → `<data_dir>/mcp.port` (Cmdr MCP) or `tauri-mcp.port` (Tauri MCP bridge) → throw `PortDiscoveryError`.
  A fallback would let a test "work" against the wrong instance. Writers: `DETAILS.md` § "Who writes the port files".
- **`mcp-client.ts` and `mtp-fixtures.ts` don't read the port file.** They run inside the app (Tauri IPC), where the
  `MCP_ACTUAL_PORT` atomic is the truth. Out-of-process callers use `port-file.ts`.
- **Bulk `.dat` files are zero-fill ASCII, hardlinked, and read-only.** Tests needing real binary patterns add their own
  fixtures: the cache check samples content at a few offsets, so arbitrary content breaks the cache contract.
- **❌ Never write to a bulk `.dat` IN PLACE.** They're hardlinks into the cache, so one `truncateSync` shortens it in
  every live fixture tree on the machine at once. The cache self-heals on the next build; those trees do NOT, and the
  leak guard then blames a random innocent spec. Remove and rewrite, as `restoreBulkFile` does.
- **Text files are full copies, recreated per test; bulk files are NOT.** `recreateFixtures()` (in `beforeEach` of
  mutating specs) restores the text files and re-copies the committed `media-fixtures/` (`sample.png` 2×2 RGBA,
  `sample.pdf` 1 page) into `left/`; bulk files are read-only, so they survive across tests.

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
- `../e2e-playwright/CLAUDE.md`: Playwright conventions.
- `../e2e-linux/CLAUDE.md`: Linux Docker single-shard contract.
- `docs/tooling/instance-isolation.md`: canonical per-instance reference.
- `apps/desktop/src-tauri/src/mcp/port_file.rs`: the Rust side of the port-file protocol.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
