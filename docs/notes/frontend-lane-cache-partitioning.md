# Partitioning the frontend lane's cache inputs

Measured 2026-08-12 on the 24-day window 2026-07-19..2026-08-12 (1,439 commits, 99,763 rows of
`~/cmdr-check-log.csv`), plus direct Vitest runs on an isolated 16-core macOS machine (Node 26.4.0, Vitest 4.1.10).

The question: 21 frontend checks share one `Inputs` set (`svelteInputs`), so any edit under `apps/desktop/src/**` or
`apps/desktop/test/**` re-runs all of them, including the ~8,600-test Vitest suite. Is a narrower or partitioned input
set worth building?

Two answers, and they differ: excluding the colocated agent docs pays and shipped; splitting `svelte-tests` per area
does not pay and did not.

## What the lane actually costs

Non-cached seconds over the window, and the cache hit rate, for the checks on `svelteInputs`:

- `svelte-tests` 93,159 s (25.9 h), 899 runs, 17.1% cached, 103.6 s average
- `eslint-typecheck-svelte` 48,792 s (13.6 h), 697 runs, 28.6% cached
- `eslint-typecheck-ts` 35,682 s (9.9 h), 743 runs, 28.1% cached
- `desktop-svelte-eslint` 21,225 s (5.9 h), 773 runs, 27.1% cached
- `svelte-check` 15,869 s (4.4 h), 798 runs, 26.5% cached

Together 59.6 h of the window's 294.8 h of check time (20.2%). `svelte-tests` alone is the third most expensive check in
the repo, behind `rust-tests` (44.5 h) and `rust-integration-tests` (27.9 h).

The 103.6 s average is a contended number. Run alone the suite is much faster, which is what makes the sharding math
below come out the way it does.

## Isolated Vitest timings

| Run                                        | Wall  | Tests |
| ------------------------------------------ | ----- | ----- |
| Full suite, with coverage (`test:coverage`) | 62.3s | 8,635 |
| Full suite, no coverage                     | 47.1s | 8,635 |
| One area (`src/lib/utils`), with coverage   | 6.1s  | 147   |
| One area (`src/lib/utils`), no coverage     | 2.0s  | 147   |

So v8 coverage instrumentation costs ~15 s on the full suite, and a Vitest process costs ~2 s of fixed startup without
coverage and ~6 s with it (the coverage reporter walks all 36,978 statements of `src/lib` regardless of what ran).

## What shipped: excluding the agent docs

`agentDocExclusions` (`!**/CLAUDE.md`, `!**/DETAILS.md`) now rides on `svelteInputs`, the same veto `rustInputs` already
carried. Share of the window's 1,439 commits that invalidate the frontend lanes:

- current set: 594 commits (41.3%)
- with the doc veto: 503 commits (35.0%), a **15.3% relative cut** across all 21 checks

For scale, the Rust set's identical veto was accepted at a 13% relative cut. Ninety-one commits in the window touched
nothing in the frontend scope but a `CLAUDE.md` or `DETAILS.md`, and each one paid a full Vitest suite, two
ESLint+typecheck passes, and `svelte-check`.

It is safe on evidence, not on assumption: `TestNoFrontendSourceLoadsAgentDocs` scans every `.ts` / `.js` / `.mjs` /
`.cjs` / `.svelte` file under the frontend roots for a Markdown load (an `import` / `require` / `import.meta.glob`
specifier ending in `.md`, including Vite's `?raw` and `?url` forms, plus an assembled `readFileSync`) and finds none.
The doc-scanning lanes (`claude-md-length`, `docs-reachable`, `file-length`, …) take `wholeRepoInputs` and are
untouched.

## What did not ship, and why

### Splitting `svelte-tests` into per-area lanes

Rejected on three independent grounds, any one of which is decisive.

1. **The coverage gate is global.** `svelte-tests` runs `vitest run --coverage` and then fails any file under `src/lib`
   below 70% line coverage. A single-area run reports **1.15%** statement coverage overall, because every file the shard
   didn't exercise reads zero. A per-area split therefore needs either a full-suite coverage lane kept alongside the
   shards (which preserves the entire 62 s cost, so the shards save nothing) or a cross-shard coverage merge that
   combines a fresh shard's report with cached shards' stale ones. The second is a new mechanism whose failure mode is a
   green coverage verdict computed from data that no longer describes the tree.
2. **Static globs can't express the real input set.** `Inputs` is a path-glob list. The set a per-area Vitest shard
   actually reads is the reverse import closure of its test files, which reaches across areas constantly (`lib/utils`,
   `lib/intl`, `lib/ui` are imported nearly everywhere). A per-area glob would be systematically too narrow, which is
   the one failure direction that produces a false green. Vitest's own `--changed` does compute that closure, but it
   bypasses the fingerprint cache entirely and still can't produce a coverage verdict.
3. **The arithmetic is thin even ignoring 1 and 2.** A shard costs ~6 s of fixed coverage-run overhead. Splitting into
   eight areas turns a 62 s suite into 8 × (6 s + ~7 s marginal) ≈ 104 s when everything is dirty, and ~13 s when one
   area is. Against the measured edit distribution that is a real but modest win, bought with a coverage-merge
   mechanism and a structurally unsound input set.

### Narrowing `svelte-tests` past the shared set

Vitest's `include` covers `test/e2e-playwright/**/*.test.ts` (the Playwright suite's pure helper unit tests) but never
its `*.spec.ts` files, so `svelte-tests` alone could carry `!apps/desktop/test/e2e-playwright/**/*.spec.ts`. Worth 15
commits over the window: 503 → 488, a 3.0% cut on that lane and nothing on the other 20. Not taken. It buys ~45 minutes
per 24 days in exchange for the repo's first per-check divergence from a shared input set, and a footgun the day someone
adds a `.spec.ts` Vitest does read.

## Where the remaining invalidations come from

Of the 503 commits that still invalidate the frontend lanes, counted by which prefix group they touch (a commit can
touch several):

- `apps/desktop/src/**`: 337 (274 of them touch nothing else in scope)
- `scripts/check/**` (a `GlobalInput`): 106 (84 alone)
- `apps/desktop/test/e2e-playwright/**`: 86 (40 alone)
- other config: 55, `apps/desktop/test/**` (rest): 5, `pnpm-lock.yaml`: 5

The large majority is genuine: the agent really was editing frontend source. The 84 commits invalidated only by an edit
to the check runner's own source are correct by design (the runner's behavior changed) and cheap to re-establish.

## How to re-run this

The commit-touch measurement replays `git log --since=<date> --name-only` through the same glob semantics `matchesAny`
uses, so a variant is a candidate `Inputs` list and the output is the share of commits that would invalidate it. The
cost side comes from `~/cmdr-check-log.csv` grouped by `check`, summing `duration_s` for non-`cached` rows. Both are
short scripts; neither is checked in, because the inputs (the log and the log's window) move.
