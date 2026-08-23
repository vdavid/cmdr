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
