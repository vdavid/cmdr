# Cmdr

This file is for AI agents. Human contributors, see [CONTRIBUTING.md](CONTRIBUTING.md).

Cmdr is an extremely fast AI-native file manager written in Rust, free forever for personal use on macOS (BSL license).
Downloadable at [the website](https://getcmdr.com).

- Dev server: `pnpm dev` at repo root
- Prod build: `pnpm build` at repo root

## File structure

This is a monorepo containing these apps:

- Cmdr: Currently for macOS only. Rust, Tauri 2, Svelte 5, TypeScript, and custom CSS.
- Analytics dashboard: Private SvelteKit metrics dashboard. Deployed to Cloudflare Pages.
- getcmdr.com website: Astro + Tailwind v4. Deployed via Docker + Caddy.
- API server: Cloudflare Worker + Hono. Licensing, telemetry, crash reports, downloads, and admin endpoints.

Core structure:

- `/.github/workflows/` - GitHub Actions workflows
- `/apps/`
  - `analytics-dashboard/` - Private metrics dashboard (SvelteKit + CF Pages)
  - `desktop/` - The Tauri desktop app
    - `test/e2e-linux/` - WebDriverIO + tauri-driver tests (Docker, tests real Tauri app)
    - `src/` - Svelte frontend. Uses SvelteKit with static adapter. TypeScript strict mode. Tailwind v4.
      - `lib/` - Components
      - `routes/` - Routes
    - `src-tauri/` - Latest Rust, Tauri 2, serde, notify, tokio
    - `static/` - Static assets
    - `test/` - Vitest unit tests
  - `api-server/` - Cloudflare Worker (Hono). Licensing, telemetry, crash reports, downloads, and admin endpoints.
  - `website/` - Marketing website (getcmdr.com)
- `/scripts/check/` - Go-based unified check runner
- `/docs/` - Dev docs
  - `guides/` - How-to guides
  - `notes/` - Temporary reference notes (benchmarks, analysis) linked from CLAUDE.md files
  - `tooling/` - Internal tooling docs
  - `architecture.md` - Map of all subsystems with links to their `CLAUDE.md` files
  - `style-guide.md` - Writing, code, and design style rules
  - `security.md` - Security policies
- Feature-level docs live in **colocated `CLAUDE.md` files** next to the code (for example,
  `src/lib/settings/CLAUDE.md`). Claude Code auto-discovers these. See `docs/architecture.md` for the full map.

## Testing and checking

Always use the checker script for compilation, linting, formatting, and tests. Its output is concise and focused — no
`2>&1`, `head`, or `tail` needed. Don't run raw `cargo check`, `cargo clippy`, `cargo fmt`, `cargo nextest run`, etc.

- Specific checks: `./scripts/check.sh --check <name>` (e.g. `--check clippy`, `--check rustfmt`). Use `--help` for the
  full list, or multiple `--check` flags.
- All Rust/Svelte checks: `./scripts/check.sh --rust` or `--svelte`
- All checks: `./scripts/check.sh`
- Specific tests by name (the one exception where direct commands are fine):
  - Rust: `cd apps/desktop/src-tauri && cargo nextest run <test_name>`
  - Svelte: `cd apps/desktop && pnpm vitest run -t "<test_name>"`
- E2E (Playwright): See `apps/desktop/test/e2e-playwright/CLAUDE.md` — build with `playwright-e2e` feature, start app,
  run tests
- E2E (legacy): See `apps/desktop/test/e2e-linux/CLAUDE.md` and `apps/desktop/test/e2e-macos/CLAUDE.md`
- Ubuntu test VM: See `apps/desktop/test/e2e-linux/CLAUDE.md` § "Ubuntu test VM"
- CI: Runs on PRs and pushes to main for changed files. Full run: Actions → CI → "Run workflow".

## Debugging

- **Data dirs (dev and prod are separate!)**: Prod: `~/Library/Application Support/com.veszelovszki.cmdr/`, Dev:
  `~/Library/Application Support/com.veszelovszki.cmdr-dev/`. Set by `resolved_app_data_dir()` in
  `src-tauri/src/config.rs`.
- **Logging**: Frontend and backend logs appear together in terminal and in `~/Library/Logs/com.veszelovszki.cmdr/`.
  **Read [docs/tooling/logging.md](docs/tooling/logging.md) before using `RUST_LOG`** — it has copy-paste recipes for
  every subsystem. Key gotcha: the Rust library target is `cmdr_lib`, not `cmdr`. Use `RUST_LOG=cmdr_lib::module=debug`.
- **Crash reports**: When the app crashes, it writes a crash file to the data dir (`crash-report.json` alongside
  `settings.json`). On next launch, the app detects this file and offers to send a crash report. See
  `src-tauri/src/crash_reporter/CLAUDE.md` for architecture details.
- **Hot reload**: `pnpm dev` hot-reloads. Max 15s for Rust, max 3s for frontend.
- **Index DB queries**: The index SQLite DB uses a custom `platform_case` collation, so the `sqlite3` CLI can't query
  it. Use `cargo run -p index-query -- <db_path> "<sql>"` instead. See
  [docs/tooling/index-query.md](docs/tooling/index-query.md) for examples and DB paths.

## MCP (testing the running app)

Two MCP servers are available when the app is running via `pnpm dev`:

- **cmdr** (port 9224) — High-level app control: navigation, file operations, search, dialogs, state inspection. This is
  the primary way to test and interact with the running app. Architecture docs: `src-tauri/src/mcp/CLAUDE.md`.
- **tauri** (port 9223) — Low-level Tauri access: screenshots, DOM inspection, JS execution, IPC calls. Use for visual
  verification and UI automation.

**Before making any MCP calls**, read [docs/tooling/mcp.md](docs/tooling/mcp.md) for usage patterns, connection
resilience, and common pitfalls.

## Where to put instructions

- **User-generic preferences** (e.g. "never use git stash", "don't take external actions without approval") →
  `~/.claude/CLAUDE.md`. These apply across all projects.
- **Project-specific instructions** → `AGENTS.md` (this file) for repo-wide rules, or colocated `CLAUDE.md` files for
  module-specific docs. These are version-controlled and visible to all contributors.
- **Don't use** the project-level `memory/MEMORY.md` for either category. It's not transparent and not in the repo.

## Critical rules

- ❌ NEVER use `git stash`, `git checkout`, `git reset`, or any git write operation unless explicitly asked. Multiple
  agents may be working simultaneously.
- ❌ NEVER add dependencies without checking license compatibility (`cargo deny check`) and verifying the latest version
  from npm/crates.io/GitHub. Never trust training data for versions.
- ❌ When adding code that loads remote content (`fetch`, `iframe`), ask whether to disable in dev mode.
  `withGlobalTauri: true` in dev mode is a security risk.
- ❌ When testing the Tauri app, DO NOT USE THE BROWSER. Use the MCP servers.
- ❌ Don't ignore linter warnings — fix them or justify with a comment.
- Always use CSS variables defined in `apps/desktop/src/app.css`. Stylelint catches undefined/hallucinated variables.
- Never use raw `px` values for `font-size`, `border-radius`, `font-family`, or `z-index` >= 10. Use
  `var(--font-size-*)`, `var(--radius-*)`, `var(--font-*)`, and `var(--z-*)` tokens. Stylelint enforces this.
- **Coverage allowlist is a last resort.** Extract pure functions and test them. Only allowlist what genuinely can't be
  tested. Name the specific untestable API in the reason.
- When adding a new user-facing action, add it to `command-registry.ts` and `handleCommandExecute` in `+page.svelte`.
- If you added a new Tauri command touching the filesystem, check `docs/architecture.md` § Platform constraints.

## Workflow

- **Always read** [style-guide.md](docs/style-guide.md) before touching code. Especially sentence case!
- Cover your code with tests until you're confident. Don't go overboard. Test per milestone.

Happy coding! 🦀✨
