# Instance isolation: execution plan

Companion to [`instance-isolation-context.md`](instance-isolation-context.md), which freezes the "why" behind every
decision below. Read it first; this plan is the "how."

## Goal

Replace the patchwork of bundle-ID / data-dir / port / Keychain / clipboard / fixture-root isolation with a single
`CMDR_INSTANCE_ID` primitive. After this lands, prod, `pnpm dev`, `pnpm dev --worktree <name>`, and N concurrent E2E
shards each run in fully isolated grooves: separate data dir, separate Keychain service, separate ephemeral MCP and
Tauri-MCP-plugin ports, separate Vite dev port, separate `productName` (Dock label), and a process-local mock
NSPasteboard during E2E. Two parallel dev sessions from two worktrees coexist with no manual juggling; two parallel E2E
shards never stomp on each other's `settings.json`, Keychain entries, clipboard, or `/tmp/cmdr-e2e-fixtures-*` trees.
Prod is byte-identical to today: zero migration, zero behavior change.

## Out of scope

- Cargo `target/` sharing across worktrees. Cargo's incremental cache handles this well; instance isolation is about
  runtime collisions, not build artifacts.
- TCC isolation beyond what a fresh bundle ID gives us automatically. macOS TCC is bundle-ID-keyed; every new identifier
  is a fresh entry. E2E side-steps this via `CMDR_MOCK_FDA` (already in
  [`permissions.rs:114`](../../apps/desktop/src-tauri/src/permissions.rs)).
- Spotlight indexing of the new data dirs. macOS Recent Items / SF Symbols cache.
- Pre-existing test failures: `FilterChips.svelte.test.ts:513`, the file-viewer drag spec, and the QueryDialog
  svelte-check finding. All owned by other workstreams; do not fix in this branch.
- mDNS service-name suffixing. Verified in this branch:
  [`network/mdns_discovery.rs:87`](../../apps/desktop/src-tauri/src/network/mdns_discovery.rs) only `browse()`s, never
  registers. There is no advertised name to collide on. The context bundle's clash item #9 is a false positive; recorded
  here so a future pass doesn't reopen it.

## Architecture overview

`CMDR_INSTANCE_ID` is the one knob. The wrapper (`tauri-wrapper.js`) or the E2E checker sets it, then composes the
derived values (`CMDR_DATA_DIR`, a generated `tauri.instance.json`, an ephemeral Vite port) up front. By the time the
Tauri binary spawns, every downstream layer reads its env var or its config field. No logic gets duplicated.

**Key insight (verified against `tauri-plugin-store` 2.4.2 source).** The plugin has NO global path override API. Its
`Builder` exposes only `register_serialize_fn` / `register_deserialize_fn` / `default_serialize_fn` /
`default_deserialize_fn` / `build()`. Per-store paths resolve at every JS `load(...)` call via
`StoreBuilder::new(manager, path)` → `resolve_store_path` → `BaseDirectory::AppData`. `BaseDirectory::AppData` is
identifier-driven. So the identifier override in P1's generated config is sufficient to redirect `settings.json` for
plugin-store, plus the equivalent state file for `tauri-plugin-window-state` 2.4.1 (same `BaseDirectory::AppData` path).
No per-call-site override needed. The identifier IS the fix.

```
                                  ┌──────────────────────────────────┐
   pnpm dev [--worktree foo]  ──▶ │ tauri-wrapper.js (Node)          │
   E2E checker (Go) ─────────────▶│ - resolves instance ID           │
                                  │ - sanitizes worktree slug        │
                                  │ - allocates ephemeral Vite port  │
                                  │ - allocates ephemeral MCP-plugin │
                                  │   port (passed to plugin config) │
                                  │ - writes tauri.instance.json     │
                                  │ - exports env vars + -c flag     │
                                  └──────────────┬───────────────────┘
                                                 │
                            ┌────────────────────┴────────────────────┐
                            │ env: CMDR_INSTANCE_ID, CMDR_DATA_DIR,   │
                            │      CMDR_VITE_PORT, CMDR_SECRET_STORE, │
                            │      CMDR_CLIPBOARD_BACKEND,            │
                            │      CMDR_MCP_BRIDGE_PORT, ...           │
                            │ tauri -c <abs>/tauri.instance.json:     │
                            │      identifier, productName,           │
                            │      build.devUrl, updater.endpoints    │
                            └────────────────────┬────────────────────┘
                                                 │
                                  ┌──────────────▼──────────────┐
                                  │ Tauri binary (Cmdr)         │
                                  │ - identifier flows into     │
                                  │   BaseDirectory::AppData →  │
                                  │   plugin-store + window-    │
                                  │   state files auto-land in  │
                                  │   per-instance dir          │
                                  │ - MCP server binds ephemeral│
                                  │   writes <data_dir>/mcp.port│
                                  │ - tauri-MCP plugin binds    │
                                  │   127.0.0.1 (security fix)  │
                                  │   on wrapper-supplied port; │
                                  │   <data_dir>/tauri-mcp.port │
                                  │ - secrets: Cmdr-<instance>  │
                                  │ - clipboard mock or real    │
                                  └──────────────┬──────────────┘
                                                 │
   MCP CLI / agent helpers / E2E fixtures ──────▶│ reads <data_dir>/mcp.port for ephemeral port
   FE (in-process) ─────────────────────────────▶│ still uses get_mcp_port IPC
```

### Where the suffix lands

| Surface                          | How it gets the suffix                                                                              | Authoritative file                                                 |
| -------------------------------- | --------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| Bundle identifier                | `tauri.instance.json` `identifier: com.veszelovszki.cmdr-<instance>`                                | wrapper-generated, `$TMPDIR/cmdr-tauri-instance-<pid>-<rand>.json` |
| `productName` / Dock label       | `tauri.instance.json` `productName: Cmdr (<instance label>)`                                        | same                                                               |
| `app_data_dir()` (Tauri)         | Driven by identifier; resolves to `~/Library/Application Support/com.veszelovszki.cmdr-<instance>/` | Tauri internals                                                    |
| `tauri-plugin-store`             | Identifier-driven (`BaseDirectory::AppData`); `settings.json` auto-lands in per-instance dir        | no code change in lib.rs                                           |
| `tauri-plugin-window-state`      | Identifier-driven (same `BaseDirectory::AppData`); window state auto-lands                          | no code change in lib.rs                                           |
| `CMDR_DATA_DIR` (direct file IO) | Wrapper sets it to match the above                                                                  | `apps/desktop/scripts/tauri-wrapper.js`                            |
| Keychain `SERVICE_NAME`          | `keychain_macos.rs` reads `CMDR_INSTANCE_ID`; `Cmdr-<instance>` else `Cmdr`                         | `apps/desktop/src-tauri/src/secrets/keychain_macos.rs`             |
| MCP port (Cmdr server)           | `bind 127.0.0.1:0`, write port to `<data_dir>/mcp.port` atomically                                  | `apps/desktop/src-tauri/src/mcp/server.rs`                         |
| Tauri-MCP plugin port            | Wrapper picks ephemeral; passes via `base_port`; also forces `127.0.0.1` bind                       | `apps/desktop/src-tauri/src/lib.rs` (init line ~219)               |
| Vite dev port                    | Wrapper picks ephemeral, writes `build.devUrl` into generated config; `CMDR_VITE_PORT`              | `vite.config.js` reads `CMDR_VITE_PORT`                            |
| Updater endpoint                 | Non-prod: dead URL in generated config                                                              | wrapper-generated config                                           |
| NSPasteboard                     | `#[cfg(feature = "playwright-e2e")]` module-level switch; mock vs real                              | `apps/desktop/src-tauri/src/clipboard/`                            |
| Fixture root                     | Checker creates `/tmp/cmdr-e2e-fixtures-<instance>/`, hardlinks bulk .dat from cache                | `apps/desktop/test/e2e-shared/fixtures.ts` + Go checker            |

### Precedence rules (read these before touching wiring)

1. **`CMDR_DATA_DIR` is authoritative for data-dir paths.** If set, the backend uses it as-is. The wrapper composes both
   `CMDR_DATA_DIR` and the matching identifier; either alone works (with caveats below). Keep
   `config::resolved_app_data_dir()` reading `CMDR_DATA_DIR` first
   ([`config.rs:27`](../../apps/desktop/src-tauri/src/config.rs)).

2. **`CMDR_INSTANCE_ID` is authoritative for Keychain service name, clipboard backend selection (when the Cargo feature
   isn't already on), and the `productName` shown in the Dock.** It does NOT participate in data-dir resolution (that's
   `CMDR_DATA_DIR` + Tauri's identifier-derived default).

3. **MCP port read precedence (in code, not docs):** `CMDR_MCP_PORT` env (manual pin) → `<data_dir>/mcp.port` (ephemeral
   discovery) → typed error `PortDiscoveryError::NoPinNoFile`. Never silently fall back to a legacy hardcoded default;
   that hides bugs. **Write precedence:** if `CMDR_MCP_PORT` is set, the server still also writes the bound port to the
   port file so external readers don't need to special-case the pinned variant.

4. **MCP port FE path:** stays on the `get_mcp_port` IPC
   ([`bindings.ts:1049`](../../apps/desktop/src/lib/ipc/bindings.ts)). That call lives inside the running webview, so it
   can read the in-process `MCP_ACTUAL_PORT` atomic directly. The port file is for external readers only (CLI, E2E
   setup, agents). Both paths report the same value because both read `MCP_ACTUAL_PORT` (the server stamps it after
   `bind_with_probe` returns and before writing the file).

5. **Wrapper always sets both `CMDR_DATA_DIR` and `CMDR_INSTANCE_ID`.** A worktree user running
   `pnpm dev --worktree foo` gets `CMDR_INSTANCE_ID=dev-foo` AND
   `CMDR_DATA_DIR=~/Library/Application Support/com.veszelovszki.cmdr-dev-foo/`. The identifier in the generated
   `tauri.instance.json` matches, so Tauri's own `app_data_dir()` and our `CMDR_DATA_DIR` agree.

6. **If instance is set but data dir isn't:** backend logs a `warn!` once at startup and falls through to Tauri's
   identifier-derived default. They'll match in practice (same suffix), so it works, but it's an unsupported config.

7. **If data dir is set but instance isn't:** today's behavior, unchanged. Data dir is honored. Keychain stays `"Cmdr"`.
   MCP uses the build-mode default port. The P3 checker rewire stops this state existing in practice.

## The six phases

(Round-1 P1 collapsed into P2; see B1 in the round-1 review. Numbering below reflects the final shape.) Each phase is
one commit (bisectable). After every phase, run `./scripts/check.sh`; after P5 also run
`./scripts/check.sh --include-slow` because the E2E surface changed.

---

### P1: `CMDR_INSTANCE_ID` resolution + generated `tauri.instance.json`

**Scope (in):** Introduce the env var, worktree slug parsing, the generated config file. Replace static `tauri.dev.json`
with the wrapper-generated `tauri.instance.json`. Wire `--worktree <name>` flag. This single identifier change
auto-redirects `tauri-plugin-store`'s `settings.json` AND `tauri-plugin-window-state`'s state file, because both resolve
via `BaseDirectory::AppData` which is identifier-driven (verified against plugin source in `Cargo.lock`-pinned 2.4.2 /
2.4.1). No `lib.rs` plugin-init change needed.

**Scope (out):** Port wiring (P2), Keychain suffix (P3), Vite port reservation (P4), fixture root / NSPasteboard mock
(P5).

**Why first.** Every later phase reads `CMDR_INSTANCE_ID`. Standing it up early (with all downstream consumers still on
their current behavior) lets each subsequent phase be a focused commit instead of a sprawling cross- cutting change.
Plus this phase alone fixes today's biggest concrete pain (E2E shards stomping on the user's real `settings.json`) once
the checker is also rewired to pass the env var (P3).

**Why generated config file (not runtime API).** Tauri's `identifier` is read at startup before any IPC handler exists.
The only way to override it is `-c <path>` on the CLI. Once the binary is running, the identifier is fixed: it's used by
`app_data_dir()`, the bundle's `CFBundleIdentifier` lookup, and every system call that asks "which app am I?". A runtime
API would arrive too late.

**Why one env var (not many).** The alternative is `CMDR_BUNDLE_ID`, `CMDR_SECRET_NAMESPACE`, `CMDR_PRODUCT_NAME`,
`CMDR_DATA_DIR`, `CMDR_MCP_PORT` set independently. Each could drift. One env var means every consumer derives from the
same string via pure functions in one place. The wrapper still sets `CMDR_DATA_DIR` explicitly because (a) it's an
existing contract and (b) the path is non-obvious (Application Support vs `/tmp/cmdr-e2e-data-*`).

**Worktree slug sanitization rules:**

- Input: arbitrary `--worktree <name>` argument.
- Output: lowercase ASCII, `[a-z0-9-]+`, max 32 chars (so the resulting bundle ID stays well under macOS's 78-byte
  `CFBundleIdentifier` practical cap).
- Replace non-`[a-z0-9-]` with `-`, lowercase, collapse runs of `-`, trim leading/trailing `-`.
- Empty after sanitization → reject with `--worktree must be 1–32 alphanumeric/dash characters after sanitization`.
- Examples: `foo` → `foo`; `Feature/Onboarding-Revamp` → `feature-onboarding-revamp`; `café` → `caf` (after unicode
  strip + trim; accepted); `   ` → reject.
- Sanitization lives in the wrapper (Node), so failures happen before any Rust process spawns.
- The wrapper does NOT validate the slug against the current worktree directory name. Pinning your own slug from a
  non-worktree shell (`pnpm dev --worktree foo` outside any worktree) is fine; it just produces a `dev-foo` instance.

**Instance ID derivation:**

- `pnpm dev` (no `--worktree`): `dev`.
- `pnpm dev --worktree foo`: `dev-foo`.
- E2E (Go checker, wired in P3): `e2e-<kind>-<pid>` (`e2e-mtp-12345`, `e2e-nonmtp1-12345`).
- Prod (`pnpm build`): unset. Bundle ID stays `com.veszelovszki.cmdr`. Zero migration.

**Generated `tauri.instance.json`:**

- **Path: `$TMPDIR/cmdr-tauri-instance-<pid>-<rand>.json`** via Node's
  `fs.mkdtempSync(path.join(os.tmpdir(), 'cmdr-tauri-instance-'))`. Pass an absolute path to `-c`. NOT inside the repo
  (the wrapper's cwd at `pnpm dev` time is repo root per AGENTS.md, so a relative file would land at `<repo>/`,
  polluting tracked space and requiring a `.gitignore` entry that future agents would forget). `/tmp` auto-prunes on
  macOS, so stale files from crashes don't accumulate.
- Cleanup on `process.on('exit')` is best-effort. SIGKILL / OOM / terminal close skip it; `/tmp` handles that.
- Schema fields:
  - `identifier`: `com.veszelovszki.cmdr-<instance>` (omitted when instance is unset → prod default).
  - `productName`: `Cmdr (<label>)` where label is `dev`, `dev foo`, `E2E nonmtp1`, etc. Spaces are fine (Dock string,
    not bundle ID). For prod, omitted.
  - `app.withGlobalTauri`: `true` for dev / E2E only (matches today's `tauri.dev.json`).
  - `plugins.updater.endpoints`: non-prod uses a dead URL (`https://localhost.invalid/no-updater`). Prod omits the field
    so canonical `tauri.conf.json` applies.
  - `build.devUrl`: filled in by P4. For P1, omitted (the static `http://localhost:1420` from `tauri.conf.json` still
    applies).

**Implementation steps:**

1. In `apps/desktop/scripts/tauri-wrapper.js`:
   - Parse `--worktree <name>` (positionally placed before the `--` separator).
   - Confirm `pnpm dev --worktree foo -- --features virtual-mtp` parses correctly (instance before `--`, Tauri args
     after); add a wrapper-self-test for this layout.
   - Sanitize via a new `apps/desktop/scripts/instance-id.js` (split out so it's Vitest-testable).
   - Resolve `CMDR_INSTANCE_ID` (env wins, then `--worktree`-derived `dev-<slug>`, then `dev` in dev mode, else unset
     for prod).
   - Compute `CMDR_DATA_DIR` (env wins, else for non-prod). On macOS:
     `~/Library/Application Support/com.veszelovszki.cmdr-<instance>/`. On Linux: `$XDG_DATA_HOME ?: ~/.local/share`
     joined with `com.veszelovszki.cmdr-<instance>`. Mirrors today's
     [`tauri-wrapper.js:38-39`](../../apps/desktop/scripts/tauri-wrapper.js).
   - `fs.mkdtempSync` + write the generated config under `$TMPDIR`. Pass absolute path to `-c`.
   - Register `exit` / `SIGINT` / `SIGTERM` cleanup (best-effort).
2. Delete `apps/desktop/src-tauri/tauri.dev.json` (replaced by generated file).
3. Update every reference to `tauri.dev.json` in the tree:
   - `AGENTS.md:156` (Debugging > Data dirs paragraph).
   - `CONTRIBUTING.md:62` (dev launch instructions).
   - `docs/security.md:13-15` (withGlobalTauri rationale).
   - `apps/desktop/CLAUDE.md:13` (running section).
   - `apps/desktop/src-tauri/src/config.rs:58` (security comment).
   - `apps/desktop/src-tauri/src/mtp/CLAUDE.md:24` (virtual-mtp launch command).

**Files added / modified:**

- `apps/desktop/scripts/tauri-wrapper.js`: sanitization call, env composition, file generation, cleanup.
- `apps/desktop/scripts/instance-id.js` (new): pure-Node sanitizer + ID derivation; exported for Vitest.
- `apps/desktop/src-tauri/tauri.dev.json`: deleted.
- The 6 doc/code refs above.

**TDD / test plan:**

- Vitest at `apps/desktop/test/wrapper-instance-id.test.ts`: imports the sanitizer from `instance-id.js`. Cases: `foo`,
  `Feature/X`, empty, dev, 200-char truncation, unicode, runs-of-dashes collapse. Plus a derive-instance-ID test
  covering the four scenarios in the table above.
- A second test verifies the arg-layout parser handles `pnpm dev --worktree foo -- --features virtual-mtp` without
  swallowing `--features`.
- Rust unit test in `config.rs::tests`: set `CMDR_DATA_DIR=/tmp/cmdr-p1-test-<pid>`, call `resolved_app_data_dir`,
  assert. Restore env.

**Docs to update:**

- `apps/desktop/CLAUDE.md`: section on `--worktree`, what `CMDR_INSTANCE_ID` means, precedence table mirroring this
  plan.
- `AGENTS.md` § Worktrees: "Use `pnpm dev --worktree <name>` inside a worktree so plugins, ports, and clipboard don't
  collide with your other sessions."
- `AGENTS.md` § Debugging "Data dirs": generalize to "prod, dev, and dev-per-worktree each get their own data dir; the
  wrapper sets `CMDR_INSTANCE_ID` and `CMDR_DATA_DIR` together."

**Checks to run before commit:** `./scripts/check.sh`.

**Commit message:**

```
Tooling: introduce CMDR_INSTANCE_ID + --worktree flag

- Wrapper resolves CMDR_INSTANCE_ID (env, --worktree slug, or "dev"
  in dev mode; unset in prod) and writes a generated tauri.instance.json
  with identifier + productName suffixed accordingly. Generated under
  $TMPDIR so the repo stays clean even on crash.
- Replaces the static tauri.dev.json.
- Identifier change auto-redirects tauri-plugin-store (settings.json)
  and tauri-plugin-window-state because both resolve via
  BaseDirectory::AppData. Verified against plugin source 2.4.2 / 2.4.1.
- Worktree slug sanitized (lowercase, [a-z0-9-]+, max 32 chars) in
  Node so users see rejection before any Rust process starts.
- Prod path unchanged: no instance ID set → no config override → same
  identifier, same data dir, same dock label.
```

**Definition of done:**

- `pnpm dev` → log `Using CMDR_DATA_DIR: ~/Library/Application Support/com.veszelovszki.cmdr-dev`. Dock shows
  `Cmdr (dev)`.
- `pnpm dev --worktree foo` → `Cmdr (dev foo)`, separate data dir. **Crucial verification:** after toggling a setting in
  the per-worktree session and quitting, the file at
  `~/Library/Application Support/com.veszelovszki.cmdr-dev-foo/settings.json` exists with the toggle persisted, and the
  prod `~/Library/Application Support/com.veszelovszki.cmdr/settings.json` is untouched. Same for `window-state.json`
  (window-state plugin file).
- Two `pnpm dev --worktree {X,Y}` from two worktrees coexist; their `settings.json` writes don't collide.
- `pnpm build` → identifier still `com.veszelovszki.cmdr`, no generated config file lingers.

---

### P2: ephemeral Cmdr-MCP port + port file + tauri-MCP plugin security fix

**Scope (in):** Cmdr's MCP server binds `127.0.0.1:0`, writes the bound port to `<data_dir>/mcp.port` atomically.
Tauri-MCP plugin (`tauri-plugin-mcp-bridge` 0.11.1) gets two changes: **(a) force `bind_address("127.0.0.1")` regardless
of instance: this is a security fix; the plugin defaults to `0.0.0.0`, exposing the bridge to the LAN**, and (b) bind on
a wrapper-supplied ephemeral port, with `<data_dir>/tauri-mcp.port` written by the **wrapper before Tauri launches**
(see N1 below for why). Update `developer.mcpPort` UI to reflect ephemeral-by-default.

**Scope (out):** Auth changes. Cross-machine bridging.

**Why ephemeral ports (not env vars).** Env vars are bound at process launch. The actual Cmdr-MCP port isn't known until
`bind(127.0.0.1:0)` returns. Clients (CLI helpers, agent fixtures) need a runtime-discoverable address. A file in the
data dir is the most direct contract: every client already knows where the data dir is, the file is atomic to write, and
stale files are detectable (ECONNREFUSED on the contained port).

**Why a file (not, for example, mDNS-style discovery).** Localhost only. No daemon. No new protocol. Data dir is already
the per-instance namespace.

**Tauri-MCP plugin verified state (read against 0.11.1 cached source):**

- `Config::default()` has `bind_address: "0.0.0.0"` and `base_port: 9223`.
- Only public configurators are `bind_address(&str)` and `base_port(u16)`.
- `init_with_config` calls `find_available_port(&bind_address, base_port)` synchronously in `setup`, then
  `WebSocketServer::new(port, ...)` and `ws_server.start()` does its own `TcpListener::bind(&self.addr).await`
  (`websocket.rs:123`). Two binds: one sync probe, one async real one. Race window even with a wrapper-allocated
  `base_port`.
- `find_available_port` (`discovery.rs:16-29`) silently returns `base_port` when no port in the
  `base_port..base_port+99` range is free. No `Result`, no error path. The plugin then tries to bind a taken port; the
  spawned task logs an error and exits.
- **No public method to query the bound port.** No accessor on `Plugin`, no event, nothing.

**Decided path for the tauri-MCP plugin (resolves N1):**

1. The wrapper allocates the port via `net.createServer().listen(0)`, releases immediately, exports
   `CMDR_MCP_BRIDGE_PORT` (renamed from `CMDR_MCP_BRIDGE_PORT` per N8 to match the plugin's actual name).
2. **The wrapper writes `<data_dir>/tauri-mcp.port` BEFORE launching Tauri** (it already has both the port and the data
   dir from the same composition step). External readers see the file appear at the same moment as the Tauri process;
   the race window is asymmetric (wrapper writes early, plugin binds late) but in the right direction: readers that
   connect early get ECONNREFUSED on the right port, which they retry, then succeed once the plugin has bound.
3. On Tauri startup, Rust reads `CMDR_MCP_BRIDGE_PORT`, sets `.bind_address("127.0.0.1").base_port(port)`.
4. After plugin setup completes, Rust runs a 500 ms post-bind probe: `TcpStream::connect("127.0.0.1:port")`. On success,
   log `info!` "tauri-MCP bound to 127.0.0.1:<port>". On failure, log `warn!` "tauri-MCP plugin did not bind to <port>
   within 500 ms; the port file at <data_dir>/tauri-mcp.port may be stale" and continue (don't block startup; external
   readers will discover staleness on first request).

Vendoring/forking the plugin to add a public bound-port channel is the alternative; rejected as heavier than the problem
warrants for the current use case.

**`developer.mcpPort` setting reframe (resolves N3 + N4):**

- Today: [`settings-registry.ts:683-700`](../../apps/desktop/src/lib/settings/settings-registry.ts) is a
  `SettingNumberInput` with `min: 1024, max: 65535`, default `19225` (dev) / `19224` (prod).
  `McpServerSection.svelte:54-57` already pulls the running port via `getMcpPort()` into `runningPort` and displays it
  at `:195-201`: `"Server is running on port {runningPort}"` plus
  `"(port {getSetting('developer.mcpPort')} was in use)"` when they differ.
- New:
  1. Lower `min` from `1024` to `0` in the registry. Comment: `// 0 = ephemeral`. **One-line change.** Without this the
     new default is rejected at save time or silently clamped.
  2. Change default to `0` (ephemeral).
  3. In the existing `{#if serverRunning && runningPort}` block in `McpServerSection.svelte:195-201`: when the setting
     is `0`, render `"Server is running on port {runningPort} (ephemeral)"` instead of the today copy. When the setting
     is non-zero and matches `runningPort`, today's copy unchanged. When the setting is non-zero and differs, today's
     copy unchanged. **No new IPC, no new copy block.** Extends the existing conditional.
- Tests:
  - Update `McpServerSection.a11y.test.ts:13,26,30` mock: setting=`0`, `getMcpPort` returns `57821`; assert the rendered
    string contains both `'0'` and `'57821'` (structural pattern; don't pin the exact "(ephemeral)" string).
  - Add a small Vitest behavior test in `McpServerSection.test.ts` (new or extend existing): setting=0 + running →
    rendered text contains "(ephemeral)"; setting=19000 + running on 19000 → no "(ephemeral)", no "(in use)".

**Port-file format and write protocol:**

- Path: `<data_dir>/mcp.port` (Cmdr MCP server) and `<data_dir>/tauri-mcp.port` (tauri-MCP plugin).
- Content: ASCII decimal port plus trailing newline. Nothing else.
- Server-side write:
  1. After `bind_with_probe()` returns the actual port
     ([`server.rs:114`](../../apps/desktop/src-tauri/src/mcp/server.rs)), open `<data_dir>/mcp.port.tmp.<pid>` for
     write.
  2. Write `"{port}\n"`.
  3. `fsync`.
  4. `rename` to `<data_dir>/mcp.port` (POSIX atomic rename, same fs).
  5. On shutdown (handle drop / SIGTERM), `unlink` best-effort.
- Client-side read (Bash for `mcp-call.sh`, Node for E2E fixtures):
  - Poll every 50 ms up to 5 s. The file appears non-empty only after the atomic rename, so a zero-byte read can't
    happen.
  - Parse `u16`; reject parse failures with a typed error (`PortDiscoveryError::InvalidContent`). Don't silently fall
    back to a legacy default; that hides bugs.
- **Read precedence:** `CMDR_MCP_PORT` env → port file → typed error.
- **Write precedence:** if `CMDR_MCP_PORT` is set, the server uses that port AND writes it to the file. External readers
  don't need to special-case pinned mode.
- **FE path unchanged:** `get_mcp_port` IPC keeps working as today
  ([`bindings.ts:1049`](../../apps/desktop/src/lib/ipc/bindings.ts), used at `mcp-client.ts:18`, `smb.spec.ts:78`). It
  reads `MCP_ACTUAL_PORT` in-process; the port file is the out-of-process equivalent.

**Implementation steps:**

1. New `apps/desktop/src-tauri/src/mcp/port_file.rs`: `write_port_file`, `remove_port_file`, `read_port_file`, typed
   `PortDiscoveryError`.
2. In `mcp/server.rs`, after `MCP_ACTUAL_PORT.store(...)`, call `write_port_file(data_dir, "mcp.port", port)`. In
   `stop_mcp_server()`, call `remove_port_file(data_dir, "mcp.port")` best-effort. Store the resolved data dir in a
   `OnceLock<PathBuf>` so the remove path doesn't need the AppHandle.
3. In `lib.rs`, change the tauri-MCP plugin init (around line 219) to:
   - Always set `.bind_address("127.0.0.1")` (security fix; flag this in the commit).
   - If `CMDR_MCP_BRIDGE_PORT` env is set, pass `.base_port(port)`; else fall back to default `9223`.
   - After plugin setup, run a 500 ms post-bind `TcpStream::connect` probe at `127.0.0.1:<port>`. On success,
     `log::info!`. On failure, `log::warn!` that the port file may be stale; don't block startup.
4. In wrapper: allocate `CMDR_MCP_BRIDGE_PORT` via `net.createServer().listen(0)`, then immediately:
   - Write `<data_dir>/tauri-mcp.port` via tempfile+rename (Node `fs.mkdtempSync` for the tempfile parent, then
     `fs.renameSync`). The wrapper has both port and data dir from the same composition step.
   - Thread `CMDR_MCP_BRIDGE_PORT` through the env to the Tauri spawn.
   - Register cleanup to `unlink` the file on `exit` (best-effort; `/tmp` covers crash leftovers).
5. In `scripts/mcp-call.sh`: when `CMDR_INSTANCE_ID` is set, resolve `CMDR_DATA_DIR` via the same per-OS rules, poll
   `<data_dir>/mcp.port` with a 5 s deadline, parse, use. `CMDR_MCP_PORT` still wins.
6. New `apps/desktop/test/e2e-shared/port-file.ts`: Node helper for the port-file read protocol. Used by any E2E fixture
   that needs MCP. **`mcp-client.ts` keeps the `get_mcp_port` IPC path** (it has a tauriPage; no reason to read the
   file).
7. **`developer.mcpPort` reframe (N3 + N4):**
   - `settings-registry.ts:683-700`: change `min: 1024` to `min: 0` with a `// 0 = ephemeral` comment; change default to
     `0`.
   - `McpServerSection.svelte:195-201`: extend the existing `{#if serverRunning && runningPort}` block so setting=0
     renders `"Server is running on port {runningPort} (ephemeral)"`. Today's copy for non-zero settings unchanged. No
     new IPC.
   - `McpServerSection.a11y.test.ts`: update mock to setting=0 + `getMcpPort` returns 57821; assert structural pattern
     (contains `'0'` AND `'57821'`).
   - `McpServerSection.test.ts`: behavior test confirming `(ephemeral)` rendered when setting=0.

**Files added / modified:**

- `apps/desktop/src-tauri/src/mcp/server.rs`: port-file write/remove.
- `apps/desktop/src-tauri/src/mcp/port_file.rs` (new): read/write/remove + typed error.
- `apps/desktop/src-tauri/src/lib.rs`: tauri-MCP plugin `bind_address("127.0.0.1")` + ephemeral `base_port` from
  `CMDR_MCP_BRIDGE_PORT` + 500 ms post-bind probe.
- `apps/desktop/scripts/tauri-wrapper.js`: allocates `CMDR_MCP_BRIDGE_PORT` AND writes `<data_dir>/tauri-mcp.port`
  before Tauri launch.
- `scripts/mcp-call.sh`: port discovery via file when instance is set.
- `apps/desktop/test/e2e-shared/port-file.ts` (new): Node poll helper.
- `apps/desktop/src/lib/settings/settings-registry.ts`: `min: 0` + default 0 + `// 0 = ephemeral` comment.
- `apps/desktop/src/lib/settings/sections/McpServerSection.svelte`: extend existing `{#if}` block, no new copy block.
- `apps/desktop/src/lib/settings/sections/McpServerSection.a11y.test.ts`: mock update + structural assertion.
- `apps/desktop/src/lib/settings/sections/McpServerSection.test.ts` (new or extended): behavior test.

**TDD / test plan:**

- Rust unit in `port_file.rs::tests`: write port to tempdir, read back, assert; zero bytes → typed error; missing →
  `NotFound`.
- Rust unit / integration in `mcp::server`: `tokio::test` against tempdir as data dir; start server; verify file appears
  with parseable port; shutdown; verify file gone.
- Vitest unit for `port-file.ts`: write `12345\n` to a tempdir, poll-read, assert; deadline expires on missing.
- Vitest behavior for `McpServerSection`: setting=0 + `getMcpPort` returns 57821 → rendered text contains `(ephemeral)`;
  setting=19000 + `getMcpPort` returns 19000 → no `(ephemeral)`, no `(in use)`.
- A11y test: structural assertion (contains `'0'` AND `'57821'`), not exact copy.
- No new Playwright spec for the FE flow; the security fix is verified manually by `lsof -i :9223` on a running app
  showing only `127.0.0.1` bind.

**Docs to update:**

- `apps/desktop/src-tauri/src/mcp/CLAUDE.md` § Server lifecycle: port file, atomic-write protocol, precedence rules
  (read + write).
- `docs/tooling/mcp.md`: port file auto-discovery for the CLI; `CMDR_INSTANCE_ID=dev-foo ./scripts/mcp-call.sh ...`
  recipe; security fix note for the tauri-MCP plugin bind address.

**Checks to run before commit:** `./scripts/check.sh`.

**Commit message:**

```
MCP: ephemeral port + port file + tauri-MCP bind 127.0.0.1

- Cmdr's MCP server binds 127.0.0.1:0 by default; writes the actual
  port to <data_dir>/mcp.port via tempfile+rename (atomic, fsync'd).
- Tauri-MCP plugin (0.11.1) now binds 127.0.0.1 (was 0.0.0.0, exposing
  the bridge to the LAN: load-bearing security fix). Plugin port is
  wrapper-allocated ephemeral via CMDR_MCP_BRIDGE_PORT. The wrapper
  writes <data_dir>/tauri-mcp.port BEFORE launching Tauri (plugin has
  no public bound-port accessor and silently keeps base_port on
  exhaustion). Rust runs a 500 ms post-bind probe and warn-logs on
  mismatch; readers retry on ECONNREFUSED.
- Read precedence: CMDR_MCP_PORT env → port file → typed error.
  Write precedence: pinned port still gets written to file so readers
  don't need to special-case.
- FE keeps the get_mcp_port IPC path.
- developer.mcpPort registry min lowered from 1024 to 0 (0 = ephemeral);
  default is 0. McpServerSection.svelte extends the existing
  serverRunning block to append "(ephemeral)" when setting=0. No new
  IPC, no new copy block.
```

**Definition of done:**

- `pnpm dev` writes `~/.../cmdr-dev/mcp.port` and `~/.../cmdr-dev/tauri-mcp.port`. `lsof -i -P` shows both bound to
  `127.0.0.1` only (no `*:9223`).
- Two `pnpm dev --worktree {a,b}` sessions: each writes distinct ports.
- `CMDR_INSTANCE_ID=dev-a scripts/mcp-call.sh --list-tools` works against the right session.
- Settings window shows `Server is running on port NNNNN (ephemeral)` with the actual port when setting=0.

---

### P3: Keychain service-name suffix + checker rewire to set `CMDR_INSTANCE_ID`

**Scope (in):** Append `-<instance>` to the Keychain `SERVICE_NAME` when `CMDR_INSTANCE_ID` is set. Go E2E checker sets
`CMDR_INSTANCE_ID=e2e-<kind>-<pid>` per shard. The `productName` change already landed in P1; this phase verifies the
process labels via `pgrep` for cleanup scripts.

**Scope (out):** mDNS suffix (verified out of scope; no advertising code exists, see top of plan). File-backed secret
store changes (dev keeps `CMDR_SECRET_STORE=file`; E2E already short-circuits via `is_e2e_mode()` at
[`secrets/mod.rs:88`](../../apps/desktop/src-tauri/src/secrets/mod.rs)).

**Why Keychain suffix.** Belt-and-suspenders. E2E already lands on the file backend, but a stray manual launch under
prod-flavored settings could still hit the real Keychain. Per-instance `SERVICE_NAME` makes that safe too.

**Migration:** Prod (`CMDR_INSTANCE_ID` unset) → `"Cmdr"` unchanged; dev was always file-backed; E2E recreates per run.
Zero data loss anywhere.

**Implementation steps:**

1. In `secrets/keychain_macos.rs`: replace `const SERVICE_NAME: &str = "Cmdr";` with a
   `service_name() -> Cow<'static, str>` helper. Read `CMDR_INSTANCE_ID` once via `OnceLock<String>`. Empty/unset →
   borrow `"Cmdr"`. Else owned `format!("Cmdr-{instance}")`. Pass through to `set_generic_password`,
   `get_generic_password`, `delete_generic_password`.
2. In `scripts/check/checks/desktop-svelte-e2e-playwright.go`:
   - Compose instance ID per shard: helper `shardInstanceID(s shardSpec) string` returning `"e2e-mtp-<pid>"` etc.
   - Add `"CMDR_INSTANCE_ID=" + shardInstanceID(s)` to the env in `startTauriApp` (file already at line ~386).
   - Add `"CMDR_MCP_BRIDGE_PORT=" + strconv.Itoa(s.tauriMcpPort)` per shard (P2 needs this for the plugin to bind
     ephemerally per shard; reserve via Go's `net.Listen("tcp", "127.0.0.1:0")` at plan time, mirroring the wrapper).
   - Cleanup `pkill` can now target `pgrep -f 'Cmdr (E2E '` specifically. Keep the existing `target.*Cmdr` fallback for
     stale local prod binaries.

**Files modified:**

- `apps/desktop/src-tauri/src/secrets/keychain_macos.rs`: service name helper.
- `scripts/check/checks/desktop-svelte-e2e-playwright.go`: set `CMDR_INSTANCE_ID` + `CMDR_MCP_BRIDGE_PORT` per shard;
  pgrep target update.

**TDD / test plan:**

- Rust unit in `keychain_macos.rs::tests` (gated `#[cfg(target_os = "macos")]`): set env, call `service_name()`, assert;
  document the once-cache contract by asserting a second call with mutated env returns the cached value.
- Go unit (extend existing checks tests): `shardInstanceID` produces the right formats for each kind.

**Docs to update:**

- `apps/desktop/src-tauri/src/secrets/CLAUDE.md`: instance suffix on `SERVICE_NAME`.

**Checks to run before commit:** `./scripts/check.sh`.

**Commit message:**

```
Isolation: Keychain SERVICE_NAME + E2E checker pass instance ID

- macOS Keychain SERVICE_NAME becomes "Cmdr-<instance>" when set, else
  "Cmdr". Prod path unchanged.
- E2E checker stamps CMDR_INSTANCE_ID=e2e-<kind>-<pid> and
  CMDR_MCP_BRIDGE_PORT per shard. pgrep cleanup now targets
  "Cmdr (E2E " specifically; fallback to target.*Cmdr stays.
- mDNS suffix dropped from scope: mdns_discovery.rs only browses,
  doesn't advertise. False problem in the original context bundle.
```

**Definition of done:**

- `CMDR_INSTANCE_ID=e2e-abc` + force Keychain backend → `security find-generic-password -s Cmdr-e2e-abc` returns the
  entry.
- Two E2E shards in parallel: their Cmdr processes appear as `Cmdr (E2E nonmtp1)` and `Cmdr (E2E nonmtp2)` in Activity
  Monitor.

---

### P4: Vite dynamic port + non-prod updater stub

**Scope (in):** Wrapper picks an ephemeral port via `net.createServer().listen(0)` for Vite. Writes it into the
generated `tauri.instance.json` as `build.devUrl: http://localhost:<port>` and exports `CMDR_VITE_PORT`.
`vite.config.js` reads it. Updater endpoint stub for non-prod was already in P1's generated config; verified here.

**Scope (out):** HMR secondary port (`1421`). Currently unused on macOS dev (no `TAURI_DEV_HOST`); leave it alone. Defer
if observed.

**Why ephemeral Vite port.** Two `pnpm dev` from two worktrees today: the second's Vite tries to bind `1420`,
`EADDRINUSE`, the second wrapper dies. Ephemeral ports fix it cleanly.

**Why allocate in wrapper (not Vite).** Vite knows `CMDR_VITE_PORT` only after reading. Tauri's `build.devUrl` is read
by `tauri dev` to point the webview, also at startup. Wrapper has to know the port to fill both. The allocation race
window (between `server.listen(0)` and Vite bind) is tens of ms; if it ever flakes, switch to a probe loop.

**Implementation steps:**

1. In wrapper: helper `pickEphemeralPort()` via Node's `net`. Set `env.CMDR_VITE_PORT = String(port)`. Add
   `build.devUrl: \`http://localhost:${port}\`` to the generated config.
2. In `vite.config.js`: read `process.env.CMDR_VITE_PORT`, fall back to `1420` when unset (raw `pnpm vite dev` outside
   the wrapper still works). `strictPort: true` stays: hard failure on race, not silent migration.

**Files modified:**

- `apps/desktop/scripts/tauri-wrapper.js`: Vite port allocation + devUrl threading.
- `apps/desktop/vite.config.js`: read `CMDR_VITE_PORT`.

**TDD / test plan:**

- Vitest in `wrapper-instance-id.test.ts` (extend): mock `net.createServer`, assert port threaded into env + generated
  config.

**Docs to update:**

- `apps/desktop/CLAUDE.md`: note Vite uses an ephemeral port in dev via wrapper; raw `pnpm vite dev` is 1420.

**Checks to run before commit:** `./scripts/check.sh`.

**Commit message:**

```
Tooling: ephemeral Vite dev port per instance

- Wrapper allocates port via net.createServer().listen(0), threads
  into CMDR_VITE_PORT and generated config's build.devUrl.
- vite.config.js reads CMDR_VITE_PORT; falls back to 1420 for raw
  pnpm vite dev outside the wrapper.
- strictPort stays on: a race fails loudly rather than silent-migrate.
- Two pnpm dev sessions from two worktrees coexist with no
  EADDRINUSE on 1420.
```

**Definition of done:**

- Two `pnpm dev --worktree {a,b}`: each logs a different `Vite listening on http://localhost:NNNNN`. Both windows load.
- `pnpm vite dev` (no wrapper) still binds 1420.

---

### P5: per-instance fixture root + hardlink cache + NSPasteboard mock

**Scope (in):** macOS Playwright E2E fixtures move to `/tmp/cmdr-e2e-fixtures-<instance>/`. Bulk .dat files hardlinked
from a content cache at `/tmp/cmdr-e2e-fixtures-cache/` (verified-rename pattern). Clipboard backend split via
`#[cfg(feature = "playwright-e2e")]` at module level (no `dyn` trait). Backend selection via the Cargo feature (primary)
plus `CMDR_CLIPBOARD_BACKEND=mock` env (manual override for dev debugging).

**Scope (out):**

- **Linux Docker E2E.** Single shard, no concurrent runs, less benefit. Keep `/tmp/cmdr-e2e-fixtures` shared and
  uncached. macOS-only changes. Flagged here explicitly so a future contributor doesn't accidentally extend.
- Mocking the system clipboard for non-E2E users.
- Linux `text/uri-list` clipboard work (future).

**Why hardlinks for fixtures.** Each shard's bulk fixture data is ~170 MB. With N=3 shards, 510 MB per checker
invocation. With concurrent runs (multi-agent), `/tmp` quota becomes real. Bulk .dat files are read-only by tests, so
hardlinks are safe: any mutating test would also mutate the cache, which none do.

**Why mock NSPasteboard (not save-and-restore).** Save-and-restore is racy: the user might `Cmd+C` mid-test. Worse,
macOS `changeCount` is global; reading it can race with another app. Never touch the real pasteboard.

**Clipboard backend selection (verified call-site survey):**

Three free functions exported from `clipboard/mod.rs:12`, all `#[cfg(target_os = "macos")]`:

- `read_file_urls_from_clipboard` (1 call site: `commands/clipboard.rs:170`).
- `read_text_from_clipboard` (1: `commands/clipboard.rs:213`).
- `write_file_urls_to_clipboard` (4: `commands/clipboard.rs:45, 77, 107, 146`).

Every call site is inside `app.run_on_main_thread(move || { ... })`. The functions themselves don't take any `Send`
types; the marshaling is the caller's concern. Threading contract: function bodies must run on the main thread
(NSPasteboard is not thread-safe).

**Mechanism: module-level `#[cfg]` switch, not a `dyn` trait.** Call sites stay identical (same free functions, same
signatures). `clipboard/mod.rs` becomes:

```rust
#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
mod pasteboard;
#[cfg(all(target_os = "macos", not(feature = "playwright-e2e")))]
pub use pasteboard::{read_file_urls_from_clipboard, read_text_from_clipboard, write_file_urls_to_clipboard};

#[cfg(all(target_os = "macos", feature = "playwright-e2e"))]
mod mock_pasteboard;
#[cfg(all(target_os = "macos", feature = "playwright-e2e"))]
pub use mock_pasteboard::{read_file_urls_from_clipboard, read_text_from_clipboard, write_file_urls_to_clipboard};
```

Mock impl uses a `LazyLock<Mutex<Option<(Vec<PathBuf>, String)>>>`. The mock can be called from any thread (the mutex
serializes); the `run_on_main_thread` marshaling at call sites is harmless overhead in mock mode. No `dyn`, no trait
object, no `objc2::Send` problem. The `CMDR_CLIPBOARD_BACKEND=mock` env override is a runtime check inside the **prod**
module: when set, the prod functions delegate to the same in-process store the mock uses (extracted into a shared
`clipboard/store.rs`). Both code paths exercised on demand without recompiling.

**Hardlink cache, verified-rename pattern:**

- Cache canonical path: `/tmp/cmdr-e2e-fixtures-cache/` (version `v1` implied; bump dir name if file shape changes).
- Build protocol:
  1. `mkdtempSync('/tmp/cmdr-e2e-fixtures-cache-tmp-')` → unique `cache-tmp-<rand>/`.
  2. Generate all files into `cache-tmp-<rand>/`.
  3. **Verify each file's content hash** (sha256 of zero-filled content of the expected size is deterministic; hardcode
     in the fixture script for each `.dat` size). NOT just `statSync.size`: a crashed mid-write process leaves a
     right-size zero-filled file that trivially passes a size check but might be torn.
  4. If verify passes, `fs.renameSync('/tmp/cmdr-e2e-fixtures-cache-tmp-<rand>', '/tmp/cmdr-e2e-fixtures-cache')`.
     POSIX-atomic rename. If the target already exists from a concurrent winner, `renameSync` overwrites on POSIX but
     throws on Linux when the target is a non-empty dir: catch `EISDIR`/`ENOTEMPTY`, delete our `cache-tmp-` and treat
     the winner as authoritative.
  5. **`EXDEV` handling:** if `renameSync` throws `EXDEV` (Linux bind-mounted `/tmp`), fall back to a full copy into the
     destination (slow but correct). Document this; macOS doesn't hit it.
- Consumer protocol: each shard's `createFixtures()`:
  1. If `/tmp/cmdr-e2e-fixtures-cache/` exists, hardlink the bulk files into the per-instance dir
     (`fs.linkSync(cacheFile, fixtureFile)`).
  2. Else, build the cache (the protocol above), then hardlink.
  3. Two concurrent builders are fine: each builds into its own `cache-tmp-<rand>`, only one wins the rename. The
     loser's tmp dir is removed in step (4); the loser then hardlinks from the winner's cache.
- **Owner: Node.** Lives in `apps/desktop/test/e2e-shared/fixtures.ts`. Fixture creation is already Node; keeps the
  cache logic colocated with the consumer.

**`CMDR_E2E_START_PATH` migration:**

Every read site updated to use per-instance paths (when `CMDR_INSTANCE_ID` is set):

- `apps/desktop/test/e2e-playwright/helpers.ts:773`.
- `apps/desktop/test/e2e-playwright/accessibility.spec.ts:51`.
- `apps/desktop/test/e2e-playwright/viewer.spec.ts:20`.
- `apps/desktop/test/e2e-playwright/global-setup.ts:17`.
- `apps/desktop/test/e2e-playwright/global-teardown.ts:16`.
- `apps/desktop/test/e2e-linux/e2e-linux.sh:435`: Linux scope out (stays shared, see scope).
- `apps/desktop/src/lib/file-explorer/pane/initialization.ts:37`: FE-side reference, no change needed (runs inside the
  app process; sees the same env the wrapper composed). Listed for completeness so a future search doesn't surprise.
- Docs: `apps/desktop/test/CLAUDE.md` § Shared fixture system; `apps/desktop/test/e2e-playwright/CLAUDE.md` running
  sections; `apps/desktop/test/e2e-linux/CLAUDE.md` (note that Linux stays shared).

**Spec-file fallbacks:** every `process.env.CMDR_E2E_START_PATH ?? '/tmp/cmdr-e2e-fallback'` (or similar) becomes
`throw new Error('CMDR_E2E_START_PATH must be set...')`. The silent fallback hides setup bugs.

**Implementation steps:**

1. Add `apps/desktop/src-tauri/src/clipboard/mock_pasteboard.rs` with the three free functions backed by a
   `LazyLock<Mutex<...>>`.
2. Update `clipboard/mod.rs` with the `#[cfg]` switch as above.
3. Add `CMDR_CLIPBOARD_BACKEND=mock` runtime check inside `pasteboard.rs`'s three functions: when set, delegate to the
   same store the mock module uses. Extract the store into `clipboard/store.rs` shared by both modules.
4. In `apps/desktop/test/e2e-shared/fixtures.ts`:
   - Add `getCacheRoot()`, `ensureCacheBuilt()` with the verified-rename + `EXDEV` handling.
   - Rewrite `createFixtures(instanceId?: string)`: return `/tmp/cmdr-e2e-fixtures-<instance>-<ts>/` (or
     `/tmp/cmdr-e2e-fixtures-<ts>/` when no instance, preserving legacy behavior for Linux).
   - Replace `largeFiles`/`mediumFiles` `dd` loops with `fs.linkSync(cacheFile, fixtureFile)` after
     `ensureCacheBuilt()`. Text files stay as real copies.
   - `cleanupFixtures()`: widen the prefix guard to `/tmp/cmdr-e2e-fixtures-` (was `/tmp/cmdr-e2e-`).
   - Hardcode the sha256 of zero-filled 50 MB and 1 MB content as constants for verify.
5. In `global-setup.ts`: pass the shard's instance ID from env.
6. In every spec / helper / linux script listed above: replace `?? '/tmp/cmdr-e2e-fallback'` with throws.
7. In `desktop-svelte-e2e-playwright.go`: `createE2EFixtures()` calls the Node helper with the shard's instance ID.

**Files added / modified:**

- `apps/desktop/src-tauri/src/clipboard/mock_pasteboard.rs` (new).
- `apps/desktop/src-tauri/src/clipboard/store.rs` (new): shared `LazyLock<Mutex<...>>` for mock mode.
- `apps/desktop/src-tauri/src/clipboard/mod.rs`: `#[cfg]` module switch.
- `apps/desktop/src-tauri/src/clipboard/pasteboard.rs`: runtime env check delegating to store.
- `apps/desktop/test/e2e-shared/fixtures.ts`: cache, hardlinks, per-instance root, EXDEV.
- `apps/desktop/test/e2e-playwright/global-setup.ts`: pass instance ID.
- The 5 spec/helper files listed under the `CMDR_E2E_START_PATH` migration: throw instead of fallback.
- `scripts/check/checks/desktop-svelte-e2e-playwright.go`: pass instance ID to fixture call.

**TDD / test plan:**

- Rust unit in `clipboard/store.rs::tests`: concurrent writers serialize; read returns last write; read empty returns
  empty.
- Rust unit at `clipboard/pasteboard.rs`: `CMDR_CLIPBOARD_BACKEND=mock` causes the prod fn to NOT touch NSPasteboard
  (pragmatic: use a once-flag the test sets and the prod fn checks).
- Vitest in `apps/desktop/test/e2e-shared/fixtures.test.ts` (new): cache build happy path; cache hit; lock-held
  (simulate via pre-created tmp dir + delayed rename); per-instance fixture creation; inode equality between fixture and
  cache (`fs.statSync(fixturePath).ino === fs.statSync(cachePath).ino`); content-hash verify catches a torn file.
- One Playwright spec gets a small added assertion: after `Cmd+C` of a fixture file, `execSync('pbpaste')` from Node
  returns NOT the fixture path. One spec body ≤ 1 s after `ensureAppReady()`.
- Manual: open TextEdit with random clipboard contents; run full E2E; `pbpaste` unchanged at the end.

**Docs to update:**

- `apps/desktop/src-tauri/src/clipboard/CLAUDE.md`: module-switch mechanism, env override, acceptance criterion (E2E
  doesn't touch real pasteboard).
- `apps/desktop/test/CLAUDE.md`: fixture cache + hardlinks; per-instance fixture root format; verified-rename protocol;
  macOS-only scope.
- `apps/desktop/test/e2e-linux/CLAUDE.md`: explicit note: Linux Docker stays shared, no per-instance fixture, no cache.
  Single shard, less benefit.

**Checks to run before commit:** `./scripts/check.sh` then `./scripts/check.sh --include-slow` (E2E surface changed:
verify two concurrent runs from two worktrees coexist with no fixture races).

**Commit message:**

```
E2E: per-instance fixture root + clipboard mock backend

- Fixtures live at /tmp/cmdr-e2e-fixtures-<instance>-<ts>/ (was
  shared /tmp/cmdr-e2e-<ts>/). Bulk .dat hardlinked from
  /tmp/cmdr-e2e-fixtures-cache/. Cache build: tmp-dir +
  content-hash verify + atomic rename; EXDEV fallback for bind-mounts.
- Clipboard backend split via cfg(feature = "playwright-e2e") at the
  module level (no dyn trait). Mock impl is a process-local
  Mutex<Option<...>>. CMDR_CLIPBOARD_BACKEND=mock env override works
  on the prod module too via a shared store.
- macOS-only: Linux Docker E2E (single shard) stays on the shared
  fixture root, no cache.
- E2E runs no longer mutate the user's macOS clipboard; two concurrent
  runs from two worktrees no longer race for the fixture root.
- Spec-file CMDR_E2E_START_PATH fallbacks now throw instead of silently
  using /tmp/cmdr-e2e-fallback.
```

**Definition of done:**

- Full Playwright suite then `pbpaste`: same content as before.
- Two `./scripts/check.sh --check desktop-e2e-playwright` invocations from two worktrees coexist with no fixture errors,
  no clipboard mutation, no port collisions.
- `du -sh /tmp/cmdr-e2e-fixtures-*` after a 3-shard run: three small dirs (text only) plus one ~170 MB cache.
- Linux Docker E2E run is unchanged.

---

### P6: docs sweep + acceptance smoke

**Scope (in):** Sweep colocated `CLAUDE.md`s for port numbers, bundle IDs, fixture roots. Two-worktree dev + two-
worktree E2E smoke tests. Add `docs/tooling/instance-isolation.md` so future contributors find the concept by searching
any of its surfaces.

**Scope (out):** Renaming. Removing legacy env vars (`CMDR_DATA_DIR`, `CMDR_MCP_PORT`, `CMDR_SECRET_STORE`; all still
authoritative). Promoting any setting from internal to visible.

**Why last.** Docs sweep wants the full picture. Per-phase sweeps re-rewrite the same files.

**Implementation steps:**

1. Walk and update:
   - `AGENTS.md`: § Debugging "Data dirs", § Worktrees, § "MCP", § Workflow.
   - `apps/desktop/CLAUDE.md`: § Running, § Structure, new § "Instance isolation".
   - `apps/desktop/src-tauri/src/mcp/CLAUDE.md`: § Server lifecycle, § Configuration, port mentions.
   - `apps/desktop/src-tauri/src/secrets/CLAUDE.md`: service name (verify post-P3).
   - `apps/desktop/src-tauri/src/clipboard/CLAUDE.md`: mock backend (verify post-P5).
   - `apps/desktop/src-tauri/src/settings/CLAUDE.md`: data-dir resolution + identifier-driven path.
   - `apps/desktop/test/CLAUDE.md`: fixture cache (verify post-P5).
   - `apps/desktop/test/e2e-playwright/CLAUDE.md`: fixture paths, per-shard data dir.
   - `apps/desktop/test/e2e-linux/CLAUDE.md`: explicit "stays shared" note.
   - `scripts/check/CLAUDE.md`: § Self-contained E2E checks, instance-ID env mention.
   - `docs/tooling/mcp.md`: port discovery + security bind-address.
   - `docs/architecture.md`: quick mention in § "Dev mode" pointing to the new primer.
2. Add `docs/tooling/instance-isolation.md` (≤ 200 lines): the primitive, env vars, precedence, links to colocated
   `CLAUDE.md`s. Canonical reference; future fixes get a one-liner here.
3. Smoke tests:
   - Two worktrees, `pnpm dev --worktree a` and `pnpm dev --worktree b`. Both windows open. Quitting one doesn't affect
     the other.
   - Third terminal: `./scripts/check.sh --check desktop-e2e-playwright` in each worktree concurrently. Both pass.
     Spot-check `du -sh /tmp/cmdr-e2e-fixtures-cache` is ~170 MB (one cache, shared).

**Files added / modified:** The 12 docs above plus `docs/tooling/instance-isolation.md`.

**TDD / test plan:** None automated. Smoke tests are manual.

**Checks to run before commit:** `./scripts/check.sh` then `./scripts/check.sh --include-slow` (final gate).

**Commit message:**

```
Docs: sweep for CMDR_INSTANCE_ID, ports, fixture roots

- Update every colocated CLAUDE.md that mentioned fixed ports, bundle
  IDs, fixture paths, or the dev-vs-prod data-dir split.
- New docs/tooling/instance-isolation.md as the canonical primer.
- AGENTS.md § Debugging: generalize the data-dir paragraph;
  § Worktrees: mention pnpm dev --worktree.
```

**Definition of done:**

- A new contributor reading `AGENTS.md` cold understands they should pass `--worktree` from a worktree.
- Searching the repo for `19224` returns only release-notes / changelog hits.
- Both smoke tests pass.

---

## Cross-cutting concerns

### Compat / migration (prod users)

**No migration.** Prod stays on `com.veszelovszki.cmdr` with `CMDR_INSTANCE_ID` unset. Data dir, Keychain entries, MCP
port default, plugin-store path, updater endpoint all bit-for-bit identical. `tauri-plugin-store` and
`tauri-plugin-window-state` already resolve via `BaseDirectory::AppData`; without an instance, that's the same prod path
as today.

### macOS TCC

Bundle-ID-keyed. Existing prod users don't re-grant. Each new instance is a fresh TCC entry. E2E uses
`CMDR_MOCK_FDA=granted` ([`permissions.rs:124`](../../apps/desktop/src-tauri/src/permissions.rs)) to skip TCC entirely.
Dev experience unchanged: first `pnpm dev` triggers the FDA onboarding flow, dev bundle ID lands in TCC alongside prod.

### Keychain migration

Prod: zero (`SERVICE_NAME` stays `"Cmdr"`). Dev: file-backed already, nothing to lose. E2E: ephemeral data dir, nothing
to migrate.

### Worktree name → instance ID slug rules

Full definition in P1. Lowercase, `[a-z0-9-]+`, max 32, runs of `-` collapse, trim leading/trailing `-`, reject empty
post-trim. Sanitization in Node.

### Port-file format + atomicity

- `<data_dir>/mcp.port` (Cmdr server, written by Rust) and `<data_dir>/tauri-mcp.port` (plugin, written by the wrapper
  before Tauri launches: see N1 / P2 for why). ASCII decimal port plus `\n`.
- Writer: write to `.port.tmp.<pid>`, `fsync`, `rename` to `.port` (POSIX atomic on same fs).
- Client: poll every 50 ms up to 5 s. Reject parse failures with typed error.
- Read precedence: `CMDR_MCP_PORT` env → port file → typed error.
- Write precedence: pinned port still writes to file.
- FE path: `get_mcp_port` IPC (in-process), unchanged.

### Wrapper-allocated ephemeral ports: race and mitigation

Two consumers use the wrapper's `net.createServer().listen(0)` allocation trick. The race window (between Node closing
the listener and the downstream process binding) differs by consumer; so does the blast radius. Document both in one
place to avoid drift.

| Consumer              | Allocator | Race window                                                        | Mitigation                                                                                                                                                                            |
| --------------------- | --------- | ------------------------------------------------------------------ | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Vite dev server (P4)  | wrapper   | ~10 ms                                                             | `strictPort: true` in `vite.config.js`: fail loud on `EADDRINUSE`                                                                                                                     |
| Tauri-MCP plugin (P2) | wrapper   | up to seconds (Cargo rebuild on Rust change can delay Tauri start) | Wrapper writes `tauri-mcp.port` BEFORE Tauri launches; Rust runs a 500 ms post-bind `TcpStream::connect` probe and `warn!`-logs on mismatch; external readers retry on `ECONNREFUSED` |

The Tauri-MCP race is asymmetric in the right direction: the wrapper writes the file early, then Tauri starts. Readers
that connect before the plugin binds get `ECONNREFUSED` on the correct port and back off; once the plugin binds, the
next connect succeeds. The wrong-direction failure mode (plugin binds a DIFFERENT port than the file claims because
another process grabbed the allocated port mid-startup) is caught by the 500 ms probe and surfaced in logs. Both
consumers tolerate the brief uncertainty without dropping data.

### Mock-backend convention

The repo already mixes patterns. Resolved going forward:

- **Cargo feature** when the mock would otherwise require pulling in or compiling out heavy platform deps (`objc2`,
  `security-framework`, OS-specific FFI). Compile-time switch keeps prod binaries lean and removes whole code paths.
  Example: clipboard mock (P5) uses `#[cfg(feature = "playwright-e2e")]`.
- **Runtime env var** when the mock is just an alternative implementation of an existing trait/function and the prod
  path is light. Example: `CMDR_MOCK_FDA` (gates a few syscalls), `CMDR_E2E_MODE=1` (toggles soft hooks like the
  title-bar stripe), `CMDR_CLIPBOARD_BACKEND=mock` (delegates to the shared store from within the prod module, for
  ad-hoc dev debugging without a recompile).
- Both patterns can coexist on one subsystem (clipboard does: feature flag for the E2E build; env for the manual
  override on a prod-feature dev build). Document in the colocated `CLAUDE.md` which hook(s) the subsystem honors.

### Process-launch contract for E2E

Per-shard Go-checker sequence:

1. Compose env: `CMDR_INSTANCE_ID=e2e-<kind>-<pid>`, `CMDR_DATA_DIR=/tmp/cmdr-e2e-data-<instance>`, `CMDR_E2E_MODE=1`,
   `CMDR_MOCK_FDA=granted`, `CMDR_MCP_BRIDGE_PORT=<allocated>`,
   `CMDR_PLAYWRIGHT_SOCKET=/tmp/tauri-playwright-<instance>.sock`,
   `CMDR_E2E_START_PATH=/tmp/cmdr-e2e-fixtures-<instance>-<ts>`.
2. Wipe data dir if it exists (paranoid; instance ID includes `<pid>` so collisions don't happen in practice).
3. Launch binary.
4. Wait for `<data_dir>/mcp.port` (poll, 60 s) AND playwright socket (existing code).
5. Connect.

### Clipboard mock

`#[cfg(feature = "playwright-e2e")]` module-level switch (NOT a dyn trait): in E2E binaries `mock_pasteboard.rs`
replaces `pasteboard.rs` at compile time. The prod module additionally checks `CMDR_CLIPBOARD_BACKEND=mock` and
delegates to the shared `store.rs` when set. Acceptance: full E2E suite leaves `pbpaste` unchanged.

### Hardlink cache for fixtures

`/tmp/cmdr-e2e-fixtures-cache/`. Build: tmp-dir + content-hash verify + atomic rename. `EXDEV` fallback to copy. Owner:
Node. Per-instance dirs hardlink from it. Cache invalidation: never (bump dir name on file-shape change). macOS-only;
Linux Docker stays on the shared root.

### What if env-var combos go weird?

- Instance set, data dir unset → backend warn, falls through to identifier-derived default. Works but unsupported.
- Data dir set, instance unset → legacy. Data dir honored, no Keychain suffix, fixed-port MCP. P3's checker rewire
  eliminates this in practice.

## Parallelism notes

Default sequential per `/execute` rules. P1 introduces the env var; nothing parallelizes with it. P2 and P3 both touch
`lib.rs` (P2: MCP plugin init; P3: only keychain + Go checker, no lib.rs), so P3 can technically run in parallel with P4
(P4 only touches the wrapper + vite.config.js). Keep sequential anyway; the diff per phase is small enough that
parallelism saves little. The round-1 P1 collapse means there's no longer a "trivially-parallel two-line plugin fix" to
overlap with anything.

## Open questions for the executor leader

None. The round-2 review resolved the last hedge (`tauri-plugin-mcp-bridge` has no bound-port accessor, confirmed by
source read; P2's decided path is "wrapper writes port file before launch + Rust 500 ms post-bind probe"). Execution can
start at P1.
