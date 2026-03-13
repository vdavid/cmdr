# Cmdr

This file is for AI agents. Human contributors, see [CONTRIBUTING.md](CONTRIBUTING.md).

Cmdr is an extremely fast AI-native file manager written in Rust, free forever for personal use on macOS (BSL license).
Downloadable at [the website](https://getcmdr.com).

- Dev server: `pnpm dev` at repo root
- Prod build: `pnpm build` at repo root

## File structure

This is a monorepo containing these apps:
- Cmdr: Currently for macOS only. Rust, Tauri 2, Svelte 5, TypeScript, and custom CSS.
- getcmdr.com website: Astro + Tailwind v4. Deployed via Docker + Caddy.
- License server: Cloudflare Worker + Hono. Generates and verifies Ed25519-signed keys for Cmdr.

Core structure:

- `/.github/workflows/` - GitHub Actions workflows
- `/apps/`
    - `desktop/` - The Tauri desktop app
        - `test/e2e-smoke/` - Playwright smoke tests (browser-based, works on macOS)
        - `test/e2e-linux/` - WebDriverIO + tauri-driver tests (Docker, tests real Tauri app)
        - `src/` - Svelte frontend. Uses SvelteKit with static adapter. TypeScript strict mode. Tailwind v4.
            - `lib/` - Components
            - `routes/` - Routes
        - `src-tauri/` - Latest Rust, Tauri 2, serde, notify, tokio
        - `static/` - Static assets
        - `test/` - Vitest unit tests
    - `license-server/` - Cloudflare Worker (Hono). Receives Paddle webhooks, generates & validates Ed25519-signed keys.
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

## Testing

- Rust test: `cd apps/desktop/src-tauri && cargo nextest run <test_name>`
- Svelte test: `cd apps/desktop && pnpm vitest run -t "<test_name>"`
- All Rust/Svelte tests: `./scripts/check.sh --rust` or `--svelte`
- Specific checks: `./scripts/check.sh --check <name>` (use `--help` for the full list, or multiple `--check` flags)
- All checks: `./scripts/check.sh`
- E2E: See colocated CLAUDE.md files in `apps/desktop/test/e2e-linux/` and `apps/desktop/test/e2e-macos/`
- Ubuntu test VM: See `apps/desktop/test/e2e-linux/CLAUDE.md` § "Ubuntu test VM"
- CI: Runs on PRs and pushes to main for changed files. Full run: Actions → CI → "Run workflow".

## Debugging

- **Data dirs (dev and prod are separate!)**: Prod: `~/Library/Application Support/com.veszelovszki.cmdr/`,
  Dev: `~/Library/Application Support/com.veszelovszki.cmdr-dev/`. Set by `resolved_app_data_dir()` in
  `src-tauri/src/config.rs`.
- **Logging**: Frontend and backend logs appear together in terminal and in
  `~/Library/Logs/com.veszelovszki.cmdr/`. Full reference with `RUST_LOG` recipes:
  [docs/tooling/logging.md](docs/tooling/logging.md).
- **Hot reload**: `pnpm dev` hot-reloads. Max 15s for Rust, max 3s for frontend.
- **Index DB queries**: The index SQLite DB uses a custom `platform_case` collation, so the `sqlite3` CLI can't query
  it. Use `cargo run --bin index_query -- <db_path> "<sql>"` instead. See
  [docs/tooling/index-query.md](docs/tooling/index-query.md) for examples and DB paths.

## MCP

Two MCP servers available: "cmdr" (high-level app control) and "tauri" (low-level Tauri access).
If you don't find these but need them, ask the user. Run the app in dev mode first.

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
- **Coverage allowlist is a last resort.** Extract pure functions and test them. Only allowlist what genuinely can't be
  tested. Name the specific untestable API in the reason.
- When adding a new user-facing action, add it to `command-registry.ts` and `handleCommandExecute` in `+page.svelte`.
- If you added a new Tauri command touching the filesystem, check `docs/architecture.md` § Platform constraints.

## Workflow

- **Always read** [style-guide.md](docs/style-guide.md) before touching code. Especially sentence case!
- Cover your code with tests until you're confident. Don't go overboard. Test per milestone.

Happy coding! 🦀✨
