# Desktop app

The Cmdr desktop app: Rust + Tauri 2 backend (`src-tauri/`), Svelte 5 + TypeScript frontend (`src/`), custom CSS with
design tokens.

Repo-wide rules: [`/AGENTS.md`](../../AGENTS.md). Subsystem map: [`/docs/architecture.md`](../../docs/architecture.md).
Running, debugging, MCP control, testing, and worktree setup: [DETAILS.md](DETAILS.md). Feature must-knows live in each
directory's colocated `CLAUDE.md`.

## Must-knows

- **Run from the repo root**: `pnpm dev` / `pnpm build`. Both go through `scripts/tauri-wrapper.js`, the single source
  of truth for dev/prod data-dir and port separation; never `cargo tauri dev` / `cargo build` directly (wrong data dir,
  or a binary with no embedded frontend). Side-by-side worktree sessions: `pnpm dev --worktree <slug>`. See
  [DETAILS.md](DETAILS.md) § Running.
- **Don't delete either `pnpm dev` watcher shield.** [`src-tauri/.taurignore`](src-tauri/.taurignore) excludes `*.md`
  (else every colocated-doc edit rebuilds the whole app), and `vite.config.js` excludes `src-tauri/` from Vite. A new
  always-edited non-build file type under `src-tauri/` goes in `.taurignore`, not into a "don't save" habit. See
  [DETAILS.md](DETAILS.md) § Dev watcher.
- **Data dirs are separate** for prod, plain dev, and each `--worktree` slug, and an FF-merge leaves the worktree's dev
  data dir behind (~1 GB) to clean up by hand. Debugging, logging (`RUST_LOG=cmdr_lib::…`), crash/error reports, and dev
  mock flags: [DETAILS.md](DETAILS.md) § Debugging.
- **Run Playwright E2E via `pnpm check desktop-e2e-playwright`** (full lifecycle: build, launch, run, teardown). Raw
  `npx playwright test` fails with `ECONNREFUSED` — the suite connects to a running app over a socket, it doesn't launch
  one. Single-spec iteration and the manual launch+`pkill` recipe:
  [`test/e2e-playwright/CLAUDE.md`](test/e2e-playwright/CLAUDE.md).
- **The frontend is i18n-ized: user-facing strings live in the message catalog, not in components.** Resolve copy via
  `t()` / `getMessage()` / `<Trans>` from `$lib/intl`, with keys in `src/lib/intl/messages/en/<area>.json` carrying a
  translator `@key` description. Hardcoding a string in a known sink fails `cmdr/no-raw-user-facing-string`. English-only
  ships today; it's translation-ready. How it all works + adding strings/locales + leading translator agents:
  [`/docs/guides/i18n.md`](../../docs/guides/i18n.md); runtime must-knows: [`src/lib/intl/CLAUDE.md`](src/lib/intl/CLAUDE.md).

## Structure

- `src/`: Svelte frontend (SvelteKit static adapter, TypeScript strict).
- `src-tauri/`: Rust backend (Tauri 2, serde, notify, tokio).
- `scripts/`: dev/build scripts, mainly `tauri-wrapper.js`; see its [CLAUDE.md](scripts/CLAUDE.md).
- `test/`: Vitest unit tests, plus `test/e2e-playwright/`, `test/e2e-linux/`, and `test/smb-servers/` fixtures.

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
