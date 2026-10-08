# CI

How the GitHub workflows fit together, and the invariants that keep them honest. The workflows live in
`.github/workflows/`; the checks they run live in `scripts/check/` (see `scripts/check/CLAUDE.md`).

## Workflow inventory

| Workflow                | Trigger                                 | What it does                                                                                         |
| ----------------------- | --------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `ci.yml`                | PRs, pushes to main, manual             | Main suite. Change detection gates the per-app jobs; `hygiene` always runs; deploys website on green |
| `slow-checks.yml`       | Every 6 days (3 AM UTC), manual         | cargo-audit/deny/udeps, govulncheck, type-aware ESLint, website Docker build, manual 30-min SMB soak |
| `advisories.yml`        | Daily (4:17 AM UTC), manual             | cargo-audit and cargo-deny on `Cargo.lock` only (no compile): new advisories surface in a day        |
| `deploy-api-server.yml` | Push to main touching `apps/api-server` | Deploys the Cloudflare Worker.                                                                       |
| `deploy-dashboard.yml`  | Push to main touching the dashboard     | Builds and deploys the analytics dashboard to Cloudflare Pages.                                      |
| `release.yml`           | `v*` tags                               | Only calls `release-pipeline.yml` (so attestations name the reusable workflow as signer).            |
| `release-pipeline.yml`  | Called by `release.yml`                 | Builds, signs, and publishes the desktop app on macOS runners. `docs/guides/releasing.md`            |
| `scorecard.yml`         | Push to main, weekly, rule edits        | OpenSSF Scorecard: publishes the score (README badge) and SARIF to code scanning. § below            |
| `codeql.yml`            | Push to main, weekly, manual            | CodeQL (Actions, JS/TS, Rust); a high or critical finding fails it and blocks releases. § below      |
| `gradle-wrapper.yml`    | Push to main, weekly, manual            | Checks the IntelliJ plugin's `gradle-wrapper.jar` against Gradle's published checksums. § Scorecard  |

`slow-checks.yml` also runs the fuzz smoke job (120 s per target, findings uploaded as `fuzz-artifacts`;
`fuzz/CLAUDE.md`). `ci.yml`'s `Full run (run_all)` job runs only on a `run_all` dispatch: both release gates match it by
name to prove a run was full, so renaming it breaks releases (`docs/guides/releasing.md`).

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
  filter (gating the Linux E2E job, which builds the whole app) includes the Rust workspace inputs. The root
  `eslint-plugins/` (custom rules both the desktop app and the dashboard enforce) is in the `svelte` and `dashboard`
  filters for the same reason.
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
   `scripts/check/checks/DETAILS.md` § Field semantics). A reason on a check that IS referenced fails too, so excuses
   can't go stale.
3. Every concrete path in `ci.yml`'s filter block exists (glob entries are checked via their static directory prefix).
4. Every static path prefix in a registry check's `Inputs` (and in `GlobalInputs`, which now names the runner core file
   by file) exists. A dead `Inputs` glob would silently make the input-fingerprint cache skip a check whose real
   (renamed) sources changed. Same robust file-existence shape as rule 3; it deliberately does NOT reconcile `Inputs`
   against the filter sets (the filter ↔ app ↔ check mapping isn't 1:1, so a strict reconciliation would be flaky). The
   cache's real correctness backstop is that `--ci` always runs fresh. See the input-fingerprint cache section in
   `scripts/check/CLAUDE.md`.

Practical consequence: **when you add a check, CI fails until you either add a workflow step for it or set `NotInCI`
with a reason.** When you rename one, CI fails until every workflow catches up. That's the point.

## The hygiene job

Always runs (no change gate). Holds the checks whose inputs no per-app filter can cover:

- `oxfmt` formats the whole monorepo (docs, configs, workflows). Before the hygiene job, a docs-only commit ran zero CI,
  so unformatted markdown could land on main and fail the next unrelated PR's CI. Locally, the git hooks apply all three
  formatters at commit and push time: `docs/tooling/git-hooks.md`.
- `changelog-commit-links` (previously duplicated across three jobs; needs `fetch-depth: 0`).
- `workflows-hardening`, `workflows-rustup`, and `ci-coverage` — the workflow files they scan are no app's territory.

## Build caching

The Rust compile dominates CI time (a cold ~1000-crate Tauri tree is ~10-16 min). Four caches keep it down. Each has a
load-bearing companion step — **don't remove these without re-checking the cache; several of the failure modes are
silent** (a warning, not a red job):

- **`desktop-rust`** (per push): `Swatinem/rust-cache` (SHA-pinned; `workflows-hardening` requires the pin) caches
  `target/` + cargo registry/index/git deps, keyed on Cargo.lock + rustc. An unchanged-deps push recompiles only the
  `cmdr` crate (~16 → ~7 min). **Load-bearing companion: the "Free disk space" step.** A warm restore (~1.7 GB target +
  cargo) plus the SMB integration build plus Docker SMB images overflow the runner's ~14 GB free disk ("No space left on
  device" linking `libcmdr_lib.a`). The step reclaims unused preinstalled SDKs (~9 GB Android alone) first. Cold runs
  pass without it; warm runs don't — so this only shows up once the cache is populated.
- **`desktop-e2e-linux`**: caches the Docker-side cargo + `target/` via host bind mounts (`/tmp/cmdr-docker-cache/*`),
  keyed on Cargo.lock with a restore-keys fallback. Separate from `desktop-rust` because the e2e binary builds a
  different feature set (`playwright-e2e,virtual-mtp,smb-e2e`). **Load-bearing companion: the "Reclaim cache dir
  ownership" step.** The Docker build runs as root and writes the bind-mounted dirs as root; the actions/cache post-step
  runs as the runner user, so `tar` can't read them and the **save fails as a warning** — which is exactly why this
  cache never persisted for months (every e2e run did a cold ~10-min build). A `chown` back to the runner after the
  Docker run fixes it (warm e2e build ~10 min vs ~16-17 cold). If e2e ever starts building cold again, check that the
  save isn't warning "Permission denied" again.
- **`desktop-e2e-linux` base image**: the E2E Docker base image (apt packages + Node + Rust; see
  `apps/desktop/test/e2e-linux/DETAILS.md` § "Build caching") cached as a `docker save` tar, keyed on
  `hashFiles(Dockerfile.base)` with **no restore-keys** (a stale tar's image carries a different content-hash tag, so
  `e2e-linux.sh` would rebuild anyway). Saves the cold ~4-min image build per run. **Load-bearing companions: the "Free
  disk space" step and the "Export E2E base image" step.** The job briefly holds the loaded image (~3.5 GB) plus, on a
  cache miss, its tar — without the SDK reclaim that can overflow the runner's ~14 GB free disk. The export step is what
  puts the tar on disk for the cache post-step; without it the cache saves an empty dir and every run builds cold
  (silently — the cache "hits" but contains nothing, which is why the load step would fail loudly on a missing tar
  rather than soft-skip).
- **`slow-checks` dependency-checks**: rust-cache for cargo-udeps's nightly `target/`, plus the same "Free disk space"
  step. Kept alive by the 6-day cron (below).

rust-cache keys per-job, so these are independent caches and each prunes itself. The shared risk is GitHub's **10 GB
per-repo ceiling**: rust-cache 1.7 GB + e2e cargo/target 0.9 GB + the e2e base-image tar (~0.9 GB zstd-compressed; ~3.5
GB raw) + mise/registry lands the total around ~5.5 GB, still under, but if more big caches land, LRU evicts the
least-used. Protect the per-push `desktop-rust` cache first; the weekly nightly cache is the one to drop under pressure.
Watch with `gh cache list`.

**Critical path.** The per-push wall clock is the longest single job, because `desktop-e2e-linux` gates only on change
detection (not on `desktop-svelte`) and so starts at t=0 alongside everything else. E2E (~10 min warm) is that floor.
Don't re-add a `needs: desktop-svelte` to the e2e job — it only added ~4 min of latency for a compute-saving skip that
isn't worth it (see the comment on the job).

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

## macOS lane (`desktop-rust-macos`)

Every other `ci.yml` job runs on ubuntu, so code under `cfg(target_os = "macos")` used to be compiled only on whichever
Mac last ran `pnpm check`. This job runs on `macos-26` (the release builder's image, so it lints against the shipping
SDK; free for a public repo) and asks the three questions Linux can't answer for that code: `desktop-rust-clippy`,
`desktop-rust-rustdoc`, and `desktop-rust-tests`. The platform-blind scanners stay in `desktop-rust` alone. Gated on the
`rust` filter like `desktop-rust`, so it runs in parallel and adds no wall time unless it outlasts that job (~29 min
warm on 2026-10-05).

- **Required.** It's in `ci-ok`'s `needs`, so a red macOS job fails the run, `ci-ok`, and both release gates. Its first
  run (2026-10-06) failed on two tests that assumed David's Mac (a malloc zone macOS 26 doesn't register, a tight
  watchdog margin); the second was green, and it was promoted then.
- **Left out on purpose**: `macos-availability` (the committed selector list records the newest SDK it was built on, and
  an older runner SDK that knows a different selector set fails by design), `disk-images` (a candidate once the job has
  a track record), and the Docker fixture lanes (no Docker on GitHub's macOS runners).
- **Cache**: its own rust-cache entry (keys are per job and OS), saved only from `main` so a PR can't add a second
  multi-GB entry. The repo already sits at the 10 GB ceiling (11.6 GB on 2026-10-06, three ~1.9 GB `docker-e2e` entries,
  one per `Cargo.lock`), so LRU evicts the stale E2E entries first; if this job starts building cold, check
  `gh cache list`.

The CI failure history that motivated it: `docs/notes/ci-health-2026-10.md`.

## OpenSSF Scorecard

`scorecard.yml` runs `ossf/scorecard-action` and publishes to the public API that backs the README badge and
`https://scorecard.dev/viewer/?uri=github.com/vdavid/cmdr`; the SARIF lands in the repo's code-scanning tab. The API
verifies the workflow before accepting a result, so keep it minimal (the header comment lists the rules). Run it locally
with the `scorecard` release binary:
`GITHUB_AUTH_TOKEN=$(gh auth token) scorecard --repo=github.com/vdavid/cmdr --show-details` (the `gcr.io` image needs a
billed GCP project).

Where the score stands (scorecard v5.5.0 against the pushed `main`, 2026-10-07: 7.1 with Vulnerabilities erroring out;
the CodeQL, digest-pin, and OSV-config changes of 2026-10-08 should lift it to ~7.5 once pushed):

- **Held by `workflows-hardening`**: Pinned-Dependencies' GitHub Actions half, Token-Permissions (every workflow has a
  read-only top-level `permissions:`, jobs ask for write themselves), and part of Dangerous-Workflow (no
  `pull_request_target`).
- **SAST** reads check runs on merged PRs; with none, it scores on whether a CodeQL workflow exists, so `codeql.yml`
  takes it to 10.
- **Binary-Artifacts**: the one binary is `tools/intellij-plugin/gradle/wrapper/gradle-wrapper.jar`. Scorecard stops
  counting it only when a `gradle/actions/wrapper-validation` workflow has a successful run on `main`'s latest commit,
  so `gradle-wrapper.yml` runs on every push with no path filter (10 s). A push's Scorecard run could in theory finish
  first and still see 9; the next run corrects it.
- **Structurally low for a solo, no-PR project**: Branch-Protection, Code-Review, and CI-Tests (n/a).
- **Structurally capped**: License (BSL isn't OSI, 9), CII-Best-Practices (needs an OSI license), Contributors.
- **Vulnerabilities is 10 minus a raw OSV count** over every lockfile, not cargo-deny's macOS-scoped view. Per-lockfile
  `osv-scanner.toml` files (root, `convert-clip-model/`, `fuzz/`, `benchmarks/smb/`, `scripts/release-finish/`) ignore
  what can't put the shipped app at risk, each with its reason: the frozen CLIP-conversion pins, Linux-only GTK/Wayland
  crates, build- and test-only crates, dev-only npm tooling with no fix, "unmaintained" notices with no known
  vulnerability (`ttf-parser`, five `unic-*` crates, each naming the upgrade that drops it), and `rsa`, whose Marvin
  timing attack needs a decryption oracle our client-side SSH signing isn't (`deny.toml` holds the analysis). ❌ Never
  ignore a vulnerability that applies to the shipped app. That leaves 0 (10/10, verified with osv-scanner v2.6.0,
  2026-10-08). Preview with `osv-scanner scan source -r .` (Scorecard runs osv-scanner's v2 library with the same
  per-directory configs).
- **Pinned-Dependencies' container half**: every pulled image is digest-pinned, and Renovate's `docker images` group
  keeps the digests fresh (`renovate.json`). The one gap is `FROM ${BASE_IMAGE}` in the E2E `Dockerfile`, our own
  locally built base, which nothing can pin.

## CodeQL

`codeql.yml` runs CodeQL (`build-mode: none`, the default `code-scanning` suite) on GitHub Actions workflows, JS/TS, and
Rust. Rust takes ~6x the other two together, so a push runs it only when Rust sources changed; the weekly run and a
dispatch always run all three. The Rust extractor analyzes the shipped target (`aarch64-apple-darwin`) with `cfg(test)`
off, through `CODEQL_EXTRACTOR_RUST_OPTION_*` env vars.

- **What it scans**: `.github/codeql/codeql-config.yml`. Test code, fixtures, the E2E harness, and generated files stay
  out (their hard-coded passwords and `http://` URLs are the point of a test), and `rust/cleartext-logging` is excluded:
  it judges by identifier name and flagged 38 log lines, none logging a secret.
- **Blocking**: `.github/codeql/filter-sarif.sh` drops the findings `accepted-findings.json` lists (rule + file, each
  with a reason), uploads the rest, and fails the run on anything at security-severity 7.0+ or level `error`. A false
  positive in shipped code goes in that list; ❌ don't dismiss it in GitHub's UI, which the run can't see.
- **Release gate**: `scripts/release-codeql-gate.sh` needs a green CodeQL run on the released commit AND no open high or
  critical CodeQL alert on `main` (which catches a Rust finding from an older run). `release-ci-gate.sh` calls it with
  `--wait`; `release-pipeline.yml`'s `ci-gate` job calls it again, behind the same `RELEASE_SKIP_CI_GATE_TAG` bypass.
- **Run it locally** (the action's own bundle, `codeql-bundle-v<cli>` from `github/codeql-action` releases, with the CLI
  version in that action tag's `src/defaults.json`):
  `codeql database create db --language=rust --build-mode=none --codescanning-config=.github/codeql/codeql-config.yml`
  (with the two Rust env vars set), then `codeql database analyze db --format=sarif-latest --output=rust.sarif`, then
  `filter-sarif.sh rust.sarif out.sarif`. Timing on an M-series Mac (CLI 2.27.1, 2026-10-08): Rust 8.5 min, JS/TS 1.3
  min, Actions 10 s.

## Branch protection

`ci-ok` is the single required status check. It needs every first-class job and fails if any needed job failed or was
cancelled; skipped jobs (change detection said "not affected") count as OK. `desktop-rust-macos` isn't one of them yet
(see § macOS lane).
