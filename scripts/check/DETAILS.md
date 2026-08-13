# Check runner details

Pull-tier docs for `scripts/check/`: architecture, flows, and decision rationale. Must-know invariants and gotchas live
in `CLAUDE.md`. For check authoring (how to add a new check, `CheckDefinition` shape, naming/`CLIName()` rules, common
helpers, the allowlist), see `checks/CLAUDE.md`.

## Quick start

```bash
# Run all checks (excludes slow checks by default)
pnpm check

# Run a specific check (accepts ID or nickname)
pnpm check clippy

# Run multiple specific checks (commas work too: oxfmt,clippy)
pnpm check rustfmt clippy

# Run a tech group (rust, svelte, go) or an app (desktop, website, api-server, scripts)
pnpm check rust
pnpm check website

# Include slow checks
pnpm check --include-slow

# Run only slow checks
pnpm check --only-slow

# Run only the curated fast pre-commit lane (~10s)
pnpm check --fast

# CI mode (no auto-fixing, stop on first failure)
pnpm check --ci --fail-fast

# Run compat checks on freestyle VM, incompat checks locally, in parallel
pnpm check --prefer-freestyle

# Run only freestyle-compatible checks on the VM (skip Rust, Docker)
pnpm check --only-freestyle
```

## Command-line options

Positional args select what to run: check IDs/nicknames, app names (`desktop`, `website`, `api-server`, `scripts`), and
tech groups (`rust`, `svelte`, `go`), in any mix, space- or comma-separated, with flags anywhere in between
(`parseInterspersed` re-parses around positionals since Go's stdlib `flag` stops at the first one). Named checks run
even if slow or CI-only; app/group selectors keep the default lanes. `ValidateCheckNames` rejects any check ID or
nickname that would shadow a group/app keyword (`reservedSelectorNames` in `main.go`).

- **`--app NAME`**: Run checks for specific apps (repeatable or comma-separated)
- **`--rust`, `--rust-only`**: Run only Rust checks (desktop)
- **`--svelte`, `--svelte-only`**: Run only Svelte checks (desktop)
- **`--check ID`**: Run specific checks by ID or nickname (same as naming them positionally)
- **`--ci`**: Disable auto-fixing (for CI)
- **`-v`, `--verbose`**: Print a line per check instead of the collapsed summary (details below); `--ci` implies it
- **`--include-slow`**: Include slow checks (excluded by default)
- **`--only-slow`**: Run only slow checks
- **`--fast`**: Run only the curated fast pre-commit check set
- **`--fresh`**: Bypass the input-fingerprint cache: run everything selected, then refresh it
- **`--only-freestyle`**: Run freestyle-compatible checks on a VM (skip the rest)
- **`--prefer-freestyle`**: Run compat checks on VM + the rest locally in parallel
- **`--fail-fast`**: Stop on first failure
- **`--no-log`**: Disable CSV stats logging
- **`--graph`**: Render the check dependency graph (weights + lanes + median wall-time) and exit
- **`--graph-format`**: Graph output: `tree` (default, colored terminal), `mermaid`, `dot`
- **`--docs-graph`**: Render the doc-discoverability tree (rooted at the repo-root `CLAUDE.md`) with per-doc usage, and
  exit
- **`-h`, `--help`**: Show help message

`--graph` honors the same selectors (positional or flag form), so `pnpm check rust --graph` graphs only the Rust checks.
It renders before the slow/fast/CI filters, so every lane shows with its size badge. Each node also shows
`~<median wall-time>` from the recent (last 20) passing runs in `~/cmdr-check-log.csv`, so the graph doubles as a perf
dashboard — pairing the CPU-weight (how heavy) with the typical duration (how long) for spotting the next optimization
target. Missing log (CI / `--no-log` / fresh machine) just omits the times. `mermaid` output pastes into a Markdown
```mermaid block or https://mermaid.live; `dot` pipes to Graphviz
(`pnpm check --graph --graph-format dot | dot -Tpng -o checks.png`).

### `--docs-graph` and its usage enrichment

`--docs-graph` (in `docs_graph_render.go`, over the graph from `checks/docs_graph.go`) prints the doc-discoverability
tree rooted at the repo-root `CLAUDE.md`, then annotates every node with how it's actually used, e.g.
`scripts/check/CLAUDE.md Read 63x (8%), written 1x`. That enrichment lives in `docs_graph_usage.go` and mines the Claude
Code transcript store at `~/.claude/projects/`. The model:

- **Session = one transcript JSONL file** (top-level or subagent, so subagents and team members count separately; a
  post-compaction session with a new id counts separately too). The store is scanned across every `*cmdr*` project slug
  (main clone, all worktree slugs, and the `/private/tmp/ab/cmdr` target clone), recursing into per-session subdirs for
  subagent transcripts. Files are filtered by mtime to a rough ~30-day window (`usageWindow`); the boundary is coarse by
  design, so a long session straddling it is kept or dropped whole.
- **Touch = a `Read`/`Edit`/`Write`/`MultiEdit`/`NotebookEdit` naming a file.** Each file path is folded to a canonical
  repo-relative path by splitting on `/cmdr/` and stripping any `.claude/worktrees/<name>/` prefix, so a doc edited in a
  worktree or the target clone lands on the same node. Paths outside the repo, and files in since-moved locations, are
  dropped (accepted data loss).
- **Read count = sessions where the doc was loaded.** A touch of any file loads the `CLAUDE.md` of that file's directory
  and every ancestor directory (Claude Code's autoload), plus those files' transitive `@`-imports (root `CLAUDE.md`
  `@import`s `AGENTS.md`; plain Markdown links do NOT autoload). `DETAILS.md` and `docs/` files therefore only score a
  read when a session explicitly `Read`s them.
- **Write count = sessions that `Edit`/`Write` the doc.** A doc created fresh via `Write` can show writes > reads.
- **Denominator = sessions that touched ≥1 repo file** (the `%` base). Because `.` is an ancestor of every path, the
  root `CLAUDE.md` and `AGENTS.md` load in every counted session, so both must read **exactly 100%** — a built-in
  correctness check: if either drifts off 100%, the computation is wrong. When the transcript store is missing, the tree
  renders without annotations.

The annotation is dim for scannability except the read percentage, which is colored by absolute threshold
(`readColorFor`): never read is red (dead weight), read by up to 20% of sessions is yellow (domain-specific docs), and
above 20% is green (broadly loaded). Orange is deliberately unused (too close to red to distinguish on screen), and
thresholds are absolute rather than percentile-based so the buckets carry a fixed meaning instead of shifting with the
distribution.

The scan costs a few seconds (streaming JSONL, prefiltering to lines containing `"tool_use"`); it always runs, since
`--docs-graph` is an on-demand diagnostic, not part of `pnpm check`.

**Quiet mode is the default**, because ~50 check lines drown the signal for the reader who matters most: an agent that
only ever sees the final captured stdout (the live "Waiting for:" status line is already TTY-only). It drops the
`📦 pnpm` and `🔍 Running N checks` headers and collapses every silent pass (clean OK results and cache hits) into one
summary line: `✅ 41 checks OK, 1 warn (4.4s)`. What still streams verbatim, because the agent must act on it: warns,
failures, skips (a skip didn't verify anything), and passes that changed files (a formatter rewriting the tree, flagged
by `MadeChanges`). The warn/skip counts ride into the summary so a collapsed warn is never silently lost.

`-v` / `--verbose` opts back into the per-check lines, and `--ci` implies it: log volume is free in a job log, and a
collapsed run is far harder to debug after the fact.

Implementation: `parseFlags` derives `cliFlags.quiet` as `!(verbose || ci)`, `Runner.suppressedInQuiet` picks which
per-check lines to hide, and `printSuccess` / `summarizeRun` in `main.go` build the summary line. Caching, logging, and
exit codes are unchanged. Suppression is output-only.

## Architecture

```
pnpm check [flags]
  -> scripts/check.sh [flags]
    -> ValidateCheckNames()          # startup: catch ID/nickname collisions + reserved-keyword shadows
    -> parseFlags()                  # flags + positional selectors (checks, apps, groups), interspersed
    -> findRootDir()                 # walk up to repo root
    -> handleFreestyleFlags():
        --prefer-freestyle:          # parallel: VM (compat) + local (incompat)
          goroutine: freestyleRun()  #   push sync branch, run on VM
          local: Runner.Run()        #   FreestyleIncompat checks only
          wait + reconcile results
        --only-freestyle:            # VM only, skip incompat
          freestyleRun()
    -> selectChecks()                # filter AllChecks by flags
    -> applyLaneFilters()            # FilterSlow/CIOnly/Fast/Freestyle/onlySlow, in order
    -> planCache() (plan.go)         # input-fingerprint cache, BEFORE pnpm+SMB:
        CollectRepoFingerprintData() #   one repo-wide `git ls-files`+`git status` pass
        per check: FingerprintFor()  #   hash its Inputs ∪ GlobalInputs from that pass
        split selected -> toRun / cached  # cache hit = entry fingerprint matches
    -> ensurePnpmDependencies()      # pnpm install once at root (skipped if all node checks cached)
    -> setupSmbOrchestratorIfNeeded()# Docker/SMB up only if a NON-cached check NeedsSmb
    -> Runner.Run():
        reportCached()               # print + log the cache hits as "OK (cached)" first
        goroutine pool (NumCPU semaphore)
        for each pending check: canStart() checks DependsOn deps
          -> dep pending/running: wait
          -> dep failed/blocked: mark StatusBlocked, print BLOCKED
          -> all deps done: launch goroutine -> runCheck() -> completedCh
          (a cached dep is absent from toRun, so canStart treats it as satisfied)
        status line goroutine (200ms tick, TTY only): "Waiting for: foo, bar..."
    -> plan.recordRun()              # cache this run's passing fingerprints (skipped under --ci)
    -> print summary ("N ran, M cached"), exit 0/1
```

## Key files

- **`main.go`**: Entry point: flag parsing, root dir discovery, check selection, pnpm gating, runner delegation
- **`runner.go`**: Parallel executor: CPU-weighted admission gate, dependency graph, fail-fast, live TTY status line
- **`graph.go`**: `--graph` renderer: dependency forest with CPU weights, size lanes, and median wall-time from the
  stats CSV (tree / mermaid / dot)
- **`stats.go`**: CSV stats logging: one row per check to `~/cmdr-check-log.csv` (`logCheckStats`), plus one row per
  individual test to `~/cmdr-test-log.csv` (`logTestStats`, § "The per-test log")
- **`checks/test-log.go`**: the per-test vocabulary (`TestRecord`, `TestOutcome`, `TestRecorder`) every test lane
  records through
- **`plan.go`**: Input-fingerprint cache planning: splits selected checks into cache hits and misses BEFORE pnpm/SMB;
  records passes after the run
- **`checks/fingerprint.go`**: Git-aware content fingerprint per check (one repo-wide `git ls-files`+`git status` pass,
  filtered per check's Inputs)
- **`checks/cache.go`**: Per-worktree cache file load/save (`node_modules/.cache/cmdr-check-cache.json`), atomic write,
  corrupt-tolerant
- **`checks/inputs.go`**: Shared `Inputs` building blocks (mined from ci.yml filters) + `inputs()` concatenator
- **`colors.go`**: ANSI color constants
- **`utils.go`**: `findRootDir()` (walks up until `apps/desktop/src-tauri/Cargo.toml` is found)
- **`smb_orchestrator.go`**: Runner-level SMB Docker lifecycle: acquires a machine-wide lease (via `smblease`) at init,
  releases at exit
- **`smblease/`**: Library: the machine-wide flock + holder-id refcount that makes the shared `smb-consumer` stack safe
  across worktrees
- **`smb-lease/`**: Thin `package main` CLI onto `smblease` (`acquire`/`release`/`reconcile`/`status`) that the bash
  scripts shell out to
- **`freestyle.go`**: All freestyle.sh remote-VM execution logic, including `preferFreestyleRun`
- **`checks/`**: One file per check, plus `common.go` (shared utils) and `registry.go` (the `AllChecks` ordered list)

## Runner-level patterns

**Dependency graph:** Flat `DependsOn` slice per check. Blocked checks get `StatusBlocked` on dep failure and are
counted as failed. Dependencies not in the selected run set are treated as satisfied. Visualize it with
`pnpm check --graph` (every check currently has ≤1 dependency, so it renders as a clean forest rooted at `oxfmt` /
`rustfmt` / `gofmt`).

**CPU-weighted admission:** Instead of a count semaphore, `tryStartPending` admits a check only when
`sum(running CpuWeight) + weight ≤ NumCPU` (`runner.go`). A check first clears its dependencies (`canStart`), then the
weight gate; if deps are ready but the budget is full it stays `Pending` and retries once a running check frees its
weight. The `usedWeight == 0` clause lets an over-budget check run alone rather than deadlock. This keeps two CPU-heavy
checks (e.g. `svelte-tests` w11 + `clippy`-cold w8) from piling up and oversubscribing the machine, while light checks
(the `eslint-typecheck-{svelte,typescript}` passes w2, the Docker checks) overlap freely. See the Key decision below and
`docs/notes/check-cpu-contention.md`.

**Exclusive resources:** `CheckDefinition.Exclusive` names a resource a check needs to itself; two checks naming the
same one never overlap, whatever the weight budget allows. It sits between the dependency gate and the weight gate in
`tryStartPending`, and the holder releases it on every exit path (deferred), so a red or panicking check can't strand
every other lane. It can't deadlock: a check takes at most one resource and holds it only between its own start and
finish.

The one resource today is `ResourceCargoBuildDir`, held by every lane that COMPILES against the shared `target/`
(`clippy`, `rust-tests`, `integration-tests`, `bindings-fresh`, `cargo-udeps`, `groq-smoke`). Cargo takes an exclusive
lock on its build directory for a whole command, so those lanes were always serial; undeclared, the loser sat on
`Blocking waiting for file lock on build directory` while still holding 6-8 weight, so a quiet run looked hung and the
reserved cores went unused. Declaring it costs no wall clock and hands that weight back. Metadata-only commands
(`cargo metadata`, `about`, `deny`, `machete`) take the package-cache lock instead and stay undeclared;
`rust-tests-linux` builds in its container's own `CARGO_TARGET_DIR`, and `rustdoc` owns a private one. Measurements:
`docs/notes/check-cpu-contention.md` § "Cargo's build-directory lock".

**Slow checks:** `IsSlow: true` marks checks excluded by default (currently: `rust-tests-linux`, `desktop-e2e-linux`,
`desktop-e2e-playwright`). Naming a check (positionally or via `--check`) implicitly includes slow checks
(`includeSlow = len(checkNames) > 0`); group/app selectors don't.

**Fast lane (`--fast`):** `IsFast: true` marks the curated pre-commit check set: ~28 checks that finish in roughly 10s
on a warm cache, intended to run before every commit. It's an editorial pick, not a timing-derived list (see Key
decisions below). Named check invocations bypass the filter so `pnpm check --fast svelte-check` still runs svelte-check.
Mutually exclusive with `--include-slow` / `--only-slow` — combining them errors out, since the lanes are intentionally
separate.

**CI-only checks:** `CIOnly: true` marks checks that run only in `--ci` mode (currently `cargo-udeps`, `jscpd-rust`, and
`groq-smoke`). They're silently dropped from local runs (no SKIPPED line) and are not pulled in by `--include-slow` or
`--only-slow`. Escape hatch: an explicit `pnpm check cargo-udeps` always runs, so you can verify locally before pushing.

**Self-contained E2E checks:** `desktop-e2e-playwright` manages the full lifecycle (build the binary, create per-shard
fixtures, start N Tauri instances, run N Playwright processes in parallel, cleanup). The build is fingerprinted and
skipped when the binary on disk already matches the tree it would be built from, which is worth 172 s per run because
the build isn't incremental (`checks/DETAILS.md` § "The Playwright lane's binary is fingerprinted"). Each shard runs in
its own isolated `CMDR_DATA_DIR` with its own Unix socket and MCP port (asked of the OS per run, never a fixed base:
`checks/DETAILS.md` § "Nothing a shard owns is shared between runs"), plus a per-shard
`CMDR_INSTANCE_ID` of the form `e2e-<short>-<pid>` (for example, `e2e-mtp-12345`, `e2e-nonmtp1-12345`). The instance ID
drives the macOS Keychain `SERVICE_NAME` suffix (`Cmdr-e2e-<short>-<pid>`) so two parallel shards can never collide on
credentials, and reshapes the Dock label into `Cmdr (E2E <short>)` so cleanup scripts can target with
`pgrep -f 'Cmdr (E2E '`. One shard is dedicated to MTP specs (serialized; the virtual MTP backing dir at
`/tmp/cmdr-mtp-e2e-fixtures` is shared by every Tauri instance). Stale processes on each port are killed before
starting. Per-shard logs go to `/tmp/cmdr-e2e-playwright-<shard>-<timestamp>.log`. See
`docs/tooling/instance-isolation.md` § "How E2E gets isolated per shard".

`RUST_LOG` is forwarded to the app (via inherited `os.Environ()`), so trace-level output is one shell-prefix away:

```bash
RUST_LOG=cmdr_lib::file_system::volume::mtp=trace pnpm check desktop-e2e-playwright
```

The chosen `RUST_LOG` value is echoed at the top of the timestamped log so it's obvious from a glance which level was
captured. When unset, the log starts with `=== RUST_LOG unset (default warn level) ===`.

After a successful run, both E2E checks flag (warn-only) any individual test that took more than 2 s wall-clock, against
a per-platform allowlist. See `checks/DETAILS.md` § "E2E test duration flagger".

**TTY detection:** `golang.org/x/term.IsTerminal` gates the live status line; CI logs stay clean.

**CSV stats logging:** Each check run appends a row to `~/cmdr-check-log.csv` with timestamp, app, check name, duration,
result (pass/fail/skip/blocked/cached), and optional counts (total, issues, changes). `CheckResult` has `Total`,
`Issues`, `Changes` fields (`-1` = N/A, rendered as `N/A` in CSV). Disabled by `--no-log` or `--ci`. Implementation in
`stats.go`. A cache hit logs as `cached` (not `pass`) so `--graph`'s median, which counts only `pass` rows, isn't
dragged down by ~0s hits.

## The per-test log

`~/cmdr-test-log.csv` records INDIVIDUAL tests, one row each, so "which 15 tests cause most of my red runs, and which
are slowest" is a query rather than an archaeology dig. It's the companion to `~/cmdr-check-log.csv`, which stays
lane-level: a red lane logs the message `rust tests failed` there and names no test, which is exactly the gap this
closes.

**Two files, never one.** `~/cmdr-check-log.csv` has ~98 000 rows against a nine-column header, and every reader of it
(Go's `csv.Reader`, Python's `csv` / pandas) hard-errors on a field-count mismatch, so widening that schema would
destroy the history in place. One row per test is also the better data model. Don't merge them.

**Schema** (`timestamp,check,test_id,status,duration_s,attempt`):

- `timestamp`: `YYYY-MM-DD HH:MM:SS`, identical for every row of one check run, so rows group into runs by
  `(check, timestamp)`.
- `check`: the lane's CLI name (`rust-tests`, `svelte-tests`, `desktop-e2e-playwright`, …).
- `test_id`: stable identity. Nextest lanes use `<binary>::<test path>`; the Vitest and Playwright lanes use
  `<spec file>::<describe chain joined with " › ">::<title>`, the same key the E2E duration allowlist uses.
- `status`: `pass` / `fail` / `flaky` / `timeout` / `leak` / `skip`. `flaky` is a retry-rescued test, which both runners
  exit 0 on; `timeout` is a kill at the runner's cap; `leak` passed its assertions but outlived itself.
- `duration_s`: wall clock of the attempt that produced `status`, three decimals, or `N/A` when the reporter gave none.
  For a retried test it's the worst SINGLE attempt, never the sum of them.
- `attempt`: 1-based attempt that produced `status`, so a `flaky` row says which try rescued it.

**Not every row is written.** Anything that isn't a clean pass is always logged, so failure counts are exact. A PASSING
test earns a row only at or over `testLogSlowSeconds` (1.0 s, `stats.go`), and skips are never logged. Without that
threshold a single Rust run would write ~5 000 rows saying "fast test was fast again", roughly half a gigabyte a month.
So: absence of a test means "fast, or never ran", never "passed". A slow-test ranking is unaffected — a test under the
threshold isn't one of the slow ones.

**Covered lanes:** `rust-tests`, `rust-integration-tests`, `rust-tests-linux` (parsed from nextest's captured status
lines, `checks/rust-test-diagnostics.go`), `svelte-tests` (Vitest's `json` reporter, `checks/vitest-test-log.go`), and
both E2E lanes (Playwright's JSON report, `checks/e2e-test-log.go`). Every other check writes nothing.

**One mechanism, not six.** A lane's verdict travels as a `CheckResult` on the green path and an `error` on the red one,
so neither can carry per-test detail. `checks/test-log.go` is the side-channel: the runner hands each check its own
`TestRecorder` on a private copy of `CheckContext` (the lanes run concurrently, so one shared sink couldn't say which
lane a record came from), each lane calls `ctx.RecordTests(...)` BEFORE its pass/fail branch, and `logTestStats` in
`stats.go` drains it. Extend the `TestOutcome` vocabulary rather than growing a per-lane variant.

**Instrumentation never changes a verdict.** A missing or unparsable report records nothing and says nothing: no warn,
no failure, no note. The contention re-run (`checks/rust-test-contention.go`) is deliberately NOT recorded either, or a
starved test would look like it ran twice as often as it did.

**And it never invents a result.** The Playwright report paths are fixed (`/tmp/cmdr-e2e-report-<shard>.json`), so a run
that dies before writing one leaves the previous run's file sitting there; `recordPlaywrightTests` skips any report
older than the moment this run's Playwright started. The duration flagger needs no such guard, since it only runs on the
success path.

**Disabled by `--no-log` and `--ci`**, like the check-level log, so CI runners don't write a log nobody reads.

Example queries (the log is plain CSV; `duration_s` can be `N/A`, so cast defensively):

```bash
# Which tests fail most often, worst first
python3 -c "
import csv, collections
rows = [r for r in csv.DictReader(open('$HOME/cmdr-test-log.csv')) if r['status'] in ('fail', 'timeout')]
for (check, test), n in collections.Counter((r['check'], r['test_id']) for r in rows).most_common(15):
    print(f'{n:4}  {check:24}  {test}')
"

# Which tests are slowest (worst run ever seen per test)
python3 -c "
import csv, collections
worst = collections.defaultdict(float)
for r in csv.DictReader(open('$HOME/cmdr-test-log.csv')):
    if r['duration_s'] != 'N/A':
        key = (r['check'], r['test_id'])
        worst[key] = max(worst[key], float(r['duration_s']))
for (check, test), s in sorted(worst.items(), key=lambda kv: -kv[1])[:15]:
    print(f'{s:8.3f}s  {check:24}  {test}')
"
```

A failure RATE needs the run count as its denominator, which lives in the other log: count the rows for that check in
`~/cmdr-check-log.csv` over the same window.

## Input fingerprint cache

`pnpm check` re-runs a check IFF that check's inputs changed since it last passed. This unifies affected-only selection
and result caching in one baseline-free mechanism: agents can run `pnpm check` constantly and only pay for what they
touched.

**Mechanism (`plan.go` + `checks/fingerprint.go` + `checks/cache.go`):**

- Each check declares `Inputs` (path globs it reads) in `registry.go`; the shared sets live in `checks/inputs.go`, mined
  from ci.yml's `dorny/paths-filter` rules. Every check also carries the implicit `GlobalInputs` (`.mise.toml`,
  `scripts/check/**`): a toolchain bump or an edit to the runner's own source invalidates everything.
- Fingerprinting is git-aware and runs ONE repo-wide pass (`git ls-files -s` for index blob SHAs,
  `git status --porcelain -z` for the few dirty/untracked/deleted files, which are hashed from disk), then filters per
  check in-process. It never walks `node_modules/` or `target/`; the whole pass is well under a second.
- The fingerprint of a passing run is stored per check in `node_modules/.cache/cmdr-check-cache.json` (shares
  node_modules' fate, like the pnpm-install marker; atomic temp+rename write). A later run with the same fingerprint is
  a cache hit: reported as `OK (cached)` at ~0s, the pass's own message replayed for context.

**Invalidation:** any content change, add, or removal within a check's input set changes its fingerprint (the sorted
path list is hashed too, so adds/removes shift it). A formatter's auto-fix changes file contents, which changes OTHER
checks' fingerprints — correct and free, since fingerprinting is per-check at planning time.

**Exclusions (`!pattern`):** an Inputs entry starting with `!` takes matching paths back OUT of the set, whatever else
matched — including out of the GlobalInputs the check carries implicitly. `matchesAny` gives a veto priority over every
include, so appending a pattern can never quietly re-include something an exclusion took out. This is the one construct
that can make an input set too NARROW, which is the failure mode that ships a regression, so each one names files
nothing in the check's pipeline reads and carries the reasoning where it's declared.

The only exclusion is `agentDocExclusions` (`!**/CLAUDE.md` / `!**/DETAILS.md`), carried by `rustInputs` and
`svelteInputs`. Roughly 400 agent docs sit inside those trees and get edited on nearly every session by house rule, and
no code lane reads one: `TestRustInputsCoverEveryEmbeddedFile` proves no `include_str!` reaches a `.md`, nextest runs no
doctests, and `TestNoFrontendSourceLoadsAgentDocs` proves no frontend module imports one (Vite's `./X.md?raw` is the
form that would). The doc-scanning lanes take `wholeRepoInputs` and still see every edit.
`TestInputSetsExcludeOnlyAgentDocs` fails on any other `!` entry, in any shared set. Over the 1,439 commits of
2026-07-19..2026-08-12 the veto took the Rust lanes from 62% of commits to 54% and the 21 frontend lanes from 41.3% to
35.0%; `docs/notes/frontend-lane-cache-partitioning.md` has the frontend measurements and why a per-area split of
`svelte-tests` is not the next step. `ci-coverage` rule 4 validates an exclusion's static prefix the same as an
include's, so a stale one still fails.

**What's cached:** only `StatusOK` (not warn) results. Failures, warns, and skips always re-run AND drop any stale cache
entry. Warns aren't cached because warn-only checks are cheap and their messages are the product, not a verdict.

**Flags / escape hatches:**

- `pnpm check` is cache-aware by default (all lanes: `--fast`, `--include-slow`, `--only-slow`). `--include-slow` thus
  means "affected slow checks too".
- `--fresh` (or `CMDR_CHECK_NO_CACHE=1`) bypasses the cache: runs everything selected, then refreshes the entries.
- `--ci` always runs fresh and never writes the cache. **CI is the authoritative backstop against a wrong `Inputs`
  list** — a too-narrow `Inputs` can only mask a regression locally until the next CI run, never ship one.
- Explicitly NAMED checks (positional or `--check`) always run fresh, matching the existing "named ⇒ actually run"
  escape hatch. Group/app selectors stay cache-aware.

**Ordering (load-bearing):** planning happens BEFORE pnpm install and SMB/Docker bring-up, so a run whose node/SMB
checks are all cache hits never installs deps or starts a container. A cached dependency is absent from the run set, so
`canStart` treats it as satisfied (it passed on identical inputs). A corrupt or missing cache, or a non-git tree,
degrades to "run everything" — never an error.

**ci-coverage rule 4:** every static path prefix in a check's `Inputs` (and in `GlobalInputs`) must exist on disk, so a
renamed dir can't silently leave a check fingerprinting nothing (and thus cache-skipping real changes). It does NOT try
to reconcile `Inputs` against the ci.yml filter sets — that mapping isn't 1:1 and a strict reconciliation would be
flaky; CI-runs-fresh is the real correctness backstop.

## Output format

Each check outputs a single line:

```
Desktop: Rust / clippy... OK (1.23s) - No warnings
```

Status can be: `OK` (green), `warn` (yellow), `SKIPPED` (yellow), `FAILED` (red), `BLOCKED` (yellow).

## Troubleshooting

### Check is blocked

A check shows "BLOCKED" when its dependency failed. Fix the dependency first.

### Check needs a tool installed

Use `CommandExists()` to check if a tool is installed, and auto-install if possible via `EnsureGoTool`.

## Key decisions

**Decision**: CPU-weight-aware admission instead of a count semaphore. **Why**: The old gate allowed up to `NumCPU`
concurrent checks, but a single check (vitest, a cold cargo compile) can itself saturate every core. So the short
CPU-heavy checks all launched at once and oversubscribed the machine 2-3×, which starved timing-sensitive checks — the
E2E modal/popover timeouts and the 8s-cap `file_viewer` test flaked under `--include-slow` for exactly this reason. Each
check now carries a `CpuWeight` (avg busy cores, Docker-VM-aware) and the runner only starts a check when the running
weights fit the core budget. Wall-clock stays bounded by the critical path (the Docker E2E checks under
`--include-slow`; cold `clippy` for the default suite) while peak oversubscription drops to ~1×. Weights were measured
by an isolation sweep (`docs/notes/check-cpu-contention.md`); unmeasured/fast checks default to 1. The key insight from
the sweep: the longest checks (`e2e-linux`, `rust-tests-linux`) are NOT the heaviest — they idle ~1 core or run entirely
in the Docker VM, so they make ideal backbone fillers for the CPU-heavy short checks. (The sweep's original long pole,
`eslint-typecheck` at ~15 min, turned out to be a projectService batching cliff and was split into two ~15 s passes.)

**Decision**: positional selectors are the primary way to name checks; `--check` stays as an alias. **Why**: Task
runners idiomatically take targets as positional args (`make lint test`, `just fmt`, `turbo run lint build`);
`pnpm check oxfmt clippy` reads naturally where `--check oxfmt --check clippy` is ceremony. Resolution order per token:
check ID/nickname first, then app name, then tech group — and `ValidateCheckNames(reservedSelectorNames...)` fails
startup if a future check ID/nickname would shadow a group/app keyword, so the order can't silently change meaning.
Named checks keep `--check`'s escape-hatch semantics (implicitly include slow/CI-only); group and app selectors keep the
default lanes, matching their flag forms. `--check` survives because CI workflows, docs in the wild, and agent muscle
memory use it; the `ci-coverage` contract greps workflows for `--check <name>`, so workflows keep that form.

**Decision**: `check.sh` runs `go run .`, not `go run *.go`. **Why**: the `*.go` glob matches `_test.go` files, and
`go run` refuses test files, so the old form broke the moment the main package gained a test. `go run .` builds the
package and excludes tests by definition.

**Decision**: Go instead of Bash for the check script. **Why**: Cross-platform support (especially Windows), type-safe,
better error handling, and ability to build complex logic (parallel checks, dependency graph, colored output). Go is
already in the toolchain via mise.

**Decision**: `cargo-nextest` instead of `cargo test`. **Why**: Faster test execution (parallel by default), better
output formatting, clearer failure messages. Auto-installed by the check script if missing.

**Decision**: Auto-fix locally, check-only in CI. **Why**: Developers get instant fixes locally (less friction), CI
ensures code is properly formatted before merge. Controlled by the `--ci` flag. Formatters/linters fix files locally,
report only in CI. `runPrettierCheck` and `runESLintCheck` in `checks/common.go` handle both modes.

**Decision**: Skip `pnpm install` when lockfile is unchanged. **Why**: `pnpm install` takes ~20s and pegs all CPUs even
when deps haven't changed. A marker file (`node_modules/.pnpm-install-marker`) stores `pnpm-lock.yaml`'s mtime after
each successful install. On the next run, if the mtime matches, install is skipped. The marker lives inside
`node_modules/` so it's automatically invalidated if `node_modules` is deleted. Always runs in CI (`--ci`).

**Decision**: SMB Docker container lifecycle is owned by a runner-level orchestrator that holds a machine-wide lease,
not per-check and not per-process. **Why**: Multiple checks (`desktop-rust-integration-tests`, `desktop-e2e-linux`) need
the shared `smb-consumer` Docker Compose project. Two layers of contention had to be solved:

- _Intra-process_: each check used to own the lifecycle (start in entry, `defer ./stop.sh` in cleanup); two in one run
  raced each other. `SmbOrchestrator` (`scripts/check/smb_orchestrator.go`) lifts lifecycle one level up — at runner
  init, after `selectChecks()` resolves the planned set, it brings up the union of `NeedsSmb` modes (`SmbModeCore` for
  integration tests, `SmbModeE2E` for e2e) once, and tears down once at runner exit. Checks marked `NeedsSmb` assume the
  containers are up and call `waitForSmbContainers` as a cheap mid-run zombie-guard.
- _Cross-process / cross-worktree_: two `check.sh` runs (or a `check.sh` plus a manual `start.sh`) in different
  worktrees have independent orchestrators, so the in-process map can't stop them racing the same containers. The
  orchestrator therefore takes a **machine-wide lease** via the `smblease` library (holder-id = its own `check.sh` PID).
  `EnsureStarted` calls `smblease.Acquire` (adopt-or-reconcile under a flock); `Stop` calls `smblease.Release` (down
  only at zero holders, lock held across the down). The orchestrator imports the lib in-process — no subprocess —
  because it's already Go in the same module.

The standalone scripts (`start.sh`, `e2e-linux.sh::start_smb_containers`) take their **own** leases (`manual` for
`start.sh`, `$$` for `e2e-linux.sh`), so a manual run alongside a `check.sh` run just registers as a second holder and
neither tears the other's stack down. The SIGINT handler in `main.go` captures the orchestrator via shared variable so a
Ctrl+C also releases the lease (with a banner) before exiting 130. See [`smblease/smblease.go`](smblease/smblease.go)
for the lock/lease/policy model.

**Decision**: cmdr's SMB stack binds a dedicated host-port range (11480+), not smb2's default (10480+). **Why**: cmdr
runs a _vendored copy_ of smb2's `consumer` compose under its own project name (`smb-consumer`), while smb2's own test
harness runs the same compose under project `consumer` on 10480+. Same ports + different project = mutually exclusive: a
stack leaked by an interrupted smb2 run (its `Drop` teardown doesn't fire on SIGKILL) squats 10480+ and blocks every
cmdr `check.sh` with `port is already allocated`, cascading until manually cleaned. The orchestrator now calls
`checks.ApplySmbPortEnv()` (`checks/smb_ports.go`) before bring-up, shifting cmdr to 11480+ via smb2's existing
per-service env override. It flows by process-env inheritance — `docker compose up` (start.sh), the Rust integration
tests (`guest_port()` reads `SMB_CONSUMER_*_PORT`), and the macOS E2E app (`SMB_E2E_*_PORT`) all pick it up; the Linux
Docker E2E is unaffected (it talks to containers over the Docker network on internal `:445`, set explicitly in its
`docker run -e`). Net: cmdr and smb2's harnesses coexist, and smb2's defaults/`guest_port()` contract stay untouched so
every other smb2 consumer is unaffected.

**Decision**: those host ports publish to `127.0.0.1`, not all interfaces. **Why**: Docker's default binding is
`0.0.0.0`, so every bring-up put ~15 unauthenticated Samba servers (guest shares included) on the LAN and tailnet of
whoever ran the checks, for as long as the containers lived, which the lease model deliberately makes longer than one
run. The bind address is a `${SMB_BIND_ADDR:-127.0.0.1}` prefix on each `ports:` entry in smb2's compose (the vendored
source of truth), so it survives re-vendoring; setting `SMB_BIND_ADDR=0.0.0.0` restores the old behavior when a client
outside the Docker host needs in. No check lost anything: the Rust integration tests run on the host and reach
`localhost:11480+`, and the Linux Docker E2E was never using host ports (it joins `smb-consumer_default` and talks to
`:445` by container name). Don't add a `ports:` block to `.compose/docker-compose.override.yml` to change this: Compose
concatenates `ports` across files rather than replacing them, so the override collides on the host port instead of
rebinding it.

**Decision**: `IsFast` field on `CheckDefinition` and a curated `--fast` pre-commit lane. **Why**: A pre-commit run
should finish in ~10s so it actually gets used. The list is editorially curated, not derived from CSV timings: warm
average is what matters, but cold-cache outliers (`cargo-audit` spiking to ~3 min on advisory DB refresh) would silently
make the lane unreliable on the first run of the day. Mirrors the `IsSlow` / `CIOnly` field pattern (negative-sense
boolean default, same colocated style). Mutually exclusive with `--include-slow` / `--only-slow` to keep the semantics
unambiguous: "give me the fast lane" and "give me the slow lane" can't both be true. Named check invocations bypass the
filter (same escape hatch as `IsSlow` and `CIOnly`).

**Decision**: `CIOnly` field on `CheckDefinition` (mirrors `IsSlow` and `FreestyleIncompat`). **Why**: Keeps "this check
runs only in CI" colocated with the check definition rather than as a hardcoded list elsewhere. `FilterCIOnlyChecks` in
`registry.go` drops them outside `--ci`, with a named-check escape hatch so devs can verify locally before pushing.
Orthogonal to `IsSlow`: `--include-slow` and `--only-slow` do NOT pull in CI-only checks (you'd otherwise lose the
ability to run "all slow checks locally without the CI-only ones"). Negative-sense default (`false` = runs locally)
matches the other gating fields.

## Freestyle.sh remote execution

Two modes for offloading checks to a freestyle.sh VM:

- `--only-freestyle`: runs only freestyle-compatible checks on the VM, skips the rest entirely.
- `--prefer-freestyle`: runs freestyle-compatible checks on the VM and the rest locally, in parallel. This is the "run
  everything as fast as possible" mode: Rust checks run on your Mac while Node/Go checks run on the VM simultaneously.

**How it works:** Creates a temporary git commit of the full working tree (without modifying the local index/worktree),
pushes it to a temp branch, fetches on the VM, runs checks, cleans up the branch.

**What's freestyle-compatible:** Node/TS checks (Svelte, Astro, API server), Go checks, and metrics; any check without
`FreestyleIncompat: true`. The VM uses `--freestyle-remote` internally to filter to only these checks.

**What's not:** Rust checks (dep compilation exceeds freestyle's ~15 min API timeout) and Docker checks (no Docker
daemon on freestyle VMs). With `--prefer-freestyle` these run locally in parallel; with `--only-freestyle` they're
skipped.

**VM lifecycle:** The VM is created once (toolchain setup), then uses `persistent` storage so it survives freestyle's
resource management. It auto-suspends after 5 min idle but resumes in <1s. VM ID is stored in `.freestyle-vm-id`
(gitignored). On wake, a health check verifies the toolchain; if it fails, the VM is replaced. Setup parallelizes pnpm +
Playwright install and uses a shallow clone.

**Key files:** `freestyle.go` (all freestyle logic including `preferFreestyleRun`), `main.go` (`handleFreestyleFlags`
dispatches to the right mode).

**Decision**: `FreestyleIncompat` field on `CheckDefinition` instead of hardcoded check lists. **Why**: Keeps freestyle
compatibility co-located with each check's definition. Easy to flip when freestyle constraints change. Negative-sense
boolean means the Go zero value (`false`) = compatible, so only the few incompatible checks (Rust, Docker) need to opt
out.

**Decision**: Skip Rust checks entirely on freestyle (not just slow ones). **Why**: Freestyle's free tier has a hard ~15
min server-side timeout on `exec-await`. Compiling the full Tauri dependency tree (clippy, cargo-udeps, etc.) on 4 x86
vCPUs exceeds this. The 8 GB RAM also causes swap pressure when Rust and Node run in parallel. Attempted workarounds
(2-VM split, nohup background builds) all failed due to VM lifecycle issues (auto-suspend kills background processes,
`stopped` VMs lose disk state).

**Decision**: mise's standalone pnpm disabled on freestyle VMs. **Why**: The pnpm binary mise installs ships a baked-in
V8 snapshot that crashes on freestyle's x86 Linux VMs. We install pnpm via `npm install -g pnpm@10` instead, configured
via `[settings] disable_tools = ["pnpm"]` in `/root/.config/mise/config.toml`.

## Gotchas

**`--only-slow` needs ~20 min timeout.** Slow checks (E2E tests, `rust-tests-linux`) take significantly longer than the
default checks. When running `--only-slow` via an agent or CI, set the timeout to at least 20 minutes (1,200,000 ms).

**Concurrent SMB-touching runs across worktrees now coexist.** Two `pnpm check` invocations in different worktrees (or a
`check.sh` alongside a manual `start.sh` / `pnpm test:e2e:linux`) each take a machine-wide `smblease` lease and share
the same `smb-consumer` stack. Whichever finishes first releases its lease but sees a non-zero refcount, so it does
**not** down the stack — the other run keeps serving. The stack downs only when the last holder leaves. The old
`Cannot reach smb-consumer-X` cascade (one run's teardown killing another's mid-test) is the exact failure the lease
closes.

A leaked or lingering stack (a forgotten manual `start.sh`, or a numeric holder whose PID got recycled) is the benign
direction: it stays up until a human reaps it. Check state with `(cd scripts/check && go run ./smb-lease status)`; force
it down with `rm -rf /tmp/cmdr-smb-leases && apps/desktop/test/smb-servers/stop.sh`. See
`apps/desktop/test/smb-servers/README.md` § "Shared stack across worktrees" and `smblease/smblease.go`.

## Dependencies

`golang.org/x/term`, `golang.org/x/sys` (transitive). Go 1.25.
