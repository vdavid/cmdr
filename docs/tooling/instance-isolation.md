# Instance isolation

Canonical reference for how Cmdr keeps prod, dev, per-worktree dev, and concurrent E2E shards from colliding on disk,
ports, Keychain, clipboard, fixtures, or process names.

One env var (`CMDR_INSTANCE_ID`) drives every per-instance suffix. Read [`AGENTS.md`](../../AGENTS.md) for repo-wide
rules. This doc is the canonical reference — the design has fully landed in code.

## The primitive

`CMDR_INSTANCE_ID` is a short ASCII string the wrapper or the E2E checker sets at launch. Everything else (bundle
identifier, data dir, Keychain service name, MCP ports, Vite port, clipboard backend, fixture root, Dock label) is a
pure function of that string. Prod leaves it unset and ends up byte-identical to before instance isolation existed.

## Derivation table

| Scenario                   | `CMDR_INSTANCE_ID` value | Bundle identifier                     | Data dir                                               | macOS Keychain `SERVICE_NAME` | Dock label / `productName` |
| -------------------------- | ------------------------ | ------------------------------------- | ------------------------------------------------------ | ----------------------------- | -------------------------- |
| Prod                       | (unset)                  | `com.veszelovszki.cmdr`               | `~/Library/Application Support/com.veszelovszki.cmdr/` | `Cmdr`                        | `Cmdr`                     |
| `pnpm dev`                 | `dev`                    | `com.veszelovszki.cmdr-dev`           | `~/.../com.veszelovszki.cmdr-dev/`                     | `Cmdr-dev`                    | `Cmdr (dev)`               |
| `pnpm dev --worktree foo`  | `dev-foo`                | `com.veszelovszki.cmdr-dev-foo`       | `~/.../com.veszelovszki.cmdr-dev-foo/`                 | `Cmdr-dev-foo`                | `Cmdr (dev-foo)`           |
| E2E nonmtp shard (PID `N`) | `e2e-nonmtp1-N`          | `com.veszelovszki.cmdr-e2e-nonmtp1-N` | `/tmp/cmdr-e2e-data-e2e-nonmtp1-N/`                    | `Cmdr-e2e-nonmtp1-N`          | `Cmdr (E2E nonmtp1)`       |
| E2E MTP shard (PID `N`)    | `e2e-mtp-N`              | `com.veszelovszki.cmdr-e2e-mtp-N`     | `/tmp/cmdr-e2e-data-e2e-mtp-N/`                        | `Cmdr-e2e-mtp-N`              | `Cmdr (E2E mtp)`           |

Slug rules for `--worktree`: lowercase ASCII `[a-z0-9-]+`, max 32 chars, runs of `-` collapsed, leading/trailing `-`
trimmed. Rejection happens in Node before any Rust process spawns. Source of truth: `sanitizeWorktreeSlug` in
[`apps/desktop/scripts/instance-id.js`](../../apps/desktop/scripts/instance-id.js).

## Per-resource breakdown

| Resource                                                                                        | Per-instance behavior                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                       | Authoritative file                                                                                                                           |
| ----------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------------------------------------------------------------------------------------------------------------------------- |
| Data dir                                                                                        | `CMDR_DATA_DIR` env wins; otherwise Tauri's `app_data_dir()` resolves from the identifier in the generated config. Wrapper sets both so they agree.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | [`config.rs`](../../apps/desktop/src-tauri/src/config.rs), [`instance-id.js`](../../apps/desktop/scripts/instance-id.js)                     |
| `tauri-plugin-store` (`settings.json`, `shortcuts.json`, `app-status.json`, `viewer-tail.json`) | When `CMDR_DATA_DIR` is set, the frontend loads each store from `<resolved_data_dir>/<name>` (absolute path) via `get_isolated_store_path`, matching the Rust side. Production (no `CMDR_DATA_DIR`) keeps the plain `BaseDirectory::AppData` path, byte-identical. Don't rely on the generated config's identifier alone: the Playwright E2E checker launches the pre-built binary directly (no wrapper, no `-c`), and a runtime `-c` identifier override does NOT redirect `app_data_dir()`, so without this the stores read the real prod files. The command sanitizes the frontend-supplied `store_name` (rejects path separators, `..`, absolute paths) so the resolved path can't escape the data dir. | [`settings.rs`](../../apps/desktop/src-tauri/src/commands/settings.rs), [`store-path.ts`](../../apps/desktop/src/lib/settings/store-path.ts) |
| `tauri-plugin-window-state`                                                                     | Same identifier-driven redirect.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                            | (same)                                                                                                                                       |
| macOS Keychain                                                                                  | `SERVICE_NAME` becomes `Cmdr-<instance>` when the env is set; `Cmdr` otherwise. Cached once via `OnceLock`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | [`keychain_macos.rs`](../../apps/desktop/src-tauri/src/secrets/keychain_macos.rs)                                                            |
| Secret store backend                                                                            | Wrapper exports `CMDR_SECRET_STORE=file` for any non-prod instance, so dev and per-worktree dev never trigger the Keychain password dialog. E2E forces the same path via `is_e2e_mode()`.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | [`tauri-wrapper.js`](../../apps/desktop/scripts/tauri-wrapper.js), [`secrets/mod.rs`](../../apps/desktop/src-tauri/src/secrets/mod.rs)       |
| Cmdr MCP HTTP port                                                                              | Ephemeral by default (`developer.mcpPort = 0`). Server binds `127.0.0.1:0`, writes the actual port to `<data_dir>/mcp.port` atomically. `CMDR_MCP_PORT` still pins.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         | [`mcp/server.rs`](../../apps/desktop/src-tauri/src/mcp/server.rs), [`mcp/port_file.rs`](../../apps/desktop/src-tauri/src/mcp/port_file.rs)   |
| Tauri MCP bridge port                                                                           | Wrapper allocates via `net.createServer().listen(0)`, exports `CMDR_MCP_BRIDGE_PORT`, writes `<data_dir>/tauri-mcp.port` BEFORE Tauri launches. Plugin forced to `127.0.0.1` (was `0.0.0.0`, a LAN exposure: load-bearing security fix).                                                                                                                                                                                                                                                                                                                                                                                                                                                                    | [`tauri-wrapper.js`](../../apps/desktop/scripts/tauri-wrapper.js), [`lib.rs`](../../apps/desktop/src-tauri/src/lib.rs)                       |
| Vite dev port                                                                                   | Wrapper allocates ephemeral, exports `CMDR_VITE_PORT`, writes `build.devUrl` into the generated config so the Tauri webview points at the same number Vite binds.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           | [`tauri-wrapper.js`](../../apps/desktop/scripts/tauri-wrapper.js), [`vite.config.js`](../../apps/desktop/vite.config.js)                     |
| Updater endpoint                                                                                | Non-prod gets `https://localhost.invalid/no-updater` in the generated config so dev or E2E never phones home accidentally.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                  | [`instance-id.js`](../../apps/desktop/scripts/instance-id.js)                                                                                |
| Clipboard (NSPasteboard)                                                                        | Compiled out for E2E via `#[cfg(feature = "playwright-e2e")]`: mock module replaces the real one. Process-local store, never touches the user's pasteboard. `CMDR_CLIPBOARD_BACKEND=mock` env override delegates to the same store from prod-feature builds.                                                                                                                                                                                                                                                                                                                                                                                                                                                | [`clipboard/mod.rs`](../../apps/desktop/src-tauri/src/clipboard/mod.rs)                                                                      |
| Fixture root (macOS E2E)                                                                        | `/tmp/cmdr-e2e-fixtures-<instance>-<timestamp>/`. Bulk `.dat` files hardlinked from `/tmp/cmdr-e2e-fixtures-cache/` (built once via tmp-dir + content-hash verify + atomic rename). Text files are full copies because tests mutate them.                                                                                                                                                                                                                                                                                                                                                                                                                                                                   | [`e2e-shared/fixtures.ts`](../../apps/desktop/test/e2e-shared/fixtures.ts)                                                                   |
| Fixture root (Linux E2E)                                                                        | Stays at `/tmp/cmdr-e2e-<timestamp>/`, no cache. Single shard, low benefit.                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                 | (same)                                                                                                                                       |
| Playwright socket                                                                               | Per-shard at `/tmp/tauri-playwright-<instance>.sock` via `CMDR_PLAYWRIGHT_SOCKET`. Plugin falls back to `/tmp/tauri-playwright.sock` when unset (manual / Linux paths).                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                     | [`lib.rs`](../../apps/desktop/src-tauri/src/lib.rs)                                                                                          |

## Wrapper architecture

`apps/desktop/scripts/tauri-wrapper.js` is the single composition point. The split between it and
`apps/desktop/scripts/instance-id.js` exists so the pure helpers (slug sanitization, instance derivation, config payload
builder, port-file write protocol) are unit-testable via Vitest without spawning Tauri.

The launch sequence:

1. Parse `--worktree <slug>` from argv (before the `--` separator, so Tauri flags after `--` stay intact).
2. Resolve `CMDR_INSTANCE_ID`: existing env wins, then sanitized `--worktree` slug, then `dev` in dev mode, else unset
   for prod.
3. In dev: allocate an ephemeral port for the Tauri MCP bridge (`CMDR_MCP_BRIDGE_PORT`) and one for Vite
   (`CMDR_VITE_PORT`).
4. Compute identifier, data dir, productName, generated config payload via `deriveInstance`.
5. Write the generated `tauri.instance.json` to `$TMPDIR/cmdr-tauri-instance-<rand>/` and pass `-c <abs-path>` to Tauri.
6. Write `<data_dir>/tauri-mcp.port` atomically (tempfile + fsync + rename) so external readers can discover the bridge
   port before Tauri's own bind completes.
7. Export `CMDR_DATA_DIR` (when unset) and `CMDR_SECRET_STORE=file` (when unset).
8. Spawn `pnpm exec tauri ...`. On exit / SIGINT / SIGTERM, remove the tmp config dir and the tauri-mcp port file
   (best-effort; `/tmp` self-prunes on macOS anyway).

The generated config never lands in the repo: it lives under `$TMPDIR` so a crashed wrapper can't pollute tracked space.

## Two parallel dev sessions

```sh
# Terminal A (worktree A):
cd ~/projects-git/vdavid/cmdr/.claude/worktrees/feature-a
pnpm dev --worktree a

# Terminal B (worktree B, simultaneously):
cd ~/projects-git/vdavid/cmdr/.claude/worktrees/feature-b
pnpm dev --worktree b
```

Each session gets:

- A unique bundle identifier (`com.veszelovszki.cmdr-dev-a` vs `-dev-b`), so `tauri-plugin-store` and
  `tauri-plugin-window-state` land in `~/Library/Application Support/com.veszelovszki.cmdr-dev-{a,b}/`.
- A unique Dock label (`Cmdr (dev-a)` vs `Cmdr (dev-b)`).
- An ephemeral Vite port (no `EADDRINUSE` on `1420`).
- Ephemeral Cmdr MCP and Tauri MCP bridge ports, discovered via `<data_dir>/mcp.port` and `<data_dir>/tauri-mcp.port`.
- An isolated Keychain service namespace (`Cmdr-dev-a` vs `Cmdr-dev-b`), though both default to the file backend in dev
  so the Keychain isn't hit anyway.
- An isolated file-backed secret store (under the per-instance data dir).

Quitting one session has no effect on the other.

## How E2E gets isolated per shard

The Playwright checker (`scripts/check/checks/desktop-svelte-e2e-playwright.go`) runs the suite as multiple parallel
shards: one MTP shard (serialized) plus two non-MTP shards split by `--shard X/2`. Per shard the checker stamps:

- `CMDR_INSTANCE_ID=e2e-<kind>-<pid>` (for example, `e2e-mtp-12345`, `e2e-nonmtp1-12345`).
- `CMDR_DATA_DIR=/tmp/cmdr-e2e-data-<instance>/`.
- `CMDR_MCP_PORT=<9429 + offset>` (for the Cmdr MCP server: pinned per-shard so test fixtures don't have to read the
  port file).
- `CMDR_MCP_BRIDGE_PORT=<ephemeral>` for the Tauri MCP bridge.
- `CMDR_PLAYWRIGHT_SOCKET=/tmp/tauri-playwright-<instance>.sock`.
- `CMDR_E2E_START_PATH=/tmp/cmdr-e2e-fixtures-<instance>-<ts>/` (created with hardlinks from the shared cache).
- `CMDR_E2E_MODE=1`, `CMDR_MOCK_FDA=granted`.

Two concurrent `pnpm check desktop-e2e-playwright` runs from two worktrees never collide on data dir, ports, sockets,
fixture roots, Keychain, or processes (the Dock label `Cmdr (E2E <kind>)` lets `pgrep -f 'Cmdr (E2E '` target only the
right ones).

The MTP shard always runs alone because the virtual MTP backing dir (`/tmp/cmdr-mtp-e2e-fixtures/`) is shared by every
Tauri instance (the virtual device is wired into the same path globally). Running MTP specs from two shards at once
would corrupt that backing dir.

## Debug recipes

**Which port is the Cmdr MCP server on right now?**

```sh
# In-process (FE only): the get_mcp_port IPC reads MCP_ACTUAL_PORT directly.
# Out-of-process (CLI, agent helpers):
cat ~/Library/Application\ Support/com.veszelovszki.cmdr-dev/mcp.port
```

For a worktree session, swap `cmdr-dev` for `cmdr-dev-<your-slug>`. For an E2E shard, the path is under
`/tmp/cmdr-e2e-data-<instance>/`.

The `scripts/mcp-call.sh` helper auto-discovers: it resolves the data dir (from `CMDR_DATA_DIR`, else
`CMDR_INSTANCE_ID`, else the `dev` instance) and reads the port file. `CMDR_MCP_PORT` still pins.

```sh
CMDR_INSTANCE_ID=dev-a ./scripts/mcp-call.sh --list-tools
```

**Where's my data dir?**

The wrapper logs it at startup: `Using CMDR_DATA_DIR: ...`. The same path is what `CMDR_DATA_DIR` exports and what
Tauri's `app_data_dir()` returns. `crash-report.json`, `settings.json`, `window-state.json`, `mcp.port`,
`tauri-mcp.port`, the log dir, and the file-backed secret store all live under it.

**Why is the Dock showing `Cmdr (dev-a)` and not `Cmdr`?**

You started the app with an instance ID set (either via `--worktree a` or because `CMDR_INSTANCE_ID` was already in your
env). `productName` mirrors the bundle identifier so cleanup scripts and Dock interactions can target the right process.

**The Tauri MCP bridge isn't responding on `9223`.**

There's no longer a fixed port. The wrapper allocates an ephemeral one per instance and writes
`<data_dir>/tauri-mcp.port`. Read that file. The plugin now binds `127.0.0.1` only (was `0.0.0.0`, a LAN exposure we
fixed at the same time).

## Generated and on-disk files

| File                                                     | Owner        | Lifetime                                         | Purpose                                                                   |
| -------------------------------------------------------- | ------------ | ------------------------------------------------ | ------------------------------------------------------------------------- |
| `$TMPDIR/cmdr-tauri-instance-<rand>/tauri.instance.json` | wrapper      | per-launch (cleaned on exit / SIGINT)            | Tauri `-c` override for identifier, productName, devUrl, updater endpoint |
| `<data_dir>/mcp.port`                                    | Rust         | server lifetime (best-effort delete on shutdown) | actual bound port of the Cmdr MCP HTTP server                             |
| `<data_dir>/tauri-mcp.port`                              | wrapper      | per-launch (best-effort delete on exit)          | wrapper-allocated port the Tauri MCP bridge will bind                     |
| `/tmp/cmdr-e2e-fixtures-cache/`                          | Node         | persistent (rebuild on file-shape change)        | shared hardlink source for E2E bulk `.dat` fixtures                       |
| `/tmp/cmdr-e2e-fixtures-<instance>-<ts>/`                | Node         | per-run                                          | per-shard fixture root                                                    |
| `/tmp/cmdr-e2e-data-<instance>/`                         | Go checker   | per-run                                          | per-shard data dir                                                        |
| `/tmp/tauri-playwright-<instance>.sock`                  | Tauri plugin | per-run                                          | per-shard Playwright IPC socket                                           |

## Precedence rules

1. **`CMDR_DATA_DIR` is authoritative for data-dir paths.** If set, the backend uses it as-is.
2. **`CMDR_INSTANCE_ID` is authoritative for Keychain service name, clipboard backend selection (when the Cargo feature
   isn't already on), and the Dock label.** It does NOT participate in data-dir resolution.
3. **MCP port read precedence** (external clients): `CMDR_MCP_PORT` env → `<data_dir>/mcp.port` → typed error. Never
   silently fall back to a legacy hardcoded default; that hides bugs.
4. **MCP port write precedence**: even when `CMDR_MCP_PORT` is set, the server still writes the bound port to the file
   so external readers don't have to special-case the pinned variant.
5. **Wrapper always sets both `CMDR_DATA_DIR` and `CMDR_INSTANCE_ID`** so Tauri's `app_data_dir()` and our
   `CMDR_DATA_DIR` agree.

## Mock-backend convention

Two patterns coexist on purpose:

- **Cargo feature** (`#[cfg(feature = "playwright-e2e")]`) when the mock would otherwise compile in heavy platform deps
  (`objc2`, `security-framework`, OS FFI). Compile-time switch keeps prod binaries lean and removes whole code paths.
  Example: the clipboard mock.
- **Runtime env var** when the mock is an alternative implementation of an existing function and the prod path is light.
  Examples: `CMDR_MOCK_FDA` gates a few syscalls; `CMDR_E2E_MODE=1` toggles soft hooks (title-bar stripe);
  `CMDR_CLIPBOARD_BACKEND=mock` lets prod-feature dev builds delegate to the shared store for ad-hoc debugging.

Both patterns can coexist on one subsystem (the clipboard does: feature flag for the E2E build, env for the manual
override on a prod-feature build). Each subsystem's `CLAUDE.md` documents which hooks it honors.

## Acceptance smoke

Two manual tests that prove the primitive holds. Re-run these after any change to the wrapper, the E2E checker, or any
of the per-resource derivation paths. Future regressions are caught by re-running them.

### Test 1: two parallel `pnpm dev` sessions from two worktrees

1. Open two terminal shells, each in a different worktree (typically
   `~/projects-git/vdavid/cmdr/.claude/worktrees/<name>/`).
2. In shell A: `pnpm dev --worktree a`.
3. In shell B: `pnpm dev --worktree b`.
4. Both windows open with no `EADDRINUSE` errors on the wrapper or Vite. Dock shows `Cmdr (dev-a)` and `Cmdr (dev-b)`.
5. Change a setting in window A (for example, toggle hidden files). Quit window A. Confirm
   `~/Library/Application Support/com.veszelovszki.cmdr-dev-a/settings.json` exists with the change persisted, and
   `~/Library/Application Support/com.veszelovszki.cmdr-dev-b/settings.json` is untouched.
6. Window B is still running and responsive. The B session's MCP port (in
   `~/Library/Application Support/com.veszelovszki.cmdr-dev-b/mcp.port`) is reachable.
7. `lsof -i -P | grep Cmdr | grep LISTEN`: every line shows `127.0.0.1` (no `*:` bind), and the port numbers per session
   don't overlap.

### Test 2: two parallel E2E runs from two worktrees

1. Open two shells, each in a different worktree.
2. In each shell: `pnpm check desktop-e2e-playwright`.
3. Mid-run, in a third shell:
   - `ls /tmp/cmdr-e2e-fixtures-*` shows distinct `<instance>-<ts>` dirs for each shard from each run, plus one shared
     `/tmp/cmdr-e2e-fixtures-cache/`.
   - `ls /tmp/cmdr-e2e-data-*` shows distinct data dirs per shard.
   - `ls /tmp/tauri-playwright-*.sock` shows distinct sockets per shard.
   - `lsof -i -P | grep Cmdr | grep LISTEN`: every line bound to `127.0.0.1`, distinct ports per shard.
   - `pgrep -fl 'Cmdr (E2E '`: process labels are distinct per shard.
4. Open TextEdit before starting the runs with some content in your real clipboard. After both runs finish, `pbpaste`
   returns the same content (the clipboard mock never touched the system pasteboard).
5. Both runs complete with the same pass/fail result they'd have in isolation.
6. `du -sh /tmp/cmdr-e2e-fixtures-cache`: roughly 170 MB, paid once across both runs (hardlinks).

## Related docs

- [`docs/tooling/mcp.md`](mcp.md): MCP server overview, port discovery for external clients, action-tool ack contract.
- [`AGENTS.md`](../../AGENTS.md) § Debugging / § MCP / § Worktrees: repo-wide cross-references.
- [`apps/desktop/CLAUDE.md`](../../apps/desktop/CLAUDE.md): desktop app overview, `--worktree` flag.
- [`apps/desktop/src-tauri/src/mcp/CLAUDE.md`](../../apps/desktop/src-tauri/src/mcp/CLAUDE.md): server lifecycle and
  port-file protocol.
- [`apps/desktop/src-tauri/src/secrets/CLAUDE.md`](../../apps/desktop/src-tauri/src/secrets/CLAUDE.md): Keychain
  service-name suffix.
- [`apps/desktop/src-tauri/src/clipboard/CLAUDE.md`](../../apps/desktop/src-tauri/src/clipboard/CLAUDE.md): mock
  backend.
- [`apps/desktop/test/CLAUDE.md`](../../apps/desktop/test/CLAUDE.md): fixture cache, per-instance root.
- [`scripts/check/CLAUDE.md`](../../scripts/check/CLAUDE.md) § Self-contained E2E checks: per-shard env composition.
