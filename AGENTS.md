# Cmdr

This file is for AI agents. Human contributors, see [CONTRIBUTING.md](CONTRIBUTING.md).

Cmdr is an extremely fast AI-native file manager written in Rust, free forever for personal use on macOS (BSL license).
Downloadable at [the website](https://getcmdr.com).

- Dev server: `pnpm dev` at repo root
- Prod build: `pnpm build` at repo root
- Both must run **at repo root**. The root `package.json` has no `tauri` script, so `pnpm tauri dev` only works from
  inside `apps/desktop/`. Prefer the root form: both paths flow through `tauri-wrapper.js` and are equivalent, but the
  root form is what's documented and what other tooling assumes.

## Principles

These are general principles for the whole project. These are not just empty sentences on our wall, we live these:

### Top 6:

1. **Deliver delightful UX.** We always go the extra mile to make it absolutely delightful to use our software. Not just
   functional, but noticeably pleasant. Thoughtful phrasing. Accessible focus indicators. Putting real effort in
   dark/light modes. Nice images and anims. OS-custom everything. Respect the system font, sizing, theme,
   `prefers-reduced-motion`, etc.
2. **Elegance above all.** We have time to do outstanding work. We prefer a clean and elegant architecture over hacks,
   both internally for ourselves, and externally toward the user. We think about the long run.
3. **The app should feel rock solid.** The UI must _always_ be responsive. We never block the main thread. Every user
   action triggers immediate feedback, even if it's just a spinner. We communicate what's actually happening. Show
   progress. An ETA when possible. No progress bars stuck at 100%; show the real state. Long operations are always
   cancelable, stopping background work too, not just the UI. The user is always in control. Assume the hostile case
   (dead network mount, huge directory, crashed mid-operation) and handle it gracefully.
4. **Protect the user's data.** Use safe overwrite patterns like temp+rename. Offer rollback for destructive operations.
   Use atomic ops where possible. Design for the crash mid-operation. Test the shit out of the parts that write data.
5. **Be respectful to the user's resources.** Minimize CPU use, memory use, don't thrash the disks.
6. **Humans to humans.** It's okay to use AI to build the _internals_ of the app, which is the code. But everything that
   meets human eyes (any UI component, layout, articles, copy, microcopy, images, anims, human-targeted docs) should be
   made or closely reviewed and edited by a human.

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
9. **Name internals after the UI.** When a feature or action has a user-facing name, its internal identifiers (command
   ids, file/function/type names, settings keys, MCP tools) use the same vocabulary. If the UI says "Go to latest
   download", the code says `goToLatest`, not `revealLatest`. A UI "Go to…" backed by a `reveal_*` command forces every
   reader to keep a mental translation table, and the mismatch rots as the label drifts. Rename internals when you
   rename the UI.

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
- Feature-level docs live in **colocated `CLAUDE.md` + `DETAILS.md` files** next to the code (for example,
  `src/lib/settings/CLAUDE.md`). `CLAUDE.md` is the push tier: Claude Code auto-injects it whenever files in that
  directory are read, so every word costs tokens in every session that touches the area. `DETAILS.md` is the pull tier:
  the area's real docs, read on demand. The litmus: could an agent editing a random file here silently break something
  without this line? Then it's `CLAUDE.md`. Everything else is `DETAILS.md`. So `CLAUDE.md` = invariants, gotchas,
  don't-do-X-because-Y, a 2–3 line module map, and a pointer; target ~400–600 words. `DETAILS.md` = architecture
  narrative, data flows, decision rationale with depth, edge-case catalogs. Read it in whole before structural changes
  in an area. Never `@`-import `DETAILS.md` from a `CLAUDE.md` (that would rebuild the auto-load cost the split exists
  to remove). See `docs/architecture.md` for the full map.

## Testing and checking

**Before adding or modifying tests**, read [docs/testing.md](docs/testing.md): the testing playbook (decision table,
anti-patterns, per-feature checklist). The companion file [docs/tooling/testing.md](docs/tooling/testing.md) is the
tools inventory.

Always use `pnpm check` (in repo root!) for compilation, linting, formatting, and tests. It delegates to the checker
script and keeps output concise and focused: no `2>&1`, `head`, or `tail` needed. Don't run raw `cargo check`,
`cargo clippy`, `cargo fmt`, `cargo nextest run`, etc.

- Specific checks: name them positionally, space- or comma-separated (e.g. `pnpm check clippy rustfmt`, accepts IDs and
  nicknames). Use `--help` for the full list. `--check <name>` still works as an alias.
- Check groups, also positional: `pnpm check rust` / `svelte` / `go` (tech groups), `pnpm check desktop` / `website` /
  `api-server` / `scripts` (apps). Flag forms (`--rust`, `--app website`) work too.
- All checks: `pnpm check`

### When to run what

Three cadences. Pick the one that matches where you are in the work, not the one closest to "done." All three are
**cache-aware**: `pnpm check` re-runs a check only when that check's inputs changed since it last passed, so a run with
scoped changes is near-instant and a fully-unchanged tree runs nothing. The cadences below describe what each lane
_covers_; the cache decides what actually _runs_. `--fresh` (or `CMDR_CHECK_NO_CACHE=1`) forces a full fresh run; `--ci`
always runs fresh. See the input-fingerprint cache section in `scripts/check/CLAUDE.md`.

- **`pnpm check --fast` — every few file edits, on a self-imposed rhythm (~7 s cold, ~0 s when nothing it covers
  changed).** Don't wait for "before commit"; that's too late, by then a regression is buried under follow-up edits. Run
  after a small natural unit of work: a function rewritten, a test added, a config touched. Catches roughly half the
  things the full suite catches, for ~5% of the wall time, so use it liberally. The lane is editorially curated, not
  derived from timings; mutually exclusive with `--include-slow` / `--only-slow`. Covers:
  - All formatters (`oxfmt`, `rustfmt`, `gofmt`) and most non-compiling static linters (`cfg-gate`, `log-error-macro`,
    `error-string-match`, `lock-poison`, `ipc-enum-camelcase`, `cargo-machete`, `knip`, `import-cycles`, `type-drift`,
    `stylelint`, `css-unused`, `a11y-contrast`, `btn-restyle`, `a11y-coverage`, `bare-poll`, `e2e-linux-typecheck`,
    `ci-coverage`).
  - Go: `go-vet`, `staticcheck`, `ineffassign`, `misspell`, `gocyclo`, `go-tests`.
  - API server: `typecheck`, `tests`.
  - Website: `html-validate`, `bundle-size` (both self-skip when `dist/` is absent).
  - Warn-only metrics: `file-length`, `claude-md-reminder`, `changelog-links`.
  - **Does NOT cover**: `clippy`, Rust tests, `cargo-audit`, `cargo-deny`, `jscpd`, `bindings-fresh`, desktop ESLint /
    `svelte-check` / Svelte tests, website ESLint / typecheck / build / e2e, `docker-build`, or any E2E suite.
- **`pnpm check` — before every commit.** The full default suite (everything not marked `IsSlow`). Catches what `--fast`
  skips: `clippy`, Rust tests, audit/deny, svelte-check, website build, etc. This is the contract that what you're
  committing won't break CI. Now cheap when your changes are scoped: only the checks whose inputs you touched re-run, so
  there's no reason to skip it for "just a small change."
- **`pnpm check --include-slow` — before wrapping a milestone, declaring a feature done, or pushing a branch you've been
  sitting on.** Adds the slow lane on top of the default suite: `desktop-e2e-linux`, `desktop-e2e-playwright`,
  `rust-tests-linux`. Cache-aware too, so this means "affected slow checks as well" — a slow suite whose inputs are
  unchanged is a cache hit. Allow ~20 min when they do run; this is the gate before "I'm done."
- **`oxfmt` must always run before you call a task done.** It's monorepo-wide (markdown, YAML, JSON, JS/TS across every
  app) and takes ~1 second, so there's no reason to skip it. It's registered under `AppOther`, which means `--rust` and
  `--svelte` do NOT include it. If you only ran those, CI will catch unformatted markdown / JSON / etc. that you missed.
  Always finish with either `pnpm check` (the full suite) or at minimum `pnpm check oxfmt` after your other checks. No
  exceptions.
- Specific tests by name (the one exception where direct commands are fine):
  - Rust: `cd apps/desktop/src-tauri && cargo nextest run <test_name>`
  - Svelte: `cd apps/desktop && pnpm vitest run -t "<test_name>"`
  - Playwright: see `apps/desktop/test/e2e-playwright/CLAUDE.md` § "Running a single spec"
- **When iterating on one test, run only that test.** The full suite is for confirming CI-green before declaring done,
  not for each tweak. Running the whole Playwright suite for one new spec wastes ~10 minutes per cycle and produces
  noisy "cascade" failures when the broken test takes the app down with it (subsequent specs fail with connection
  errors). Same principle at smaller scales for Rust and Vitest.
- E2E (Playwright): See `apps/desktop/test/e2e-playwright/CLAUDE.md`. Build with `playwright-e2e` feature, start app,
  run tests
- Ubuntu test VM: See `apps/desktop/test/e2e-linux/CLAUDE.md` § "Ubuntu test VM"
- Docker SMB containers: 14 Samba containers (guest, auth, readonly, slow, flaky, unicode, deep nesting, etc.) for
  integration tests. Start with `apps/desktop/test/smb-servers/start.sh`. Connect from Rust via
  `smb2::testing::guest_port()` and friends. See `apps/desktop/test/smb-servers/README.md` for details.
- CI: Runs on PRs and pushes to main for changed files. Full run: Actions → CI → "Run workflow". The workflow map,
  change-detection filter rules, and the registry↔CI contract (`ci-coverage`) are in
  [docs/tooling/ci.md](docs/tooling/ci.md).

## Debugging

- **Data dirs (prod, dev, and dev-per-worktree are separate!)**: Prod:
  `~/Library/Application Support/com.veszelovszki.cmdr/`. Dev (plain `pnpm dev`):
  `~/Library/Application Support/com.veszelovszki.cmdr-dev/`. A per-worktree dev session started with
  `pnpm dev --worktree foo` lives at `~/Library/Application Support/com.veszelovszki.cmdr-dev-foo/`. The wrapper
  (`apps/desktop/scripts/tauri-wrapper.js`) resolves `CMDR_INSTANCE_ID` from flags and env, writes a fresh
  `tauri.instance.json` under `$TMPDIR` with the matching identifier (so Tauri's own `app_data_dir()` lands on the right
  path), and exports `CMDR_DATA_DIR` to the same path so direct file I/O (crash reports, logs, file-backed secret store)
  agrees without round-tripping through Tauri's API. See
  [`docs/tooling/instance-isolation.md`](docs/tooling/instance-isolation.md) for the per-resource breakdown and the
  precedence rules.
- **Logging**: Frontend and backend logs appear together in terminal and in the log dir (dev: `<CMDR_DATA_DIR>/logs/`,
  prod: `~/Library/Logs/com.veszelovszki.cmdr/`). **Read [docs/tooling/logging.md](docs/tooling/logging.md) before using
  `RUST_LOG`**: it has copy-paste recipes for every subsystem. Key gotcha: the Rust library target is `cmdr_lib`, not
  `cmdr`. Use `RUST_LOG=cmdr_lib::module=debug`. Note: `cmdr_lib` (lib) and `Cmdr` (bin) are both in the `cmdr` Cargo
  package, so `Compiling cmdr` in build output covers BOTH targets. Cargo won't show `Compiling cmdr_lib` separately.
- **Crash reports**: When the app crashes, it writes a crash file to the data dir (`crash-report.json` alongside
  `settings.json`). On next launch, the app detects this file and offers to send a crash report. See
  `src-tauri/src/crash_reporter/CLAUDE.md` for architecture details.
- **Error reports**: When triaging an error report bundle (zip + `manifest.json`), read
  `src-tauri/src/error_reporter/CLAUDE.md` first: it documents the bundle layout, what each piece captures, and the
  redaction conventions.
- **Hot reload**: `pnpm dev` hot-reloads. Max 15s for Rust, max 3s for frontend. Markdown edits trigger nothing:
  `apps/desktop/src-tauri/.taurignore` shields the Tauri CLI's watcher (which otherwise rebuilds + restarts the app on
  ANY file change under `src-tauri/`, including the colocated `CLAUDE.md`s) and `server.watch.ignored` in
  `vite.config.js` shields Vite from `src-tauri/`. Don't delete either.
- **Index DB queries**: The index SQLite DB uses a custom `platform_case` collation, so the `sqlite3` CLI can't query
  it. Use `cargo run -p index-query -- <db_path> "<sql>"` instead. See
  [docs/tooling/index-query.md](docs/tooling/index-query.md) for examples and DB paths.

## MCP (testing the running app)

Two MCP servers are available when the app is running via `pnpm dev`:

- **cmdr** (Cmdr MCP HTTP server): high-level app control: navigation, file operations, search, dialogs, state
  inspection. This is the primary way to test and interact with the running app. Architecture docs:
  `src-tauri/src/mcp/CLAUDE.md`.
- **tauri** (Tauri MCP bridge): low-level Tauri access: screenshots, DOM inspection, JS execution, IPC calls. Use for
  visual verification and UI automation.

Both bind `127.0.0.1` only on ephemeral ports per instance. External clients (the `scripts/mcp-call.sh` CLI, agent
helpers) read the actual port from `<CMDR_DATA_DIR>/mcp.port` (Cmdr server) or `<CMDR_DATA_DIR>/tauri-mcp.port`
(bridge); `CMDR_MCP_PORT` still pins. See [docs/tooling/instance-isolation.md](docs/tooling/instance-isolation.md) for
the per-resource breakdown and [docs/tooling/mcp.md](docs/tooling/mcp.md) for usage patterns, connection resilience, and
common pitfalls.

**If the `mcp__cmdr-dev__*` / `mcp__tauri__*` tools are unavailable or erroring in your session** (spawned agents often
start without them connected), use `./scripts/mcp-call.sh` — it talks to the same Cmdr MCP server over HTTP and
discovers the port and bearer token by itself. Run it with `--help` for usage and `--list-tools` for every tool + its
parameter schema.

## Where to put instructions

Split by kind and by level:

- **Imperatives** ("always / never X") → `rules/` files: `~/.claude/rules/` for cross-project preferences,
  `.claude/rules/` here for project rules. Keep them concise.
- **Knowledge** (how the codebase works, gotchas, how-tos) → this `AGENTS.md` and colocated `CLAUDE.md` files.
- User-level rules already apply to every project, so don't restate them here.
- **Don't use** `memory/MEMORY.md` for either: it's not transparent and not version-controlled. Prefer rules and docs.

## Hard rules

The project's hard rules live as focused, auto-loaded files in [`.claude/rules/`](.claude/rules/), each concise and
pointing to its detailed colocated doc. They're always in context, so this file stays knowledge, not rules. Non-Claude
agents should read (and add/edit when needed) the relevant files in `.claude/rules/` manually.

Two project facts worth stating here directly: tool versions are mise-managed (Go, Node, etc., pinned in `.mise.toml`;
shims on PATH; if `go` / `node` isn't found, check that `~/.local/share/mise/shims` is on `$PATH`). Icons come from
`unplugin-icons` + `@iconify-json/lucide` (inline SVGs from `~icons/lucide/{icon-name}`); see `docs/guides/icons.md`.

## Worktrees

- When working in a linked git worktree under `.claude/worktrees/`, the gitignored
  `apps/desktop/src-tauri/resources/ai/` (llama-server binaries, ~30 MB) starts empty. You don't need to do anything:
  `apps/desktop/src-tauri/build.rs` invokes `apps/desktop/scripts/download-llama-server.go` on demand, which symlinks
  the dir from the main clone at `~/projects-git/vdavid/cmdr/` when its `.version` matches, and falls back to
  downloading otherwise. So raw `cargo check` Just Works in fresh worktrees. Don't paper over a missing `resources/ai/`
  with a placeholder file.
- When using worktrees, always branch off from _local_ `main` (not `origin/main`) and rebase and FF _local_ main.
- To run two dev sessions in parallel from different worktrees, pass `--worktree <slug>` to `pnpm dev` in each. The
  wrapper picks per-instance ports (Vite, MCP, Tauri MCP bridge), a per-instance data dir
  (`~/Library/Application Support/com.veszelovszki.cmdr-dev-<slug>/`), and a per-instance Dock label so the sessions
  never collide. See [docs/tooling/instance-isolation.md](docs/tooling/instance-isolation.md) for the full breakdown.

## Workflow

- **Follow** [style-guide.md](docs/style-guide.md) when touching code. Especially sentence case!
- Cover your code with tests until you're confident. Don't go overboard. Test per milestone.
- **We don't use PRs.** Changes land on `main` via fast-forward merge from a worktree branch. The "PR" section in
  `.claude/rules/git-conventions.md` is only for the rare case David explicitly asks for one. No `gh pr create`.
- **Don't `git push` without explicit approval, and don't push routinely** (solo work, limited CI). See the
  `push-cadence` and `no-external-actions` user rules.
- **The delivery pipeline is fully wired; don't re-audit it.** Releases are agent-automated end to end (tag → CI
  build/sign/notarize → publish `latest.json` → website deploy → silent in-app update via the FDA-preserving updater),
  and user-feedback loops are live (crash reports → email cron, error reports → instant Discord webhook, anonymous
  analytics → PostHog + analdash). See [docs/guides/releasing.md](docs/guides/releasing.md).

Happy coding! 🦀✨

Read docs/architecture.md next!
