# Desktop app

The Cmdr desktop app. Rust + Tauri 2 backend (`src-tauri/`), Svelte 5 + TypeScript frontend (`src/`).

See [`/AGENTS.md`](../../AGENTS.md) for repo-wide rules and [`/docs/architecture.md`](../../docs/architecture.md) for
the full subsystem map. Feature-level docs live in colocated `CLAUDE.md` files next to the code.

## Running

Run from repo root: `pnpm dev` (start) and `pnpm build` (release build). Don't `cd` here just to run the app.

If you've already `cd`d into `apps/desktop/`, `pnpm tauri dev` is equivalent; both paths invoke
`scripts/tauri-wrapper.js`, which sets `CMDR_DATA_DIR` and injects `tauri.dev.json`. The wrapper is the single source of
truth for dev/prod path separation; bypassing it (raw `cargo tauri dev`, raw `cargo build`) gives you the wrong data dir
or a binary with no embedded frontend.

## Structure

- `src/`: Svelte frontend (SvelteKit static adapter, TypeScript strict, custom CSS with design tokens)
- `src-tauri/`: Rust backend (Tauri 2, serde, notify, tokio)
- `scripts/tauri-wrapper.js`: dev/build wrapper (env vars, dev config injection)
- `test/`: Vitest unit tests
- `test/e2e-linux/`: WebDriverIO + tauri-driver E2E tests (Docker)
- `test/e2e-playwright/`: Playwright E2E tests
- `test/smb-servers/`: Samba container fixtures for SMB integration tests
