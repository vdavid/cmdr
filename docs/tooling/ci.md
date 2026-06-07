# CI

How the GitHub workflows fit together, and the invariants that keep them honest. The workflows live in
`.github/workflows/`; the checks they run live in `scripts/check/` (see
[`scripts/check/CLAUDE.md`](../../scripts/check/CLAUDE.md)).

## Workflow inventory

| Workflow                | Trigger                                 | What it does                                                                                                                |
| ----------------------- | --------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `ci.yml`                | PRs, pushes to main, manual             | The main suite. Change-detection gates per-app jobs; a `hygiene` job always runs; deploys the website on green main pushes. |
| `slow-checks.yml`       | Weekly (Mon 3 AM UTC), manual           | cargo-audit/deny/udeps, govulncheck, type-aware ESLint, website Docker build. Plus the manual-only 30-min SMB soak.         |
| `deploy-api-server.yml` | Push to main touching `apps/api-server` | Deploys the Cloudflare Worker.                                                                                              |
| `deploy-dashboard.yml`  | Push to main touching the dashboard     | Builds and deploys the analytics dashboard to Cloudflare Pages.                                                             |
| `release.yml`           | `v*` tags                               | Builds, signs, and publishes the desktop app (self-hosted macOS runners).                                                   |

The website deploy is a job inside `ci.yml` (gated on the website checks passing), NOT a standalone workflow. There used
to be a standalone `deploy-website.yml` on the same path filters; it deployed a second time per push and didn't wait for
checks, so it could deploy a broken site. Don't reintroduce it.

## Change detection (`ci.yml`'s `changes` job)

`dorny/paths-filter` computes one output per app-ish area (`rust`, `svelte`, `desktop`, `website`, `api-server`,
`dashboard`, `scripts`); each job gates on its output, and `workflow_dispatch` with `run_all` bypasses all gates.

Rules the filter block follows (comments in `ci.yml` restate these next to the filters):

- **A filter covers every path its job's checks read**, not just the app dir. The `rust` filter includes
  `apps/desktop/test/smb-servers/` because the SMB integration tests run against those containers; the `svelte` filter
  includes `apps/desktop/eslint-plugins/` and `test/e2e-shared/` because ESLint and Vitest cover them; the `desktop`
  filter (gating the Linux E2E job, which builds the whole app) includes the Rust workspace inputs.
- **`.mise.toml` and `.github/workflows/ci.yml` are in every filter**: a toolchain bump or a CI edit can change any
  job's behavior, so everything reruns.
- **`pnpm-lock.yaml` is in every Node-based filter** so lockfile-only bumps (`pnpm dedupe`, transitive updates) still
  run the affected apps' checks.

The `ci-coverage` check (below) validates that every concrete path in the filter block exists, so a renamed config file
can't silently rot a filter again (this happened: the filter watched `vite.config.ts` long after the file became
`vite.config.js`).

## The registry ↔ CI contract (`ci-coverage`)

CI runs checks by explicit name (`--check <id>`), which means two failure modes are silent: a check added to the
registry but never wired into a workflow simply never runs in CI, and a check renamed in the registry breaks the
workflow that still invokes the old name — but only when that workflow next runs (for `slow-checks.yml`, up to a week
later). Both happened before this guard existed: a dozen checks (including `bindings-fresh`, which AGENTS.md described
as CI-enforced) ran nowhere, and the `eslint-typecheck` split left `slow-checks.yml` invoking a name the tool rejects.

`ci-coverage` (in the always-on `hygiene` job and the `--fast` lane) enforces:

1. Every `--check <name>` in any workflow resolves to a registry ID or nickname.
2. Every registry check is referenced by some workflow, or carries a `NotInCI` reason on its `CheckDefinition` (see
   [`scripts/check/checks/CLAUDE.md`](../../scripts/check/CLAUDE.md) § Field semantics). A reason on a check that IS
   referenced fails too, so excuses can't go stale.
3. Every concrete path in `ci.yml`'s filter block exists (glob entries are checked via their static directory prefix).

Practical consequence: **when you add a check, CI fails until you either add a workflow step for it or set `NotInCI`
with a reason.** When you rename one, CI fails until every workflow catches up. That's the point.

## The hygiene job

Always runs (no change gate). Holds the checks whose inputs no per-app filter can cover:

- `oxfmt` formats the whole monorepo (docs, configs, workflows). Before the hygiene job, a docs-only commit ran zero CI,
  so unformatted markdown could land on main and fail the next unrelated PR's CI.
- `changelog-commit-links` (previously duplicated across three jobs; needs `fetch-depth: 0`).
- `workflows-hardening`, `workflows-rustup`, and `ci-coverage` — the workflow files they scan are no app's territory.

## Slow checks cadence

Weekly (Mondays 3 AM UTC), deliberately reduced from nightly in June 2026. Two gotchas:

- **A manual disable in GitHub's UI survives file edits.** If scheduled runs stop appearing, check
  `gh workflow list --all` for `disabled_manually` and re-enable with `gh workflow enable "Slow checks"`. The workflow
  was disabled this way for ~3 months once, which silently paused all security scanning.
- GitHub also auto-disables schedules after 60 days without repo activity (not a risk while development is active).

## Branch protection

`ci-ok` is the single required status check. It needs every first-class job and fails if any needed job failed or was
cancelled; skipped jobs (change detection said "not affected") count as OK.
