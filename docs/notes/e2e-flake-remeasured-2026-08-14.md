# The E2E red rate, re-measured (2026-08-14)

There's a number that gets repeated in this project: **`desktop-e2e-playwright` fails 60% of the time at the run
level.** Two E2E infrastructure bugs have since been fixed, which is a reason to suspect the number is stale. This note
re-measures it and reports what the data can and can't support.

**The short version:**

1. **The 60% was right for its window.** Over 2026-07-19..2026-08-12 the lane failed **59.1%** of runs (110 of 186).
   `desktop-e2e-linux` failed **33.6%** (38 of 113). Nobody was exaggerating.
2. **Post-fix, the sample is far too small to quote a new rate.** Both fixes were in for exactly **one** logged
   `desktop-e2e-playwright` run. Even counting from the first of them, it's six. Anyone quoting today's rate as a number
   is reading noise.
3. **The concurrency bugs were real, and they were never the main cause.** Their fingerprint is visible: a run
   overlapping another run of the SAME lane failed much more often (Linux 64.7% against 26.6%, p≈0.002). But only ~9% of
   runs overlapped, so removing that effect moves the aggregate by about a point, not by forty.
4. **The red rate is a BREADTH effect, and quarantining can't touch it.** Over the instrumented window: 14 test failures
   across 10 red runs, spread over 10 different specs, with **zero test failing twice**. A per-test failure rate of
   0.17% over 281 tests predicts a ~38% red-run rate all by itself, which is what we see. There's no top offender to
   quarantine because there's no top offender.
5. **Directionally it does look better** (41.4% against 59.1% since 2026-08-12 18:07), but that comparison is p≈0.07,
   and the window also contains several unrelated de-flaking commits. Encouraging, not established. § "How to tell in a
   week" says what would settle it.

## What was fixed, and when

- **`672cbb597` (2026-08-13 16:28)** — the check runner used fixed MCP ports with a pre-flight that stopped whatever
  held them, so a second E2E suite took down the first one's app mid-test.
- **`efd2b7d68` (2026-08-14 02:36)** — everything the E2E lanes wrote under `/tmp` was shared between concurrent runs,
  including the JSON report three checks read a run's verdict out of, so a run could be judged on another run's results.
- **`739d980f9` (2026-08-12 18:07)**, worth naming as a third: a Playwright run that died before writing its report
  could log the previous run's results as this run's. This one didn't need concurrency to bite.

⚠️ **A timestamp cut is an upper bound on how "post-fix" a run really is.** `pnpm check` compiles the runner from the
worktree's OWN `scripts/check/`, so a worktree branched before a fix keeps running the buggy runner afterwards. Every
"since" figure below therefore over-counts post-fix runs rather than under-counting them.

## Run-level rates

From `~/cmdr-check-log.csv` (one row per lane run; `timestamp` is when the check FINISHED and `duration_s` is its wall
clock, so a run occupies `[timestamp − duration_s, timestamp]`).

- **2026-07-19..2026-08-12** (the folklore window): playwright 110 red of 186 = **59.1%**; linux 38 of 113 = **33.6%**.
- **Since 2026-08-12 18:07** (stale-report fix): playwright 12 of 29 = **41.4%**; linux 2 of 18 = **11.1%**.
- **Since 2026-08-13 16:28** (port fix): playwright 3 of 6 = 50.0%.
- **Since 2026-08-14 02:36** (both fixes): playwright 0 of 1.

**The last two are not results.** Six runs put a 95% interval on 50% of roughly 12%–88%; one run says nothing at all.

The 41.4% figure is the interesting one, and it still doesn't settle the question: 12 of 29 against 110 of 186 is
z≈1.79, **p≈0.073**. The drop is suggestive and fails a conventional test. Its Wilson interval (25.5%–59.3%) includes
the old 59.1%. And 2026-08-12/13 carried several unrelated E2E fixes (`7dfdc44e5`, a stuck search dialog no longer
taking a whole shard with it; `c79a46351`, a unit test no longer truncating the shared fixture cache under every running
shard), so even a real improvement isn't attributable to the two named bugs.

Linux moved from 33.6% to 11.1% (2 of 18) over the same cut, z≈1.93, **p≈0.054**. Same verdict: promising, unproven.

## The concurrency bugs' fingerprint, measured

Both fixed bugs need two runs of the same lane in flight at once. That's testable on the large pre-fix sample: classify
each run by whether its `[start, end]` interval overlapped another run of the same lane, and compare.

Pre-2026-08-13 16:28:

- **playwright**: 187 solo runs at **56.7%** red, 19 overlapped runs at **68.4%**.
- **linux**: 109 solo runs at **26.6%** red, 17 overlapped runs at **64.7%**.

What that says:

- **Linux: the effect is unambiguous** — 26.6% → 64.7%, z≈3.09, p≈0.002. Concurrent same-lane runs more than doubled its
  red rate.
- **Playwright: not distinguishable** — 56.7% → 68.4%, z≈1.04, p≈0.30. With 19 overlapped runs and a base rate already
  above half, the effect (if it's the same one) has nowhere to show.
- **Control**: classifying by overlap with the OTHER E2E lane instead (CPU contention, but no shared port and no shared
  report path) gives 52.2% → 61.1% for playwright and 17.6% → 28.3% for Linux, the latter on only 17 solo runs. The
  same-lane effect is the sharper one, which is what the two bugs predict and what plain CPU contention doesn't.

**What this is worth.** Only 21 of 212 playwright runs and 17 of 129 Linux runs overlapped a same-lane run. So even
taking the effect at face value, removing it takes playwright's aggregate from 57.8% to about its solo rate of 56.0%.
**The concurrency bugs cost roughly one point of the macOS lane's red rate.** They mattered a great deal for Linux, and
for whoever's afternoon went into a green branch reporting 38 failures it didn't cause, and they were worth fixing. They
are not why this lane goes red.

## What actually fails: breadth, not offenders

`~/cmdr-test-log.csv` covers E2E from 2026-08-12 18:09 (29 playwright runs, 18 Linux). Per-test, at last.

Twelve red playwright runs, of two very different shapes:

- **Two whole-shard collapses** (46 failures across 13 specs; 38 across 8), both before the port fix. A run where a
  third of a shard dies is one dead app, not per-test flake, and folding it into a ranking swamps everything.
- **Ten ordinary red runs, 14 test failures total.** 14 failures, **14 distinct tests**, 10 distinct specs. Not one test
  failed twice outside a collapse, and no failing test recurred in the next run.

That last fact is the finding. The specs involved were `app`, `focus-trap`, `ask-cmdr`, `operation-queue` (×3), `mtp`
(×3), `indexing`, `search-modes`, `search-recent`, `file-operations`, `archive-browsing`. Ten specs, one failure each.

**So arithmetic, not a hit list, explains the red rate.** 14 failures over roughly 29 × 281 ≈ 8,150 test executions is a
per-test failure rate of **0.17%** (95% CI ~0.09%–0.29%). A run is red if ANY of its 281 tests fails:

```
1 − (1 − 0.0017)^281 = 38%
```

against an observed ordinary red rate of 10 of 29 = 34.5%. The model fits. **A suite this wide converts an excellent
per-test rate into a terrible run-level one**, and that's the dominant term. Halving the per-test rate would take the
run rate from ~38% to ~21%; quarantining the worst spec takes it from ~38% to ~36%, and there isn't a worst spec.

⚠️ Read `~/cmdr-test-log.csv` with its threshold in mind: a passing test earns a row only at or over 1.0 s, so absence
means "fast, or never ran", never "passed". Failure counts are exact; pass counts are not, which is why the denominator
above comes from the check log's "281 tests passed across 3 shards" message rather than from row counts.

## Retry-passes: only the Linux lane can have them

`playwright.config.ts` sets `retries: process.env.CI ? 1 : 0`. The Linux Docker lane runs with `CI=true`; the local
macOS lane doesn't, bar two `test.describe.configure({ retries: 1 })` carve-outs. So:

- **macOS logged zero `flaky` rows in the whole window, and structurally almost cannot log one.** "Rank the top
  offenders by retry-pass" has no macOS answer. That's the config, not a clean suite.
- **Linux logged 10 retry-passes over 18 runs**, led by
  `search-recent.spec.ts::Search dialog: recent searches::Open-in-pane persists the query to the backend recent-search store`
  (4), then one each in `app`, `archive-browsing`, `conflict-copy`, `conflict-move`, `dialog-inset`, `dialog-resize`.
  Same shape as the macOS failures: spread, barely repeating.

**The two lanes' red rates are therefore not comparable**, and 41.4% against 11.1% is mostly this. macOS reports an
UNRETRIED rate. The zero-retry local default is deliberate (a real race should surface immediately rather than be
papered over) and this note doesn't argue with it, but any comparison that forgets it is wrong. `search-recent`'s
Open-in-pane test is the one test on both lanes' lists, so it's the single best de-flaking target the data offers.

## What this measurement cannot tell you

- **It can't separate a flake from a genuine regression.** The check log records that a lane went red, not why. On the
  instrumented window it can be inferred (every red run was followed by a green one, and no failing test recurred, so
  those 14 were flakes), but the 24-day figures include real breakage the suite caught doing its job. **The historical
  rates above are red-run rates, NOT false-alarm rates**, and are an upper bound on the latter.
- **Runs come from many worktrees on different code.** Neither log has a worktree column.
- **Runs with `--ci` or `--no-log` are invisible**, so overlap detection can undercount concurrency.
- **`flaky` is a lane-config artifact** as much as a test property (above).

## How to tell in a week

Don't wait on the run-level rate. **Track the per-test rate**: it has ~280× the sample per run, and two days of it
already gives ±0.1pp where the run rate after two days gives ±17pp.

```sh
# Per-test failures, the metric that converges. A run with >10 of them is one
# dead app, not per-test flake: check the shape before averaging.
sqlite3 -column -header :memory: '.import --csv ~/cmdr-test-log.csv t' \
  "select \"check\", timestamp, count(*) as failures, count(distinct test_id) as distinct_tests
   from t where \"check\" like 'desktop-e2e%' and status in ('fail','timeout','leak')
     and timestamp >= '2026-08-14'
   group by 1,2 order by 2"

# The run-level rate, once there are enough runs to bother.
sqlite3 -column -header :memory: '.import --csv ~/cmdr-check-log.csv c' \
  "select \"check\", count(*) as runs, sum(result='fail') as red,
          round(100.0*sum(result='fail')/count(*),1) as pct
   from c where \"check\" in ('desktop-e2e-playwright','desktop-e2e-linux')
     and timestamp >= '2026-08-14 02:36'
   group by 1"
```

**Use `sqlite3`, never `awk` or `cut`**: `test_id` contains commas and is RFC-4180-quoted, so a field-splitting
one-liner mangles exactly the rows you care about. `scripts/check/DETAILS.md` § "The per-test log" has more recipes.

**The bar for calling it.** Against the fixed 59.1% baseline, distinguishing a true 41% at 80% power and α=0.05 needs
about **60 post-fix runs**. At the recent cadence (~8 playwright runs a day) that's roughly **8 days**, so the run-level
question answers itself around 2026-08-22 with nobody doing anything except letting the log fill. Don't run the lane to
feed this: concurrent E2E runs on one machine contend for CPU and pollute the signal they're measuring.

**What would overturn § "breadth, not offenders":** a single test appearing in three or more red runs. That's the signal
that a specific test rather than the suite's width is driving the rate, and the point where quarantining becomes the
right tool instead of the wrong one.

## Related

- `flake-corpus-2026-08-08.md` — the per-test corpus this note is the run-level companion to: every test seen failing
  without a defect behind it, ranked over 48 shard-runs, with cause hypotheses. Its `/tmp`-derived counts can't be
  re-derived; everything here can, from the two CSV logs.
- `scripts/check/DETAILS.md` § "The per-test log" — the schema, what "absent" means, and why the two logs stay separate.
