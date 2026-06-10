# Desktop app

The Cmdr desktop app. Rust + Tauri 2 backend (`src-tauri/`), Svelte 5 + TypeScript frontend (`src/`).

See [`/AGENTS.md`](../../AGENTS.md) for repo-wide rules and [`/docs/architecture.md`](../../docs/architecture.md) for
the full subsystem map. Feature-level docs live in colocated `CLAUDE.md` (must-knows, auto-injected) + `DETAILS.md`
(depth, read on demand) files next to the code; see `AGENTS.md` § File structure for the split contract.

## Running

Run from repo root: `pnpm dev` (start) and `pnpm build` (release build). Don't `cd` here just to run the app.

If you've already `cd`d into `apps/desktop/`, `pnpm tauri dev` is equivalent; both paths invoke
`scripts/tauri-wrapper.js`, which resolves `CMDR_INSTANCE_ID` (from `--worktree <slug>` or the existing env, else
`"dev"`), composes `CMDR_DATA_DIR` to match, and writes a fresh `tauri.instance.json` under `$TMPDIR` that overrides the
bundle identifier and `productName` for this instance. The wrapper is the single source of truth for dev / prod path
separation; bypassing it (raw `cargo tauri dev`, raw `cargo build`) gives you the wrong data dir or a binary with no
embedded frontend.

To run two dev sessions side-by-side from different worktrees without colliding on `settings.json`, ports, or the Dock
label, pass `--worktree <slug>`: `pnpm dev --worktree my-feature`. The slug is sanitized to lowercase ASCII (max 32
chars) and feeds into both the bundle identifier (`com.veszelovszki.cmdr-dev-my-feature`) and the data dir. See
[`docs/tooling/instance-isolation.md`](../../docs/tooling/instance-isolation.md) for the per-resource breakdown.

The Vite dev port is also ephemeral per instance. The wrapper reserves it via `net.createServer().listen(0)`, exports
`CMDR_VITE_PORT`, and rewrites `build.devUrl` in the generated `tauri.instance.json` so Tauri's webview points at the
same port Vite actually binds. Raw `pnpm vite dev` outside the wrapper still defaults to 1420 for backwards
compatibility.

For the canonical reference on `CMDR_INSTANCE_ID` (per-resource breakdown, precedence rules, debug recipes, acceptance
smoke), see [`/docs/tooling/instance-isolation.md`](../../docs/tooling/instance-isolation.md).

### Dev watcher and markdown files

Two watchers run during `pnpm dev`, each with its own shield:

- The **Tauri CLI** watches `src-tauri/` and the workspace crates, and rebuilds + restarts the whole app on ANY file
  change there. [`src-tauri/.taurignore`](src-tauri/.taurignore) (gitignore syntax) excludes `*.md`, because 20+
  colocated `CLAUDE.md` files live under `src-tauri/src/` and every docs edit used to restart the app. Verified
  empirically: without the file, the watcher logs `File src-tauri/src/mcp/CLAUDE.md changed. Rebuilding application...`;
  with it, markdown edits are silent while `.rs` edits still rebuild.
- **Vite** watches the rest of `apps/desktop/`; `server.watch.ignored` in `vite.config.js` excludes `src-tauri/` so Rust
  builds don't churn the frontend watcher. Markdown elsewhere is harmless to Vite: `.md` files aren't in the module
  graph, and SvelteKit only full-reloads on route files, `app.html`, hooks, and `svelte.config.js`.

Don't delete either shield. If a new always-edited non-build file type shows up under `src-tauri/`, add it to
`.taurignore` rather than teaching people to avoid saving.

## Structure

- `src/`: Svelte frontend (SvelteKit static adapter, TypeScript strict, custom CSS with design tokens)
- `src-tauri/`: Rust backend (Tauri 2, serde, notify, tokio)
- `scripts/tauri-wrapper.js`: dev/build wrapper (env vars, dev config injection)
- `test/`: Vitest unit tests
- `test/e2e-linux/`: WebDriverIO + tauri-driver E2E tests (Docker)
- `test/e2e-playwright/`: Playwright E2E tests
- `test/smb-servers/`: Samba container fixtures for SMB integration tests
