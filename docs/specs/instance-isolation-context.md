# Instance isolation — context bundle

Frozen record of the decisions David made before the plan was written. The
plan lives in `instance-isolation-plan.md`; this file is the "why" the plan
refers back to. Don't edit this once the plan is approved.

## The change in one sentence

Replace the patchwork of bundle-ID / data-dir / port / Keychain isolation
with a single `CMDR_INSTANCE_ID` primitive so prod, dev, dev-per-worktree,
and concurrent E2E shards never collide on disk, ports, Keychain, mDNS, or
process names.

## The problem we're solving

Today's isolation is half-built:

- `tauri.dev.json` overrides `identifier` to `com.veszelovszki.cmdr-dev` so
  `tauri-plugin-store` lands in the dev path (commit `519a8781`).
- The wrapper sets `CMDR_DATA_DIR` to the same path for direct file I/O.
- The E2E checker sets per-shard `CMDR_DATA_DIR`, `CMDR_MCP_PORT`, and
  `CMDR_PLAYWRIGHT_SOCKET`.

What still collides:

1. **`tauri-plugin-store`** ignores `CMDR_DATA_DIR`. E2E shards all share the
   prod `settings.json`. So does `tauri-plugin-window-state`.
2. **macOS Keychain** uses a hardcoded `SERVICE_NAME = "Cmdr"`. E2E hits real
   Keychain (the file-backed fallback is only set in dev). Two E2E shards
   stomp on each other's SMB credentials + AI keys.
3. **MCP port** (`19224` prod / `19225` dev) is fixed. Auto-probes upward
   when taken, but the frontend uses the configured value, not the actual
   bound one. Two concurrent dev sessions misbehave.
4. **Tauri MCP plugin port** (`9223`) is fixed.
5. **Vite dev port** (`1420`) is fixed. Two `pnpm dev` from two worktrees →
   the second one fails.
6. **NSPasteboard** is system-wide. E2E tests that copy/paste mutate the
   user's real clipboard mid-session. Annoying for David; also a clash risk
   if two E2E runs are in flight.
7. **`/tmp/cmdr-e2e-fixtures`** is one shared fixture root for non-MTP
   shards. Two concurrent E2E runs (different worktrees, agents in parallel)
   would race on it.
8. **macOS TCC** is bundle-ID-keyed. E2E binaries today use prod ID, so any
   TCC interaction in E2E pollutes prod's TCC list.
9. **mDNS** service names in `virtual_smb_hosts.rs` aren't instance-suffixed
   — two parallel SMB E2E shards would advertise the same name.
10. **Updater endpoint** in prod hits `api.getcmdr.com`; E2E inherits it.

David's real-world constraints:

- He runs multiple agents in parallel from different worktrees.
- Each agent might run `pnpm dev` or `./scripts/check.sh
  --check desktop-e2e-playwright` while others are doing the same.
- He doesn't want E2E tests touching his real clipboard.
- He has endless time/budget for "do it right".

## David's resolved decisions

### Pre-questions round 1

1. **NSPasteboard clipboard** — **mock it**. Tests don't actually use the
   system clipboard. Per-process IPC clipboard layer with a mock backend
   during E2E.
2. **Per-shard fixture root** — yes, include the instance ID in the path:
   `/tmp/cmdr-e2e-fixtures-<instance>/`. Use **hardlinks** for the bulk
   `.dat` files so we don't copy 170 MB per shard. Bulk fixture data is
   read-only by the tests, so hardlinks don't compromise isolation.
3. **Worktree** — created at `.claude/worktrees/instance-isolation/`.

## The unified architectural fix: `CMDR_INSTANCE_ID`

One env var, one suffix, everything derives from it.

| Scenario | `CMDR_INSTANCE_ID` value | Bundle ID | Data dir | MCP port |
| --- | --- | --- | --- | --- |
| Prod | (unset) | `com.veszelovszki.cmdr` | canonical | 19224 |
| `pnpm dev` | `dev` | `com.veszelovszki.cmdr-dev` | `~/.../cmdr-dev/` | ephemeral, written to data_dir |
| `pnpm dev --worktree foo` | `dev-foo` | `com.veszelovszki.cmdr-dev-foo` | `~/.../cmdr-dev-foo/` | ephemeral |
| E2E shard 1 | `e2e-nonmtp1-<pid>` | `com.veszelovszki.cmdr-e2e-nonmtp1-<pid>` | `/tmp/cmdr-e2e-data-nonmtp1-<pid>/` | ephemeral |
| E2E shard 2 | `e2e-nonmtp2-<pid>` | `com.veszelovszki.cmdr-e2e-nonmtp2-<pid>` | `/tmp/cmdr-e2e-data-nonmtp2-<pid>/` | ephemeral |

### Derivation rules

- **Bundle identifier**: injected via a generated `tauri.instance.json` (the
  wrapper writes it to a temp path at startup, Tauri merges it via `-c`).
  This replaces the static `tauri.dev.json`.
- **`CMDR_DATA_DIR`**: `~/Library/Application Support/com.veszelovszki.cmdr-<instance>/`
  (or `/tmp/cmdr-e2e-data-<instance>/` for E2E). Wrapper composes this.
- **`tauri-plugin-store`**: initialise with an explicit `Some(data_dir.join("settings.json"))`
  in `lib.rs` so it honours `CMDR_DATA_DIR`. One-line change.
- **`tauri-plugin-window-state`**: same explicit path override.
- **Keychain service name**: include instance ID in `SERVICE_NAME` (or short-
  circuit to file-backed when instance is non-empty). Non-prod runs get
  isolated entries; dev keeps its no-dialog file backend.
- **MCP port**: if `CMDR_MCP_PORT` is set, use it; otherwise bind
  `127.0.0.1:0` (ephemeral) and write the bound port to `<data_dir>/mcp.port`.
  Clients (MCP CLI, fixtures, agent helpers) read the port from that file
  when `CMDR_INSTANCE_ID` is set.
- **Tauri MCP plugin port** (`9223`): same pattern.
- **Vite dev port**: wrapper reserves an ephemeral port (Node
  `net.createServer().listen(0)`), writes it to the generated
  `tauri.instance.json` as `build.devUrl`. `vite.config.ts` reads
  `CMDR_VITE_PORT`.
- **Updater endpoint**: non-prod instances point at a harmless dead URL so
  E2E or dev never phones home accidentally.
- **mDNS service name**: include instance ID in the advertised name so two
  parallel SMB E2E shards don't collide on the network.
- **`productName`** (process / Dock label): mirror the identifier. `Cmdr`,
  `Cmdr (dev)`, `Cmdr (dev-foo)`, `Cmdr (E2E shard1)`. Lets cleanup scripts
  target only the right processes via `pgrep -f "Cmdr (E2E"`.

### Clipboard mock (NSPasteboard)

The frontend clipboard (`Cmd+C/X/V`) is a Tauri IPC surface today. Add a
backend setting / env (`CMDR_CLIPBOARD_BACKEND=mock`) that swaps the real
`NSPasteboard` interop for an in-process mock store. E2E builds (the
`playwright-e2e` Cargo feature, or the wrapper for E2E launches) flip this
to `mock`. The mock holds clipboard contents in a process-local
`Mutex<Vec<ClipboardEntry>>` and exposes the same IPC surface so tests are
unchanged.

Acceptance: running E2E should NOT mutate the user's real macOS clipboard.

### Per-instance fixture root with hardlinks

`/tmp/cmdr-e2e-fixtures` becomes `/tmp/cmdr-e2e-fixtures-<instance>/`. Bulk
.dat files (50 MB × 3 + 1 MB × 20 = ~170 MB) are hardlinked from a shared
content cache at `/tmp/cmdr-e2e-fixtures-cache/` instead of copied. Text
files (file-a.txt, sub-dir contents, etc.) are full copies because tests
mutate them. The cache is built once, hardlinked into each instance's
fixture root; per-instance text files are recreated by
`recreateFixtures()`.

Acceptance: two concurrent E2E runs from two worktrees can coexist with
zero cross-talk, no extra disk usage past the first run's cache build.

## Suggested execution shape

Seven phases. Each commits separately for bisect.

1. **P1**: Plugin-store + window-state explicit paths. Honours `CMDR_DATA_DIR`.
   Smallest, immediate win for E2E settings isolation. No behavior change in prod.
2. **P2**: `CMDR_INSTANCE_ID` resolution in the wrapper + generated
   `tauri.instance.json`. Adds the `--worktree <name>` flag to dev. Replaces
   `tauri.dev.json`.
3. **P3**: Ephemeral MCP port + port file. Update clients (E2E fixtures,
   MCP CLI helpers in `docs/tooling/mcp.md`).
4. **P4**: Keychain service-name suffix + mDNS suffix + `productName`
   suffix. Process-label cleanup.
5. **P5**: Vite dynamic port + updater endpoint stub for non-prod.
6. **P6**: Per-instance fixture root with hardlink cache + checker shard
   wiring. Plus the NSPasteboard mock backend.
7. **P7**: Docs sweep (`AGENTS.md`, `scripts/check/CLAUDE.md`,
   `apps/desktop/test/CLAUDE.md`, `apps/desktop/src-tauri/src/mcp/CLAUDE.md`,
   `apps/desktop/src-tauri/src/secrets/CLAUDE.md`, plus the colocated
   modules). Test coverage and visual smoke (two parallel dev sessions
   from two worktrees; two parallel E2E runs).

## What's NOT in scope

- Cargo `target/` sharing across worktrees (Cargo handles itself well).
- TCC isolation beyond what the new bundle ID gives us automatically.
- macOS Recent Items / SF Symbols cache (unlikely to collide).
- Spotlight indexing.

## Files to read in full before drafting

- `AGENTS.md`
- `docs/architecture.md`
- `docs/style-guide.md`
- `apps/desktop/CLAUDE.md`
- `apps/desktop/scripts/tauri-wrapper.js`
- `apps/desktop/src-tauri/tauri.conf.json`
- `apps/desktop/src-tauri/tauri.dev.json`
- `apps/desktop/src-tauri/src/lib.rs` (plugin init, around lines 200–270)
- `apps/desktop/src-tauri/src/config.rs` (data dir resolution)
- `apps/desktop/src-tauri/src/mcp/CLAUDE.md`
- `apps/desktop/src-tauri/src/mcp/config.rs` (port defaults)
- `apps/desktop/src-tauri/src/mcp/server.rs` (bind + serve)
- `apps/desktop/src-tauri/src/secrets/CLAUDE.md`
- `apps/desktop/src-tauri/src/secrets/keychain_macos.rs` (`SERVICE_NAME`)
- `apps/desktop/src-tauri/src/secrets/mod.rs` (backend selection)
- `apps/desktop/src-tauri/src/clipboard/` (the NSPasteboard interop)
- `apps/desktop/src-tauri/src/network/virtual_smb_hosts.rs` (mDNS)
- `apps/desktop/src-tauri/src/settings/loader.rs` (data_dir + settings.json)
- `apps/desktop/test/CLAUDE.md`
- `apps/desktop/test/e2e-playwright/CLAUDE.md`
- `apps/desktop/test/e2e-playwright/playwright.config.ts`
- `apps/desktop/test/e2e-playwright/fixtures.ts`
- `apps/desktop/test/e2e-playwright/global-setup.ts`
- `apps/desktop/test/e2e-shared/fixtures.ts`
- `scripts/check/CLAUDE.md`
- `scripts/check/checks/desktop-svelte-e2e-playwright.go` (shard composition)
- `docs/tooling/mcp.md` (MCP CLI conventions)
- `apps/desktop/vite.config.ts`
