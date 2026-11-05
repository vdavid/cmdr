# CI

How the GitHub workflows fit together, and the invariants that keep them honest. The workflows live in
`.github/workflows/`; the checks they run live in `scripts/check/` (see `scripts/check/CLAUDE.md`).

## Workflow inventory

| Workflow                | Trigger                                 | What it does                                                                                        |
| ----------------------- | --------------------------------------- | --------------------------------------------------------------------------------------------------- |
| `ci.yml`                | PRs, pushes to main, nightly, manual    | Main suite: change detection, one `uses:` job per lane (§ Lane files), the deploy, and `CI OK`      |
| `<lane>-lane.yml`       | Called by `ci.yml`                      | One lane's steps each: `rust`, `rust-fixtures`, `macos`, `svelte`, `e2e-linux`, `website`, `light`  |
| `slow-checks.yml`       | Nightly (3 AM UTC), manual              | osv-scanner, cargo-deny/udeps, govulncheck, website Docker build, 30-min SMB soak                   |
| `advisories.yml`        | Daily (4:17 AM UTC), manual             | osv-scanner (every lockfile) and cargo-deny (no compile): new advisories surface in a day           |
| `deploy-api-server.yml` | Push to main touching `apps/api-server` | Deploys the Cloudflare Worker.                                                                      |
| `deploy-dashboard.yml`  | Push to main touching the dashboard     | Builds and deploys the analytics dashboard to Cloudflare Pages.                                     |
| `release.yml`           | `v*` tags                               | Only calls `release-pipeline.yml` (so attestations name the reusable workflow as signer).           |
| `release-pipeline.yml`  | Called by `release.yml`                 | Builds, signs, and publishes the desktop app on macOS runners. `docs/guides/releasing.md`           |
| `scorecard.yml`         | Push to main, weekly, rule edits        | OpenSSF Scorecard: publishes the score (README badge) and SARIF to code scanning. § below           |
| `codeql.yml`            | PRs, push to main, weekly, manual       | CodeQL (Actions, JS/TS, Rust); a high or critical finding fails it and blocks releases. § below     |
| `gradle-wrapper.yml`    | Push to main, wrapper PRs, weekly       | Checks the IntelliJ plugin's `gradle-wrapper.jar` against Gradle's published checksums. § Scorecard |

`slow-checks.yml` also runs the long fuzz job (20 minutes per target, findings uploaded as `fuzz-artifacts`;
`fuzz/DETAILS.md` § "Running and reproducing"), Miri, property mutants, and the pre-release gates. Kani and Shuttle run
on the landing path instead, as jobs of the Rust lane (§ Slow checks cadence). `ci.yml`'s `Full run (run_all)` job runs
only on a `run_all` dispatch: the release's CI gate matches it by name to prove a run was full, so renaming it breaks
releases (`docs/guides/releasing.md` § The CI gate).

The website deploy is a job inside `ci.yml` (gated on the website checks passing), NOT a standalone workflow. There used
to be a standalone `deploy-website.yml` on the same path filters; it deployed a second time per push and didn't wait for
checks, so it could deploy a broken site. Don't reintroduce it.

## Change detection (`ci.yml`'s `changes` job)

`dorny/paths-filter` computes one output per lane (`rust`, `rust-fixtures`, `rust-macos`, `svelte`, `desktop`,
`website`) and per light-lane section (`scripts`, `api-server`, `dashboard`, `termbase`, `changelog`, `scorecard`); each
lane gates on its output, and `workflow_dispatch` with `run_all` bypasses all gates. The three Rust lanes share their
inputs through the `rust-inputs` YAML anchor, which dorny expands.

Rules the filter block follows (comments in `ci.yml` restate these next to the filters):

- **A filter covers every path its job's checks read**, not just the app dir. The `rust` filter includes
  `apps/desktop/test/smb-servers/` because the SMB integration tests run against those containers; the `svelte` filter
  includes `apps/desktop/eslint-plugins/` and `test/e2e-shared/` because ESLint and Vitest cover them; the `desktop`
  filter (gating the Linux E2E job, which builds the whole app) includes the Rust workspace inputs. The root
  `eslint-plugins/` (custom rules every app's ESLint config imports) is in the `svelte`, `website`, `api-server`, and
  `dashboard` filters for the same reason.
- **Each lane's filter lists its own lane file and the composite actions it uses**, and no other lane's file, so an edit
  to one lane's steps reruns that lane alone (§ Lane files).
- **`.mise.toml` and `.github/workflows/ci.yml` are in every filter**: a toolchain bump or an edit to the orchestration
  can change any job's behavior, so everything reruns.
- **The check runner (`scripts/check/**`, `scripts/check.sh`) is in every filter whose lane runs checks through it**
  (the Rust lanes, `svelte`, `website`, `api-server`, `dashboard`, and `scripts`), minus its `_test.go` files, which
  change no lane's behavior and run in the light lane's Go section. So a runner edit reruns those lanes; lane markers
  hash the runner too.
- **`pnpm-lock.yaml` is in every Node-based filter** so lockfile-only bumps (`pnpm dedupe`, transitive updates) still
  run the affected apps' checks.
- **A filter that gates one light-lane check carries that check's whole fingerprint** (`termbase`, `changelog`,
  `scorecard`): its `Inputs`, `GlobalInputs`, and the runner files its `Run` reaches. `scorecard` is the one deliberate
  narrowing (§ The light lane).
- **`rust`, `svelte`, `desktop`, and `scripts` skip `CLAUDE.md` and `DETAILS.md`**
  (`predicate-quantifier: some-with-excludes` plus `!` patterns), so an agent-docs edit no longer runs the half-hour
  macOS lane. Their jobs never read those files; every docs check, and `analytics-event-catalog` (the one check that
  reads a `DETAILS.md`), runs in the light lane on every change. Other `.md` files stay in: the agent prompts and
  `CHANGELOG.md` compile into the app.

The `ci-coverage` check (below) validates that every concrete path in the filter block exists, so a renamed config file
can't silently rot a filter again (this happened: the filter watched `vite.config.ts` long after the file became
`vite.config.js`), and enforces the lane-file and gated-check rules.

### Lane markers (skip a lane whose exact inputs already passed)

A filter compares the PR's whole delta against `main`, so a docs fix pushed onto a green Rust branch reruns every Rust
lane. `scripts/lane-markers/` hashes each lane's exact inputs (its filters' matches at the commit, its workflow and
local actions, the toolchain pins, the check runner); a lane that passes saves a `lane-green-<lane>-<hash>` Actions
cache entry, a later PR run that finds it in a trusted scope skips the lane, and `CI OK` re-proves every such skip.
Pushes, dispatches, and `run_all` ignore markers. A light run doesn't run the heavy lanes, so it saves no marker for
them; the land run does, and a later land run on the same inputs skips them (§ Light runs and land runs). Trust
argument, store choice, the one risk (a filter missing an input) and its guards, and the per-lane wiring:
`scripts/lane-markers/DETAILS.md`.

The six path-filtered lanes are wired (`desktop-rust`, `desktop-rust-fixtures`, `desktop-rust-macos`, `desktop-svelte`,
`desktop-e2e-linux`, `website`); the light lane isn't, since it gates on no filter. In `ci.yml`: `changes` plans
(`lanes` output), each lane's `if:` adds `!fromJSON(needs.changes.outputs.lanes)['<id>'].verified` and passes its `.key`
as `marker-key`, `e2e-base-image` follows the E2E lane's verdict, and `CI OK` runs `lane-markers verify` after its jq
gate. In each lane file, the `lane-marker` action saves the key: as the last step of a one-job lane, or in a `marker`
job that `needs` every other job of a multi-job lane (Rust, Svelte, website), so one green job can't vouch for another.

A PR run hashes `refs/pull/N/merge`, the PR merged with the current `main`. So a follow-up push skips a lane only while
`main` hasn't moved any of that lane's inputs since the run that passed; a land that touched them makes a new tree and
the lane runs again, as it should (PR #968, 2026-11-05: a docs push that also caught the branch up with `main` reran
every lane, since `main` had moved crates, the app, and `trust-mdm.ts`).

### Rust test scope in the Rust lanes

On a pull request, `desktop-rust`, `desktop-rust-fixtures`, and `desktop-rust-macos` run the `rust-test-scope` action
after building the check tool: it measures the change against the merge commit's first parent (the checkout fetches two
commits) and exports `CMDR_RUST_TEST_SCOPE`, so the suite runs only the crates the change can affect plus their
dependents (a `cmdr-update-trust` edit ran 5,918 of 13,932 tests, the Linux test step 259 s → 127 s; the fixtures lane
barely moved, since `cmdr` is in nearly every scope and its build dominates; PR #969, 2026-11-05). A push, a dispatch,
and `run_all` never set it and test everything. Every uncertainty widens to `all`. How the scope is computed and why
it's safe: `scripts/check/checks/DETAILS.md` § "The Rust test scope". Its tests keep the Rust lane filters and the
scope's rule table in step (every path the scope says a test reads starts a Rust lane).

## The registry ↔ CI contract (`ci-coverage`)

CI runs checks by explicit name (`--check <id>`), which means two failure modes are silent: a check added to the
registry but never wired into a workflow simply never runs in CI, and a check renamed in the registry breaks the
workflow that still invokes the old name — but only when that workflow next runs (for `slow-checks.yml`, up to a day
later). Both happened before this guard existed: a dozen checks ran nowhere, and the `eslint-typecheck` split left
`slow-checks.yml` invoking a name the tool rejects.

Some checks legitimately can't run in CI; they carry a `NotInCI` reason instead of a workflow step. The clearest example
is `bindings-fresh`: the committed `bindings.ts` is the macOS command surface (Cmdr ships macOS-only), and
platform-gated `#[tauri::command]`s mean a Linux runner regenerates a different surface, so the check would always
report "stale" there. It stays a local macOS pre-commit check. The Playwright E2E suite is similar (needs a macOS window
server).

`ci-coverage` (in the light lane and the `--fast` lane) enforces:

1. Every `--check <name>` in any workflow resolves to a registry ID or nickname. Only lines that mention `scripts/check`
   count, so a multi-check step keeps its comma list on the invoking line.
2. Every registry check is referenced by some workflow, or carries a `NotInCI` reason on its `CheckDefinition` (see
   `scripts/check/checks/DETAILS.md` § Field semantics). A reason on a check that IS referenced fails too, so excuses
   can't go stale.
3. Every concrete path in `ci.yml`'s filter block exists (glob entries are checked via their static directory prefix).
4. Every static path prefix in a registry check's `Inputs` (and in `GlobalInputs`, which now names the runner core file
   by file) exists. A dead `Inputs` glob would silently make the input-fingerprint cache skip a check whose real
   (renamed) sources changed. Same robust file-existence shape as rule 3; it deliberately does NOT reconcile `Inputs`
   against the per-app filter sets (the filter ↔ app ↔ check mapping isn't 1:1, so a strict reconciliation would be
   flaky). The cache's real correctness backstop is that `--ci` always runs fresh. See the input-fingerprint cache
   section in `scripts/check/CLAUDE.md`.
5. The `CI OK` gate sees every job and always reports (§ Branch protection).
6. Every filter a lane's caller reads (in its `if:` or `with:`) matches the lane file, every local composite action it
   uses (followed into nested actions), `ci.yml`, and `.mise.toml`; no filter names another lane's file outright; and
   every `*-lane.yml` has a caller (`scripts/check/checks/ci-lane-files.go`). Before it, `macos-lane.yml` sat only in
   the `finder-extension` filter (which gates one step), and no filter watched `.github/actions/`, so an edit to the
   macOS lane's steps or to a composite action ran nothing that tested it.
7. Each filter in `ciFilterGatedChecks` covers its check's whole fingerprint, so gating a check that used to run on
   every change never skips a change it reads. That's the light lane's sections (`scripts`, `api-server`, `dashboard`)
   and its three slow hygiene checks. Deliberate narrowings sit in `ciFilterNarrowedInputs` (one pattern) and
   `ciFilterNarrowedChecks` (`scripts-go-tests`, whose tests read the whole tree) with a reason.
8. Every path-filtered lane's filters (all that its caller reads, which is exactly what its lane marker hashes) cover
   the fingerprint of every `--check` its lane file runs, plus what the lane runs outside the runner (`pnpm install`,
   `e2e-linux.sh`: `laneExtraInputs`), in `scripts/check/checks/ci-lane-check-inputs.go`. Without it a lane skipped on a
   change to a file only its checks read, and a lane marker vouched for a tree whose `clippy.toml`, `deny.toml`, or
   `knip.json` it never saw (the CI gate review of 2026-11-05, F1). Narrowings would go in `laneCheckNarrowedInputs`,
   which is empty: each one is a place a marker vouches for a tree it didn't see.

Practical consequence: **when you add a check, CI fails until you either add a workflow step for it or set `NotInCI`
with a reason.** When you rename one, CI fails until every workflow catches up. That's the point.

## Lane files

Each lane's steps live in a reusable workflow of its own (`rust-lane.yml`, `rust-fixtures-lane.yml`, `macos-lane.yml`,
`svelte-lane.yml`, `e2e-linux-lane.yml`, `website-lane.yml`, `light-lane.yml`), and `ci.yml` keeps the triggers,
`changes`, one `uses:` job per lane, the base-image, warm, prune, publish, and deploy jobs, and `CI OK`. About a third
of PR runs touched `ci.yml` (40 runs to 2026-11-04), and with every lane's steps in it, each such edit ran every lane:
~2 runner-hours, the macOS lane's 34 min among them. Now an edit to one lane file reruns that lane, and editing `ci.yml`
itself still reruns all.

- **The calling job keeps the lane's ID and `name:`**, so `CI OK`'s `needs`, the release gate, and `pnpm land`'s
  early-red matching (which reads a reusable workflow's jobs as `<caller name> / <job name>`) see the same lanes. A
  lane's result is its whole file's: one red job inside makes the caller `failure`.
- **What a caller can't do**: set `timeout-minutes` or `runs-on` (the lane's own jobs carry them), or pass its `env`
  down, so each lane file that saves a Rust cache restates `SAVE_RUST_CACHE`. Permissions only narrow: the caller grants
  what the lane's jobs may ask for (`pull-requests: read` for the light lane, `packages: read` for E2E).
- **`e2e-base-image` stays in `ci.yml`**: OpenSSF Scorecard waives a job's `packages: write` only in a workflow it
  recognizes as packaging, which `ci.yml` is (its `docker push`); in `e2e-linux-lane.yml` it was a new Token-Permissions
  finding (scorecard v5.5.0, `pnpm check scorecard`, 2026-11-05).
- The website lane's `demo` job uploads the `website-demo` artifact, which `publish-website-demo` in `ci.yml` downloads:
  artifacts are per run, so a called workflow's jobs and the caller's share them.

## The light lane

`light-lane.yml`, one job on `ubuntu-latest`: everything that needs no Rust and no browser, so a narrow PR (docs, Go,
API server, dashboard, CI config) pays setup once and takes one runner. It runs on every unverified PR and push (§
Landing doesn't re-run the suite), except a PR that changes no file. Every check step runs once setup is done, even
after another failed, so a red run lists every failure, like the four separate jobs it replaced did.

**No self-hosted runner.** A capped one on David's Hetzner box ran this lane briefly and was removed (2026-11-05): a
docs-only light job took 61 s there against 63 s hosted (throwaway PRs #971–#974), and the box ran one job at a time, so
three PRs pushed together queued behind each other. Hosted runners run them in parallel.

- **Repo hygiene, on every change**: the checks whose inputs no per-app filter can cover. `oxfmt` formats the whole
  monorepo (before hygiene ran on every change, a docs-only commit ran zero CI, so unformatted markdown could land on
  `main` and fail the next unrelated PR; locally the git hooks apply all three formatters: `docs/tooling/git-hooks.md`).
  Then `changelog-security`, the docs checks, the translator-guide and unused-key scans, the cross-app guards, and the
  workflow guards (`workflows-hardening`, `workflows-rustup`, `ci-coverage`: the files they scan are no app's
  territory). Grouped a few per step: one invocation runs its checks in parallel.
- **Per filter**: the Go checks (`scripts`, one invocation, so they overlap), the API server (`api-server`), and the
  dashboard (`dashboard`).
- **nilaway and Scorecard run in `go-tools`, a second job**, gated on `scripts` (nilaway), `scorecard` (Scorecard), or a
  full run, which takes the two slowest Go tools off the light job's critical path. It restores the check tool, the Go
  caches (nilaway's binary among them), and the scorecard binary, and saves nothing. nilaway fans out two modules at a
  time (`scripts/check/checks/DETAILS.md`).
- **Two slow hygiene checks, per filter** (each filter carries the check's fingerprint, `ci-coverage` rule 7):
  `desktop-i18n-termbase` (~40 s) on `termbase`; `changelog-commit-links` on `changelog`, which alone needs history, so
  the lane fetches it (`git fetch --unshallow`) only then. `scorecard-local` on `scorecard` is the third, in `go-tools`.
- **Scorecard's trigger has two halves**, both decided in `changes`. Its file checks read workflows, Dockerfiles, the
  Renovate config, the license, security policy, and fuzz harness: the `scorecard` filter. But Binary-Artifacts and
  Pinned-Dependencies' shell half read EVERY file, by content (read in ossf/scorecard v5.5.0's source, 2026-11-05: its
  raw Binary-Artifacts check matches magic bytes or a binary extension, and its shell-download validator a shell
  extension or shebang), so on a PR the `scorecard-scan` step reads the PR's file list (`pulls/{n}/files`) and runs
  Scorecard when a file, a deleted one or a rename's old path included, has a binary or shell extension, a Dockerfile or
  `.github/` path, a shebang, or a NUL in its first 1,024 bytes. 3,000+ files (the API's cap) or a failed lookup runs
  it. A push or dispatch always runs it.

**A PR whose head tree equals its base branch's skips it** (`changes`' `same-tree` step, pull requests only). That's a
`pnpm land -record N` record (`docs/guides/handling-prs.md`): an `ours` merge that changes no file, only ancestry, so
hygiene has nothing to check. The path-filtered lanes skip too (the PR lists no files), `CI OK` passes on all-skipped
lanes, and the record lands in about a minute instead of waiting out a busy queue.

- It compares tree SHAs (the head commit's, and the base branch tip's as the step runs), which match only when every
  byte does; a file-list diff would be a weaker proxy. The live base tip, because the event's `base.sha` can trail it on
  a re-push.
- A failed lookup leaves `same-tree` unset, so hygiene runs: the fallback is never a skip.
- Verified offline only (2026-10-21, the step's script against the live API: `main`'s tip read same, PR #393's head
  differed, a missing SHA warned and ran hygiene). The first real record PR is its live test.

**Its caches** (`.github/actions/light-cache`, shared with `warm-light-cache` so keys and paths match): `node_modules`
(every workspace's, keyed on the lockfile, `pnpm-workspace.yaml`, and `.mise.toml`; a hit skips `pnpm install`), the
built check tool (keyed on its non-test Go sources and the Go version), the Go caches (module and build caches plus the
Go checks' `go install`ed tools; restored only when the Go checks run), and the scorecard binary (its ~2 min
`go install`, an entry of its own: in a shared one, a Go-only run's save, which held no scorecard, hit exactly on every
later run, so scorecard was never cached; seen on the probe PR #964, 2026-11-05). The lane only restores them;
`warm-light-cache` is their one writer, on every `main` push. `GOFLAGS=-count=1` keeps test results out of the Go build
cache, so `go test` always runs for real. Two things keep `light-go` small (it reached 1.6 GB, 4.5 GB unpacked, on
2026-11-05, measured by restoring it in a throwaway run):

- **The writer restores it without `fallback`**, so a new key's entry holds only what that commit's Go checks used. A
  prefix restore carried every older entry's modules and build cache forward into each new save.
- **Scorecard installs into Go caches of its own**, thrown away after: its `go install` pulled ~3 GB of modules and ~1.3
  GB of build cache into `light-go`, of which only the binary (in `light-scorecard`) is worth keeping.

A fresh `light-go` weighs ~0.23 GB (the Go checks run cold with empty caches, then tar+zstd like actions/cache: build
cache 92 MB, modules 106 MB, tools 35 MB; measured locally, 2026-11-05).

**The pnpm store** (`.github/actions/pnpm-store`): the Svelte lane, the website lane, the demo build, the deploys, and
`slow-checks` restore pnpm's store before `pnpm install`, at the path `pnpm store path` reports (pnpm 12 keeps it under
`$XDG_DATA_HOME`; every job used to cache `~/.pnpm-store`, which didn't exist, so they cached nothing). Exact keys only,
saved from `main` only, and only by a whole-workspace install (the Svelte lane's `checks`, the website's `site`, and
`warm-light-cache`, which fills the host entry with `pnpm fetch`); the website's `site` job runs in a container with
another `HOME`, so it has an entry of its own (`-container-` in the key). `prune-superseded-caches` keeps one per
family.

## Build caching

The Rust compile dominates CI time (a cold ~1000-crate Tauri tree is ~10-16 min). Four caches keep it down. Each has a
load-bearing companion step — **don't remove these without re-checking the cache; several of the failure modes are
silent** (a warning, not a red job):

- **`desktop-rust`** (per push): `Swatinem/rust-cache` (SHA-pinned; `workflows-hardening` requires the pin) caches
  `target/` + cargo registry/index/git deps, keyed on Cargo.lock + rustc. An unchanged-deps push recompiles only the
  `cmdr` crate (~16 → ~7 min). The entry is named by `shared-key: desktop-rust` (not derived from the job ID, which in a
  lane file is `lane`), so `desktop-rust-fixtures` restores it too, never saving, and the repo keeps one Linux Rust
  entry. Both jobs reclaim disk only on a small runner, through the `free-disk-if-small` action (`.github/actions/`): it
  removes the unused preinstalled SDKs when `/` starts under 60 GB free, and logs the free space before and after. Most
  runners have a 145 GB root disk with 86 GB free at the start, where a cold run of each peaked at ~80 GB used (sampled
  every 5 s, PR #593, 2026-10-20) and an unconditional reclaim cost ~1.5 min for nothing. But the `ubuntu-latest` fleet
  isn't uniform: a runner with a 72 GB disk (`/dev/nvme0n1p1`, image 20261101.64.1) ran `cargo test --no-run` out of
  space mid-link (run 41839837176, 2026-11-04). Both jobs end on `df -h /`.
- **The `desktop-rust` entry stays lean two ways**, measured with cold builds of the cached set (clippy, rustdoc, test
  binaries) in a throwaway run, tar+zstd after dropping workspace artifacts as rust-cache does (runs 42024754473 and
  42028774426, 2026-11-05). The live entry was 2.7 GB (12 GB unpacked); a fresh one is ~1.7 GB, and ~1.1 GB without
  dependency debuginfo:
  - **Dependencies build without debuginfo**, for everyone: `[profile.dev.package."*"] debug = false` in the root
    `Cargo.toml`, so every lane and local build use the same profile. Dependency `.debug_*` sections were ~0.9 GB
    unpacked, mostly in proc-macro and cdylib `.so`s. Workspace crates keep their line tables, so a test's backtrace
    still names our files and lines. Locally, a cold `cargo build -p cmdr` went 12 GB -> 9.9 GB of `target/` and 440 s
    -> 366 s, and a warm rebuild after touching `cmdr-listing` 43 s -> 31 s (median of three; macOS arm64, a busy
    laptop, so the times are rough; 2026-11-05).
  - **`warm-rust-cache` builds a new key from a clean `target/`.** On a miss rust-cache restores the newest older entry,
    and its pre-save cleanup keeps every artifact of a dependency it still lists, whatever version or feature set built
    it, so stale builds rode along into each new save (three copies of `tauri-plugin-mcp-bridge`'s 338 MB staticlib, for
    one). The lane itself still builds on the older entry when it runs on `main`, which a landed PR's verified push
    skips.
- **The Rust caches save from `main` only** (`desktop-rust` and `desktop-rust-macos`; `SAVE_RUST_CACHE` at the top of
  `ci.yml` and restated in each lane file that saves one, which also gates the E2E target cache below). PR runs restore
  `main`'s entry and save nothing. When every PR saved its own ~1.7 GB entry, the repo sat at ~12 GB, LRU evicted
  entries within minutes, and nearly every PR run built cold (PR #593, 2026-10-20). **Load-bearing companion: the
  `warm-rust-*` jobs.** A verified `main` push skips the Rust lanes, so nothing else would write `main`'s entries.
  Post-land on `main`, one job per entry restores it with the lane's exact rust-cache inputs; on an exact hit it skips
  every build step (~1 min), on a miss it runs the lane's compiling checks and saves. They skip when the Rust lanes ran
  on that push, and they're exempt from `ci-ok` (`ciOKGateExempt`). Their rust-cache inputs and the steps before them
  must match the lane's: rust-cache hashes the rustc version and `CARGO*`/ `RUST*` env into the key, which is also why
  the switch is named `SAVE_RUST_CACHE`.
- **Every rust-cache step follows the `pinned-rust-toolchain` action** (`.github/actions/`). rust-cache hashes the rustc
  version of EVERY installed toolchain, and the runner image preinstalls its own `stable`. While GitHub rolls out a new
  image (days, after each Rust release), runners differ, so one job wrote two keys run by run: `e3a6d410` with the old
  stable next to the pin, `770ca156` without it (2026-10-22). Each flip restored a partial match, recompiled ~20 min,
  and saved another ~1.8 GB entry. The action installs the pin and uninstalls every other toolchain, so the key depends
  on `rust-toolchain.toml` alone. A new rust-cache step needs it too. ❗ The action also exports
  `CARGO_CI_PROFILES_HASH`, a hash of the root `Cargo.toml`'s `[profile*]` tables: rust-cache hashes member manifests
  and `Cargo.lock` but never the virtual root manifest, so without it a profile edit keeps every key, each hit rebuilds
  every dependency, and an exact hit never saves the fix.
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
  2026-10-19, #557). It runs post-land on `main`, rebuilds the E2E target cache only when the current key has no entry,
  and is exempt from `ci-ok` (`ciOKGateExempt`). Its cache `path`s and key must stay identical to the lane's. It has no
  `restore-keys`, so a new key's entry starts empty: nothing prunes `target/` or the registry, so building on the older
  entry carried every stale artifact forward.
- **The E2E base image is not an Actions cache.** It lives in GitHub's container registry, tagged by the hash of
  `Dockerfile.base`: the `e2e-base-image` job builds and pushes it only when the tag is missing, and `desktop-e2e-linux`
  (which needs that job) pulls it. As a cache entry it was the LRU victim whenever the repo went over budget, and the
  test job then rebuilt it inside its own 40-min cap, which slow apt mirrors ran out (2026-10-22).
  `apps/desktop/test/e2e-linux/DETAILS.md` § "CI base image". (The E2E job has no "Free disk space" step: the runner's
  root disk is 145 GB with 107+ GB free, measured on 109 runs in October 2026, and the reclaim cost ~1.6 min a run,
  #557.)
- **The Linux apt install goes through the `tauri-linux-deps` action** (`.github/actions/`). On 2026-10-22 the plain
  install took up to 38.9 min (usually ~0.5) and ran `desktop-rust-fixtures` out of its 40-min cap. The Azure mirror
  crawled rather than stalled (~15 MB in 8 min), so a timeout plus a plain retry against it still failed. The action
  layers:
  - A cache of the ~64 MB of downloaded `.deb`s (`tauri-apt-<ImageOS>-<ImageVersion>-<hash of the action>`), so most
    runs skip the mirror. A new runner image misses once and re-saves. `warm-rust-cache` runs the action on every `main`
    push, even on a Rust cache hit, because a PR reads only its own and `main`'s caches.
  - A time-boxed download (4 min, then 8), with the second attempt dropping `azure.archive.ubuntu.com` for
    `archive.ubuntu.com`, under a 20-min step cap.
  - apt's own 30-s connection timeout and retries.
- **`slow-checks` dependency-checks**: rust-cache for cargo-udeps's nightly `target/`, plus the same "Free disk space"
  step. Kept alive by the 6-day cron (below).
- **mise's tool cache saves from `main` only** (`cache_save` on every mise-action a PR reaches). Its key holds the
  runner image (`ImageOS`) and the config hash, plus an `install_args` hash for the Go-only install in
  `lane-markers-tool`, so one run touches up to four entries of ~0.1–0.3 GB each; a container job (the website lane's)
  has no `ImageOS`, so mise files it under `self-hosted`. When PRs saved their own, PR #982 alone held 0.4 GB of mise
  (2026-11-05). `prune-superseded-caches` keeps one `main` entry per variant.

rust-cache keys per-job, so these are independent caches and each prunes itself. The shared risk is GitHub's **10 GB
per-repo ceiling**, and lane markers are the first LRU victims, since nothing ever restores one (#981). On 2026-11-05
the repo sat at ~9.5 GB (`gh api repos/vdavid/cmdr/actions/caches`): `desktop-rust-macos` 3.6, `desktop-rust` 2.7,
`light-go` 1.6, `docker-e2e` ~1.2, four mise entries ~0.9 between `main` and one PR, `light-*` `node_modules` 0.27, and
the pnpm store 0.2. The fixes above put `desktop-rust` at ~1.1, `light-go` at ~0.23, and `light-node-modules` at ~0.19,
and PRs no longer add mise. Every PR saves only its markers and the lane-markers tool (~2 MB). Before the Rust caches
went main-only, every PR also saved ~1.7 GB of Rust and ~1.2 GB of E2E target, the repo sat at ~12 GB, and LRU evicted
entries within minutes.

**Load-bearing companion: the `prune-superseded-caches` job.** A new `Cargo.lock` gives each big entry a new key, and
the old one lingers until 7 unused days pass, so two generations put the repo at ~11.5 GB (2026-10-22). Post-land on
`main`, after every job that saves one, it keeps the most recently used `main` entry per family (`v0-rust-*`,
`*-docker-e2e-*`, the light lane's `light-*`, and `pnpm-store-*`, the key minus its trailing hash parts; for
`mise-v1-*`, minus only the last one, since an `install_args` hash before it names a variant) and deletes the rest. It's
exempt from `ci-ok`. Protect the per-push `desktop-rust` cache first; the udeps cache is the one to drop under pressure.

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

## Nightly full run

`ci.yml`'s `schedule` (02:17 UTC daily) runs it in full on `main`: `changes`' `full` output is on for a schedule as for
a `run_all` dispatch, so every lane runs, lane markers and the Rust test scope are ignored, the paths filter doesn't
run, and `CI OK` reds any skipped lane. A `main` tree no single run tested (two PRs that each passed alone, then landed
as `verified` pushes; a scoped PR run that never reached a crate a landed PR broke; a scope rule that missed a read) is
caught within a day instead of at the next release's gate. Its lanes save fresh markers and Rust caches into `main`'s
scope, which seeds every PR. It doesn't deploy: the deploy jobs run on pushes and dispatches only. A red one is as
visible as a red `main` push: GitHub emails a failed scheduled run to whoever last edited the cron, as it emails a
push's failure to its pusher. A schedule rather than a dispatch from another workflow: dispatching needs
`actions: write`, which Scorecard's Token-Permissions flags.

## Slow checks cadence

Nightly (`0 3 * * *`). The repo is public, so runner minutes are free, and a red run then points at one day's commits.
On the every-six-days cadence it ran before, a new break landed before the last one's fix, and the workflow stayed red
from 2026-10-13 to 2026-11-05 with a different job failing almost every run (fuzz smoke, osv, the OpenAI smoke, a fuzz
crash, then a Kani proof that a new `OpKind` arm made stale). Two things keep it green now:

- **The quick Linux slow-tier checks run before landing**, with the Rust lane: `desktop-rust-kani` (~4 min) and
  `desktop-rust-shuttle` (~10 min) as parallel jobs in `rust-lane.yml`. Each is shorter than the lane's main job, so
  they add no wall-clock time, and the lane's `marker` job waits on all three. They carry no build cache (the 10 GB
  ceiling below). What stays nightly takes too long for the landing path (the long fuzz, ~3.5 h; property mutants, ~30
  min, a build per mutant; cargo-udeps; the real-API smokes; the macOS pre-release gates) or would cost a second macOS
  job per land run, which queues (§ macOS lane): Miri, ~4 min, whose pure-Rust `unsafe` modules rarely change.
- **A red run files an issue.** The `report` job opens one `slow-checks` issue (labels `bug`, `claude-created`) on a red
  run on `main`, comments on it while it stays open, and closes it on the next fully green run. The board's "Auto-add to
  project" workflow puts it on the backlog board; set its fields when you pick it up. A cancelled run and a branch
  dispatch leave it alone.

Gotchas:

- **A manual disable in GitHub's UI survives file edits.** If scheduled runs stop appearing, check
  `gh workflow list --all` for `disabled_manually` and re-enable with `gh workflow enable "Slow checks"`. The workflow
  was disabled this way for ~3 months once, which silently paused all security scanning.
- GitHub also auto-disables schedules after 60 days without repo activity (not a risk while development is active).
- **Cache size, not just age.** The nightly cron defeats _time_ eviction (7 days unused); it does nothing against _size_
  (LRU) eviction. With three multi-GB Rust caches in the repo (per-push `desktop-rust`, the e2e Docker cache, and this
  job's udeps one on the nightly toolchain), the total can cross GitHub's 10 GB ceiling and LRU-evict the least-used —
  which is this one. If this job starts compiling cold again despite the cron, check `gh cache list` and consider
  dropping this cache (the per-push one is far more valuable).

## macOS lane (`desktop-rust-macos`)

Every other `ci.yml` job runs on ubuntu, so code under `cfg(target_os = "macos")` used to be compiled only on whichever
Mac last ran `pnpm check`. This job runs on `macos-26` (the release builder's image, so it lints against the shipping
SDK; free for a public repo) and asks the three questions Linux can't answer for that code: `desktop-rust-clippy`,
`desktop-rust-rustdoc`, and `desktop-rust-tests`, in that order. The other platform-blind scanners stay in
`desktop-rust` alone. Gated on the `rust-macos` filter (the Rust lanes' shared inputs plus its own lane file), so it
runs in parallel with `desktop-rust`.

Its steps live in the reusable `macos-lane.yml`, called by `ci.yml`'s `desktop-rust-macos` (§ Lane files).

First, before clippy, it runs `desktop-finder-extension-compile`: both slices of the Swift Finder extension,
cross-compiled by `swiftc` on the one runner. Nothing else compiles it before a release build, which is where `Logger`
(macOS 11) broke the 10.15 x86_64 slice (`v0.58.1-pipelinetest.1`, 2026-11-03). It took 65 s on a cold runner (mostly
`swiftc` loading the macOS frameworks; 4 s on a warm Mac, 2026-11-03), so the step runs only when the `finder-extension`
filter matches (the extension, its check, or `macos-lane.yml`) or on `run_all`, which the release gate always is. The
`rust` filter carries `apps/desktop/finder-extension/**` too, so a Swift-only change starts the lane at all.

Gotcha for any second gate on a commit (a dispatched run, a merge queue): GitHub's ruleset reads only the newest run of
a workflow on a commit. A `ci.yml` dispatch's green `CI OK` was shadowed by that commit's PR run, which started three
seconds later and reported no `CI OK`, and the push was refused with "Required status check "CI OK" is expected"
(2026-10-21). Separate workflows don't shadow each other, as `codeql.yml` and `ci.yml` show.

Last, it runs `desktop-rust-module-cycles`, whose baseline is a macOS module graph (`scripts/check/checks/DETAILS.md` §
"Rust module cycles"), so it can't move to ubuntu. It first installs the cargo-modules version the check pins;
rust-cache carries `~/.cargo/bin`, so that multi-minute build runs only on a cold entry or a bump.

Why one job: a split-out tests job saved ~5 min of wall time, but the account runs at most five macOS jobs at once, and
with several PRs in flight the second macOS job per run queued for longer than that (§ Build caching, Critical path).

On `main` pushes the job joins one job-level concurrency group shared across runs (`macos-lane-main`,
`cancel-in-progress: false`): one runs, the newest waits, and a newer push replaces the waiting one. Each `main` commit
checks a superset of the one before it, and a burst of unverified lands used to queue a ~40 min macOS run each against
the account's five-job cap. PRs and dispatches (the release gate's `run_all`) get a group of their own (the run ID), so
nothing replaces them. The replaced job reads `cancelled`, which `ci-ok` counts as covered (§ Branch protection). The
group is job-level because the workflow-level one must stay per-run (§ Triggers and concurrency), and GitHub allows
`concurrency` on a job that calls a reusable workflow (docs: "Supported keywords for jobs that call a reusable
workflow", checked 2026-10-21). `warm-rust-macos-cache` stays out of the group: it runs only when the lane skipped, and
mostly hits the cache and exits.

- **Required.** It's in `ci-ok`'s `needs`, so a red macOS job fails the run, `ci-ok`, and the release's CI gate. Its
  first run (2026-10-06) failed on two tests that assumed David's Mac (a malloc zone macOS 26 doesn't register, a tight
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
  critical CodeQL alert on `main` (which catches a Rust finding from an older run). `release.sh` dispatches that run on
  the release branch; the release's CI gate (`scripts/release-finish/ci.go`) calls the script on the commit whose full
  CI run counted, with `--wait` on the laptop and without in `release-pipeline.yml`'s `ci-gate` job, behind the same
  bypass (`docs/guides/releasing.md` § The CI gate).
- **Run it locally** (the action's own bundle, `codeql-bundle-v<cli>` from `github/codeql-action` releases, with the CLI
  version in that action tag's `src/defaults.json`):
  `codeql database create db --language=rust --build-mode=none --codescanning-config=.github/codeql/codeql-config.yml`
  (with the two Rust env vars set), then `codeql database analyze db --format=sarif-latest --output=rust.sarif`, then
  `filter-sarif.sh rust.sarif out.sarif`. Timing on an M-series Mac (CLI 2.27.1, 2026-10-08): Rust 8.5 min, JS/TS 1.3
  min, Actions 10 s.

## Light runs and land runs

An agent pushes each milestone with `pnpm land -no-wait`, and most of those pushes are never landed as they are. So a PR
run is **light** until someone asks to land its head: `desktop-rust`, `desktop-rust-fixtures`, `desktop-rust-macos`,
`desktop-e2e-linux`, and `e2e-base-image` skip, and every other lane runs as usual. `pnpm land` asks by adding the PR's
`land` label; that `labeled` run, any re-run of it, and every later push while the label stays are **land runs**, where
every lane the change needs runs or skips as a verified marker (§ Lane markers), the same gate as before.

- **The gate's name is the line `main` holds.** `ci-ok`'s `name:` is `CI OK` on a land run and `CI light` on a light
  one, so a light run's green gate is a check the ruleset doesn't accept. GitHub creates a job's check run only when the
  job is queued, after its `needs` finished, with the name already evaluated, and a newer run whose gate is `CI light`
  doesn't shadow an older green `CI OK` on the same SHA. Verified on throwaway PR #976 (2026-11-05) against a temporary
  ruleset requiring `PROBE OK` on `refs/heads/probe-ruleset/**`: no gate check run existed while its `needs` job ran; a
  push of a SHA carrying only a green `PROBE light` was refused with
  `GH013 ... Required status check "PROBE OK" is expected`; a SHA with a green `PROBE OK` and a newer light re-run was
  accepted. The real gate on PRs #977/#978: `docs/notes/ci-light-runs-2026-11-05.md`.
- **Never a skipped `CI OK`**: GitHub counts a skipped required check as passing, and a skipped job does get a check run
  (`Full run (run_all)` reads `skipped` on every PR). The gate runs on `always()`, so its name is always the evaluated
  one. A failed `changes` on a pull request names it `CI light`, red.
- **The mode is the event payload** (`changes`' `mode` step): non-PR events (pushes to `main`, dispatches, the schedule)
  are always land runs, so `main`, `run_all`, and releases are untouched; a PR is a land run when its payload carries
  the `land` label or its author is a bot (Renovate automerges on green `CI OK`). Labelling a PR needs triage access,
  which a fork's author lacks, and a re-run keeps its payload, so it keeps its mode. Asking only ever adds lanes, so no
  fork-controlled input can turn a light run into a passing one. A fork PR's run executes the fork's own workflow, which
  the approval rule for outside contributors guards (§ The light lane).
- **Any other label** also starts a `labeled` run. It checks nothing (`verified` is set), names its gate `CI light`, and
  sits in a concurrency group of its own, so it can't cancel the PR's real run.
- **`run-name` titles a light run `Light: <PR title>`** (other runs keep GitHub's default, which an empty `run-name`
  falls back to), so `pnpm land` waits only on land runs (`scripts/land/DETAILS.md` § Decision: ask for a land run). It
  mirrors the `mode` step; drift there can only make `land` wait on the wrong run, never land on one.
- `ci-coverage` rule 5 pins the gate's name expression and the `land` output to `mode`
  (`scripts/check/checks/ci-ok-gate.go`).
- **Cost of the trade**: a light run runs no clippy, Rust test, or macOS build, so a Rust slip shows at land time
  instead of at the milestone. Agents run `pnpm check clippy` and the Rust tests they touched before each commit
  (`AGENTS.md` § Checker script).

## Branch protection

`ci-ok` is the single required status check, and `main`'s ruleset names it by its check-run name, **`CI OK`**, which the
job reports on land runs only (§ Light runs and land runs; ❌ don't change the job's `name:` expression). It needs every
job in `ci.yml` except the post-land ones (`deploy-website` and the `warm-*` cache jobs), `changes` included, and passes
only when each result is `success` or `skipped` and `changes` is `success`. A lane skipped by change detection passes; a
failed `changes`, which skips every lane, is red. A lane that calls a lane file reports its whole file's result, so a
red job inside it fails the gate.

One exception: on a `main` push, a `cancelled` `desktop-rust-macos` passes when `main` has moved past the commit (the
current head differs from it and `compare/{sha}...{head}` reads `ahead`), because a newer push replaced it in the lane's
shared group (§ macOS lane). The gate logs the head that covers it. Any other event, any other cancelled job, or a
failed lookup stays red.

It must report on EVERY run, because GitHub counts a skipped required check as passing and a missing one waits forever:

- `if: always()`, never `!cancelled()`, so a cancelled run reports red instead of skipped.
- The `pull_request` trigger takes no `paths` / `paths-ignore`: a docs-only PR still runs `changes`, the light lane, and
  the gate. Lanes gate on the `changes` filter instead.

`ci-coverage` rule 5 (`scripts/check/checks/ci-ok-gate.go`) enforces all three, plus the verified-push skip (§ Landing
doesn't re-run the suite); a job that genuinely shouldn't gate goes in its `ciOKGateExempt` map with a reason.

## Triggers and concurrency

Feature branches run CI through their PR (the land script opens it on the first push), as `pull_request`; `push` covers
only `main`, for the post-land deploy; the release's full run is a dispatch on its release branch. So a branch push runs
CI once. On a PR, `dorny/paths-filter` lists the PR's files through the API (the whole branch delta against the base, no
history needed; verified in run 38431502003, 2026-10-13: "Fetching list of changed files for PR#402 from GitHub API").

`ci.yml` and `codeql.yml` share one `concurrency` shape: a PR's new push cancels its older run, while `main` pushes and
dispatches each get their own group (the run ID). A shared group with `cancel-in-progress: false` still isn't safe for
`main`: GitHub cancels a QUEUED run when a newer one joins the group, which would drop a deploy or leave a release
commit without its run. That cancel is exactly what the macOS lane wants on `main`, so it has a job-level group of its
own there (§ macOS lane). `gradle-wrapper.yml` runs on PRs only when they touch the wrapper.

### Landing doesn't re-run the suite

CI never re-tests a tree it already passed. The `changes` job's `verified` step recognizes three cases, two on a `push`
and one on a PR. A landed commit already went green on its PR, and on a `push` it's recognized two ways:

1. **Fast-forward** (`pnpm land` when `main` didn't move): the pushed SHA is the PR's head, so its own check runs
   (`checks: read`) carry a completed, successful `CI OK` from GitHub Actions.
2. **GitHub merge commit** (`pnpm land` when `main` moved, `scripts/land/DETAILS.md`; also a rebase or squash merge made
   on GitHub): the pushed SHA is new, so the step asks `repos/{repo}/commits/{sha}/pulls` (`pull-requests: read`) for a
   merged PR into `main` whose `merge_commit_sha` is exactly the pushed SHA, then requires that same green `CI OK` on
   the PR's `head.sha`. It logs the PR and head it trusted. A merge commit is its PR's `merge_commit_sha` and the push's
   `github.sha` (verified on cli/cli PR #14858's merge commit 8553724d1, `gh api .../commits/{sha}/pulls`, 2026-10-30);
   for a rebase merge it's the last replayed commit, the push's `github.sha` too (PRs #642 and #644, 2026-10-21).

Either way `verified` is set, every lane output reads `false`, and the light lane skips too. `CI OK` then reports green
on the push in about a minute, and `deploy-website` runs off the raw `website-changed` filter. Anything else (an admin
push, a PR whose head never went green, an earlier commit of a replayed series) runs in full; so does any failed lookup.
(Verified on the first real land, run 39137865939, 2026-10-18: every lane skipped.)

3. **A PR whose tested tree already passed**: a `pull_request` run tests `refs/pull/N/merge`, checked out in `changes`.
   When that merge's tree is byte-identical to the PR head's tree (the base hasn't moved past the head's parent) and the
   head already carries a green `CI OK` from GitHub Actions, the lanes skip. Since `main` only moves forward, the run
   that went green tested that same tree. The case it's for is a release's merge-back (`pnpm land -merge`, whose head is
   the release commit the release gate's `run_all` run passed): its version bump lights up every filter, and it used to
   re-run the whole suite (31 min for v0.56.0's PR #712). A new push to the PR has no `CI OK` yet, so it runs as before;
   a moved `main` makes a new tree, so it runs too. Not yet seen live: the next release's merge-back is its first real
   test.

- **Dispatches always run in full**, so the release's `run_all` run (`docs/guides/releasing.md` § The CI gate) is
  unaffected: the lookup runs on pushes and PRs only.
- **Trust**: it trusts a `CI OK` from GitHub Actions on the SHA or on the merged PR's head, which is exactly what
  ruleset B trusts when it lets the push or the merge land. Fork PRs need no special case: their head still needs our
  own green `CI OK`.
- **The accepted gap**: a merge commit onto a moved `main` makes a tree the PR's run never saw, so a semantic clash
  between two PRs that each passed alone (no textual conflict) isn't caught on `main`. The next PR's run catches it,
  because a `pull_request` run tests `refs/pull/N/merge` (the PR merged with the current `main`), and so does the
  release gate's full run. David chose this (2026-10-21): re-running the suite after every such land cost 255 macOS
  minutes for six lands in one night, more than all PR runs together, and the macOS runners are the bottleneck.
- `ci-coverage` rule 5 keeps it whole: every lane's `if:` reads `needs.changes.outputs`, and every filter-derived
  `changes` output is gated on `steps.verified` (outputs named `*-changed` are the raw filter, for deploys).
