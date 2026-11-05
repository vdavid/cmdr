# CI gate review: can a change reach `main` on a false green? (2026-11-05)

Adversarial read-only review of branch `worktree-agent-aecfe67ff0c65fc36` (PR #968, head `159d5d9151`) for one property:
no change reaches `main` with a green `CI OK` unless every lane its inputs need actually passed, for those exact inputs.
Line numbers are on that branch. Method: reading the diff, the lane files, `scripts/lane-markers/`,
`scripts/check/checks/rust-test-scope.go`, `scripts/land/`, and the rulesets; nothing was run.

## Findings

### F1. Heavy-lane filters miss files their own checks read, and markers turn each gap into a follow-up-push false green (High, Confirmed)

- **Where**: the `rust`, `svelte`, and `desktop` filters in `.github/workflows/ci.yml:281–399`; the hash is "files the
  lane's filters match" plus definition and `alwaysInputs` (`scripts/lane-markers/hash.go:44–56`). Nothing checks a
  heavy lane's filter against the fingerprint `Inputs` of the checks it runs: `ci-coverage` rule 7
  (`scripts/check/checks/ci-lane-files.go:25`) covers only the light lane's gated checks.
- **Live gaps** (each file is read by a check or tool the lane runs, and matches none of the lane's filters or
  `alwaysInputs`):
  - `desktop-rust` and `desktop-rust-macos`: `clippy.toml` and `rustfmt.toml` (repo root; clippy and rustfmt read them),
    `deny.toml`, `pnpm-lock.yaml`, `THIRD-PARTY-NOTICES.md`, and
    `apps/desktop/src/lib/licensing/third-party-packages.gen.json` (all `desktop-third-party-notices` `Inputs`,
    `registry.go:1847`), `fuzz/Cargo.lock` (`desktop-rust-fuzz-lock-in-step`, `registry.go:408`).
  - `desktop-svelte`: `apps/desktop/knip.json`, `apps/desktop/.stylelintrc.mjs`, and `apps/desktop/test/**` beyond
    `e2e-shared`/`e2e-playwright` (for example `test/e2e-linux/`, which `desktop-svelte-e2e-linux-typecheck` reads). All
    are in `svelteInputs` (`scripts/check/checks/inputs.go:310–330`), whose comment says it "mirrors ci.yml's `svelte`
    filter"; it doesn't. Also root `package.json` and `pnpm-workspace.yaml` (`pnpm install --frozen-lockfile`).
  - `desktop-e2e-linux`: `pnpm-lock.yaml`. `apps/desktop/scripts/e2e-linux.sh:415` installs from it, and
    `desktopAppInputs` (`inputs.go:335–345`, "Mirrors ci.yml's `desktop` filter") lists it, but the `desktop` filter
    doesn't.
- **Scenario**: push 1 of a PR edits a crate; `desktop-rust` runs green and saves `lane-green-desktop-rust-<H>` in the
  PR's scope. Push 2 adds a `disallowed-methods` entry to `clippy.toml` that the code violates (or bumps an npm dep
  without regenerating the notices). The Rust hash is still `H`, so the plan verifies, the lane skips, and `CI OK`'s
  verify passes (filter on, plan verified, key recomputed equal, marker listed). Before this branch the `rust` filter
  was on for the whole PR delta, so push 2 reran the lane and went red.
- **Fix**: make the hash cover what the lane's checks fingerprint, structurally. Either (a) `lane-markers` asks the
  check runner for the union of `Inputs`, `GlobalInputs`, and runner files of every `--check` a lane file invokes and
  adds them to the lane's inputs, or (b) extend rule 7 to every lane: each filter must cover the fingerprint of every
  check its lane file runs (with a reasoned narrowing list, like `ciFilterNarrowedInputs`). (a) also closes the
  pre-existing path-filter skip for the same files. Either way, add the missing paths above now. Tools a lane runs
  outside the check runner (`pnpm install`, `e2e-linux.sh`) need an explicit list in the same place.

### F2. The Rust test scope trusts `main`'s tree, but `main`'s merged trees are never tested, and scoped PR runs no longer re-prove them (Medium, Confirmed by design)

- **Where**: `.github/actions/rust-test-scope/action.yml` measures against `HEAD^1` (the base tip); the `verified` step
  (`ci.yml:123–186`) skips every lane on a `main` push that GitHub merged from a green PR head; `main-ci-required.json`
  has `strict_required_status_checks_policy: false`.
- **Scenario**: PR A changes `cmdr-fs` (tested with its dependents against `main` M0). PR B changes `cmdr-listing`,
  which depends on `cmdr-fs`, also against M0. Both go green. A lands; B lands through `pnpm land`'s GitHub merge into
  the moved `main`, so M2 = A + B, a tree no run tested, and its push is `verified`. PR C then changes only `cmdr-git`:
  its scope is `cmdr-git` and its dependents, so `cmdr-listing`'s tests against the new `cmdr-fs` never run, and C's
  `CI OK` is green while the Rust lane would fail on C's tree. `ci.yml:70–72` says such a clash "surfaces on the next
  PR's run"; with scoping it surfaces only at the release gate's `run_all`.
- **Fix** (recommended first): stop verifying a `main` push whose tree differs from the tree the PR's run tested. The
  cleanest form uses the markers themselves: on a `main` push, trust the landed PR's own scope (`refs/pull/<n>/merge`,
  same-repo head) by hash, so a lane whose inputs equal what the PR tested skips and any other lane runs (unscoped,
  since it's a push). Alternative: measure the scope from the newest commit carrying a `main`-scope
  `lane-green-desktop-rust-*` marker, not `HEAD^1`.

### F3. Bot PRs are trusted for markers though the runner allowlist distrusts them (Medium, Plausible)

- **Where**: `trustedRefs` (`scripts/lane-markers/markers.go:29–38`) trusts the PR's own scope for any same-repo head;
  the light-runner step (`ci.yml:228–244`) excludes `user.type == 'Bot'` because those PRs bring third-party code a test
  run executes.
- **Scenario**: a Renovate PR bumps an npm package whose install script runs in the Svelte lane. Any process in a job
  can reach the Actions runtime token (the Cacheract technique) and save `lane-green-desktop-rust-<hash of this tree>`
  in `refs/pull/<n>/merge`. A re-run, or a later Renovate rebase that leaves the Rust inputs equal, then skips the Rust
  lane, and verify accepts it. A forged marker needs only the current tree, which the job has checked out.
- **Fix**: pass the PR author type to `plan` and `verify` and drop the PR's own scope for bot authors, matching the
  runner rule (they keep `main`'s markers). More generally, any code a lane executes can mint markers for its own PR;
  that's acceptable only while every author in that scope is trusted.

### F4. The self-hosted runner's persistent state reaches more code than the allowlist intends (Medium, Plausible)

- **Where**: the allowlist (`ci.yml:228–244`) trusts a same-repo PR with a human author. `docs/guides/handling-prs.md`
  step 1 cherry-picks an outside contributor's commit onto a branch David pushes, so `pnpm land` opens a same-repo,
  human-authored PR, and the contributor's Go tests, scripts, and dashboard build run on `cmdr-light`. A human PR that
  adds an npm dependency does the same with third-party code. The box keeps mise's tools, pnpm's store, and the Go
  module cache, which `GOFLAGS=-modcacherw` (`light-lane.yml:64`) makes writable.
- **Scenario**: one job overwrites a cached module file or a mise-installed binary (by accident or not); every later
  light lane on that box, `main` pushes included, runs against that state, which no tree hash or filter sees. That's a
  false green (or red) unrelated to the inputs.
- **Fix**: an ephemeral runner (`--ephemeral` with a fresh workspace and read-only seeded caches per job), or
  `go mod verify` plus pnpm store integrity checks before the checks run; and route `pnpm land -record` PRs (or any PR
  carrying a commit whose author isn't a collaborator) to `ubuntu-latest`.

### F5. A check that skips for an environment reason still lets its lane save a seven-day marker (Low, Plausible)

- **Where**: checks return `Skipped` on environment trouble, for example `dockerUnavailable`
  (`desktop-rust-linux-container.go:327–333`), "Docker not installed", "dist/ not found". The lane passes, and now its
  marker vouches for those inputs on every later push of the PR.
- **Fix**: under `--ci`, fail on `Skipped` unless the reason is on a short allowlist of deterministic skips (the scope's
  `none`, "macOS only" on Linux), or have the check runner write a "no environment skips" flag the marker step requires.

### F6. `pnpm land` can take an older run's re-run green over a newer red run (Low, Plausible, pre-existing)

- **Where**: `ciVerdict` (`scripts/land/state.go:334–354`) takes the newest run by `created_at` for status, but the
  highest `CI OK` check ID across every run on the SHA. The branch doesn't change `scripts/land` code (only `DETAILS.md`
  wording), so this predates it.
- **Scenario**: a reopened PR gets a new run R1 on a newer merge tree, which goes red. Someone re-runs the older run R0
  (the older merge tree); its new `CI OK` has the highest ID and is green, so `land` reports green, and the non-strict
  ruleset accepts the same newest check.
- **Fix**: read `CI OK`'s conclusion from the newest run's own jobs (`runs/{id}/jobs?filter=latest`), not from the
  commit's check list.

### F7. The Rust scope's out-of-crate scan has blind spots (Low, Plausible)

- **Where**: `TestEveryOutOfCrateReadIsClassified` (`rust-test-scope_test.go:576`) sees plain string literals with two
  or more segments or a `..`; it skips literals with `{}` or spaces, raw strings, and `.join("a").join("b")` chains.
  Both reader rules that exist for tree walks (`cmdr-operations` reading `src-tauri/src`, `cmdr` walking
  `cmdr-menu/src`, the latter added in `edda2c4eab` after CI caught it) show reads that slip past it. A missed read
  means a change outside a crate never schedules the crate that reads it.
- **Fix**: keep the scan, and add a backstop that measures it: on every `run_all`, compute each landed PR's scope and
  flag any failing test outside it as a scope bug.

## Pre-existing, outside this branch's change

- The ruleset requires `CI OK` from integration 15368 (GitHub Actions), which a fork PR's run also produces under a
  workflow the fork controls. The repo's approval rule for all outside contributors is the only guard.
- The `verified` step counts any green `CI OK` on a SHA (`ci.yml:133–136`), including an older attempt that a newer one
  contradicts, or another PR's run with the same head.

## Checked and holds

- **Fork markers**: `trustedRefs` trusts the PR's scope only when the head repo equals this repo; `parseMarkerList` and
  `decide` compare the ref by equality; GitHub sets an entry's ref from the saving run. A fork can write only its own
  PR's scope.
- **Who trusts**: only `pull_request` runs (`markers.go:30`). A `main` push, a dispatch, and `run_all` ignore markers;
  every lane's `if:` starts with `inputs.run_all ||`, and verify reds any skipped lane on a full run (`verify.go:56`).
- **Saving only on success**: each lane's marker is its last step, with no `if:`; no lane step uses `continue-on-error`;
  the `if: always()` steps (`rm -rf target/doc`, `df -h`) can't turn a failed job's `success()` true; the Svelte and
  website lanes save from a `marker` job that needs every other job. Cancellation saves nothing; a partly uploaded cache
  entry isn't listed.
- **Key binding**: only a lane's own job may read its `.key` (`workflow.go:257`); the `lane-marker` action refuses a key
  without the `lane-green-` prefix; a lane missing from the plan gets an empty key and fails its last step.
- **Verify**: recomputes every hash from its own checkout of the same SHA, lists markers again, requires every lane in
  `ci-ok`'s `needs`, and `CI OK` runs on `always()` with `changes` required to be `success`. A plan that lied or a
  skipped prerequisite is red.
- **Hash coverage of definitions**: `ci.yml`, the reusable lane file, every local `uses:` target followed recursively
  (composite actions as whole directories), the filters of every job a lane `needs`, `.mise.toml`,
  `rust-toolchain.toml`, and `scripts/check/**`; remote actions are SHA-pinned inside hashed text; the digest covers
  mode, object ID, and path, plus the lane ID and `hashVersion`. No submodules; the one symlink
  (`.interface-design/system.md`) is in no lane.
- **Glob fidelity**: pinned to picomatch 2.3.2 verdicts; unsupported syntax refuses to hash.
- **Eviction and API failures**: plan failures mean "run"; verify failures mean red. Never a skip.
- **Tool caches**: the `lane-markers` binary key covers its Go sources, module files, and the Go pin; the check tool's
  key covers its non-test sources, and `go:embed` is guarded by `TestGoCompileLanesReadOnlyGoSources`; `--ci` neither
  reads nor writes the check runner's result cache.
- **Scope wiring**: set on `pull_request` only (`rust-lane.yml:89`, `rust-fixtures-lane.yml:62`, `macos-lane.yml:68`);
  base is the merge commit's first parent with `fetch-depth: 2`; anything but a two-parent HEAD, any error, an unknown
  path, a manifest, the lockfile, the toolchain, `vendor/`, fixture servers, or a Rust lane file or action answers
  `all`; dev, build, and normal path edges close transitively; renames count both sides; an unknown crate name in the
  env fails the lane; the build stays workspace-wide.
- **Runner allowlist events**: forks, bot-authored PRs, and any trigger other than `push` (only `main`),
  `workflow_dispatch`, and same-repo `pull_request` land on `ubuntu-latest`; outside contributors' runs need approval
  (`all_external_contributors`).
- **Land re-runs**: a re-run of the same run adds a higher-ID check, so an older attempt's green can't outvote a newer
  attempt of that run.
