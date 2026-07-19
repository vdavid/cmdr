# Cmdr

This file is for AI agents. Human contributors, see [CONTRIBUTING.md](CONTRIBUTING.md).

Cmdr is an extremely fast, keyboard-first two-pane file manager in Rust, free forever for personal use on macOS (BSL
license), at [getcmdr.com](https://getcmdr.com). Started 2025-12-25; in open beta with a few dozen early-stage-aware
users.

This is a monorepo of four apps:

- **`apps/desktop/`**: the app itself (Rust + Tauri 2 backend, Svelte 5 + TypeScript frontend). Read
  [`apps/desktop/CLAUDE.md`](apps/desktop/CLAUDE.md) before working here.
- **`apps/website/`**: getcmdr.com marketing site (Astro + Tailwind v4). See
  [`apps/website/CLAUDE.md`](apps/website/CLAUDE.md).
- **`apps/api-server/`**: Cloudflare Worker + Hono (licensing, telemetry, crash/error reports, downloads, admin). See
  [`apps/api-server/CLAUDE.md`](apps/api-server/CLAUDE.md).
- **`apps/analytics-dashboard/`**: private SvelteKit metrics dashboard on CF Pages. See
  [`apps/analytics-dashboard/CLAUDE.md`](apps/analytics-dashboard/CLAUDE.md).

Shared tooling: the Go check runner ([`scripts/check/CLAUDE.md`](scripts/check/CLAUDE.md)) and dev docs
([`docs/architecture.md`](docs/architecture.md)).

## Where to look (router)

- **Editing code**: the colocated `CLAUDE.md` autoloads when you touch a directory. For "where does symbol X live", use
  CodeGraph (`codegraph_search`), not a doc. CodeGraph is enabled for the project and is up to date! Autoload is
  touch-based, so read a subsystem's `CLAUDE.md` before running its tooling/tests (e.g. `test/e2e-playwright/CLAUDE.md`
  before the E2E suite).
- **Planning in an unfamiliar area**: [`docs/architecture.md`](docs/architecture.md), the subsystem map (what + where +
  a pointer to each area's docs).
- **A procedure** (release, screenshots, deps, adding a window or icon): [`docs/guides/`](docs/guides) and the skills.
- **Debugging a running app / reading logs**: [This](docs/tooling/logging.md) is the first stop, not `Console.app` or
  grepping code. It gives all (FE & BE) log-file paths, format, and `RUST_LOG` recipes. Watch RAM per line with
  `CMDR_LOG_RAM_USE=1`.
- **Branding / marketing**: [`brand/CLAUDE.md`](brand/CLAUDE.md), `apps/website/`, and [`README.md`](README.md). You
  don't need app internals.
- **Writing, code, or UI-copy style**: [`docs/style-guide.md`](docs/style-guide.md) (read before writing user-facing
  strings or non-trivial code). Product and UX values: [`docs/design-principles.md`](docs/design-principles.md).
- **Translating the app / adding a language**: [`docs/guides/i18n-translation.md`](docs/guides/i18n-translation.md) (the
  translator process, per-language style guides under [`docs/i18n/`](docs/i18n), and the local reference pile).

## Principles

Full product and design values: [`docs/design-principles.md`](docs/design-principles.md). The highest-level ones:

1. **Delightful UX**, not just functional: thoughtful phrasing, real dark/light modes, OS-native everything, respect the
   system font, theme, and `prefers-reduced-motion`.
2. **Elegance above all**: clean architecture over hacks; we're here for the long run.
3. **Rock solid**: never block the main thread, immediate feedback, honest progress and ETA, everything cancelable
   (background work too), handle the hostile case (dead mount, huge dir, crash mid-operation).
4. **Protect the user's data**: safe-overwrite (temp+rename), atomic ops where possible, design for the crash, test
   data-writing paths hard.
5. **Respect the user's resources**: minimize CPU, memory, and disk thrash.
6. **Humans to humans**: AI builds the internals (code); anything meeting human eyes (UI, copy, images, human docs) is
   made or closely reviewed by a human.

Engineering principles: smart backend / thin frontend (business logic in Rust, IPC commands are pass-throughs);
organized by feature, not layer (component + module + tests + docs colocated); subscribe, don't poll; invest in
testability and tooling; name internals after the UI; keyboard-first with full mouse support and a11y (AA+ contrast,
screen readers).

## Docs

Two colocated tiers per code area, enforced by checks:

- **`CLAUDE.md`** (push tier): auto-injected whenever a (sub)agent touches a dir, in every such session and worktree.
  Holds ONLY must-knows: invariants, gotchas, "don't do X because Y" guardrails, a 2–3 line module map, and a pointer to
  `DETAILS.md`. The litmus: "would editing a file here get something wrong or silently break sg, without this line?" If
  not, it's not a must-know. Hard ceiling is 600 words, but try to keep it at far less. Only the essentials.
  `claude-md-length` warns past 600.
- **`DETAILS.md`** (pull tier): everything else, read on demand. Architecture, data flows, decision rationale, edge-case
  catalogs. Unlimited length.
- We abbreviate these to `C.md`, `D.md`, and `C+D.md` together.
- **Every area `C.md` has a sibling `D.md` and links it** (enforced by `claude-md-details-sibling`). Default new content
  to `D.md`; promote to `C.md` only if it clears the must-know bar. Never `@`-import `D.md` from a `C.md`.
- If you need to cut `C.md`, do it radically: make its parts sound like tweets, and move stuff to `D.md` as-needed. Aim
  for 3–400 words. OR consider splitting the dir if it's genuinely too complex to describe in 600 words!
- The doc graph is enforced: `docs-reachable` (every doc reachable from this file by link-walking), `docs-dead-links`
  (no broken links), and `resident-doc-budget` (the always-resident bundle, this file plus its `@`-imports plus
  `.claude/rules/`, can't silently regrow). Keep this section crisp: it's the contract every agent replicates.
- How the doc system works and how to slim it (playbook, principles, why): [`docs/doc-system.md`](docs/doc-system.md).

## Writing voice

Full rules in [`docs/style-guide.md`](docs/style-guide.md). Always: active voice, friendly and concise, sentence case
for every title and label, Oxford comma, ISO dates (YYYY-MM-DD), no em-dashes (en-dash for ranges only), spell out one
through nine, thousands separators on user-facing counts, gender-neutral, avoid "just/simple/easy". Error messages stay
conversational and actionable and never use the words "error" or "failed". The website speaks product-first (no "I" or
"we"); the app may speak as David where deliberately personal (onboarding, About).

## File structure

- `apps/desktop/`: `src/` (Svelte frontend), `src-tauri/` (Rust backend), `test/` (Vitest, Playwright, Linux Docker E2E,
  SMB fixtures), `scripts/`. The other three apps are listed above.
- `brand/`: tracked brand and press-kit assets.
- `docs/`: [`architecture.md`](docs/architecture.md) (the map), [`guides/`](docs/guides) (how-tos), `tooling/` (service
  and workflow references), [`specs/index.md`](docs/specs/index.md) (per-development plans, periodically wiped),
  [`notes/README.md`](docs/notes/README.md) (benchmarks and analysis), `style-guide.md`, `design-principles.md`,
  `security.md`, `maintenance.md`.
- `scripts/check/`: the Go check runner. `.github/workflows/`: CI.

## Checker script

Always use **`pnpm check`** at the repo root (never raw `cargo` / `vitest` / etc.); it's cache-aware. Cadence: `--fast`
while iterating, plain `pnpm check` per milestone, `--include-slow` before wrapping; prefer **`-q`** (`--quiet`), it
collapses passes to one line. You can also scope by name (`pnpm check clippy`), tech (`rust` / `svelte` / `go`), or app
(`desktop` / `website` / ...). [Full docs](scripts/check/CLAUDE.md). **Finish every unit of work by running the right
checks.** Don't even try to tail the checker script.

## Testing

Before adding or changing tests, read [`docs/testing.md`](docs/testing.md) (the playbook) and
[`docs/tooling/testing.md`](docs/tooling/testing.md) (the tools inventory). Desktop-specific test, MCP, and E2E
mechanics live in [`apps/desktop/CLAUDE.md`](apps/desktop/CLAUDE.md).

## Hard rules

Project hard rules are focused, autoloaded files in [`.claude/rules/`](.claude/rules) (always in context; non-Claude
agents should read them manually). Two facts worth stating directly: tool versions are mise-managed (`.mise.toml`; if
`go` / `node` isn't found, check that `~/.local/share/mise/shims` is on `$PATH`), and icons come from `unplugin-icons` +
`@iconify-json/lucide` (see [`docs/guides/icons.md`](docs/guides/icons.md)).

## Workflow

- Desktop worktree setup (target clone, CodeGraph, data-dir cleanup) is in
  [`apps/desktop/CLAUDE.md`](apps/desktop/CLAUDE.md). For parallel-subagent efforts, see
  [`docs/guides/multi-agent-refactors.md`](docs/guides/multi-agent-refactors.md).
- Before doing **legwork**, read the [guide](docs/guides/agent-legwork-guide.md).
- **TDD where reasonable** (red → green); cover code with tests until confident, not beyond.
- Step back per milestone: is it solid AND elegant?
- **The delivery pipeline is fully wired; don't re-audit it.** Releases are agent-automated end to end (tag → CI
  build/sign/notarize → publish `latest.json` → website deploy → FDA-preserving silent update), and feedback loops are
  live (crash → email cron, error → Discord, anonymous analytics → PostHog). See
  [`docs/guides/releasing.md`](docs/guides/releasing.md).

Happy coding! 🦀✨
