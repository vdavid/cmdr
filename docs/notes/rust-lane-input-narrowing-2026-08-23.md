# Rust lane input narrowing (2026-08-23)

What replacing the single `rustInputs` set with per-lane, per-member input blocks bought, and why the cargo lanes are
still on `--workspace` afterwards. Kept as the regression anchor for `scripts/check/checks/inputs.go`: if a future
change re-merges the blocks or reaches for per-package `-p` lanes, these are the numbers it argues against.

## Method

macOS 15, M-series, rustc 1.97.1, warm 96 GB `target/`, worktree `.claude/worktrees/sftp-follow-ups` at `5524a066f`.
Cargo rows are one command each, timed end to end, `Compiling <crate>` lines counted from output, no source changed
between rows. Check rows are `pnpm check` (default lane) with a one-line comment appended to one file, the cache settled
with two no-edit runs between cases; the lanes that ran come from `~/cmdr-check-log.csv` rather than from the terminal.
Commit statistics are the 3,704 commits of 2026-06-01..2026-08-23, 1,889 of which touch a Rust input.

`desktop-rust-integration-tests` failed in every check row for an unrelated reason (a sibling worktree held the SFTP
fixture lease with a different config, so the runner adopted a foreign stack). It is a constant ~12 s in every row.

## What each edit invalidates, before and after

Lanes whose fingerprint changes, counted across the whole registry:

| Edit                                     | Before | After | What stopped re-running                                     |
| ---------------------------------------- | -----: | ----: | ----------------------------------------------------------- |
| `crates/cmdr-sftp/src/…`                 |     46 |    39 | the six app-tree-only scanners, `cargo-audit`, `cargo-deny` |
| `crates/cmdr-fs/src/…`                   |     46 |    39 | same                                                        |
| `crates/index-query/src/…`               |     46 |    35 | same, plus the four `KindApp`-only scanners                 |
| `apps/desktop/src-tauri/src/…`           |     45 |    43 | `cargo-audit`, `cargo-deny`                                 |
| `tools/…`                                |     44 |    12 | every Rust lane and both E2E lanes                          |
| `pnpm-lock.yaml`                         |     86 |    58 | all 28 Rust lanes but `bindings-fresh`                      |
| `apps/desktop/test/{smb,sftp}-servers/…` |     66 |    38 | all Rust lanes but `rust-integration-tests`                 |
| `CHANGELOG.md`                           |     50 |    52 | nothing; **two lanes were ADDED** (see below)               |

## Wall clock, after

| Case                                              |  Wall | Lanes that ran |
| ------------------------------------------------- | ----: | -------------: |
| `pnpm check --fresh` (cold cache, warm `target/`) | 4m49s |            109 |
| No edit, settled cache                            |   11s |              1 |
| One file in `apps/desktop/src-tauri/src/`         | 1m51s |             37 |
| One file in `crates/cmdr-fs/`                     |  2m3s |             33 |
| One file in `crates/cmdr-sftp/`                   | 1m50s |             33 |
| One file in `tools/`                              |   12s |             14 |

**Say it plainly: the crate cases barely moved.** The lanes a crate edit stopped re-running are all sub-second scanners,
so `cmdr-sftp` and `cmdr-fs` cost within noise of what they cost before. The real wall-clock wins are the three
dead-weight paths (`tools/`, `pnpm-lock.yaml`, the fixture-server dirs), and `tools/` is the big one because it also
released `desktop-e2e-playwright` (400 s mean) and `desktop-e2e-linux` (520 s mean).

Frequencies, over the 1,889 Rust-touching commits: 22 touch `tools/` (all but one exclusively), 10 touch only
`pnpm-lock.yaml`, seven touch only the fixture-server dirs. So this is ~2% of commits going from a full Rust battery to
nothing, not a change in the median commit.

## Why the cargo lanes stay on `--workspace`

The premise "editing one crate rebuilds the world" is true and NECESSARY. `cmdr` depends on all five library crates, so
a crate edit legitimately invalidates the app, and cargo's own incrementality already limits the rebuild to the affected
units. There is nothing for an input set to skip.

Measured on the shared `target/`:

| Command                                                             |  Wall | Compiled                                |
| ------------------------------------------------------------------- | ----: | --------------------------------------- |
| `cargo build --workspace --features cmdr/virtual-mtp --tests`, warm | 0.4 s | 0                                       |
| Same, after appending a line to `apps/desktop/src-tauri/src/lib.rs` |  43 s | `cmdr`                                  |
| Same, after appending a line to `crates/cmdr-sftp/src/lib.rs`       |  50 s | `cmdr-sftp`, `cmdr`                     |
| Same, after appending a line to `crates/cmdr-fs/src/lib.rs`         |  55 s | `cmdr-fs` and everything above it       |
| `cargo nextest run --workspace …` test execution alone              |  22 s | app 13.5 s / crates 10.2 s, 6,586 tests |

So a crate edit's cost is the app relink, which no selection can avoid. The only slice a per-package split could recover
is the ~10 s of crate-test EXECUTION on an app-only edit, against these two costs:

- **A `-p` lane resolves features differently.** `cargo build -p cmdr-sftp --tests` took 15.4 s on a `target/` where the
  workspace build was warm, recompiling `cmdr-fs`, `futures`, `tokio-util`, `russh`, `ssh-key` and more, because the
  app's third-party feature contributions are absent from a package-scoped resolution. Those artifacts coexist rather
  than evict (the following workspace build was 0.4 s with 0 crates compiled), so it is duplication rather than thrash —
  but it is duplication forever, in a 96 GB `target/`.
- **`--workspace --exclude cmdr` DOES thrash.** 50 s to run the crate tests that way, and 44 s for the next workspace
  lane to rebuild what it evicted, against 22 s for the workspace lane alone. Alternating the two costs 4× the current
  arrangement.

**Feature unification bites, and it bites exactly where the vocabulary crate is.** Under `--workspace`, `cmdr-fs`
compiles with `testing` on: `cmdr-index`, `cmdr-archive`, `cmdr-smb`, `cmdr-sftp`, and `cmdr` all forward
`cmdr-fs/testing` through a dev-dependency. `cmdr-fs` does NOT self-enable it (`cfg(test)` covers its own tests), so
`cargo … -p cmdr-fs --tests` compiles the crate WITHOUT `testing` and never touches `cmdr_fs::testing` — the recording
and scripted host fakes every other crate's tests are built on. A `-p cmdr-fs` lane would report green over code it did
not compile. Any per-package lane here would have to force the union explicitly, which is the same thing as asking cargo
the workspace question with extra steps.

Weighting the recoverable ~10 s against these costs by real commit frequency (60.7% of Rust-touching commits are
app-only, 9% touch only `cmdr-index`, 5% only `cmdr-fs`) nets roughly −4 s on a ~250 s Rust budget, bought with five
extra lanes and a feature-parity compromise. Dropped.

`cargo-lane-feature-thrash.md` reached the same verdict in 2026-08 from the other direction (a per-package commit
simulation and the selection-scope cost); this is the re-measurement, and it agrees.

Full dependency-aware invalidation across the workspace graph was never opened. The marginal win over the above is
near-zero for the same reason — `cmdr-fs` is under everything — and its failure mode is a silent green over stale code,
which is strictly worse than a slow check.

## The narrowing found a live hole

Generalizing `TestRustInputsCoverEveryEmbeddedFile` to walk the whole registry, per check and per member, immediately
failed on `desktop-svelte-e2e-playwright` and `desktop-svelte-e2e-linux`: `desktopAppInputs()` covered
`apps/desktop/src-tauri/**` (and so `whats_new/mod.rs`) but never listed `CHANGELOG.md`, which that module pulls in with
`include_str!`. Both lanes were cache-skipping changelog edits and asserting against a binary built from the previous
changelog. Fixed by giving the set `rustEmbeddedInputs`, which is why the `CHANGELOG.md` row above goes UP by two.

This is the same bug the original test was written for, in a set the original test did not look at — which is the case
for making the guardrail registry-wide rather than per-set.

## The next win was the runner's own source, and it landed

`GlobalInputs` carried `scripts/check/**`, so editing any file of the check runner invalidated every check in the repo.
330 of the 1,889 Rust-touching commits touch it, and 205 touch nothing else: four times more commits than everything
narrowed here put together. That is now fixed. A check fingerprints the runner CORE plus the implementation files its
own `Run` reaches, derived from the AST at plan time and failing closed to the old behavior whenever the analysis is
uncertain. Editing `checks/lock-poison.go` went from 110 lanes and 3m33s to 28 lanes and 1m24s.

The mechanism, the core list and why each file is on it, what the analysis cannot prove, and the measurements:
`scripts/check/DETAILS.md` § "The runner's own source".

## Closing the runner's own residue (2026-08-24)

The pass above named the runner's own source as the next win and landed it. This is the follow-up that measured what was
LEFT, and it is mostly a report that little was: the leaf-check case is already at its floor, and the recoverable
residue turned out to sit next door, in edits to the NON-Go files under `scripts/`.

### Method

Same worktree, warm `target/`, cache settled with a no-edit run between every row (a settled run is 3 ms). A row is one
comment line appended to one file, `pnpm check` timed end to end, then the file reverted and the cache re-settled. The
lanes that ran come from `~/cmdr-check-log.csv` (rows whose `result` is not `cached`), not from the terminal. "Before"
rows are the same procedure with `checks/inputs.go`, `registry.go`, and `inputs_test.go` checked out from the parent
commit. Commit statistics are the 5,584 commits of 2026-02-21..2026-08-24.

Lane counts alone are cheap to get without running anything, which is how the search was steered. Drop this into
`scripts/check/checks/` as `zz_measure_test.go`, run
`CMDR_MEASURE_PATHS=a,b,c go test ./checks -run TestMeasureInvalidation -v`, and delete it:

```go
package checks

import ("os"; "path/filepath"; "sort"; "strings"; "testing")

func TestMeasureInvalidation(t *testing.T) {
	root, _ := filepath.Abs("../../..")
	idx := LoadRunnerSources(root)
	if idx.Err != nil { t.Fatalf("analysis gave up: %v", idx.Err) }
	defs := FilterCIOnlyChecks(FilterSlowChecks(AllChecks, false), false, nil)
	for _, path := range strings.Split(os.Getenv("CMDR_MEASURE_PATHS"), ",") {
		var hit []string
		for i := range defs {
			def := &defs[i]
			patterns := append(append(append([]string{}, def.Inputs...), GlobalInputs...), idx.For(def.ID)...)
			if matchesAny(strings.TrimSpace(path), patterns) { hit = append(hit, def.ID) }
		}
		sort.Strings(hit)
		t.Logf("== %s: %d/%d lanes\n%s", path, len(hit), len(defs), strings.Join(hit, "\n"))
	}
}
```

### Before and after, six edit cases

| Edit                                         | Lanes before | Wall before | Lanes after | Wall after |
| -------------------------------------------- | -----------: | ----------: | ----------: | ---------: |
| leaf check file (`checks/lock-poison.go`)    |           26 |       24.3s |          26 |      24.3s |
| core runner file (`runner.go`)               |          110 |      197.7s |         110 |     197.7s |
| `.mise.toml`                                 |          110 |      198.0s |         110 |     198.0s |
| sibling tool (`check-a11y-contrast/main.go`) |           23 |       23.1s |          23 |      23.1s |
| JSON allowlist beside the checks             |           22 |       22.5s |      **14** |  **15.9s** |
| agent doc under `scripts/`                   |           22 |       22.3s |      **14** |  **16.0s** |
| one crate source (`crates/cmdr-fs/`)         |           34 |      124.9s |          35 |     124.5s |
| one Svelte source (`src/lib/ui/`)            |           37 |       81.6s |          38 |      83.5s |

The first four rows are unchanged by construction and were re-measured to confirm it.

### What the 26 lanes on a leaf-check edit are, and why 24 of them can't move

Re-measured, and the breakdown holds: 10 Go lanes, 12 whole-repo doc/metric lanes, 2 registry readers, 2 real
attributions. (The earlier "28" counted two lanes that were red at the time; a failure never caches, so it re-runs in
every case regardless of inputs.)

- **The 10 Go lanes read the edited file.** `nilaway` 16.4s, `go-tests` 17.7s, `govulncheck` 10.9s, `deadcode` 6.5s,
  `staticcheck` 5.8s, `vet` 3.5s, `ineffassign` 1.6s, `gocyclo` 0.3s, `misspell` 0.1s, `gofmt` 0.2s. A `.go` file in the
  runner IS their subject matter. Nothing here is recoverable, and the whole 24.3s of this case is those lanes running
  in parallel behind `go-tests`.
- **The 12 whole-repo lanes** (`file-length`, `oxfmt`, five `docs-*`, three `claude-md-*`, `invariant-density`,
  `resident-doc-budget`) take `wholeRepoInputs` legitimately: a `.go` edit changes a line count, and the doc-graph lanes
  resolve links against real paths, so an add or a remove anywhere can flip them. ~11s of mean runtime across 12
  parallel lanes, entirely in `go-tests`' shadow. Shaving them would need a "paths matter, contents don't" input kind;
  that mechanism would cost more than the lanes do.
- **The 2 registry readers** (`ci-coverage` 0.01s, `workspace-member-coverage` 0.01s) reach every check file because
  `AllChecks` names every `Run` function and the AST closure can't distinguish "reads the registry as data" from "calls
  it". Free.

So the leaf-check case is at its floor. **What was still on the table was the other direction**: an edit to a file under
`scripts/` that is not Go at all.

### The recoverable residue: non-Go files in the Go trees

All ten Go lanes shared `scripts/**` + `apps/desktop/scripts/**`, but only two of them read anything but `.go`. That
tree also holds 13 JSON allowlists (the warn-only checks shrink-wrap them on nearly every local run), ~11 agent docs,
and the `.sh` / `.ts` / `.py` helpers.

- **357 of 5,584 commits** touched a Go tree without touching one `.go` file there: 180 touched a JSON allowlist, 131 a
  `.md`, 77 a `.js`, 44 a `.ts`, 43 a `.sh`. 162 touched an allowlist with no Go change at all.
- `goSourceInputs` (`**/*.go`, `**/go.mod`, `**/go.sum`, derived from `GetGoDirectories()`) now serves the eight lanes
  that compile. `misspell` keeps the wide set because it spell-checks every text file it walks; `scripts-go-tests` keeps
  its own, wider still.

**Say the win plainly: 22 lanes and 22.5s become 14 lanes and 15.9s, a 6.6s wall-clock cut, not a 55s one.** The eight
lanes dropped are ~55s of CPU, but they were running in parallel behind `go-tests` (16.5s), which stays in both cases.
The wall win is the tail beyond `go-tests`; the CPU win is real and shows up when an allowlist edit rides along with a
Rust or frontend battery competing for the same admission slots (`nilaway` alone holds `CpuWeight` 7).

### The correctness hole the pass found, and what it cost to close

`scripts-go-tests`' `Inputs` are the fingerprint for every Go TEST in the repo, and fifteen of those assert about the
REAL tree rather than a fixture. The set was `scripts/**` plus three SFTP fixture files, so:

- `TestRustMemberTreesMatchTheWorkspace` reads the cargo manifests. Adding a crate was a cache hit.
- `TestRustInputsCoverEveryEmbeddedFile` scans every member's `src/` for `include_str!`. Adding an embed was a cache
  hit. **This is the guard that caught the `CHANGELOG.md` hole in the pass above, and it was blind to the tree it
  scans.**
- `TestNoFrontendSourceLoadsAgentDocs` walks the frontend source roots, and is the whole reason `agentDocExclusions` is
  safe. A Svelte module importing a `.md` was a cache hit.
- `TestBindingsRegenAsksCargoTheSameQuestionAsTheOtherLanes` reads `apps/desktop/package.json`.

A guard that only re-runs when its own source changes isn't a guard. `goTestsInputs` now covers those trees, and
`realTreeReadingTests` + `TestGoTestsInputsCoverTheRealTreeItsTestsRead` fail on an undeclared real-tree test, a stale
declaration, and a declared path the lane doesn't fingerprint (17 uncovered paths on the pre-fix set).

**It cost nothing measurable.** `scripts-go-tests` joins Rust and frontend runs, and it is nowhere near the long pole in
either: 18.5s against `rust-tests`' 43.9s, 16.9s against `svelte-tests`' 62.5s. Crate edit 124.9s → 124.5s, Svelte edit
81.6s → 83.5s. Both within run-to-run noise.

### What was considered and dropped

- **Narrowing `oxfmt` to the extensions it formats** (5.9s mean, currently `**`). It would need a hand-kept extension
  list that goes silently wrong the day oxfmt gains a language, and oxfmt is never the long pole in any case measured
  here.
- **A path-literal scan as the guard for `goTestsInputs`** (walk the test sources, cover every existing repo path they
  name). Prototyped and rejected: it surfaced 73 candidates, most of them fixture path fragments written against
  `t.TempDir()` roots, and would have forced `docs/**`, `apps/website/**`, and `brand/**` into a Go lint's fingerprint.
  The AST reachability scan over `repoRootForTest` finds exactly 15 tests and is the honest question.
- **A "paths but not contents" input kind** for the doc-graph lanes. See the 12-lane bullet above.
