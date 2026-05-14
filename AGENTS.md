# Cmdr

This file is for AI agents. Human contributors, see [CONTRIBUTING.md](CONTRIBUTING.md).

Cmdr is an extremely fast AI-native file manager written in Rust, free forever for personal use on macOS (BSL license).
Downloadable at [the website](https://getcmdr.com).

- Dev server: `pnpm dev` at repo root
- Prod build: `pnpm build` at repo root
- Both must run **at repo root**. The root `package.json` has no `tauri` script, so `pnpm tauri dev` only works from
  inside `apps/desktop/`. Prefer the root form — both paths flow through `tauri-wrapper.js` and are equivalent, but the
  root form is what's documented and what other tooling assumes.

## Principles

These are general principles for the whole project. These are not just empty sentences on our wall, we live these:

### Top 5:

1. **Deliver delightful UX.** We always go the extra mile to make it absolutely delightful to use our software. Not just
   functional, but noticeably pleasant. Thoughtful phrasing. Accessible focus indicators. Putting real effort in
   dark/light modes. Nice images and anims. OS-custom everything. Respect the system font, sizing, theme,
   `prefers-reduced-motion`, etc.
2. **Elegance above all.** We have time to do outstanding work. We prefer a clean and elegant architecture over hacks,
   both internally for ourselves, and externally toward the user. We think about the long run.
3. **The app should feel rock solid.** The UI must _always_ be responsive. We never block the main thread. Every user
   action triggers immediate feedback, even if it's just a spinner. We communicate what's actually happening. Show
   progress. An ETA when possible. No progress bars stuck at 100% — show the real state. Long operations are always
   cancelable, stopping background work too, not just the UI. The user is always in control. Assume the hostile case
   (dead network mount, huge directory, crashed mid-operation) and handle it gracefully.
4. **Protect the user's data.** Use safe overwrite patterns like temp+rename. Offer rollback for destructive operations.
   Use atomic ops where possible. Design for the crash mid-operation. Test the shit out of the parts that write data.
5. **Be respectful to the user's resources.** Minimize CPU use, memory use, don't thrash the disks.

### Technicals:

1. **Think from first principles, capture intention.** Add logs. Run the code. Do benchmarks. Then document the "why"s
   and link the data where needed.
2. **Invest in finding the right tradeoff.** Elegance lives between duplication and overengineering. No premature
   abstractions, but no copy-paste either.
3. **Smart backend, thin frontend.** Complex logic lives in Rust. The frontend's job is to deliver a delightful UX:
   presenting the right states, errors, progress, and feedback. Display logic _can_ get complex, but the business logic
   lives in the backend.
4. **Organized by feature, not by layer.** Frontend components, backend modules, tests, and docs are colocated with the
   feature they belong to. Colocated `CLAUDE.md` files, colocated tests, feature-shaped directories. If we could
   technically merge a Svelte component with its Rust counterpart into one feature unit, we would.
5. **Thin IPC layer.** Tauri commands are pass-throughs. no branching, no transformation. Business logic lives in
   subsystem modules that can be tested independently.
6. **Subscribe, don't poll.** Whenever possible, we make it so that consumers can subscribe to events and receive
   updates. If not possible, we resort to polling. But we make an effort to avoid polling.
7. **Invest in testability.** We have virtual MTP devices, Docker-based SMB servers, feature flags for E2E. Tools to
   guarantee stability.
8. **Invest in tooling.** We have check runners, linters, coverage, CI. Tooling must be fast so we use it, and strict so
   it doesn't allow us to make mistakes.

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
    - `src/` - Svelte frontend. Uses SvelteKit with static adapter. TypeScript strict mode. Custom CSS with design
      tokens.
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
  - `maintenance.md` - Recurring maintenance tasks (dep bumps, allowlist trims, doc sweeps) and a log of past runs
- Feature-level docs live in **colocated `CLAUDE.md` files** next to the code (for example,
  `src/lib/settings/CLAUDE.md`). Claude Code auto-discovers these. See `docs/architecture.md` for the full map.

## Testing and checking

**Before adding or modifying tests**, read [docs/testing.md](docs/testing.md) — the testing playbook (decision table,
anti-patterns, per-feature checklist). The companion file [docs/tooling/testing.md](docs/tooling/testing.md) is the
tools inventory.

Always use the checker script for compilation, linting, formatting, and tests. Its output is concise and focused — no
`2>&1`, `head`, or `tail` needed. Don't run raw `cargo check`, `cargo clippy`, `cargo fmt`, `cargo nextest run`, etc.

- Specific checks: `./scripts/check.sh --check <name>` (e.g. `--check clippy`, `--check rustfmt`). Use `--help` for the
  full list, or multiple `--check` flags.
- All Rust/Svelte checks: `./scripts/check.sh --rust` or `--svelte`
- All checks: `./scripts/check.sh`
- **`oxfmt` must always run before you call a task done.** It's monorepo-wide (markdown, YAML, JSON, JS/TS across every
  app) and takes ~1 second — there's no reason to skip it. It's registered under `AppOther`, which means `--rust` and
  `--svelte` do NOT include it. If you only ran those, CI will catch unformatted markdown / JSON / etc. that you missed.
  Always finish with either `./scripts/check.sh` (the full suite) or at minimum `./scripts/check.sh --check oxfmt` after
  your other checks. No exceptions.
- Specific tests by name (the one exception where direct commands are fine):
  - Rust: `cd apps/desktop/src-tauri && cargo nextest run <test_name>`
  - Svelte: `cd apps/desktop && pnpm vitest run -t "<test_name>"`
  - Playwright: see `apps/desktop/test/e2e-playwright/CLAUDE.md` § "Running a single spec"
- **When iterating on one test, run only that test.** The full suite is for confirming CI-green before declaring done,
  not for each tweak. Running the whole Playwright suite for one new spec wastes ~10 minutes per cycle and produces
  noisy "cascade" failures when the broken test takes the app down with it (subsequent specs fail with connection
  errors). Same principle at smaller scales for Rust and Vitest.
- E2E (Playwright): See `apps/desktop/test/e2e-playwright/CLAUDE.md` — build with `playwright-e2e` feature, start app,
  run tests
- Ubuntu test VM: See `apps/desktop/test/e2e-linux/CLAUDE.md` § "Ubuntu test VM"
- Docker SMB containers: 14 Samba containers (guest, auth, readonly, slow, flaky, unicode, deep nesting, etc.) for
  integration tests. Start with `apps/desktop/test/smb-servers/start.sh`. Connect from Rust via
  `smb2::testing::guest_port()` and friends. See `apps/desktop/test/smb-servers/README.md` for details.
- CI: Runs on PRs and pushes to main for changed files. Full run: Actions → CI → "Run workflow".

## Debugging

- **Data dirs (dev and prod are separate!)**: Prod: `~/Library/Application Support/com.veszelovszki.cmdr/`, Dev:
  `~/Library/Application Support/com.veszelovszki.cmdr-dev/`. Dev path is set via `CMDR_DATA_DIR` env var by
  `tauri-wrapper.js`; resolved in `src-tauri/src/config.rs`.
- **Logging**: Frontend and backend logs appear together in terminal and in the log dir (dev: `<CMDR_DATA_DIR>/logs/`,
  prod: `~/Library/Logs/com.veszelovszki.cmdr/`). **Read [docs/tooling/logging.md](docs/tooling/logging.md) before using
  `RUST_LOG`** — it has copy-paste recipes for every subsystem. Key gotcha: the Rust library target is `cmdr_lib`, not
  `cmdr`. Use `RUST_LOG=cmdr_lib::module=debug`. Note: `cmdr_lib` (lib) and `Cmdr` (bin) are both in the `cmdr` Cargo
  package, so `Compiling cmdr` in build output covers BOTH targets. Cargo won't show `Compiling cmdr_lib` separately.
- **Crash reports**: When the app crashes, it writes a crash file to the data dir (`crash-report.json` alongside
  `settings.json`). On next launch, the app detects this file and offers to send a crash report. See
  `src-tauri/src/crash_reporter/CLAUDE.md` for architecture details.
- **Error reports**: When triaging an error report bundle (zip + `manifest.json`), read
  `src-tauri/src/error_reporter/CLAUDE.md` first — it documents the bundle layout, what each piece captures, and the
  redaction conventions.
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
- **Icons**: We use `unplugin-icons` with `@iconify-json/lucide`. Import as Svelte components from
  `~icons/lucide/{icon-name}` (inline SVGs, no runtime cost). See `docs/style-guide.md` § Icons for usage, sizing,
  coloring, and how to find new icons.
- Always use CSS variables defined in `apps/desktop/src/app.css`. Stylelint catches undefined/hallucinated variables.
- Never use raw `px` values for `font-size`, `border-radius`, `font-family`, or `z-index` >= 10. Use
  `var(--font-size-*)`, `var(--radius-*)`, `var(--font-*)`, and `var(--z-*)` tokens. Stylelint enforces this.
- **Coverage allowlist is a last resort.** Extract pure functions and test them. Only allowlist what genuinely can't be
  tested. Name the specific untestable API in the reason.
- When adding a new user-facing action, add it to `command-registry.ts` and `handleCommandExecute` in
  `routes/(main)/command-dispatch.ts`.
- If you added a new Tauri command touching the filesystem, check `docs/architecture.md` § Platform constraints.
- ❌ **Don't read TCC-protected paths or call NSWorkspace icon/LaunchServices APIs at app launch without the FDA gate.**
  `~/Downloads`, `~/Documents`, `~/Desktop`, `~/Pictures`, `~/Movies`, `~/Music`, `~/Library/CloudStorage`, and any
  `NSWorkspace.iconForFile:` call (even on `/Applications` or the iCloud root) can trigger macOS TCC popups during
  onboarding. We had **5–10 popups stacked on top of the in-app FDA modal** before this gate landed. Use
  `crate::fda_gate::is_fda_pending_runtime()` for launch-time call sites, or
  `crate::fda_gate::is_fda_pending(fda_choice, os_fda_granted)` for pure logic and tests. After Allow + restart, or Deny
  in-session via `start_indexing_after_fda_decision`, the gate clears and the same call sites run normally. See
  [`apps/desktop/src-tauri/src/fda_gate.rs`](apps/desktop/src-tauri/src/fda_gate.rs) and
  [`apps/desktop/src/lib/onboarding/CLAUDE.md`](apps/desktop/src/lib/onboarding/CLAUDE.md) § "FDA gate".
- ❌ **Tauri APIs fail silently without permissions.** Whenever you call a new Tauri API from a window — `setMinSize`,
  `setTitle`, `show`, plugin commands, anything new — add the matching permission to that window's capability file in
  `src-tauri/capabilities/{default,settings,viewer}.json`. Without it, the call rejects with a generic "not allowed"
  error and your feature looks broken with no obvious cause. Surface failures by `await`-ing the call inside a
  `try/catch` and logging the error rather than `void`-ing the promise. See `src-tauri/capabilities/CLAUDE.md` for the
  per-window split and naming conventions.
- We use [mise](https://mise.jdx.dev/) to manage tool versions (Go, Node, etc.), pinned in `.mise.toml`. Shims are on
  PATH via `~/.bashrc` and `~/.zshenv`, so `go` and `node` should just work. If `go` is "not found", check that
  `~/.local/share/mise/shims` is on `$PATH`.
- After bumping npm deps, run `pnpm dedupe`. Without it, transitive deps (e.g. `postcss-html`'s `postcss`,
  `@axe-core/playwright`'s `@playwright/test`) can stay pinned to older nested versions, producing weird false-positive
  failures: stylelint 17.9 misparses Svelte inline `style="..."` attributes against an old postcss; website-typecheck
  fails on a `Page` type mismatch when AxeBuilder gets a different Playwright version than the e2e specs.
- ❌ **NEVER use `eprintln!`, `println!`, or `dbg!` in `src-tauri/` code.** They bypass the fern logger — no level
  filtering, no file output, no inclusion in error-report bundles. Clippy denies them at the crate root. Use
  `log::debug!` / `log::info!` / `log::warn!` / `log::error!` with a scoped `target:` (for example
  `log::debug!(target: "open_with", "...")`) so logs are filterable via `RUST_LOG`. **READ
  [`apps/desktop/src-tauri/src/logging/CLAUDE.md`](apps/desktop/src-tauri/src/logging/CLAUDE.md) before adding any log
  call or touching the log pipeline** — it has the rules and the why.
- ❌ **NEVER build the Tauri app with raw `cargo build`.** It produces a binary without the embedded frontend (white
  screen). Always build via `pnpm tauri build` or the `node scripts/tauri-wrapper.js build` wrapper from
  `apps/desktop/`. The `beforeBuildCommand` in `tauri.conf.json` runs the llama-server download (Go) and frontend build
  — skipping it breaks the app. For E2E builds:
  `node scripts/tauri-wrapper.js build --no-bundle --target $(rustc -vV | grep host | cut -d' ' -f2) -- --features playwright-e2e,virtual-mtp,smb-e2e`.
  The binary lands in `<repo>/target/<triple>/release/Cmdr`.
  - **Don't add your own build-cache layer.** `pnpm tauri build` already caches internally — Cargo's incremental
    compilation, Vite/SvelteKit's frontend build cache, and the `beforeBuildCommand`'s own short-circuits all kick in on
    warm runs. A "skip build if hash matches" check on top of that is redundant and risks shipping a stale binary.
- ❌ **No string-matching error or state classification.** Don't classify errors, app state, or control flow by checking
  substrings of a message, stderr, error title, or any other free-form text. Use a typed enum variant, an errno code, or
  an explicit flag on the struct that crosses the IPC boundary. The wording is for the user to read — code that branches
  on it breaks silently when copy changes, when the OS localizes, or when an upstream library reformats its messages.
  - **Tests too**: prefer `assert!(matches!(err, VolumeError::AlreadyExists(_)))` over `err.message.contains("...")`.
    The variant is the contract; the message is documentation.
  - **Enforced by**: `error-string-match` (Rust check, scans `apps/desktop/src-tauri/src/`) and
    `cmdr/no-error-string-match` (ESLint rule, scans `apps/desktop/src/`).
  - **Opt out only when there's no other option** (third-party CLI with no exit-code differentiation, etc.). Add
    `// allowed-error-string-match: <reason>` on the line above (Rust) or
    `// eslint-disable-next-line cmdr/no-error-string-match -- <reason>` (TS/Svelte). Pair the opt-out with `LC_ALL=C`
    on the subprocess and snapshot tests pinning the matched strings against a tool version.
- ❌ **Type-safe IPC: no raw `invoke('...')` outside the typed bindings folder.** Tauri command names are duplicated
  across the Rust `#[tauri::command]` site and every TS call site, with no compile-time link. Renaming the Rust side
  silently breaks runtime IPC with a generic "not allowed" error. The repo wires `tauri-specta` to generate typed
  bindings into `apps/desktop/src/lib/ipc/`; call them as `commands.commandName(args)` instead.
  - **Enforced by**: `cmdr/no-raw-tauri-invoke` (ESLint rule). Bypassed only inside `lib/ipc/` (the bindings),
    `routes/debug/` (dev-only debug panels), and test files.
  - **Regenerate** with `cd apps/desktop && pnpm bindings:regen` after any change to a `#[tauri::command]` surface or a
    Type-derived DTO. CI's `bindings-fresh` check fails if the committed `bindings.ts` is stale.
  - **At call sites, prefer named locals over inline primitives.** `commands.renameFile(from, to, force, volumeId)` is
    fine; `commands.foo(true, null, 5)` isn't — extract `const force = true; const volumeId = null; const retries = 5`
    first. This is the price specta charges for type safety.
  - For the rules around adding new commands, type shape constraints (`skip_serializing_if`, `serde_json::Value`), and
    the current exclusion list, read [`apps/desktop/src/lib/ipc/CLAUDE.md`](apps/desktop/src/lib/ipc/CLAUDE.md).

## Worktrees

When working in a linked git worktree under `.claude/worktrees/`, the gitignored `apps/desktop/src-tauri/resources/ai/`
(llama-server binaries, ~30 MB) starts empty. You don't need to do anything: `apps/desktop/src-tauri/build.rs` invokes
`apps/desktop/scripts/download-llama-server.go` on demand, which symlinks the dir from the main clone at
`~/projects-git/vdavid/cmdr/` when its `.version` matches, and falls back to downloading otherwise. So raw `cargo check`
Just Works in fresh worktrees — don't paper over a missing `resources/ai/` with a placeholder file.

## Workflow

- **Always read** [style-guide.md](docs/style-guide.md) before touching code. Especially sentence case!
- Cover your code with tests until you're confident. Don't go overboard. Test per milestone.
- **We don't use PRs.** Changes land directly on `main`. The "PR" section in `.claude/rules/git-conventions.md` is for
  the rare case David explicitly asks for one — default is a regular commit on `main` (or merging a feature branch into
  `main`). No `gh pr create`, no review-app webhook, none of that.
- **Never `git push` (or `git push --tags`) without explicit approval.** Even after a clean commit on `main`, pushing is
  an external action — wait until David says to push. This applies to feature branches and tags too. The user-level rule
  `~/.claude/rules/no-external-actions.md` already covers this; restating it here so it's impossible to miss.

Happy coding! 🦀✨
