# Cmdr

This file is for AI agents. Human contributors, see [CONTRIBUTING.md](CONTRIBUTING.md).

Cmdr is an extremely fast, keyboard-first two-pane file manager written in Rust, free forever for personal use on macOS
(BSL license). Downloadable at [the website](https://getcmdr.com).

Cmdr started at 2025-12-25, and is currently an open beta, with a few dozen users, who understand the early staged.

## Principles

These are the highest level principles for the whole project.

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
10. **A11y.** Cmdr is keyboard first, but everything works with the mouse, too. We respect dark/light mode,
    prefers-reduced-motion, use AA+ contrast, and think about screen readers.

## Docs

- Feature-level docs live in **colocated `DETAILS.md` + `CLAUDE.md` files** next to the code. For example,
  `src/lib/settings/DETAILS.md` and `src/lib/settings/CLAUDE.md`. Common shorthand: `D.md` and `C.md`, or `D/C.md` — D
  always comes first because D is the default destination for additions; C is the exception.
- `CLAUDE.md`: Claude Code auto-injects it when reading files in that dir, so each word costs tokens in all sessions
  that touch the area. We try to keep this at 400–600 words. Contains everything that an agent editing a related file
  could silently break something without that info. Invariants, gotchas, don't-do-X-because-Y, a 2–3 line module map,
  and a pointer.
- `DETAILS.md` is everything else: the area's real docs, read on demand. Architecture narrative, data flows, decision
  rationale with depth, edge-case catalogs. Read it in whole before structural changes in an area.
- Never `@`-import `DETAILS.md` from a `CLAUDE.md`.
- When adding info, default to writing `D.md`. Only write `C.md` if your addition meets the importance bar (an agent
  editing nearby code could silently break something without it); even then, the depth goes in `D.md` with a pointer
  from `C.md`.
- Many areas (and apps) have a `C.md` but no `D.md` yet. That's not a decision — create the missing `D.md` without
  hesitation the first time you have details worth writing down. Same at app level (for example
  `apps/website/DETAILS.md`).
- See `docs/architecture.md` for the full map.

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
- `/brand/` - Tracked brand/press-kit assets (logos, master screenshots, marketing copy). See its `CLAUDE.md`
- `/docs/` - Dev docs
  - `guides/` - How-to guides
  - `notes/` - Temporary reference notes (benchmarks, analysis) linked from CLAUDE.md files; see
    [notes/README.md](docs/notes/README.md)
  - `specs/` - Per-development specs and task lists (temporary, periodically wiped); see
    [specs/index.md](docs/specs/index.md)
  - `tooling/` - Internal tooling docs
  - `architecture.md` - Map of all subsystems with links to their `CLAUDE.md` files
  - `style-guide.md` - Writing, code, and design style rules
  - `security.md` - Security policies
  - `maintenance.md` - Recurring maintenance tasks (dep bumps, allowlist trims, doc sweeps) and a log of past runs
- `/scripts/check/` - Go-based unified check runner

## Running

- Dev server: `pnpm dev`. Prod build: `pnpm build`. Both must run **at workspace root**. The root `package.json` has no
  `tauri` script, so `pnpm tauri dev` only works from `apps/desktop/`. But other tooling expects `pnpm dev` so use that.
- Start a per-worktree dev session with `pnpm dev --worktree foo`.

## Testing

**Before adding or modifying tests**, read [docs/testing.md](docs/testing.md): the testing playbook (decision table,
anti-patterns, per-feature checklist). The companion file [docs/tooling/testing.md](docs/tooling/testing.md) is the
tools inventory.

## Checking

Full docs in `scripts/check/CLAUDE.md` and `scripts/check/DETAILS.md`.

Always use `pnpm check` (in repo root!) for compilation, linting, formatting, and tests. It calls the checker script.
Output is concise and focused, so no `2>&1`, `head`, or `tail` needed. Don't run raw `cargo check`, `cargo clippy`,
`cargo fmt`, `cargo nextest run`, etc.

- Specific checks: name them positionally, space- or comma-separated (e.g. `pnpm check clippy rustfmt`, accepts IDs and
  nicknames). Use `--help` for the full list.
- Cache: The checker script is **cache-aware**. `pnpm check` re-runs a check only when that check's inputs changed since
  it last passed, so a run with scoped changes is near-instant and a fully-unchanged tree runs nothing. The cadences
  describe what each lane _covers_; the cache decides what actually _runs_. `--fresh` forces a full fresh run; `--ci`
  always runs fresh.
- Tech groups: `pnpm check rust` / `svelte` / `go`
- App groups: `pnpm check desktop` / `website` / `api-server` / `scripts`
- Cadences: `pnpm check --fast` / `pnpm check` / `pnpm check --include-slow` / `pnpm check --only-slow`. Pick the one
  that matches where you are in the work, not the one closest to "done."
  - **`pnpm check oxfmt`** is leanest, takes about 1 s, formats all Markdown, YAML, JSON, JS/TS in every app. It's under
    "other", so `--rust` and `--svelte` do NOT include it.
  - **`pnpm check --fast`** runs for ~7 s cold, ~0 s when nothing it covers changed. Covers all formatters, most
    non-compiling static linters, and all Go checks. Catches roughly half of the default suite, for ~5% of the time. Use
    it liberally; run it after every few file edits.
  - **`pnpm check`** is the default suite, runs for ~80s cold, ~0 s with no changes. Covers all that `--fast` covers,
    plus `clippy`, Rust tests, audit/deny, `jscpd`, `bindings-fresh`, `svelte-check`, ESLints, typechecks,
    `website build`, and `docker-build`. Run it after each milestone or so.
  - **`pnpm check --include-slow`** takes about 6 mins, runs the default suite plus `rust-tests-linux`,
    `desktop-e2e-playwright`, and `desktop-e2e-linux`. Run it before wrapping a project or milestone, depending on size.
    Cache-aware, but only as a suite, not for the individual tests.
    - Important caveat for the Playwright E2E tests: they run on the host macOS, and when the laptop is in use, the app
      windows keep popping up, so at times, some key presses and clicks end up in the app, breaking some tests. So if
      you see weird flakes, it might be this. (This does NOT apply the Linux suite!)
- Specific tests by name (the one exception where direct commands are fine):
  - **When iterating on one test, run only that test.** The full suite is for confirming CI-green before declaring done,
    not for each tweak. Running the whole Playwright suite for one new spec wastes ~6 minutes per cycle and produces
    noisy "cascade" failures when the broken test takes the app down with it (subsequent specs fail with connection
    errors). Same principle at smaller scales for Rust and Vitest.
  - Rust: `cd apps/desktop/src-tauri && cargo nextest run <test_name>`
  - Svelte: `cd apps/desktop && pnpm vitest run -t "<test_name>"`
  - Playwright: see `apps/desktop/test/e2e-playwright/DETAILS.md` § "Running a single spec"

**Very important:** Always finish each unit of your work by running the right checks!

More info on tests:

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

- **Data dirs (prod, dev, and dev-per-worktree are separate!)**:
  - Prod: `~/Library/Application Support/com.veszelovszki.cmdr/`.
  - Plain `pnpm dev`: `~/Library/Application Support/com.veszelovszki.cmdr-dev/`.
  - `pnpm dev --worktree foo`: `~/Library/Application Support/com.veszelovszki.cmdr-dev-foo/`.
- `apps/desktop/scripts/tauri-wrapper.js` resolves `CMDR_INSTANCE_ID` from flags and env, writes a fresh
  `tauri.instance.json` under `$TMPDIR` with the matching identifier (so Tauri's own `app_data_dir()` lands on the right
  path), and exports `CMDR_DATA_DIR` to the same path so direct file I/O (crash reports, logs, file-backed secret store)
  agrees without round-tripping through Tauri's API. See
  [`docs/tooling/instance-isolation.md`](docs/tooling/instance-isolation.md) for the per-resource breakdown and the
  precedence rules.
- **Logging**: Frontend and backend logs appear together in terminal and in the log dir (dev: `<CMDR_DATA_DIR>/logs/`,
  prod: `~/Library/Logs/com.veszelovszki.cmdr/`). **Read [docs/tooling/logging.md](docs/tooling/logging.md) before using
  `RUST_LOG`**: it has copy-paste recipes for every subsystem. Key gotcha: the Rust library target is `cmdr_lib`, not
  `cmdr`! Use `RUST_LOG=cmdr_lib::module=debug`. Note: `cmdr_lib` (lib) and `Cmdr` (bin) are both in the `cmdr` Cargo
  package, so `Compiling cmdr` in build output covers BOTH targets. Cargo won't show `Compiling cmdr_lib` separately.
- **Crash reports**: When the app crashes, it writes a crash file to the data dir (`crash-report.json` alongside
  `settings.json`). On next launch, the app detects this file and offers to send a crash report. See
  `src-tauri/src/crash_reporter/CLAUDE.md` for architecture details.
- **Error reports**: When triaging an error report bundle (zip + `manifest.json`), read
  `src-tauri/src/error_reporter/CLAUDE.md` first: it documents the bundle layout, what each piece captures, and the
  redaction conventions.
- **Hot reload**: `pnpm dev` hot-reloads the BE+FE. This works reliaby, and takes max 15s for Rust, max 3s for frontend.
  - `apps/desktop/src-tauri/.taurignore` ignores `*.md` and `vite.config.js` ignores `**/src-tauri/**` for right
    triggers.
- **Index DB queries**: The index SQLite DB uses a custom `platform_case` collation, so the `sqlite3` CLI can't query
  it. Use `cargo run -p index-query -- <db_path> "<sql>"` instead. See
  [docs/tooling/index-query.md](docs/tooling/index-query.md) for examples and DB paths.
- **Dev mock flags** (read by the backend process, so set them in the shell that runs `pnpm dev`):
  `CMDR_MOCK_LICENSE=commercial` mocks the license, and `CMDR_SIMULATE_UPDATE_FROM=<version>` (for example `0.20.0`)
  forces the "What's new" popup to show on every launch as if the app just updated from that version (it never stamps
  `lastSeenVersion`, so it keeps showing until you unset it). See `src/lib/whats-new/CLAUDE.md`.

## Testing the running app via MCP

Two types of MCP servers are available when the app is running via `pnpm dev`:

- **cmdr-dev** and **cmdr-prod**: high-level app control: navigation, file operations, search, dialogs, state
  inspection. This is the primary way to interact with the running app. Read `src-tauri/src/mcp/CLAUDE.md` before using.
- **tauri** (Tauri MCP bridge): low-level Tauri access: screenshots, DOM inspection, JS execution, IPC calls. Use for
  visual verification and stuff that the Cmdr MCP can't do.

Both bind `127.0.0.1` only on ephemeral ports per instance. Clients like `scripts/mcp-call.sh` read the port from
`<CMDR_DATA_DIR>/mcp.port` / `<CMDR_DATA_DIR>/tauri-mcp.port`; `CMDR_MCP_PORT` still pins. See
[docs/tooling/instance-isolation.md](docs/tooling/instance-isolation.md) for the per-resource breakdown and
[docs/tooling/mcp.md](docs/tooling/mcp.md) for usage patterns, connection resilience, and common pitfalls.

**If the `mcp__cmdr-dev__*` / `mcp__tauri__*` tools are unavailable or erroring in your session** (spawned agents often
start without them connected), use `./scripts/mcp-call.sh`. it talks to the same Cmdr MCP server over HTTP and discovers
the port and bearer token by itself. Run it with `--help` for usage and `--list-tools` for every tool + its parameter
schema.

## Where to put instructions

Split by kind and by level:

- **Imperatives** ("always / never X") → `rules/` files: `~/.claude/rules/` for cross-project preferences,
  `.claude/rules/` here for project rules. Keep them concise.
- **Knowledge** (how the codebase works, gotchas, how-tos) → this `AGENTS.md` and colocated `DETAILS.md` files.
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

- Use worktrees by default. Don't work on `main`. Started editing on `main` by mistake? Move the changes to a worktree
  (`~/.claude/docs/worktree-move-changes.md`) rather than continuing.
- When working with parallel subagents, create a hub worktree for yourself, and let them work on their own
  branches+worktrees, then reconcile them.
- Create them at `.claude/worktrees/`.
- Always branch off from _local_ `main` and rebase and FF _local_ main.
- Stuff needed in a new worktree:
  - Do `cp -c ~/projects-git/vdavid/cmdr/target <worktree>/target`. Instant+free on APFS. Deps are fingerprinted on
    version + features + rustc + profile, so only the workspace members rebuild. Win.
  - CodeGraph: Do this (from `~/.claude/docs/codegraph-worktree.md`), takes about two secs:

    ```bash
    WORKTREE=.claude/worktrees/<slug>

    mkdir -p "$WORKTREE/.codegraph"
    cp -c .codegraph/codegraph.db "$WORKTREE/.codegraph/codegraph.db"
    cp    .codegraph/config.json  "$WORKTREE/.codegraph/config.json"
    (cd "$WORKTREE" && codegraph sync)
    ```

  - llama-server binaries: (gitignored) `apps/desktop/src-tauri/resources/ai/` starts empty. No need to do anything:
    `apps/desktop/src-tauri/build.rs` invokes `apps/desktop/scripts/download-llama-server.go` on demand, which symlinks
    the dir from the main clone at `~/projects-git/vdavid/cmdr/` when its `.version` matches, and falls back to
    downloading otherwise. So raw `cargo check` Just Works in fresh worktrees.
  - Always do these. Worth those 2 seconds, even for very short-lived worktrees.

- Use `pnpm dev --worktree <slug>` on worktrees. Such sessions never collide. See
  [docs/tooling/instance-isolation.md](docs/tooling/instance-isolation.md) for the full breakdown.
  - **Run it FROM the worktree dir** (`cd` into the worktree first). Vite serves whatever source tree it's launched in,
    so `pnpm dev --worktree <slug>` from the main repo root runs **main's** frontend with only the worktree's data
    dir/ports/Dock label. Your worktree edits then never load and the app looks unfixed (a real trap: you QA the wrong
    code). The `--worktree` flag isolates the instance, not the source.
- When FF-ing main, always delete the worktree+branch. Also remove the worktree's now-orphaned dev data dir
  (`~/Library/Application Support/com.veszelovszki.cmdr-dev-<slug>`): a `pnpm dev --worktree <slug>` session leaves one
  behind (often ~1 GB once its drive index builds), and nothing cleans it up when the worktree goes. They pile up fast.

## Workflow

- **Follow** [style-guide.md](docs/style-guide.md) when touching code. Especially sentence case!
- Do your work in TDD whenever reasonable. Red → green.
- Cover your code with tests until you're confident. Don't go overboard.
- Always run the checks at the cadence mandated by the guidelines.
- **We don't use PRs.** Changes land on `main` via fast-forward merge from a worktree branch. The "PR" section in
  `.claude/rules/git-conventions.md` is only for the rare case David explicitly asks for one. No `gh pr create`.
- **Don't `git push` without explicit approval, and don't push routinely** (solo work, limited CI). See the
  `push-cadence` and `no-external-actions` user rules.
- Step back and reflect per milestone. Is what you did solid AND elegant? Are you confident AND proud?
- For large parallel-agent efforts, see [multi-agent refactors](docs/guides/multi-agent-refactors.md).
- **The delivery pipeline is fully wired; don't re-audit it.** Releases are agent-automated end to end (tag → CI
  build/sign/notarize → publish `latest.json` → website deploy → silent in-app update via the FDA-preserving updater),
  and user-feedback loops are live (crash reports → email cron, error reports → instant Discord webhook, anonymous
  analytics → PostHog + analdash). See [docs/guides/releasing.md](docs/guides/releasing.md).

Happy coding! 🦀✨

Read docs/architecture.md next if it's not pre-loaded!
