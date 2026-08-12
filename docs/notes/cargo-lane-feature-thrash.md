# Cargo lane feature thrash (2026-08-12)

What it cost to have the cargo check lanes ask cargo different questions about the same `target/`, and what aligning
them bought. Kept as the regression anchor for `SharedTargetFeatureArgs` (`scripts/check/checks/cargo-workspace.go`): if
a future lane grows its own feature flags or its own `cmd.Dir`, these are the numbers it gives back.

## Method

macOS 15, M-series, rustc 1.97.1, cargo 1.97.1, warm 113 GB `target/`, worktree
`.claude/worktrees/david+rust-lane-cache` at `2d8ae7095`. Each row is one command, timed end to end, with
`Compiling <crate>` lines counted from its output. No source changed between rows: the only variable is what was asked
of cargo. Scripts and raw logs were scratch-only; the numbers below are the whole result.

## Alternating feature sets

`A` = `cargo build --locked --workspace --tests --features cmdr/virtual-mtp` (the `desktop-rust-tests` build set). `B` =
the same without the feature (what `desktop-rust-integration-tests` used to ask for).

| Run                     | Wall  | Crates compiled                  |
| ----------------------- | ----- | -------------------------------- |
| A, repeated immediately | 1.3 s | 0                                |
| B, right after A        | 92 s  | 2 (`cmdr`, `operation-log-dump`) |
| A, right after B        | 20 s  | 2 (same two)                     |

Cargo does not keep both variants of the app crate reachable across a flip, so each alternation pays a full `cmdr`
rebuild plus everything above it. A default `pnpm check` used to flip at least twice.

Measured again as whole checks, before and after the alignment, same sequence on the same tree:

| Sequence                                   | Before | After  |
| ------------------------------------------ | ------ | ------ |
| `pnpm check bindings-fresh` (marker miss)  | 28.8 s | 2.3 s  |
| `pnpm check rust-tests`, immediately after | 70 s   | 27.7 s |
| Pair                                       | 98.8 s | 30.0 s |

The "before" `bindings-fresh` reading is a lucky one: that `target/` still held the package-scoped artifacts from the
rows below, so the regen skipped the compile it usually pays. From a `target/` holding only the workspace build, the
same regen shape cost 99.6 s.

## Package-scoped selection is a third question

`cargo build --locked --tests` run from `apps/desktop/src-tauri` (what `pnpm bindings:regen` used to do) resolves
dependency features for one package rather than for the workspace, so it rebuilds the first-party crates:

| Run                                          | Wall   | Crates compiled                                                            |
| -------------------------------------------- | ------ | -------------------------------------------------------------------------- |
| `-p cmdr --tests`, no feature, after A       | 99.6 s | 5 (`cmdr-fs`, `cmdr-fsevent-stream`, `cmdr-archive`, `cmdr-index`, `cmdr`) |
| A, right after                               | 19.9 s | 2                                                                          |
| `-p cmdr --tests`, aligned features, after A | 19.1 s | 1 (`cmdr`)                                                                 |

Selection scope alone does not evict the workspace artifacts (a following `A` was 0.6 s in a separate run), but it does
pay its own dependency build. Both effects point the same way: one question, one set of artifacts.

## Clippy is not part of this

`cargo clippy --workspace --all-targets`, with or without the feature, left the test build untouched: the following `A`
was 0.7 s with 0 crates compiled either way. Clippy's workspace units go through `clippy-driver` and live in their own
fingerprints, so it needs no alignment. It does mean the `virtual-mtp` code is compiled by the test lane but linted by
nothing; `cargo clippy --workspace --all-targets --features cmdr/virtual-mtp` passes clean today (measured here), so
closing that gap is available and cheap whenever someone wants it.

## Per-package test lanes: measured, then dropped

Splitting `desktop-rust-tests` into one lane per workspace member was the original plan. Two measurements killed it:

- **The dependency closure makes the split nearly free of savings.** Simulating 1,433 commits over 24 days against a
  per-package input set (each package plus its workspace-dependency closure): a `cmdr` lane would run on 51% of commits
  against 52% for "any Rust lane at all". `cmdr` sits at the top of the graph, and `operation-log-dump` sits above
  `cmdr`, so both are as broad as the whole lane. The crates that would genuinely skip (`cmdr-fs` 13%, `cmdr-archive`
  13%, `cmdr-fsevent-stream` 10%) are also the cheap ones.
- **It would pay the selection-scope cost on every run**, per the table above: N per-package invocations are N distinct
  questions about the same `target/`.

Narrowing the shared input set instead was worth more and cost nothing: dropping the 206 colocated `CLAUDE.md` /
`DETAILS.md` files from `rustInputs` takes the lane from 62% of commits to 54%, on the same 1,433-commit sample.
