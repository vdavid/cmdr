# Desktop app details

Operational depth for the desktop app: running, debugging, testing, MCP control, and worktree setup. Must-knows are in
`CLAUDE.md`; repo-wide knowledge is in `AGENTS.md`; the subsystem map is `docs/architecture.md`.

## Running

Run from the repo root: `pnpm dev` (start) and `pnpm build` (release build). The root `package.json` has no `tauri`
script, so don't `cd` here just to run the app. If you've already `cd`d into `apps/desktop/`, `pnpm tauri dev` is
equivalent; both paths invoke `scripts/tauri-wrapper.ts`.

The wrapper is the single source of truth for dev / prod path separation. It resolves `CMDR_INSTANCE_ID` (from
`--worktree <slug>`, the existing env, else `"dev"`), composes `CMDR_DATA_DIR` to match, and writes a fresh
`tauri.instance.json` under `$TMPDIR` that overrides the bundle identifier and `productName` for the instance. Bypassing
it (raw `cargo tauri dev` / `cargo build`) gives the wrong data dir or a binary with no embedded frontend.

`pnpm dev --worktree <slug>` runs side-by-side sessions from different worktrees without colliding on `settings.json`,
ports, or the Dock label. The slug is sanitized to lowercase ASCII (max 32 chars) and feeds the bundle identifier
(`com.veszelovszki.cmdr-dev-<slug>`) and data dir. The Vite dev port is ephemeral too: the wrapper reserves it via
`net.createServer().listen(0)`, exports `CMDR_VITE_PORT`, and rewrites `build.devUrl` in the generated
`tauri.instance.json` so the webview points at the port Vite actually binds (raw `pnpm vite dev` outside the wrapper
still defaults to 1420). Canonical reference (per-resource breakdown, precedence, debug recipes, acceptance smoke):
`docs/tooling/instance-isolation.md`.

**Run `pnpm dev --worktree <slug>` FROM the worktree dir** (`cd` into it first). Vite serves whatever source tree it's
launched in, so running it from the main repo root serves **main's** frontend with only the worktree's data dir, ports,
and Dock label: your worktree edits never load and the app looks unfixed (you QA the wrong code). The flag isolates the
instance, not the source.

### Dev watcher and markdown files

Two watchers run during `pnpm dev`, each with its own shield (don't delete either):

- The **Tauri CLI** watches `src-tauri/` and the workspace crates and rebuilds + restarts the whole app on any change
  there. `src-tauri/.taurignore` (gitignore syntax) excludes `*.md`, because 20+ colocated `CLAUDE.md` files live under
  `src-tauri/src/` and every docs edit used to restart the app. Verified empirically: without the file the watcher
  rebuilds on a `CLAUDE.md` change; with it, markdown edits are silent while `.rs` edits still rebuild.
- **Vite** watches the rest of `apps/desktop/`; `server.watch.ignored` in `vite.config.js` excludes `src-tauri/` so Rust
  builds don't churn the frontend watcher. Markdown elsewhere is harmless to Vite (`.md` isn't in the module graph;
  SvelteKit only full-reloads on route files, `app.html`, hooks, and `svelte.config.js`).

If a new always-edited non-build file type shows up under `src-tauri/`, add it to `.taurignore` rather than teaching
people to avoid saving.

Hot reload is reliable: max ~15 s for Rust, ~3 s for the frontend.

### The dev server sends `Cache-Control: no-store`

`server.headers` in `vite.config.js` forbids the webview from storing dev-server responses on disk. It applies to the
dev server only, so production builds are untouched.

WKWebView keys its network cache by URL, and a dev session serves the same modules under fresh URLs every time: the
wrapper reserves a new ephemeral port per session, so `localhost:61280/src/…` and `localhost:60129/src/…` are unrelated
cache keys, and HMR adds a `?t=<epoch-ms>` buster on every hot reload. Vite's default `no-cache` only forces
revalidation, it still permits STORING, so every one of those landed on disk and none of it was ever reusable.

Measured 2026-07-23 (dev sessions from a worktree, counting cache records by their own `localhost:<port>`, each session
confirmed booted to a rendered file explorer):

- Default `no-cache`: **1,188 records from a single session start**, plus 10 more from 15 hot reloads.
- `no-store`: **2 records** from a session start, and **0** from the same 15 hot reloads.
- What had accumulated: ~145,000 files / 871 MB in `~/Library/Caches/cmdr/WebKit` across ~161 dev sessions, on a shared
  path (dev runs an unbundled binary, so WKWebView falls back to the executable name `cmdr` for every instance and
  worktree). Prod is unaffected and sat at 763 files.

Session STARTUP dominates, not HMR: the per-session port alone re-caches the whole module graph. The 2 residual records
are the root HTML, which Vite serves without a `Cache-Control` header at all, so `server.headers` doesn't reach it and
WKWebView caches it heuristically. Two records per session is not worth chasing.

**Gotcha:** setting the header from plugin middleware does NOT work. Middleware registered in `configureServer` runs
BEFORE Vite's internal transform middleware, which sets its own `Cache-Control: no-cache` when it sends the response and
overwrites anything set earlier (verified by `curl -I` against a running dev server). `server.headers` is applied at the
right point and wins.

`localStorage` is deliberately untouched, so the persisted icon cache (`src/lib/icon-cache.ts`) still behaves in dev
exactly as in prod. Going non-persistent instead (Tauri's `incognito(true)`, which the Tauri 2.11.5 docs confirm maps to
a `nonPersistent` WKWebsiteDataStore on macOS) would have wiped it and made that divergence invisible in dev.

## Debugging

Data dirs are separate for prod, dev, and each worktree:

- Prod: `~/Library/Application Support/com.veszelovszki.cmdr/`.
- Plain `pnpm dev`: `~/Library/Application Support/com.veszelovszki.cmdr-dev/`.
- `pnpm dev --worktree foo`: `~/Library/Application Support/com.veszelovszki.cmdr-dev-foo/`.

`tauri-wrapper.ts` exports `CMDR_DATA_DIR` to the same path it gives Tauri's `app_data_dir()`, so direct file I/O (crash
reports, logs, file-backed secret store) agrees without round-tripping through Tauri's API.

- **Logging**: frontend and backend logs land together in the terminal and the log dir (dev: `<CMDR_DATA_DIR>/logs/`,
  prod: `~/Library/Logs/com.veszelovszki.cmdr/`). Read `docs/tooling/logging.md` before using `RUST_LOG`: it has
  per-subsystem recipes. Key gotcha: the Rust library target is `cmdr_lib`, not `cmdr`, so use
  `RUST_LOG=cmdr_lib::module=debug`. `cmdr_lib` (lib) and `Cmdr` (bin) are both in the `cmdr` package, so
  `Compiling cmdr` in build output covers both targets.
- **Dev live log path**: the running dev app's log is
  `~/Library/Application Support/com.veszelovszki.cmdr-dev/logs/cmdr.log` (the data-dir `logs/` subdir), NOT
  `~/Library/Logs/com.veszelovszki.cmdr-dev/` (which can be stale).
- **Hard freeze (logs stop)**: when the app wedges and the log goes silent, `sample <pid>` (macOS) the running Cmdr
  process to capture the blocked threads' stacks — that's how a deadlock gets pinpointed when the logger never flushed.
- **Crash reports**: a crash writes `crash-report.json` to the data dir; the next launch detects it and offers to send.
  See `src-tauri/src/crash_reporter/CLAUDE.md`.
- **Error reports**: to triage a bundle (zip + `manifest.json`), read `src-tauri/src/error_reporter/CLAUDE.md` for the
  layout and redaction conventions.
- **Index DB queries**: the index SQLite DB uses a custom `platform_case` collation, so the `sqlite3` CLI can't query
  it. Use `cargo run -p index-query -- <db_path> "<sql>"`; see `docs/tooling/index-query.md`.
- **Dev mock flags** (read by the backend process, so set them in the `pnpm dev` shell): `CMDR_MOCK_LICENSE=commercial`
  mocks the license; `CMDR_SIMULATE_UPDATE_FROM=<version>` forces the "What's new" popup on every launch as if just
  updated from that version (it never stamps `lastSeenVersion`). See `src/lib/whats-new/CLAUDE.md`.

## Testing the running app via MCP

Two MCP server types are available when the app runs via `pnpm dev`:

- **cmdr-dev** / **cmdr-prod**: high-level app control (navigation, file operations, search, dialogs, state). The
  primary way to drive the running app. Read `src-tauri/src/mcp/CLAUDE.md` first.
- **tauri** (Tauri MCP bridge): low-level access (screenshots, DOM, JS execution, IPC) for visual verification and what
  the Cmdr MCP can't do.

Both bind `127.0.0.1` on ephemeral per-instance ports; clients read the port from `<CMDR_DATA_DIR>/mcp.port` /
`<CMDR_DATA_DIR>/tauri-mcp.port` (`CMDR_MCP_PORT` pins it). See `docs/tooling/mcp.md` for patterns and pitfalls. If the
`mcp__cmdr-dev__*` / `mcp__tauri__*` tools are absent in your session (spawned agents often start without them), use
`./scripts/mcp-call.sh` (discovers port + token itself; `--help`, `--list-tools`).

## Testing

Read `docs/testing.md` (playbook) and `docs/tooling/testing.md` (tools inventory) before adding or changing tests. When
iterating on one test, run only that test, not the suite (the full Playwright suite wastes ~6 min per cycle and cascades
failures):

- Rust: `cd apps/desktop/src-tauri && cargo nextest run <test_name>`
- Svelte: `cd apps/desktop && pnpm vitest run -t "<test_name>"`
- Playwright: see `test/e2e-playwright/DETAILS.md` § "Running a single spec".

Suites: Vitest unit tests (`test/`), Playwright E2E (`test/e2e-playwright/`, see its `test/e2e-playwright/CLAUDE.md`),
Linux Docker E2E (`test/e2e-linux/`, see its `test/e2e-linux/CLAUDE.md` including the Ubuntu test VM). Docker SMB
fixtures: 14 Samba containers (guest, auth, readonly, slow, flaky, unicode, deep nesting, etc.); start with
`test/smb-servers/start.sh` and connect from Rust via `smb2::testing::guest_port()` and friends (see
`test/smb-servers/README.md`).

## Worktree setup (desktop specifics)

The repo-wide worktree workflow is in `AGENTS.md` § Workflow. Desktop-specific setup when creating a worktree under
`.claude/worktrees/<slug>`:

- `cp -c ~/projects-git/vdavid/cmdr/target <worktree>/target`: instant on APFS; deps are fingerprinted on version +
  features + rustc + profile, so only the workspace members rebuild.
  - **STALE-BUILD HAZARD (cost us repeatedly): the COW-cloned `target` makes a bare `cargo` / `cargo nextest` skip
    recompiling edited files** — cargo's mtime fingerprint can think the cloned objects are current, so a "green"
    bare-cargo run after editing `.rs` can be a FALSE green (tests run against stale code). Always
    `find apps/desktop/src-tauri/src -name '*.rs' | xargs touch` before a bare cargo run in a worktree, or use
    `pnpm check` (cache-aware, builds correctly). Don't trust a bare-cargo green right after edits. (Also:
    `pnpm check rust` does NOT run docs-group checks like `pluralize-noun` — run full `pnpm check` before claiming
    green.)
- CodeGraph: `mkdir -p <worktree>/.codegraph`, `cp -c .codegraph/codegraph.db` and `cp .codegraph/config.json` into it,
  then `(cd <worktree> && codegraph sync)`. Without its own populated `.codegraph`, the worktree session deadlocks
  against the main repo's DB.
- llama-server binaries (gitignored, `src-tauri/resources/ai/`): nothing to do. `src-tauri/build.rs` runs
  `scripts/download-llama-server.go` on demand, which clones from the main clone when `.version` matches (APFS clonefile
  — a self-contained copy, so the worktree also works bind-mounted into the Linux-E2E Docker container), else downloads.
  So raw `cargo check` works in a fresh worktree.

When FF-ing `main`, delete the worktree + branch AND remove the orphaned dev data dir
(`~/Library/Application Support/com.veszelovszki.cmdr-dev-<slug>`, often ~1 GB once its drive index builds): nothing
cleans it up otherwise and they pile up.
