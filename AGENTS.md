# Cmdr

This file is for AI agents. Human contributors, see `CONTRIBUTING.md`.

Cmdr is an extremely fast, keyboard-first two-pane file manager in Rust, free for personal use on macOS (BSL license),
at [getcmdr.com](https://getcmdr.com). Started 2025-12-25; in open beta with a few dozen early-stage-aware users.

This is a monorepo of four apps:

- **`apps/desktop/`**: the app itself (Rust + Tauri 2 backend, Svelte 5 + TypeScript frontend).
- **`apps/website/`**: getcmdr.com marketing site (Astro + Tailwind v4).
- **`apps/api-server/`**: Cloudflare Worker + Hono (licensing, telemetry, crash/error reports, downloads, admin).
- **`apps/analytics-dashboard/`**: private SvelteKit metrics dashboard on CF Pages.

Shared tooling: the Go check runner (`scripts/check/CLAUDE.md`) and dev docs (`docs/architecture.md`).

## Principles

Full product and design values: `docs/design-principles.md`. The highest-level ones:

1. **Protect the user's data**: safe-overwrite (temp+rename), atomic ops where possible, design for the crash, test
   data-writing paths hard.
2. **Rock solid**: never block the main thread, immediate feedback, honest progress and ETA, everything cancelable
   (background work too), handle the hostile case (dead mount, huge dir, crash mid-operation).
3. **Delightful UX**, not just functional: thoughtful phrasing, real dark/light modes, OS-native everything, respect the
   system font, theme, and `prefers-reduced-motion`.
4. **Humans to humans**: AI builds the internals (code); anything meeting human eyes (UI, copy, images, human docs) is
   made or closely reviewed by a human. (Caveat: we'll human-review _translations_ much later. This is OK.)
5. **Respect the user's resources**: minimize CPU, memory, and disk thrash.
6. **Elegance above all**: clean architecture over hacks; we're here for the long run.

Engineering principles: smart backend / thin frontend (business logic in Rust, IPC commands are pass-throughs);
organized by feature, not layer (component + module + tests + docs colocated); subscribe, don't poll; invest in
testability and tooling; name internals after the UI; keyboard-first with full mouse support and a11y (AA+ contrast,
screen readers).

## Writing voice

Full rules in `docs/style-guide.md`. Always: active voice, friendly and concise, sentence case for every title and
label, Oxford comma, ISO dates (YYYY-MM-DD), no em-dashes (en-dash for ranges only), spell out one through nine,
thousands separators on user-facing counts, gender-neutral, avoid "just/simple/easy". Error messages stay conversational
and actionable and never use the words "error" or "failed". The website speaks product-first (no "I" or "we"); the app
may speak as David where deliberately personal (onboarding, About).

## Docs

Two colocated tiers per code area, enforced by checks:

- We often call `CLAUDE.md` and `DETAILS.md` `C.md` and `D.md`, `C+D.md` together.
- **`C.md`** Auto-injected by the CC harness whenever a (sub)agent touches a dir, every session and wt. ONLY must-knows:
  gotchas, guardrails, a 2–3 line module map, and pointer to `DETAILS.md`. **Aim for 300–400 words.** `claude-md-length`
  warns past 600, but that's the alarm, not the target.
- **`D.md`** the rest. Read on demand. Architecture, data flows, decision rationale, edge-case catalogs. No length
  limit, but try to be concise to make it token-efficient. When writing, default to `D.md`; promote to `C.md`.
- `claude-md-details-sibling` enforces all `C.md` and `D.md` to exist in pairs. Never `@`-import `D.md` from a `C.md`!
- Cut `C.md` radically: make each part sound like a tweet, move depth to `D.md`, and split the module if it can't reach
  300–400 words that way.
- `docs-reachable` enforces the doc graph to be linked: (every doc reachable from this file by link-walking),
  `docs-dead-links` and `docs-link-text` (no broken or path-shaped reference), and `resident-doc-budget` (the
  always-resident bundle, this file plus its `@`-imports plus `.claude/rules/`, can't silently regrow). Keep this
  section crisp: it's the contract every agent replicates.

Rules for writing them:

- **Keep in sync.** Touch code in a `C+D.md` dir → update them. `Gotcha/Why` when a wrong assumption bit you;
  `Decision/Why` in `D.md`, plus a one-line `C.md` guardrail only if ignoring it can silently break something. Rich
  evidence (benchmarks, analysis) goes to `docs/notes/`, linked from the `D.md`. Skip all this for trivial changes.
- **Single-source.** A load-bearing claim or mechanism lives in exactly ONE canonical doc (the `D.md` nearest the code);
  everywhere else points to it by path, never restates it. Copied prose rots independently. `docs/architecture.md` is a
  map: what + where + a pointer, never how.
- **Describe current state.** Git holds the history. Drop narration of previous shapes; keep the non-obvious why,
  actionable guardrails, and past pain that encodes a constraint the code must defend. Full drop/keep lists and
  code-comment carve-outs: David's user-level `describe-current-not-history` rule.
- **Evidence-anchor volatile claims.** OS/external behavior, versions, empirical findings carry
  `(verified on <version/env>, <method>, <date-or-commit>)`. Undated confident claims about drifting behavior become
  landmines.
- **Reference a doc by a bare backticked path**, never a link repeating its own target; link only for descriptive text
  or an `#anchor`.
- **A rule is a cost.** Every `❌` line is an invariant nothing enforces, paid in tokens every session. Prefer making it
  unrepresentable in a type; `invariant-density` tracks the count per subsystem, and it only goes down.
- How the doc system works and how to slim it (playbook, principles, why): `docs/doc-system.md`. Read it before any
  sweeping `C+D.md` slimming or restructuring pass.

## Where to look (router)

- **Editing code**: for "where does symbol X live", use `codegraph_search` (enabled and up to date). The harness
  autoloads `C.md`s when you touch a dir. Read a subsystem's `C.md` proactively when running its tooling/tests without
  touching it (like `test/e2e-playwright/CLAUDE.md` before the E2E suite).
- **Before planning**, read `docs/architecture.md`: the subsystem map (what + where + a pointer to each area's docs).
- **A procedure** (release, screenshots, deps, adding a window or icon): `docs/guides/` and the skills. Building a
  dialog, settings screen, window, or form control: `docs/guides/building-ui.md` (house primitives and where each deeper
  doc lives).
- **Debugging a running app / reading logs**: [This](docs/tooling/logging.md) is the first stop, not `Console.app` or
  grepping code. All (FE & BE) log paths, format, and `RUST_LOG` recipes. RAM per line: `CMDR_LOG_RAM_USE=1`.
- **A report from a USER** (`ERR-XXXXX`, a crash, in-app feedback): `docs/tooling/feedback-and-error-digest.md`. The
  logging doc above is the LOCAL app and won't find one.
- **Branding / marketing**: `brand/CLAUDE.md`, `apps/website/`, and `README.md`. You don't need app internals.
- **Writing, code, or UI-copy style**: `docs/style-guide.md` (read before writing user-facing strings or non-trivial
  code). Product and UX values: `docs/design-principles.md`.
- **Translating the app / adding a language**: `docs/guides/i18n-translation.md` (the translator process, per-language
  style guides under `docs/i18n/`, and the local reference pile).

## File structure

- `apps/desktop/`: `src/` (Svelte frontend), `src-tauri/` (Rust backend), `test/` (Vitest, Playwright, Linux Docker E2E,
  SMB fixtures), `scripts/`. The other three apps are listed above.
- `crates/`: `cmdr-fs` (filesystem vocabulary + host primitives), `cmdr-index` (file index, media index, folder
  importance), `cmdr-archive` (the zip/tar/7z backend, and the model a new backend crate copies), `cmdr-smb` (the SMB
  backend and its protocol layer), and `cmdr-adb` (Android over ADB) carry no `tauri`, enforced by
  `index-crate-isolation`; plus two dev CLIs and a vendored `fsevent-stream` fork. Details: `docs/architecture.md`.
- `brand/`: tracked brand and press-kit assets.
- `docs/`: `docs/architecture.md` (the map), `docs/guides/` (how-tos), `tooling/` (service and workflow references),
  `docs/specs/index.md` (per-development plans, periodically wiped), `docs/notes/README.md` (benchmarks and analysis),
  `style-guide.md`, `design-principles.md`, `security.md`, `maintenance.md`.
- `tools/`: dev tooling outside every workspace and check: `tools/intellij-plugin/`, `tools/privatesize-poc/README.md`.
- `scripts/check/`: the Go check runner. `.github/workflows/`: CI.

## Dependencies

- ❌ Never add a dep without checking `cargo deny check` and verifying latest version in npm / crates.io / GitHub. Don't
  trust training data. Renovate bot handles routine updates.
- After bumping npm deps, run `pnpm dedupe`. Without it, nested transitive deps stay pinned to old versions and cause
  false-positive failures (stylelint/postcss misparsing Svelte inline styles, Playwright version skew between AxeBuilder
  and the e2e specs).

## Checker script

Always use **`pnpm check`** at the repo root (never raw `cargo` / `vitest` / etc.); it's cache-aware. Cadence: `--fast`
while iterating, plain `pnpm check` per milestone, `--include-slow` after roughly every second milestone (7-10 min: a
periodic gate, never a per-wrap ritual). Passing checks collapse to one line by default; `-v` prints a line per check.
You can also scope by name (`pnpm check clippy`), tech (`rust` / `svelte` / `go`), or app (`desktop` / `website` / ...).
Full docs in `scripts/check/CLAUDE.md`. **Finish every unit of work by running the right checks.** Don't even try to
tail the checker script.

## Testing

Before adding or changing tests, read `docs/testing.md` (the playbook) and `docs/tooling/testing.md` (the tools
inventory). Desktop-specific test, MCP, and E2E mechanics live in `apps/desktop/CLAUDE.md`.

## Hard rules

- Project hard rules are focused, autoloaded files in `.claude/rules` (non-Claude agents: read them manually!).
- Don't ignore linter warnings. Fix them, or justify the exception with a comment. Don't leave them unaddressed.
- ❌ **Never classify errors, state, or control flow by string-matching** a message, stderr, or title: wording is for
  users and breaks silently on copy edits, OS localization, or upstream reformatting. Use a typed enum variant, an
  errno, or an explicit flag across IPC; in tests too (`matches!(err, VolumeError::AlreadyExists(_))`, never
  `err.message.contains(...)`). Enforced by `error-string-match` (Rust) and `cmdr/no-error-string-match` (TS); opt out
  only when unavoidable, with the documented `allowed-error-string-match` / `eslint-disable` comment plus `LC_ALL=C` and
  a snapshot test.
- ❌ **No callback type with 2+ confusable positional params** (same type, or a bare generic): an implementation may
  declare fewer params than its type, silently binding the wrong slot. Use one object payload; opt out per-line with a
  reason. Enforced by `cmdr/no-confusable-callback-params`.
- Tool versions are mise-managed (`.mise.toml`; if `go` / `node` isn't found, check that `~/.local/share/mise/shims` is
  on `$PATH`),
- Icons come from `unplugin-icons` + `@iconify-json/lucide` (see `docs/guides/icons.md`).

## Workflow

- Desktop worktree setup (target clone, CodeGraph, data-dir cleanup) is in `apps/desktop/CLAUDE.md`. For
  parallel-subagent efforts, see `docs/guides/multi-agent-refactors.md`.
- Before doing **legwork**, read the [guide](docs/guides/agent-legwork-guide.md).
- **TDD where reasonable** (red → green); cover code with tests until confident, not beyond.
- Step back per milestone: is it solid AND elegant AND documented?
- Commits: We don't use PRs. Changes land on `main` via FF merge from a worktree branch (no squashing), so the commit
  msgs is where a change gets explained. Make it good. Use conventional commmits style. Lead with impact: the title says
  what the change achieves and why, feeds our impact-focused release notes. No hard cap for titles. Comprehensive bodies
  are okay. Use no wraps! Enclose entities in backticks. Never use `Co-Authored-By`.
- **The delivery pipeline is fully wired; don't re-audit it.** Releases are agent-automated end to end (tag → CI
  build/sign/notarize → publish `latest.json` → website deploy → FDA-preserving silent update), and feedback loops are
  live (crash → email cron, error → Discord, anonymous analytics → PostHog). See `docs/guides/releasing.md`.

Happy coding! 🦀✨
