# Desktop app

The Cmdr desktop app: Rust + Tauri 2 backend (`src-tauri/`), Svelte 5 + TypeScript frontend (`src/`), custom CSS with
design tokens.

[Subsystem map](../../docs/architecture.md). Running, dev watchers, debugging, MCP control, testing, and worktree setup
in `DETAILS.md`. Feature must-knows in the colocated `CLAUDE.md`s.

## Must-knows

- **Run from the repo root**: `pnpm dev` / `pnpm build`. Both go through `scripts/tauri-wrapper.ts`, the single source
  of truth for dev/prod data-dir and port separation; never `cargo tauri dev` / `cargo build` directly (wrong data dir,
  or a binary with no embedded frontend). Side-by-side worktree sessions: `pnpm dev --worktree <slug>`. See `DETAILS.md`
  § Running.
- **Don't delete either `pnpm dev` watcher shield.** The REPO-ROOT `.taurignore` excludes `*.md` (else every
  colocated-doc edit rebuilds the whole app), and `vite.config.js` excludes `src-tauri/` from Vite. Root, not
  `src-tauri/`: the Tauri watcher also watches the workspace crates, and one matcher covers them all. A new
  always-edited non-build file type goes in `.taurignore`, not into a "don't save" habit. See `DETAILS.md` § Dev
  watcher.
- **Data dirs are separate** for prod, plain dev, and each `--worktree` slug, and an FF-merge leaves the worktree's dev
  data dir behind (~1 GB) to clean up by hand. Debugging, logging (`RUST_LOG=cmdr_lib::…`), crash/error reports, and dev
  mock flags: `DETAILS.md` § Debugging.
- **Run Playwright E2E via `pnpm check desktop-e2e-playwright`** (full lifecycle: build, launch, run, teardown). Raw
  `npx playwright test` fails with `ECONNREFUSED` — the suite connects to a running app over a socket, it doesn't launch
  one. Single-spec iteration and the manual launch+`pkill` recipe: `test/e2e-playwright/CLAUDE.md`.
- **Gating behavior on an automated run? Call `isE2eRun()` from `$lib/app-mode`, ❌ never `getAppMode() === 'e2e'`.**
  There are four app modes (`prod` / `dev` / `e2e` / `capture`, driving the plain / pink `DEV MODE` / blue `E2E MODE` /
  yellow `SCREENSHOT` title bars), and `capture` is a REFINEMENT of `e2e`, not an alternative: the i18n screenshot run
  is an E2E run that also photographs each surface, driven by the same harness events. Comparing to `'e2e'` alone
  silently switches your gate off in a capture build, which is how you'd break the screenshot run without touching it.
  `getAppMode()` is for the VISUAL marker only. Modes and the capture run's rules: `test/e2e-playwright/DETAILS.md` §
  App modes.
- **Investigating high memory? `vmmap`'s `IOAccelerator` rows are the RUST HEAP, not GPU memory.** mimalloc tags its
  arenas with VM tag 100, which macOS names `VM_MEMORY_IOACCELERATOR`; conversely the `MALLOC_*` zones are NOT Cmdr's
  heap (mimalloc isn't a registered zone, so `malloc_zone_statistics` is blind to it). Mistaking this sends you
  bisecting the frontend for a backend leak — it has, twice. Read `docs/tooling/memory-debugging.md` before measuring.
- **The frontend is i18n-ized: user-facing strings live in the message catalog, not in components.** Resolve copy via
  `t()` / `getMessage()` / `<Trans>` from `$lib/intl`, with keys in `src/lib/intl/messages/en/<area>.json` carrying a
  translator `@key` description. Hardcoding a string in a known sink fails `cmdr/no-raw-user-facing-string`. Ten locales
  ship today. How it all works + adding strings/locales + leading translator agents: `docs/guides/i18n.md`; runtime
  must-knows: `src/lib/intl/CLAUDE.md`.

## Structure

- `src/`: Svelte frontend (SvelteKit static adapter, TypeScript strict).
- `src-tauri/`: Rust backend (Tauri 2, serde, notify, tokio).
- `scripts/`: dev/build scripts, mainly `tauri-wrapper.ts`; see its `scripts/CLAUDE.md`.
- `test/`: Vitest unit tests, plus `test/e2e-playwright/`, `test/e2e-linux/`, and `test/smb-servers/` fixtures.
