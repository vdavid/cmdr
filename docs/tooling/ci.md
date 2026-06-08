# CI

How the GitHub workflows fit together, and the invariants that keep them honest. The workflows live in
`.github/workflows/`; the checks they run live in `scripts/check/` (see
[`scripts/check/CLAUDE.md`](../../scripts/check/CLAUDE.md)).

## Workflow inventory

| Workflow                | Trigger                                 | What it does                                                                                                                |
| ----------------------- | --------------------------------------- | --------------------------------------------------------------------------------------------------------------------------- |
| `ci.yml`                | PRs, pushes to main, manual             | The main suite. Change-detection gates per-app jobs; a `hygiene` job always runs; deploys the website on green main pushes. |
| `slow-checks.yml`       | Every 6 days (3 AM UTC), manual         | cargo-audit/deny/udeps, govulncheck, type-aware ESLint, website Docker build. Plus the manual-only 30-min SMB soak.         |
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
later). Both happened before this guard existed: a dozen checks ran nowhere, and the `eslint-typecheck` split left
`slow-checks.yml` invoking a name the tool rejects.

Some checks legitimately can't run in CI; they carry a `NotInCI` reason instead of a workflow step. The clearest example
is `bindings-fresh`: the committed `bindings.ts` is the macOS command surface (Cmdr ships macOS-only), and
platform-gated `#[tauri::command]`s mean a Linux runner regenerates a different surface, so the check would always
report "stale" there. It stays a local macOS pre-commit check. The Playwright E2E suite is similar (needs a macOS window
server).

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

## Build caching

The Rust compile dominates CI wall time. Three caches keep it down:

- **`desktop-rust`** (per push): `Swatinem/rust-cache` caches `target/` + cargo registry/index/git deps, keyed on
  Cargo.lock + rustc. Without it the full ~1000-crate Tauri tree recompiled cold every push (~16 min); with it, an
  unchanged-deps push only recompiles the `cmdr` crate. This is the highest-value cache — it runs on every push.
- **`desktop-e2e-linux`**: caches the Docker-side cargo + `target/` via host bind mounts (`/tmp/cmdr-docker-cache/*`),
  keyed on Cargo.lock with a restore-keys fallback. Separate from `desktop-rust` because the e2e binary builds a
  different feature set (`playwright-e2e,virtual-mtp,smb-e2e`).
- **`slow-checks` dependency-checks**: rust-cache for cargo-udeps's nightly `target/`. Kept alive by the 6-day cron
  (below).

rust-cache keys per-job, so these are three independent caches and each prunes itself. The shared risk is GitHub's **10
GB per-repo ceiling**: three multi-GB Rust caches can cross it, triggering LRU eviction of the least-used. Protect the
per-push `desktop-rust` cache first; if pressure shows up, the weekly nightly cache is the one to drop. Pin `rust-cache`
to a SHA with a version comment (the `workflows-hardening` check requires it for every third-party action).

## Slow checks cadence

Every 6 days (`0 3 */6 * *`), reduced from nightly → weekly → every-6-days in June 2026. The 6-day cadence is deliberate
and load-bearing, **not** an approximation of weekly: GitHub evicts an Actions cache after 7 days unused, and this job
owns a multi-GB rust-cache (cargo-udeps's nightly `target/`). A 6-day gap keeps that cache warm run to run; a weekly
schedule would let it go cold every time, defeating the cache. `*/6` on day-of-month fires on the
1st/7th/13th/19th/25th/31st, so the gap is always ≤6 days, including across month boundaries (e.g. a 30-day month's 25th
→ next 1st is 6 days). Don't "tidy" it back to a `* * 1` weekly form. Gotchas:

- **A manual disable in GitHub's UI survives file edits.** If scheduled runs stop appearing, check
  `gh workflow list --all` for `disabled_manually` and re-enable with `gh workflow enable "Slow checks"`. The workflow
  was disabled this way for ~3 months once, which silently paused all security scanning.
- GitHub also auto-disables schedules after 60 days without repo activity (not a risk while development is active).
- **Cache size, not just age.** The 6-day cron defeats _time_ eviction; it does nothing against _size_ (LRU) eviction.
  With three multi-GB Rust caches in the repo (per-push `desktop-rust`, the e2e Docker cache, and this every-6-days
  nightly one), the total can cross GitHub's 10 GB ceiling and LRU-evict the least-used — which is this one. If this job
  starts compiling cold again despite the cron, check `gh cache list` and consider dropping this cache (the per-push one
  is far more valuable).

## Branch protection

`ci-ok` is the single required status check. It needs every first-class job and fails if any needed job failed or was
cancelled; skipped jobs (change detection said "not affected") count as OK.
