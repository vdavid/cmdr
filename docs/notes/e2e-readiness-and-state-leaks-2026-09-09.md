# E2E: what a "saturation flake" was hiding (2026-09-09)

Notes for whoever picks up the E2E red rate next. **Start with `docs/notes/e2e-flake-remeasured-2026-08-14.md` and
`docs/notes/flake-corpus-2026-08-08.md`**; this note only adds what those two don't already have, and one of its
findings is a straight confirmation of theirs.

## The one-line summary

Two failures that presented as load flake had fixable causes underneath. **"Different specs fail every run" is a
property of the red rate, not a diagnosis of any single failure**, and it's the sentence that stops people looking.

## Confirmed again: the breadth finding holds

`e2e-flake-remeasured-2026-08-14.md` § 4 found the red rate is a breadth effect with no top offender. Four runs on
2026-09-09, two in CI on Linux and two local on macOS, against near-identical code:

| Run               | Failed                            | Flaky                         |
| ----------------- | --------------------------------- | ----------------------------- |
| CI, `34376181232` | `media-index-network`             | `rename-chaining`             |
| CI, `34327895353` | `search-recent`                   | `indexing`, `rename-chaining` |
| local             | `conflict-dialog-matrix`          | `conflict-move`               |
| local, re-run     | `network-toggle` ×3, `servers` ×3 | `search-walk-handoff`         |

Four disjoint victim sets. `conflict-dialog-matrix:190` took 748 ms in CI and 11.1 s locally; the two local runs took
7m31s and 17m25s on the same commit, the slow one while an IntelliJ platform-test JVM was running. Load is real and it
moves the wall clock by 2×. It still didn't explain either failure below.

## Finding 1: `ensureAppReady` polls a VIRTUALIZED list, so it can't prove a listing is complete

`ensureAppReady`'s readiness poll (`helpers/app-lifecycle.ts`) asks the DOM whether every expected name has a
`[data-filename="…"]` row. `FullList.svelte:676` renders `{#each visibleFiles …}`, where `visibleFiles` is
`cache.windowRows({startIndex, endIndex})` over a virtual window plus `getVirtualizationBufferRows()`. **Rows outside
that window are not in the DOM at all.**

So the probe measures "are these rows painted", not "is the listing complete", and the gap only opens for callers
passing a wide expectation. `expectedLeftPaneEntries(fixtureRoot)` returns _every_ non-dotfile at the top of `left/` (17
names today), so a spec using it demands all 17 be in the window at once. It passes today because the window is tall
enough. It is one added fixture file, one shorter window, or one leaked "show hidden files" away from not being.

**The observed failure.** `rename-chaining:91` in CI run `34327895353` reported 14 entries, ending exactly at
`report.docx` — alphabetically the next five (`sample.docx`, `sample.pdf`, `sample.png`, `sample.tar.gz`, `sample.zip`)
are the ones missing, which is a viewport edge, not a listing in progress. The actual list also contained
`.hidden-file`, which the expectation doesn't, so hidden files were toggled on by an earlier spec: one extra row, one
fewer of the tail visible.

⚠️ **This is a different failure from the two the `Fixture-churn readiness` section already covers** (a partial listing
satisfying a too-narrow expectation, and the `FullRefresh` race, both fixed). Those are about what the pane holds. This
one is about what the pane draws, and no backend fix reaches it.

**Don't fix it by raising the 10 s deadline** — the rows are never coming, so no deadline is long enough. The probe
needs to read the pane's model rather than its DOM. `cmdr://state`'s pane entries are the obvious source, though
`ensureAppReady` deliberately avoids MCP today, so that's a real design call rather than a one-liner.

## Finding 2: a teardown that aborts halfway turns a flake into a deterministic failure

`flake-corpus-2026-08-08.md` § The shared-fixture leak established the pattern for the fixture TREE: one spec dirties
shared state, later specs die in `ensureAppReady`, and the failure names the victim rather than the culprit. **The same
pattern applies to every other piece of shared state, and settings are the one with no guard.**

`media-index-network` was the only test that genuinely FAILED CI run `34376181232`. Its `afterEach` reset two settings
in sequence. The first call exceeded the 5 s frontend-ack budget and threw, so the second never ran and
`mediaIndex.enabled` stayed on. One app process serves the whole run, so the retry re-entered with the toggle already
set and failed at `expect(hiddenAtFirst).toBe(true)` — a precondition, not the thing under test.

That's the part worth generalizing: **a retry cannot rescue a test whose own teardown left the state that makes it
fail.** It's also why this one showed as `1 failed` while `rename-chaining` showed as `1 flaky` in the same run — the
difference wasn't severity, it was whether cleanup completed.

Both halves are fixed: teardown now runs every step and reports all failures together (it swallows nothing), and
`beforeEach` establishes its preconditions instead of inheriting them. **Check any `afterEach` that writes shared state
for the same shape** — several specs still reset settings with bare sequential `await`s.

## Finding 3: the trigger was a product bug, not test slowness

Why did a `set_setting` reset blow a 5 s budget at all? `setSetting`'s idempotency guard compared with `===`. The four
`string[]` settings can never satisfy it: every writer builds a fresh array, and an MCP `set_setting` deserializes one
out of JSON. So resetting `mediaIndex.networkVolumes` to `[]` over an already-empty list ran the full notify + save +
cross-window emit, every time.

That guard's own doc comment says it exists because a heavy cascade can starve a concurrent `mcp_round_trip` waiting on
`mcp-response`, and it explicitly asked for the comparison to be narrowed if a setting ever needed deep equality. Fixed
element-wise in `settings-store.ts`.

**The transferable lesson**: "the E2E budget is too tight" and "the app does too much work" produce the same red. Before
raising a budget, check what the app actually did. Both `mcp_round_trip` timeouts here were the app, not the clock.

## How to work on this without chasing ghosts

1. **Get a quiet machine.** Check `uptime` first. These runs were taken at load 16–23 with several agent sessions live;
   at that load the suite is not a measurement instrument. A local re-run under load tells you almost nothing.
2. **Two runs, then compare the victim sets.** Disjoint sets mean you're looking at the breadth effect and should go to
   `e2e-flake-remeasured-2026-08-14.md` rather than at any one spec. The SAME spec twice is a real lead.
3. **Read a failure's retry as evidence.** A retry failing at a DIFFERENT assertion than the first attempt (as here: ack
   timeout, then a precondition) is the signature of leaked state, not of slowness. A retry failing identically is
   determinism.
4. **`1 flaky` and `1 failed` don't rank by severity.** They rank by whether the retry inherited a dirty app.
5. **A failure names the victim, not the culprit** (`flake-corpus-2026-08-08.md`). Ask what ran BEFORE it in that shard.
