# Desktop app details

Operational depth for the desktop app: running, debugging, testing, MCP control, and worktree setup. Must-knows are in
[CLAUDE.md](CLAUDE.md); repo-wide knowledge is in [`/AGENTS.md`](../../AGENTS.md); the subsystem map is
[`/docs/architecture.md`](../../docs/architecture.md).

## Running

Run from the repo root: `pnpm dev` (start) and `pnpm build` (release build). The root `package.json` has no `tauri`
script, so don't `cd` here just to run the app. If you've already `cd`d into `apps/desktop/`, `pnpm tauri dev` is
equivalent; both paths invoke `scripts/tauri-wrapper.js`.

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
[`/docs/tooling/instance-isolation.md`](../../docs/tooling/instance-isolation.md).

**Run `pnpm dev --worktree <slug>` FROM the worktree dir** (`cd` into it first). Vite serves whatever source tree it's
launched in, so running it from the main repo root serves **main's** frontend with only the worktree's data dir, ports,
and Dock label: your worktree edits never load and the app looks unfixed (you QA the wrong code). The flag isolates the
instance, not the source.

### Dev watcher and markdown files

Two watchers run during `pnpm dev`, each with its own shield (don't delete either):

- The **Tauri CLI** watches `src-tauri/` and the workspace crates and rebuilds + restarts the whole app on any change
  there. [`src-tauri/.taurignore`](src-tauri/.taurignore) (gitignore syntax) excludes `*.md`, because 20+ colocated
  `CLAUDE.md` files live under `src-tauri/src/` and every docs edit used to restart the app. Verified empirically:
  without the file the watcher rebuilds on a `CLAUDE.md` change; with it, markdown edits are silent while `.rs` edits
  still rebuild.
- **Vite** watches the rest of `apps/desktop/`; `server.watch.ignored` in `vite.config.js` excludes `src-tauri/` so Rust
  builds don't churn the frontend watcher. Markdown elsewhere is harmless to Vite (`.md` isn't in the module graph;
  SvelteKit only full-reloads on route files, `app.html`, hooks, and `svelte.config.js`).

If a new always-edited non-build file type shows up under `src-tauri/`, add it to `.taurignore` rather than teaching
people to avoid saving.

Hot reload is reliable: max ~15 s for Rust, ~3 s for the frontend.

## Debugging

Data dirs are separate for prod, dev, and each worktree:

- Prod: `~/Library/Application Support/com.veszelovszki.cmdr/`.
- Plain `pnpm dev`: `~/Library/Application Support/com.veszelovszki.cmdr-dev/`.
- `pnpm dev --worktree foo`: `~/Library/Application Support/com.veszelovszki.cmdr-dev-foo/`.

`tauri-wrapper.js` exports `CMDR_DATA_DIR` to the same path it gives Tauri's `app_data_dir()`, so direct file I/O (crash
reports, logs, file-backed secret store) agrees without round-tripping through Tauri's API.

- **Logging**: frontend and backend logs land together in the terminal and the log dir (dev: `<CMDR_DATA_DIR>/logs/`,
  prod: `~/Library/Logs/com.veszelovszki.cmdr/`). Read [`/docs/tooling/logging.md`](../../docs/tooling/logging.md)
  before using `RUST_LOG`: it has per-subsystem recipes. Key gotcha: the Rust library target is `cmdr_lib`, not `cmdr`,
  so use `RUST_LOG=cmdr_lib::module=debug`. `cmdr_lib` (lib) and `Cmdr` (bin) are both in the `cmdr` package, so
  `Compiling cmdr` in build output covers both targets.
- **Dev live log path**: the running dev app's log is
  `~/Library/Application Support/com.veszelovszki.cmdr-dev/logs/cmdr.log` (the data-dir `logs/` subdir), NOT
  `~/Library/Logs/com.veszelovszki.cmdr-dev/` (which can be stale).
- **Hard freeze (logs stop)**: when the app wedges and the log goes silent, `sample <pid>` (macOS) the running Cmdr
  process to capture the blocked threads' stacks — that's how a deadlock gets pinpointed when the logger never flushed.
- **Crash reports**: a crash writes `crash-report.json` to the data dir; the next launch detects it and offers to send.
  See [`src-tauri/src/crash_reporter/CLAUDE.md`](src-tauri/src/crash_reporter/CLAUDE.md).
- **Error reports**: to triage a bundle (zip + `manifest.json`), read
  [`src-tauri/src/error_reporter/CLAUDE.md`](src-tauri/src/error_reporter/CLAUDE.md) for the layout and redaction
  conventions.
- **Index DB queries**: the index SQLite DB uses a custom `platform_case` collation, so the `sqlite3` CLI can't query
  it. Use `cargo run -p index-query -- <db_path> "<sql>"`; see
  [`/docs/tooling/index-query.md`](../../docs/tooling/index-query.md).
- **Dev mock flags** (read by the backend process, so set them in the `pnpm dev` shell): `CMDR_MOCK_LICENSE=commercial`
  mocks the license; `CMDR_SIMULATE_UPDATE_FROM=<version>` forces the "What's new" popup on every launch as if just
  updated from that version (it never stamps `lastSeenVersion`). See
  [`src/lib/whats-new/CLAUDE.md`](src/lib/whats-new/CLAUDE.md).

## Testing the running app via MCP

Two MCP server types are available when the app runs via `pnpm dev`:

- **cmdr-dev** / **cmdr-prod**: high-level app control (navigation, file operations, search, dialogs, state). The
  primary way to drive the running app. Read [`src-tauri/src/mcp/CLAUDE.md`](src-tauri/src/mcp/CLAUDE.md) first.
- **tauri** (Tauri MCP bridge): low-level access (screenshots, DOM, JS execution, IPC) for visual verification and what
  the Cmdr MCP can't do.

Both bind `127.0.0.1` on ephemeral per-instance ports; clients read the port from `<CMDR_DATA_DIR>/mcp.port` /
`<CMDR_DATA_DIR>/tauri-mcp.port` (`CMDR_MCP_PORT` pins it). See [`/docs/tooling/mcp.md`](../../docs/tooling/mcp.md) for
patterns and pitfalls. If the `mcp__cmdr-dev__*` / `mcp__tauri__*` tools are absent in your session (spawned agents
often start without them), use `./scripts/mcp-call.sh` (discovers port + token itself; `--help`, `--list-tools`).

## Testing

Read [`/docs/testing.md`](../../docs/testing.md) (playbook) and
[`/docs/tooling/testing.md`](../../docs/tooling/testing.md) (tools inventory) before adding or changing tests. When
iterating on one test, run only that test, not the suite (the full Playwright suite wastes ~6 min per cycle and cascades
failures):

- Rust: `cd apps/desktop/src-tauri && cargo nextest run <test_name>`
- Svelte: `cd apps/desktop && pnpm vitest run -t "<test_name>"`
- Playwright: see [`test/e2e-playwright/DETAILS.md`](test/e2e-playwright/DETAILS.md) § "Running a single spec".

Suites: Vitest unit tests (`test/`), Playwright E2E (`test/e2e-playwright/`, see its
[CLAUDE.md](test/e2e-playwright/CLAUDE.md)), Linux Docker E2E (`test/e2e-linux/`, see its
[CLAUDE.md](test/e2e-linux/CLAUDE.md) including the Ubuntu test VM). Docker SMB fixtures: 14 Samba containers (guest,
auth, readonly, slow, flaky, unicode, deep nesting, etc.); start with `test/smb-servers/start.sh` and connect from Rust
via `smb2::testing::guest_port()` and friends (see [`test/smb-servers/README.md`](test/smb-servers/README.md)).

## Worktree setup (desktop specifics)

The repo-wide worktree workflow is in [`/AGENTS.md`](../../AGENTS.md) § Workflow. Desktop-specific setup when creating a
worktree under `.claude/worktrees/<slug>`:

- `cp -c ~/projects-git/vdavid/cmdr/target <worktree>/target`: instant on APFS; deps are fingerprinted on version +
  features + rustc + profile, so only the workspace members rebuild.
  - **STALE-BUILD HAZARD (cost us repeatedly): the COW-cloned `target` makes a bare `cargo` / `cargo nextest` skip
    recompiling edited files** — cargo's mtime fingerprint can think the cloned objects are current, so a "green"
    bare-cargo run after editing `.rs` can be a FALSE green (tests run against stale code). Always
    `find apps/desktop/src-tauri/src -name '*.rs' | xargs touch` before a bare cargo run in a worktree, or use
    `pnpm check` (cache-aware, builds correctly). Don't trust a bare-cargo green right after edits. (Also:
    `pnpm check rust` does NOT run docs-group checks like `pluralize-noun` — run full `pnpm check` before claiming
    green.)
- CodeGraph (from `~/.claude/docs/codegraph-worktree.md`): `mkdir -p <worktree>/.codegraph`,
  `cp -c .codegraph/codegraph.db` and `cp .codegraph/config.json` into it, then `(cd <worktree> && codegraph sync)`.
  Without its own populated `.codegraph`, the worktree session deadlocks against the main repo's DB.
- llama-server binaries (gitignored, `src-tauri/resources/ai/`): nothing to do. `src-tauri/build.rs` runs
  `scripts/download-llama-server.go` on demand, which symlinks from the main clone when `.version` matches, else
  downloads. So raw `cargo check` works in a fresh worktree.

When FF-ing `main`, delete the worktree + branch AND remove the orphaned dev data dir
(`~/Library/Application Support/com.veszelovszki.cmdr-dev-<slug>`, often ~1 GB once its drive index builds): nothing
cleans it up otherwise and they pile up.
