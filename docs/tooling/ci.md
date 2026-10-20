# CI

How the GitHub workflows fit together, and the invariants that keep them honest. The workflows live in
`.github/workflows/`; the checks they run live in `scripts/check/` (see `scripts/check/CLAUDE.md`).

## Workflow inventory

| Workflow                | Trigger                                 | What it does                                                                                         |
| ----------------------- | --------------------------------------- | ---------------------------------------------------------------------------------------------------- |
| `ci.yml`                | PRs, pushes to main, manual             | Main suite. Change detection gates the per-app jobs; `hygiene` runs unless verified; deploys website |
| `slow-checks.yml`       | Every 6 days (3 AM UTC), manual         | osv-scanner, cargo-deny/udeps, govulncheck, type-aware ESLint, website Docker build, 30-min SMB soak |
| `advisories.yml`        | Daily (4:17 AM UTC), manual             | osv-scanner (every lockfile) and cargo-deny (no compile): new advisories surface in a day            |
| `deploy-api-server.yml` | Push to main touching `apps/api-server` | Deploys the Cloudflare Worker.                                                                       |
| `deploy-dashboard.yml`  | Push to main touching the dashboard     | Builds and deploys the analytics dashboard to Cloudflare Pages.                                      |
| `release.yml`           | `v*` tags                               | Only calls `release-pipeline.yml` (so attestations name the reusable workflow as signer).            |
| `release-pipeline.yml`  | Called by `release.yml`                 | Builds, signs, and publishes the desktop app on macOS runners. `docs/guides/releasing.md`            |
| `scorecard.yml`         | Push to main, weekly, rule edits        | OpenSSF Scorecard: publishes the score (README badge) and SARIF to code scanning. § below            |
| `codeql.yml`            | PRs, push to main, weekly, manual       | CodeQL (Actions, JS/TS, Rust); a high or critical finding fails it and blocks releases. § below      |
| `gradle-wrapper.yml`    | Push to main, wrapper PRs, weekly       | Checks the IntelliJ plugin's `gradle-wrapper.jar` against Gradle's published checksums. § Scorecard  |

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

`ci-coverage` (in the `hygiene` job and the `--fast` lane) enforces:

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
5. The `CI OK` gate sees every job and always reports (§ Branch protection).

Practical consequence: **when you add a check, CI fails until you either add a workflow step for it or set `NotInCI`
with a reason.** When you rename one, CI fails until every workflow catches up. That's the point.

## The hygiene job

Runs on every PR and unverified push (no path gate; § Landing doesn't re-run the suite). Holds the checks whose inputs
no per-app filter can cover:

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
  `cmdr` crate (~16 → ~7 min). The entry is named by `shared-key: desktop-rust` (the same key rust-cache derives from
  the job ID), so `desktop-rust-fixtures` restores it too, never saving, and the repo keeps one Linux Rust entry.
  Neither job has a "Free disk space" step: the runner's root disk is 145 GB with 86 GB free at the start, and a cold
  run of each peaked at ~80 GB used with 68 GB free (sampled every 5 s, PR #593, 2026-10-20). The step dates from
  runners with ~14 GB free, where the SMB build plus the fixture images overflowed the disk. Both jobs end on `df -h /`.
- **The Rust caches save from `main` only** (`desktop-rust` and `desktop-rust-macos`; `SAVE_RUST_CACHE` at the top of
  `ci.yml`, which also gates the E2E target cache below). PR runs restore `main`'s entry and save nothing. When every PR
  saved its own ~1.7 GB entry, the repo sat at ~12 GB, LRU evicted entries within minutes, and nearly every PR run built
  cold (PR #593, 2026-10-20). **Load-bearing companion: the `warm-rust-*` jobs.** A verified `main` push skips the Rust
  lanes, so nothing else would write `main`'s entries. Post-land on `main`, one job per entry restores it with the
  lane's exact rust-cache inputs; on an exact hit it skips every build step (~1 min), on a miss it runs the lane's
  compiling checks and saves. They skip when the Rust lanes ran on that push, and they're exempt from `ci-ok`
  (`ciOKGateExempt`). Their rust-cache inputs and the steps before them must match the lane's: rust-cache hashes the
  rustc version and `CARGO*`/ `RUST*` env into the key, which is also why the switch is named `SAVE_RUST_CACHE`.
- **`desktop-e2e-linux`**: caches the Docker-side cargo + `target/` via host bind mounts (`/tmp/cmdr-docker-cache/*`),
  keyed on Cargo.lock with a restore-keys fallback. Separate from `desktop-rust` because the e2e binary builds a
  different feature set (`playwright-e2e,virtual-mtp,smb-e2e`). Split into `actions/cache/restore` and a save step that
  runs only on `main` (`SAVE_RUST_CACHE`, like the Rust caches), so PRs restore `main`'s entry and add none.
  **Load-bearing companion: the "Reclaim cache dir ownership" step.** The Docker build runs as root and writes the
  bind-mounted dirs as root; the save step runs as the runner user, so `tar` can't read them and the **save fails as a
  warning** — which is exactly why this cache never persisted for months (every e2e run did a cold ~10-min build). A
  `chown` back to the runner after the Docker run fixes it (warm e2e build ~11 min vs ~21 cold). If e2e ever starts
  building cold again, check that the save isn't warning "Permission denied" again. **Second companion: the
  `warm-e2e-linux-cache` job.** A PR restores only its own or `main`'s caches, and a verified `main` push skips the
  lane, so without it no main-scoped entry exists and every new PR's first run builds cold (4 of 9 PR runs on
  2026-10-19, #557). It runs post-land on `main`, rebuilds either E2E cache only when the current key has no entry, and
  is exempt from `ci-ok` (`ciOKGateExempt`). Its cache `path`s and keys must stay identical to the lane's.
- **`desktop-e2e-linux` base image**: the E2E Docker base image (apt packages + Node + Rust; see
  `apps/desktop/test/e2e-linux/DETAILS.md` § "Build caching") cached as a `docker save` tar, keyed on
  `hashFiles(Dockerfile.base)` with **no restore-keys** (a stale tar's image carries a different content-hash tag, so
  `e2e-linux.sh` would rebuild anyway). Saves the cold ~4-min image build per run. **Load-bearing companion: the "Export
  E2E base image" step.** (The job has no "Free disk space" step: the runner's root disk is 145 GB with 107+ GB free,
  measured on 109 runs in October 2026, and the reclaim cost ~1.6 min a run, #557.) The export step is what puts the tar
  on disk for the cache post-step; without it the cache saves an empty dir and every run builds cold (silently — the
  cache "hits" but contains nothing, which is why the load step would fail loudly on a missing tar rather than
  soft-skip).
- **`slow-checks` dependency-checks**: rust-cache for cargo-udeps's nightly `target/`, plus the same "Free disk space"
  step. Kept alive by the 6-day cron (below).

rust-cache keys per-job, so these are independent caches and each prunes itself. The shared risk is GitHub's **10 GB
per-repo ceiling**. With the Rust and E2E target caches saved from `main` only, the steady state is about 7.2 GB
(`gh cache list`, 2026-10-20, macOS entry from its last single-job save): `desktop-rust` ~1.7, `desktop-rust-macos`
~1.4, `docker-e2e` ~1.15, the e2e base-image tar ~0.8, the `slow-checks` udeps cache ~1.7 (estimate; it was evicted when
measured), mise ~0.2 per OS, plus small scorecard and CodeQL entries. Per-PR saves are only mise (~0.17 GB each on
macOS). Before this, every PR also saved ~1.7 GB of Rust and ~1.2 GB of E2E target, the repo sat at ~12 GB, and LRU
evicted entries within minutes. Protect the per-push `desktop-rust` cache first; the udeps cache is the one to drop
under pressure.

**Critical path.** The per-push wall clock is the longest single job, because `desktop-e2e-linux` gates only on change
detection (not on `desktop-svelte`) and so starts at t=0 alongside everything else. E2E is usually that floor. Measured
over 109 runs (2026-10-12 to 2026-10-19, #557), a warm E2E job is ~24.5 min: ~1.3 setup, ~12 build (the final `cmdr`
crate alone ~10), ~8.5 tests (455, one worker). The Rust work runs as three parallel jobs: on Linux `desktop-rust`
(scanners, clippy, rustdoc, the suite) and `desktop-rust-fixtures` (the Docker fixture lanes and the Nextcloud cells),
and on macOS `desktop-rust-macos` (clippy, rustdoc, the suite, module cycles; ~20 min warm). macOS stays one job on
purpose: the account runs at most 20 jobs at once, five of them macOS, and a second macOS job per run cost more queue
time than the ~5 min it saved (2026-10-20). Sharding the E2E tests (−3 to −4 min) only pays off once the Rust jobs are
shorter than E2E too. Don't re-add a `needs: desktop-svelte` to the e2e job — it only added ~4 min of latency for a
compute-saving skip that isn't worth it (see the comment on the job).

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
`desktop-rust-rustdoc`, and `desktop-rust-tests`, in that order. The other platform-blind scanners stay in
`desktop-rust` alone. Gated on the `rust` filter like `desktop-rust`, so it runs in parallel with it.

Last, it runs `desktop-rust-module-cycles`, whose baseline is a macOS module graph (`scripts/check/checks/DETAILS.md` §
"Rust module cycles"), so it can't move to ubuntu. It first installs the cargo-modules version the check pins;
rust-cache carries `~/.cargo/bin`, so that multi-minute build runs only on a cold entry or a bump.

Why one job: a split-out tests job saved ~5 min of wall time, but the account runs at most five macOS jobs at once, and
with several PRs in flight the second macOS job per run queued for longer than that (§ Build caching, Critical path).

- **Required.** It's in `ci-ok`'s `needs`, so a red macOS job fails the run, `ci-ok`, and both release gates. Its first
  run (2026-10-06) failed on two tests that assumed David's Mac (a malloc zone macOS 26 doesn't register, a tight
  watchdog margin); the second was green, and it was promoted then.
- **Left out on purpose**: `macos-availability` (the committed selector list records the newest SDK it was built on, and
  an older runner SDK that knows a different selector set fails by design), `disk-images` (a candidate once the job has
  a track record), and the Docker fixture lanes (no Docker on GitHub's macOS runners).
- **Cache**: one rust-cache entry covering the clippy, rustdoc, and test builds plus `~/.cargo/bin`, saved only from
  `main` and kept current by `warm-rust-macos-cache`, which builds all three on a miss (§ Build caching). If the job
  starts building cold, check `gh cache list`.

The CI failure history that motivated it: `docs/notes/ci-health-2026-10.md`.

## OpenSSF Scorecard

`scorecard.yml` runs `ossf/scorecard-action` and publishes to the public API that backs the README badge and
`https://scorecard.dev/viewer/?uri=github.com/vdavid/cmdr`; the SARIF lands in the repo's code-scanning tab. The API
verifies the workflow before accepting a result, so keep it minimal (the header comment lists the rules). Run it locally
with the `scorecard` release binary:
`GITHUB_AUTH_TOKEN=$(gh auth token) scorecard --repo=github.com/vdavid/cmdr --show-details` (the `gcr.io` image needs a
billed GCP project).

Where the score stands (public API, 2026-10-13: 7.6, with Vulnerabilities at 5 and SAST at 7; the fixes for both landed
2026-10-14 and should take it to ~8.1 once pushed). Two `pnpm check` lanes keep it from sliding back, and
`workflows-hardening` holds the rest:

- **`scorecard-local`** runs Scorecard's own file-based checks (Binary-Artifacts, Dangerous-Workflow,
  Dependency-Update-Tool, Fuzzing, License, Pinned-Dependencies, SAST, Security-Policy, Token-Permissions) on the
  tracked tree, and fails on any new `Warn:` finding. Accepted findings and floors, each with its reason:
  `scripts/check/checks/scorecard-local-baseline.json`. How it works: `scripts/check/checks/DETAILS.md`.
- **`osv-scanner`** is Scorecard's Vulnerabilities check, run the same way, with a fix-age grace (below).
- **Held by `workflows-hardening`**: Pinned-Dependencies' GitHub Actions half, Token-Permissions (every workflow has a
  read-only top-level `permissions:`, jobs ask for write themselves), part of Dangerous-Workflow (no
  `pull_request_target`), and SAST's PR half (CodeQL must trigger on `pull_request`).
- **SAST** scores the merged PRs in its window by whether CodeQL checked their head commit. Renovate's PRs count as
  merged, so `codeql.yml` runs on PRs; push-only CodeQL scored 7. With no merged PRs, it scores on whether a CodeQL
  workflow exists.
- **Binary-Artifacts**: the one binary is `tools/intellij-plugin/gradle/wrapper/gradle-wrapper.jar`. Scorecard stops
  counting it only when a `gradle/actions/wrapper-validation` workflow has a successful run on `main`'s latest commit,
  so `gradle-wrapper.yml` runs on every push with no path filter (10 s). A push's Scorecard run could in theory finish
  first and still see 9; the next run corrects it. `scorecard-local` can't see that run, so its floor is 9.
- **Signed-Releases** looks at the last five releases and gives 10 only when each carries a `*.intoto.jsonl` provenance
  asset (`docs/guides/releasing.md`). Releases before v0.51.0 have none, so it climbs back to 10 on its own as new
  releases replace them.
- **Structurally low for a solo, no-PR project**: Branch-Protection and Code-Review (#380 weighs an automated reviewer,
  which wouldn't move Code-Review). CI-Tests scores Renovate's PRs, which CI checks.
- **Structurally capped**: License (BSL isn't OSI, 9), CII-Best-Practices (the "passing" badge needs an OSI license;
  registering as "in progress" would score 2), Contributors.
- **Vulnerabilities is 10 minus a raw OSV count** over every lockfile, not cargo-deny's macOS-scoped view. The
  `osv-scanner` lane runs the same sweep: a new advisory warns while its fix is under six days old, then fails, and its
  pass is trusted for a day only, so a daily `pnpm check` surfaces a new one. Per-lockfile `osv-scanner.toml` files
  (root, `convert-clip-model/`, `fuzz/`, `benchmarks/smb/`, `scripts/release-finish/`) ignore what can't put the shipped
  app at risk, each with its reason: the frozen CLIP-conversion pins, Linux-only GTK/Wayland crates, build- and
  test-only crates, dev-only npm tooling with no fix, "unmaintained" notices with no known vulnerability (`ttf-parser`,
  five `unic-*` crates, each naming the upgrade that drops it), and `rsa`, whose Marvin timing attack needs a decryption
  oracle our client-side SSH signing isn't (`deny.toml` holds the analysis). ❌ Never ignore a vulnerability that
  applies to the shipped app. Preview with `pnpm check osv`; ❌ not a bare `osv-scanner scan source -r .` in a worktree,
  whose walk silently finds nothing (the main clone ignores `.claude/worktrees/`).
- **Pinned-Dependencies' container half**: every pulled image is digest-pinned, and Renovate's `docker images` group
  keeps the digests fresh (`renovate.json`). Two accepted gaps hold it at 9: `FROM ${BASE_IMAGE}` in the E2E
  `Dockerfile` (our own locally built base, with no registry digest) and the website `Dockerfile`'s
  `npm install --global pnpm@<exact>`. Fixing only the second leaves 9 (simulated with scorecard v5.5.0, 2026-10-13),
  and the first means folding the E2E image into `Dockerfile.base` as a stage, at the risk of its ~1 s rebuild.

## CodeQL

`codeql.yml` runs CodeQL (`build-mode: none`, the default `code-scanning` suite) on GitHub Actions workflows, JS/TS, and
Rust. Rust takes ~6x the other two together (12–20 min on a runner), so a PR never runs it and a push to `main` runs it
only when Rust sources changed; the weekly run and a dispatch always run all three. A Rust finding surfaces after
landing, and the release gate's open-alert check holds the next release on it. SAST only needs CodeQL to have run on a
PR, not on every language. The Rust extractor analyzes the shipped target (`aarch64-apple-darwin`) with `cfg(test)` off,
through `CODEQL_EXTRACTOR_RUST_OPTION_*` env vars.

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

`ci-ok` is the single required status check, and `main`'s ruleset names it by its check-run name, **`CI OK`** (❌ don't
rename the job's `name:`). It needs every job in `ci.yml` except `deploy-website` (a post-land deploy), `changes`
included, and passes only when each result is `success` or `skipped` and `changes` is `success`. A lane skipped by
change detection passes; a failed `changes`, which skips every lane, is red.

It must report on EVERY run, because GitHub counts a skipped required check as passing and a missing one waits forever:

- `if: always()`, never `!cancelled()`, so a cancelled run reports red instead of skipped.
- The `pull_request` trigger takes no `paths` / `paths-ignore`: a docs-only PR still runs `changes`, `hygiene`, and the
  gate. Lanes gate on the `changes` filter instead.

`ci-coverage` rule 5 (`scripts/check/checks/ci-ok-gate.go`) enforces all three, plus the verified-push skip (§ Landing
doesn't re-run the suite); a job that genuinely shouldn't gate goes in its `ciOKGateExempt` map with a reason.

## Triggers and concurrency

Feature branches run CI through their PR (the land script opens it on the first push), as `pull_request`; `push` covers
only `main`, for the post-land deploy and the release gates. So a branch push runs CI once. On a PR,
`dorny/paths-filter` lists the PR's files through the API (the whole branch delta against the base, no history needed;
verified in run 38431502003, 2026-10-13: "Fetching list of changed files for PR#402 from GitHub API").

`ci.yml` and `codeql.yml` share one `concurrency` shape: a PR's new push cancels its older run, while `main` pushes and
dispatches each get their own group (the run ID). A shared group with `cancel-in-progress: false` still isn't safe for
`main`: GitHub cancels a QUEUED run when a newer one joins the group, which would drop a deploy or leave a release
commit without its run. `gradle-wrapper.yml` runs on PRs only when they touch the wrapper.

### Landing doesn't re-run the suite

`pnpm land` fast-forwards `main`, so the landed SHA is the one whose PR run already went green. On a `push`, `changes`
looks up that SHA's check runs (`checks: read`); a completed, successful `CI OK` from GitHub Actions sets its `verified`
output, every lane output reads `false`, and `hygiene` skips too. `CI OK` then reports green on the push in about a
minute, and `deploy-website` runs off the raw `website-changed` filter. A commit that reaches `main` any other way (a
Renovate merge GitHub rebases into a new SHA, an admin push) has no green `CI OK` and runs in full; so does a failed
lookup. (Verified on the first real land, run 39137865939, 2026-10-18: every lane skipped.)

- **Dispatches always run in full**, so the release gate's `run_all` run (`scripts/release-ci-gate.sh`) is unaffected:
  the lookup is push-only.
- **Trust**: it trusts any `CI OK` from GitHub Actions on the SHA, which is exactly what ruleset B trusts when it lets
  the push land.
- `ci-coverage` rule 5 keeps it whole: every lane's `if:` reads `needs.changes.outputs`, and every filter-derived
  `changes` output is gated on `steps.verified` (outputs named `*-changed` are the raw filter, for deploys).
