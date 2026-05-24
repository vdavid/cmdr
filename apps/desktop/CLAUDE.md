# Desktop app

The Cmdr desktop app. Rust + Tauri 2 backend (`src-tauri/`), Svelte 5 + TypeScript frontend (`src/`).

See [`/AGENTS.md`](../../AGENTS.md) for repo-wide rules and [`/docs/architecture.md`](../../docs/architecture.md) for
the full subsystem map. Feature-level docs live in colocated `CLAUDE.md` files next to the code.

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
[`docs/specs/instance-isolation-plan.md`](../../docs/specs/instance-isolation-plan.md) for the full design.

## Structure

- `src/`: Svelte frontend (SvelteKit static adapter, TypeScript strict, custom CSS with design tokens)
- `src-tauri/`: Rust backend (Tauri 2, serde, notify, tokio)
- `scripts/tauri-wrapper.js`: dev/build wrapper (env vars, dev config injection)
- `test/`: Vitest unit tests
- `test/e2e-linux/`: WebDriverIO + tauri-driver E2E tests (Docker)
- `test/e2e-playwright/`: Playwright E2E tests
- `test/smb-servers/`: Samba container fixtures for SMB integration tests
