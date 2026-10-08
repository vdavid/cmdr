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
```

## Command-line options

Positional args select what to run: check IDs/nicknames, app names (`desktop`, `website`, `api-server`, `scripts`), and
tech groups (`rust`, `svelte`, `go`), in any mix, space- or comma-separated, with flags anywhere in between
(`parseInterspersed` re-parses around positionals since Go's stdlib `flag` stops at the first one). Named checks run
even if slow or CI-only; app/group selectors keep the default lanes. `ValidateCheckNames` rejects any check ID or
nickname that would shadow a group/app keyword (`reservedSelectorNames` in `main.go`). A selector that matches nothing
prints `unknown check or group`, names the accepted spellings closest to it, and exits 1, leaving a row behind for the
naming review (§ "The unrecognized-name log").

- **`--app NAME`**: Run checks for specific apps (repeatable or comma-separated)
- **`--rust`, `--rust-only`**: Run only Rust checks (desktop)
- **`--svelte`, `--svelte-only`**: Run only Svelte checks (desktop)
- **`--check ID`**: Run specific checks by ID or nickname (same as naming them positionally)
- **`--ci`**: Disable auto-fixing (for CI)
- **`-v`, `--verbose`**: Print a line per check instead of the collapsed summary, and every check's output whole instead
  of an excerpt (details below); `--ci` implies it
- **`--include-slow`**: Include slow checks (excluded by default)
- **`--only-slow`**: Run only slow checks
- **`--fast`**: Run only the curated fast pre-commit check set
- **`--fresh`**: Bypass the input-fingerprint cache: run everything selected, then refresh it
- **`--fail-fast`**: Stop on first failure
- **`--no-log`**: Disable the CSV logs (all four of them) and the saved outputs
- **`--graph`**: Render the check dependency graph (weights + lanes + median wall-time) and exit
- **`--graph-format`**: Graph output: `tree` (default, colored terminal), `mermaid`, `dot`
- **`--docs-graph`**: Render the doc-discoverability tree (rooted at the repo-root `CLAUDE.md`) with per-doc usage, and
  exit
- **`--ensure-e2e-binary`**: Build the E2E binary unless the one on disk was built from this tree, print its path as the
  only stdout line (progress on stderr), and exit. The i18n screenshot run starts with it, so it can never launch a
  stale binary. Waits out a warming worktree, skips the main-clone guard (a build writes nothing tracked), and `--fresh`
  forces the compile. It shares `EnsureE2EBinary` with the Playwright lane: `checks/DETAILS.md` § "The Playwright lane's
  binary is fingerprinted"
- **`-h`, `--help`**: Show help message

`--graph` honors the same selectors (positional or flag form), so `pnpm check rust --graph` graphs only the Rust checks.
It renders before the slow/fast/CI filters, so every lane shows with its size badge. Each node also shows
`~<median wall-time>` from the recent (last 20) passing runs in the stats log, so the graph doubles as a perf dashboard
— pairing the CPU-weight (how heavy) with the typical duration (how long) for spotting the next optimization target.
Missing log (CI / `--no-log` / fresh machine) just omits the times. `mermaid` output pastes into a Markdown ```mermaid
block or https://mermaid.live; `dot` pipes to Graphviz
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

### Output budget and the full-output log

Quiet mode also caps what ONE check can print, because the same agent reading the run pays for every line of it. The
trigger was `i18n-coverage` on a branch between writing English and translating it: 19 keys × 10 locales printed 204
lines (14 KB) to say one thing.

- **The budget** (`output_log.go`): an output over 40 lines or 8 KB prints its first 20 and last eight lines, each
  clipped to 300 characters, then one closing line:
  `Output cut to fit (204 lines, 14,253 bytes in all); full output: <path> (or rerun with -v)`. Head and tail both,
  since errors lead and summaries (nextest's failure list) trail. Under budget, output prints exactly as before. `-v`
  and `--ci` never cap.
- **The saved text**: every failed or warning check, and any output that got capped, is written whole to
  `output/<timestamp>-<pid>-<check>.log` beside the CSV logs, headed by the check, verdict, time, and worktree. Names
  sort chronologically; `pruneOutputDir` keeps the newest 500. `--no-log` saves nothing, and the closing line then names
  only `-v`.
- **`output-log.csv`** (`timestamp,check,result,output_bytes,output_lines,printed_bytes,full_output`) has one row per
  check that RAN (cache hits and blocked checks print nothing of their own). `result` is `pass` / `warn` / `fail` /
  `skip`, a warning being its own label here, unlike in `check-log.csv`. `printed_bytes` is what reached stdout: 0 for a
  pass quiet mode hid, less than `output_bytes` for a capped one. `full_output` is the saved file, or empty.

**Decision**: fix a noisy check at the source before reaching for the budget. **Why**: the budget is a backstop that
keeps one lane from flooding a run; an excerpt still costs a round trip to the file when the cut part mattered. So
`i18n-locale-check-lib.ts` prints a finding several locales share once, under all of them (`groupSharedIssues`), which
took that same failure to 23 lines, well under budget.

Which checks are loudest when they fail, and how often they got cut:

```sh
sqlite3 -column -header :memory: '.import --csv ~/.local/share/check-runner/cmdr/output-log.csv o' \
  "select \"check\", count(*) as runs, max(cast(output_bytes as int)) as max_bytes,
   sum(cast(printed_bytes as int) < cast(output_bytes as int)) as capped
   from o where result in ('fail','warn') group by 1 order by 3 desc limit 15"
```

## Architecture

```
pnpm check [flags]
  -> scripts/check.sh [flags]
    -> ValidateCheckNames()          # startup: catch ID/nickname collisions + reserved-keyword shadows
    -> parseFlags()                  # flags + positional selectors (checks, apps, groups), interspersed
    -> findRootDir()                 # walk up to repo root
    -> selectChecks()                # filter AllChecks by flags
    -> applyLaneFilters()            # FilterSlow/CIOnly/Fast/onlySlow, in order
    -> planCache() (plan.go)         # input-fingerprint cache, BEFORE pnpm+SMB:
        CollectRepoFingerprintData() #   one repo-wide `git ls-files`+`git status` pass
          LoadRunnerSources()        #   parse the checks package: which runner files each check reaches
        per check: FingerprintFor()  #   hash its Inputs ∪ GlobalInputs ∪ its own runner sources
        split selected -> toRun / cached  # cache hit = entry fingerprint matches
    -> ensurePnpmDependencies()      # pnpm install once at root (skipped if all node checks cached)
    -> setupStackOrchestratorIfNeeded() # fixture Docker up only if a NON-cached check NeedsContainers
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
- **`stats.go`**: CSV stats logging: one row per check to `check-log.csv` (`logCheckStats`), plus one row per individual
  test to `test-log.csv` (`logTestStats`, § "The per-test log"); `logPath` resolves all three logs (§ "Where the logs
  live")
- **`output_log.go`**: the stdout budget per check (`capOutput`), the saved full outputs under `output/`, and the fourth
  CSV, `output-log.csv` (§ "Output budget and the full-output log")
- **`unknown_selector_log.go`**: the third CSV, one row per selector the runner didn't recognize
  (`unknown-check-log.csv`), plus the nearest-name guess it shares with the error message (§ "The unrecognized-name
  log")
- **`checks/test-log.go`**: the per-test vocabulary (`TestRecord`, `TestOutcome`, `TestRecorder`) every test lane
  records through
- **`plan.go`**: Input-fingerprint cache planning: splits selected checks into cache hits and misses BEFORE pnpm/SMB;
  records passes after the run
- **`checks/fingerprint.go`**: Git-aware content fingerprint per check (one repo-wide `git ls-files`+`git status` pass,
  filtered per check's Inputs)
- **`checks/cache.go`**: Per-worktree cache file load/save (`node_modules/.cache/cmdr-check-cache.json`), atomic write,
  corrupt-tolerant
- **`checks/inputs.go`**: Shared `Inputs` building blocks (mined from ci.yml filters), `GlobalInputs` (the runner core
  every check carries), and the `inputs()` concatenator
- **`checks/runner-sources.go`**: The per-check half of the runner's own inputs: parses the checks package and works out
  which implementation files each check's `Run` reaches (§ "The runner's own source")
- **`autofix_notice.go`**: Brackets the run with a `git status` snapshot and names, last, every committed file an
  auto-fixer rewrote (§ "The auto-fix notice")
- **`torn_target_notice.go`**: Scans failed lanes for an inconsistent `target/` and, when it finds one, says so last so
  the reader doesn't revert a good change (§ "The inconsistent-`target/` notice")
- **`colors.go`**: ANSI color constants
- **`utils.go`**: `findRootDir()` (walks up until `apps/desktop/src-tauri/Cargo.toml` is found)
- **`stack_orchestrator.go`**: Runner-level Docker fixture lifecycle: acquires a machine-wide lease per stack (via
  `stacklease`) at init, releases each at exit
- **`stacklease/`**: Library: the machine-wide flock + holder-id refcount that makes a shared fixture stack safe across
  worktrees. `registry.go` holds the registered stacks (`smb`, `sftp`, `webdav`, `s3`); `stack.go` the `Stack` value
  itself. `stacklease.go` keeps the core (`Acquire`, `decideAction`, `Reconcile`, `Release`, `PrintStatus`, service
  resolution); `log.go` the two log sinks and the `OnReconcileStart`/`OnTeardown` hooks; `confighash.go` the config-hash
  stamp and compare; `leases.go` the per-holder lease files and the dead-PID sweep; `keymaterial.go` the
  host-key-material heal/wait pair; `soloreset.go` the resets a lease runs when it starts alone; `lock.go` the flock,
  `compose.go` the real `Composer`
- **`stack-lease/`**: Thin `package main` CLI onto `stacklease` (`acquire`/`release`/`reconcile`/`status`, each taking
  the stack name first) that the bash scripts shell out to
- **`linux-cache/`**: `package main` CLI (`seed` / `promote`) that `scripts/worktree-hooks/` shells out to, handing the
  Linux lanes' target volume between the main clone and a worktree with the lanes' own names and labels from `checks`.
  Outside `checks/` because no check reaches it, so the cache has nothing to fingerprint it for
- **`checks/`**: One file per check, plus `common.go` (shared utils) and `registry.go` (the `AllChecks` ordered list)

## Runner-level patterns

**The warm-start interlock:** `awaitWorktreeWarming` (`utils.go`) runs right after the main-clone guard and blocks while
`<root>/.warming-worktree` exists.

`~/.claude/scripts/new-worktree.sh` warms in two stages, because `cp -Rc` is per-file and that clone is ~100 s of the
run (measured 2026-09-07). It BLOCKS ~22 s to finish every `node_modules/`, then clones `target/` in a detached
background job. The sentinel exists for exactly the life of that job, carries its pid, and gains a `node_modules=done`
line when the first stage ends; `.warming-worktree.log` holds its output.

The wait lives here rather than in a note to the reader because `pnpm check` is the only sanctioned way to build in this
repo, so one blocking call covers every lane and no agent has to remember anything.

**It cannot cover `node_modules`, which is why the script blocks on that stage instead.** `pnpm check` runs pnpm's own
dependency verification BEFORE `scripts/check.sh` starts, so `awaitWorktreeWarming` has already been overtaken by the
time it runs: pnpm sees a half-cloned `node_modules`, judges it stale, and (with our `confirmModulesPurge: false`)
reinstalls it over the copy in flight without prompting. Measured 2026-09-09: 42.7 s reinstalling 767 packages on top of
a live `cp -Rc`. The runner being pure Go (`go run .`) keeps `check.sh` itself off `node_modules`, but the pnpm wrapper
in front of it is not ours to gate.

**Two runs in ONE worktree race in front of the runner, and the failure names an unrelated file.** `pnpm check` goes
through `pnpm install` first, whose `prepare` step runs `svelte-kit sync` per app; two of those walking the same
`.svelte-kit/types/` tree have one deleting a generated file while the other stats it, so the install dies with an
`ENOENT` on a path that is present a second later and the whole run fails before a single check starts (measured
2026-09-16, three agents sharing a worktree). Nothing gates it: the lease in `stacklease/` covers Docker fixtures across
worktrees, and the cache is per-worktree, but the pnpm wrapper in front of `check.sh` is not ours to serialize. So when
a lead fans agents out into one worktree, the lead runs the lanes; ❌ don't have several agents run them at once. A run
in a DIFFERENT worktree is fine, since each has its own `node_modules` and `.svelte-kit`.

A sentinel whose pid is dead is treated as "proceed, and say so", never "wait forever": the runner reports it, removes
the marker, and continues. A half-cloned `target/` is safe to build on, since cargo treats a missing artifact as one
needing a rebuild, so the worst case is a colder build. Holding the interlock on a dead worker would instead break every
future run in that worktree. Signal 0 is the liveness probe, with `EPERM` counted as alive.

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

**Mothballing a check:** `CheckDefinition.Disabled` holds the REASON a check stopped gating anyone, and
`FilterDisabledChecks` (first in `applyLaneFilters`, so every later lane sees a set it has already left) drops it from a
bare `pnpm check`, an app or tech group, `--fast`, `--include-slow`, `--only-slow`, and `--ci` alike. Naming it
(`pnpm check <id>`) still runs it, and `--help` lists it as `(disabled, run by name only)`.

**Decision**: no flag re-enables the disabled set in bulk, unlike `--include-slow` for slow lanes. **Why**: a bulk flag
would put a mothballed check back into somebody's routine run, which is the exact thing disabling it was meant to stop;
and a check worth running as a group again is a check worth un-disabling in the registry, where the reason is reviewed.
The asymmetry is the point.

The check keeps its code, its tests, and its allowlist, so re-enabling is deleting one field. A disabled check must
clear `IsFast` / `IsSlow` / `CIOnly` and carry a `NotInCI` reason (`TestDisabledChecksClaimNoLane`): a leftover lane
flag reads as "runs there" and would come back the moment `Disabled` is lifted. ❌ Never reach for `Disabled` to quiet a
check that's going red — fix the check or the code. It's for a lane we've decided not to gate on.

Mothballed today: `invariant-density` (a `❌` count can't tell a rule that earns its place from one that doesn't, so it
warned on guardrails we wanted; the table is still worth reading deliberately via `pnpm check invariant-density`).

**Exclusive resources:** `CheckDefinition.Exclusive` names a resource a check needs to itself; two checks naming the
same one never overlap, whatever the weight budget allows. It sits between the dependency gate and the weight gate in
`tryStartPending`, and the holder releases it on every exit path (deferred), so a red or panicking check can't strand
every other lane. It can't deadlock: a check takes at most one resource and holds it only between its own start and
finish.

The one resource today is `ResourceCargoBuildDir`, held by every lane that COMPILES against the shared `target/`
(`clippy`, `rust-tests`, `integration-tests`, `bindings-fresh`, `cargo-udeps`, the `<provider>-smoke` lanes). Cargo
takes an exclusive lock on its build directory for a whole command, so those lanes were always serial; undeclared, the
loser sat on `Blocking waiting for file lock on build directory` while still holding 6-8 weight, so a quiet run looked
hung and the reserved cores went unused. Declaring it costs no wall clock and hands that weight back. Metadata-only
commands (`cargo metadata`, `about`, `deny`, `machete`) take the package-cache lock instead and stay undeclared;
`clippy-linux` and `rust-tests-linux` build on their per-worktree Docker volume (one `DependsOn` the other, so they
never overlap), and `rustdoc` owns a private target dir. Measurements: `docs/notes/check-cpu-contention.md` § "Cargo's
build-directory lock".

**Slow checks:** `IsSlow: true` marks checks excluded by default (currently: `clippy-linux`, `rust-tests-linux`,
`desktop-e2e-linux`, `desktop-e2e-playwright`, `desktop-rust-webdav-nextcloud`, `desktop-rust-disk-images`). Naming a
check (positionally or via `--check`) implicitly includes slow checks (`includeSlow = len(checkNames) > 0`); group/app
selectors don't.

**Fast lane (`--fast`):** `IsFast: true` marks the curated pre-commit check set: ~28 checks that finish in roughly 10s
on a warm cache, intended to run before every commit. It's an editorial pick, not a timing-derived list (see Key
decisions below). Named check invocations bypass the filter so `pnpm check --fast svelte-check` still runs svelte-check.
Mutually exclusive with `--include-slow` / `--only-slow` — combining them errors out, since the lanes are intentionally
separate.

**CI-only checks:** `CIOnly: true` marks checks that run only in `--ci` mode (currently `cargo-udeps`, `jscpd-rust`, and
the four real-API `<provider>-smoke` lanes). They're silently dropped from local runs (no SKIPPED line) and are not
pulled in by `--include-slow` or `--only-slow`. Escape hatch: an explicit `pnpm check cargo-udeps` always runs, so you
can verify locally before pushing.

**Self-contained E2E checks:** `desktop-e2e-playwright` manages the full lifecycle (build the binary, create per-shard
fixtures, start N Tauri instances, run N Playwright processes in parallel, cleanup). The build is fingerprinted and
skipped when the binary on disk already matches the tree it would be built from, which is worth 172 s per run because
the build isn't incremental (`checks/DETAILS.md` § "The Playwright lane's binary is fingerprinted"). Each shard runs in
its own isolated `CMDR_DATA_DIR` with its own Unix socket and MCP port (asked of the OS per run, never a fixed base:
`checks/DETAILS.md` § "Nothing a shard owns is shared between runs"), plus a per-shard `CMDR_INSTANCE_ID` of the form
`e2e-<short>-<pid>` (for example, `e2e-mtp-12345`, `e2e-nonmtp1-12345`). The instance ID drives the macOS Keychain
`SERVICE_NAME` suffix (`Cmdr-e2e-<short>-<pid>`) so two parallel shards can never collide on credentials, and reshapes
the Dock label into `Cmdr (E2E <short>)` so cleanup scripts can target with `pgrep -f 'Cmdr (E2E '`. One shard is
dedicated to MTP specs (serialized; the run's virtual MTP backing dir at `/tmp/cmdr-mtp-e2e-fixtures-<pid>` is shared by
every Tauri instance in that run). Per-shard logs go to `/tmp/cmdr-e2e-playwright-<shard>-<timestamp>-<pid>.log`, and a
week-old leftover is swept at lane start. See `docs/tooling/instance-isolation.md` § "How E2E gets isolated per shard".

`RUST_LOG` is forwarded to the app (via inherited `os.Environ()`), so trace-level output is one shell-prefix away:

```bash
RUST_LOG=cmdr_lib::file_system::volume::mtp=trace pnpm check desktop-e2e-playwright
```

The chosen `RUST_LOG` value is echoed at the top of the timestamped log so it's obvious from a glance which level was
captured. When unset, the log starts with `=== RUST_LOG unset (default warn level) ===`.

After a successful run, both E2E checks flag (warn-only) any individual test that took more than 2 s wall-clock, against
a per-platform allowlist. See `checks/DETAILS.md` § "E2E test duration flagger".

**TTY detection:** `golang.org/x/term.IsTerminal` gates the live status line; CI logs stay clean.

**Where the logs live:** all four, and the `output/` dir of saved check outputs, sit in
`~/.local/share/check-runner/cmdr/` (`logPath` in `stats.go`, honoring `$XDG_DATA_HOME`), so the rest of this file names
them by bare filename. They're deliberately outside the repo: the history spans years and every worktree, and a worktree
teardown must not take it along. The `check-runner/` parent is shared with the copies of this runner living in other
repos, each under its own project subdirectory, which is why `projectLogDirName` is the only token those copies have to
change.

**CSV stats logging:** Each check run appends a row to `check-log.csv` with timestamp, app, check name, duration, result
(pass/fail/skip/blocked/cached), and optional counts (total, issues, changes). `CheckResult` has `Total`, `Issues`,
`Changes` fields (`-1` = N/A, rendered as `N/A` in CSV). Disabled by `--no-log` or `--ci`. Implementation in `stats.go`.
A cache hit logs as `cached` (not `pass`) so `--graph`'s median, which counts only `pass` rows, isn't dragged down by
~0s hits.

## The per-test log

`test-log.csv` records INDIVIDUAL tests, one row each, so "which 15 tests cause most of my red runs, and which are
slowest" is a query rather than an archaeology dig. It's the companion to `check-log.csv`, which stays lane-level: a red
lane logs the message `rust tests failed` there and names no test, which is exactly the gap this closes.

**Two files, never one.** `check-log.csv` is past 270 000 rows against a nine-column header, and every reader of it
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
lines, `checks/rust-test-diagnostics.go`), all three Vitest lanes — `svelte-tests`, `api-server-tests`,
`dashboard-tests` (Vitest's `json` reporter, `checks/vitest-test-log.go`) — and both E2E lanes (Playwright's JSON
report, `checks/e2e-test-log.go`). Every other check writes nothing.

**One mechanism, not six.** A lane's verdict travels as a `CheckResult` on the green path and an `error` on the red one,
so neither can carry per-test detail. `checks/test-log.go` is the side-channel: the runner hands each check its own
`TestRecorder` on a private copy of `CheckContext` (the lanes run concurrently, so one shared sink couldn't say which
lane a record came from), each lane calls `ctx.RecordTests(...)` BEFORE its pass/fail branch, and `logTestStats` in
`stats.go` drains it. Extend the `TestOutcome` vocabulary rather than growing a per-lane variant.

**Instrumentation never changes a verdict.** A missing or unparsable report records nothing and says nothing: no warn,
no failure, no note. The contention re-run (`checks/rust-test-contention.go`) is deliberately NOT recorded either, or a
starved test would look like it ran twice as often as it did.

**And it never invents a result.** Report paths are run-scoped (`/tmp/cmdr-e2e-report-<shard>-<pid>.json`), so no
concurrent suite can answer for this run; `recordPlaywrightTests` additionally skips any report older than the moment
this run's Playwright started, which covers a recycled pid landing on a leftover the sweep hasn't collected. The
duration flagger needs no such guard, since it only runs on the success path.

**Example queries.** Use `sqlite3` (ships with macOS) rather than `awk` or `cut`: a `test_id` contains commas, quoted
per RFC 4180, and every field-splitting one-liner gets those rows wrong. Importing the whole file takes well under a
second. `check` is a keyword, so it needs the double quotes.

Which tests cost the most red runs:

```sh
sqlite3 -column -header :memory: '.import --csv ~/.local/share/check-runner/cmdr/test-log.csv t' \
  "select test_id, count(*) as red_runs from t
   where status in ('fail','timeout','leak','flaky') group by 1 order by 2 desc limit 15"
```

Which are slowest (worst single attempt ever seen, and how often it ran):

```sh
sqlite3 -column -header :memory: '.import --csv ~/.local/share/check-runner/cmdr/test-log.csv t' \
  "select \"check\", test_id, round(max(cast(duration_s as real)),1) as worst_s, count(*) as runs
   from t where duration_s <> 'N/A' group by 1,2 order by 3 desc limit 15"
```

Narrow either to a window with `and timestamp >= '2026-08-01'`, or to one lane with `and "check" = 'rust-tests'`.
Remember what "absent" means here: fast, or never ran. Never "passed".

**Disabled by `--no-log` and `--ci`**, like the check-level log, so CI runners don't write a log nobody reads.

Example queries (the log is plain CSV; `duration_s` can be `N/A`, so cast defensively):

```bash
# Which tests fail most often, worst first
python3 -c "
import csv, collections
rows = [r for r in csv.DictReader(open('$HOME/.local/share/check-runner/cmdr/test-log.csv')) if r['status'] in ('fail', 'timeout')]
for (check, test), n in collections.Counter((r['check'], r['test_id']) for r in rows).most_common(15):
    print(f'{n:4}  {check:24}  {test}')
"

# Which tests are slowest (worst run ever seen per test)
python3 -c "
import csv, collections
worst = collections.defaultdict(float)
for r in csv.DictReader(open('$HOME/.local/share/check-runner/cmdr/test-log.csv')):
    if r['duration_s'] != 'N/A':
        key = (r['check'], r['test_id'])
        worst[key] = max(worst[key], float(r['duration_s']))
for (check, test), s in sorted(worst.items(), key=lambda kv: -kv[1])[:15]:
    print(f'{s:8.3f}s  {check:24}  {test}')
"
```

A failure RATE needs the run count as its denominator, which lives in the other log: count the rows for that check in
`check-log.csv` over the same window. Before quoting an E2E rate, read `docs/notes/e2e-flake-remeasured-2026-08-14.md`:
it's the worked version of exactly that query, and it names the three traps (a run-level rate is dominated by the
suite's width rather than by any test, the macOS lane's zero-retry config makes its rate incomparable to Linux's, and a
few days of runs can't distinguish 41% from 59%).

## The unrecognized-name log

`unknown-check-log.csv` records every invocation the runner rejected with `unknown check or group`, one row per
unrecognized name. The rows are evidence for a NAMING review: a name someone reached for and didn't get is the cheapest
available signal that a check is called something other than what people call it. Collect for about a month, look for
patterns, then rename or regroup whatever keeps getting missed. The run itself behaves as it always has, printing the
error and exiting 1.

**Schema** (`timestamp,unknown,args,did_you_mean`):

- `timestamp`: `YYYY-MM-DD HH:MM:SS`, the same shape as the other two logs, and identical across the rows of one
  invocation.
- `unknown`: the rejected token verbatim, case preserved; only the surrounding whitespace the comma splitter strips is
  gone.
- `args`: the whole argument list as typed, space-joined. What someone got RIGHT is half the signal: `clipy --fast` says
  the flags landed and only the name missed.
- `did_you_mean`: the runner's guess, space-separated, up to three names, closest first. Empty when nothing was close,
  which is itself a finding (a name from a wholly different vocabulary).

**How the guess works** (`suggestSelectors` in `unknown_selector_log.go`): Levenshtein distance against every accepted
positional name (each check's ID and nickname, plus the app and tech-group keywords in `reservedSelectorNames`), inside
a budget of `len/2` edits clamped to 1..3, so three edits from a four-character token doesn't count as a typo. A typed
name that's a FRAGMENT of an accepted one ranks as a near-miss whatever its distance, which is the shape most misses
take (`rust-test` for `rust-tests`, `e2e` for `desktop-e2e-playwright`). Ties break toward the shorter name. The same
guess goes into the user-facing error as a `Did you mean …?` line.

**Third file, never merged into the other two**, for the reason § "The per-test log" gives: a field-count change
destroys a long CSV history in place, and "one rejected name" is its own data model anyway.

**Disabled by `--no-log` and `--ci`**, like both other logs. `parseFlags` returns no `cliFlags` on its error path, so
the flag rides along on the `unknownSelectorError` itself; `main` (not `parseFlags`) does the write, keeping the parser
free of side effects and keeping `go test` out of the real log. A write failure is silent, always.

Which names people reach for and don't get:

```sh
sqlite3 -column -header :memory: '.import --csv ~/.local/share/check-runner/cmdr/unknown-check-log.csv u' \
  "select unknown, did_you_mean, count(*) as misses from u group by 1,2 order by 3 desc"
```

## Input fingerprint cache

`pnpm check` re-runs a check IFF that check's inputs changed since it last passed. This unifies affected-only selection
and result caching in one baseline-free mechanism: agents can run `pnpm check` constantly and only pay for what they
touched.

**Mechanism (`plan.go` + `checks/fingerprint.go` + `checks/cache.go`):**

- Each check declares `Inputs` (path globs it reads) in `registry.go`; the shared sets live in `checks/inputs.go`, mined
  from ci.yml's `dorny/paths-filter` rules. Every check also carries the implicit `GlobalInputs` (the toolchain pin plus
  the runner CORE) and, on top of that, the runner implementation files its own `Run` reaches (see "The runner's own
  source" below).
- Fingerprinting is git-aware and runs ONE repo-wide pass (`git ls-files -s` for index blob SHAs,
  `git status --porcelain -z` for the few dirty/untracked/deleted files, which are hashed from disk), then filters per
  check in-process. It never walks `node_modules/` or `target/`; the whole pass is well under a second.
- The fingerprint of a passing run is stored per check in `node_modules/.cache/cmdr-check-cache.json` (shares
  node_modules' fate, like the pnpm-install marker; atomic temp+rename write). A later run with the same fingerprint is
  a cache hit: reported as `OK (cached)` at ~0s, the pass's own message replayed for context.
- `recordRun` re-fingerprints at the end of the run (one more git pass, only when something passed) and records a pass
  only if its inputs are unchanged since planning. Otherwise a file edited mid-run would store its pass under the OLD
  content's key, and an A→B→A edit would later hit the cache on content that never ran.
- `recordRun` reloads the cache file just before saving and merges this run's verdicts per check ID, so two concurrent
  unnamed runs in one worktree neither clobber each other's passes nor resurrect an entry the other dropped.

**Invalidation:** any content change, add, or removal within a check's input set changes its fingerprint (the sorted
path list is hashed too, so adds/removes shift it). A formatter's auto-fix changes file contents, which changes OTHER
checks' fingerprints — correct and free, since fingerprinting is per-check at planning time.

**Exclusions (`!pattern`):** an Inputs entry starting with `!` takes matching paths back OUT of the set, whatever else
matched — including out of the GlobalInputs the check carries implicitly. `matchesAny` gives a veto priority over every
include, so appending a pattern can never quietly re-include something an exclusion took out. This is the one construct
that can make an input set too NARROW, which is the failure mode that ships a regression, so each one names files
nothing in the check's pipeline reads and carries the reasoning where it's declared.

The only exclusion is `agentDocExclusions` (`!**/CLAUDE.md` / `!**/DETAILS.md`), carried by the Rust blocks and
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

### The runner's own source

The runner is an input to every check it runs, but not all of it is an input to all of them. Editing
`checks/desktop-rust-lock-poison.go` cannot change what `cargo nextest` reports. The split is in two halves.

**The core (`GlobalInputs`, in `inputs.go`)** is what no analysis can attribute, so every check carries it:
`scripts/check/*.go` (package `main`: the executor, the cache plan, the status line, the stats logs, the Docker
orchestrator), `registry.go`, `inputs.go`, `fingerprint.go`, `cache.go`, `runner-sources.go`, `common.go`,
`test-log.go`, `fixture-stacks.go`, `smb_ports.go`, `sftp_ports.go`, plus `go.mod`, `go.sum`, `scripts/check.sh`, and
`.mise.toml`. Package `main` imports `checks` and never the reverse, so nothing a check reaches can reveal that the
EXECUTOR uses a symbol; `TestRunnerCoreCoversWhatTheExecutorReaches` walks package `main`'s `checks.X` references and
fails when one resolves to a non-core file. Two names are exempt with a reason at the declaration site:
`NightlyToolchain` (`--print-nightly`) and `BuildDocGraph` / `DocGraph` (the `--docs-graph` renderer), neither of which
runs during a check.

**The per-check half (`runner-sources.go`)** is derived from the AST at plan time, because the whole runner is ONE Go
package and package-level analysis buys nothing. Two rules, no type information:

- A declaration reaches every package-level NAME it mentions as an identifier. Selector names (`x.Foo`) are deliberately
  not resolved by name: half the package has a `String` method, and matching on the method name alone made every check
  reach every file.
- Reaching a TYPE reaches every method declared on it, wherever those live. That stands in for the type information: a
  value can only exist if something in the closure names its type, and its behavior then travels with it. Not
  theoretical: `invariant-density` reaches `docs-dead-links.go` ONLY through a method, and drops it the moment that rule
  is removed.

An `init()` is attributed to the package-level variables it assigns, so a check reading one of those reaches the init's
file. An `init()` that does anything else (registering into somebody else's table, the shape file-level analysis cannot
see) makes the analysis give up rather than answer.

❗ **It fails closed.** A parse error, an unreadable `AllChecks`, a `Run` it can't resolve, or an unattributable
`init()` drops EVERY check back to `scripts/check/**` — the pre-attribution behavior. A too-wide input set costs cache
speed; a too-narrow one reports a green describing code it never ran, which is the failure mode that has shipped here
twice (`CHANGELOG.md` missing from the shared Rust set, then from `desktopAppInputs()`). A synthetic definition with no
registered ID (the E2E build's) gets the same wide answer.

**What it can't prove**, and what covers each gap: package `main`'s own internals (nothing in `checks` can reference
them, so all of `scripts/check/*.go` is core by policy, renderers and stats included); a symbol the executor reaches
into the checks package for (`TestRunnerCoreCoversWhatTheExecutorReaches`); a file no `Run` reaches at all
(`TestGlobalInputsCoverWhatNoCheckCanReach`); and dispatch through `reflect`, which would defeat name-based analysis
entirely. Nothing in the package imports `reflect` outside its tests today, and a check that needed to would have to put
its file in the core set.

The analysis runs once per `pnpm check`, inside `CollectRepoFingerprintData`: ~30 ms for all 116 checks (parsing ~130
files), against a planning budget already spending two git forks (measured 2026-08-23, `LoadRunnerSources` timed on this
worktree).

Two things live outside what any source analysis here can reach, so the check that uses one names it in `Inputs`:

- The allowlists and baselines beside the checks are DATA, read through a path built at runtime. The eight checks that
  own one name it through `runnerDataInputs`, and `TestAllowlistFilesAreFingerprintedByTheirCheck` fails both on an
  allowlist its check doesn't fingerprint and on one nothing watches at all.
- Three checks shell out to a helper program BESIDE the runner (`scripts/check-css-unused`, `check-a11y-contrast`,
  `check-btn-restyle`), whose rules are in a different Go module. `siblingToolInputs` names those dirs, and
  `TestSiblingToolDirsAreFingerprintedByTheirCheck` is what found all three cache-skipping their own rule engines: a
  `scripts/check/**` global never matched `scripts/check-css-unused/**`, so editing a rule left the lane it governs on a
  cached pass.

**Measured on this worktree with a warm cache** (110 default lanes, 80 in `--fast`), by lanes whose fingerprint changes;
`docs/notes/rust-lane-input-narrowing-2026-08-23.md` § "Closing the runner's own residue" has the wall clock and the
harness that reproduces the counts.

| Edit                                       | Lanes | What they are                                                             |
| ------------------------------------------ | ----: | ------------------------------------------------------------------------- |
| one leaf check implementation (`.go`)      |    26 | 10 Go + 12 whole-repo doc/metric + 2 registry readers + the 2 real owners |
| one core runner file (`runner.go`)         |   110 | a `GlobalInput`, by construction                                          |
| `.mise.toml`                               |   110 | same                                                                      |
| a sibling tool (`check-a11y-contrast/`)    |    23 | 10 Go + 12 doc/metric + the one lane that runs it                         |
| a JSON allowlist beside the checks         |    14 | 12 doc/metric + `misspell` + `go-tests`                                   |
| a `CLAUDE.md` / `DETAILS.md` in `scripts/` |    14 | same                                                                      |

**The leaf `.go` case is where it stops, and that's the honest answer.** All 26 lanes read the edited file:

- The **10 Go lanes** lint and test the runner. Editing a runner source is exactly their business. The narrowing still
  available here was the other direction, an edit to a NON-Go file in those trees, which is what `goSourceInputs` closed
  (`checks/DETAILS.md` § "The Go lanes split three ways"): 22 lanes down to 14.
- The **12 whole-repo lanes** (`file-length`, `oxfmt`, the five `docs-*`, the three `claude-md-*`, `invariant-density`,
  `resident-doc-budget`) take `wholeRepoInputs` because their domain IS the whole repo: a `.go` edit changes a line
  count, and `docs-dead-links` resolves doc links against real paths, so an add or a remove anywhere can flip it. They
  total ~11 s of mean runtime across 12 parallel lanes; ❌ don't reach for a "paths-but-not-contents" input kind to
  shave that, the mechanism would cost more than the lanes do.

### Scoped runs

Those 12 lanes are registered `AppOther`, which means no scoped selector reaches them: `pnpm check svelte`, `rust`, and
`desktop` all skip formatting, `file-length`, and every `docs-*` / `claude-md-*` lane. Only a bare `pnpm check`, or
naming a lane outright (`pnpm check oxfmt`), runs them.

That matters more than it sounds, because of how the formatters behave in the two environments: a local run REWRITES the
tree, while CI only CHECKS (`--ci`). So a scoped lane can go green over a file the formatter would have silently fixed,
and CI then fails on it. Measured on 2026-09-21: a 125-character line in `routes/viewer/DETAILS.md` survived a green
`pnpm check svelte` and was only caught by a bare `pnpm check`, which reflowed it along with ten other files.

The rule that falls out: a scoped run is for iterating, never for finishing. Match the scope to the change while you
work (`.claude/rules/check-scope-matches-change.md`), then close the unit of work with a bare `pnpm check`.

- The **2 registry readers** (`ci-coverage`, `workspace-member-coverage`) reach every check file because `AllChecks`
  names every `Run` function, and the closure can't tell "reads the registry as data" from "calls it". Both are ~0.01 s.

### The Rust input blocks

The Rust lanes don't share one set. `inputs.go` declares blocks, and each lane names the ones its own verdict depends
on:

- `rustMemberTrees` is one entry per cargo workspace member: package name, `MemberKind`, and the glob covering its tree.
  It's hand-written because `Inputs` is static registry data with no repo root in hand;
  `TestRustMemberTreesMatchTheWorkspace` pins it to the real manifests in both directions, so a new crate can't land
  outside every lane's view.
- `rustCompileInputs` — every member's tree plus `Cargo.toml` / `Cargo.lock` / `rust-toolchain.toml` plus
  `rustEmbeddedInputs`. What a lane that runs cargo over the whole workspace reads. Lanes add their own tool config on
  top (`clippy.toml` for clippy, `rustfmt.toml` for rustfmt, `deny.toml` for cargo-deny, `pnpm-lock.yaml` for
  `bindings-fresh`, the fixture-server dirs for `rust-integration-tests`).
- `rustScanInputs(kinds…)` — the member trees of those kinds, for a source scanner. A scanner passes the SAME kinds it
  declares in `rustScannerJurisdictions`, so its cache key and the trees it walks come from one decision;
  `TestScannerInputsMatchTheirJurisdiction` fails when they disagree either way.
- `rustAppTreeInputs` — for the scanners whose jurisdiction is `AppTreeOnly`. A crate edit can't change their verdict.
- `rustWorkspaceConfigInputs` alone is enough for `cargo-audit` and `cargo-deny`: both answer a question about the
  lockfile, not about anybody's sources.

❌ `tools/**` is not in any of them. `tools/intellij-plugin` and `tools/privatesize-poc` are outside the cargo
workspace, so no Rust lane compiles or scans either one; carrying them cost 22 commits a full Rust battery plus both E2E
suites for nothing.

**`TestRustInputsCoverEveryEmbeddedFile` is what makes narrowing safe.** It walks the WHOLE registry and pairs each
check against each `include_str!` / `include_bytes!` site individually: a lane that covers the embedding source must
cover the embedded file. So a lane narrowed to one crate stops owing what other crates embed, while any lane still
covering the app tree still owes `CHANGELOG.md`. Generalizing it from one set to the registry caught a live hole in the
two E2E lanes on the first run.

❗ **The cargo lanes stay on `--workspace`, and per-package `-p` lanes are closed.** The app crate depends on all five
library crates, so a crate edit legitimately invalidates the app and cargo's incrementality already limits the rebuild;
a `-p` lane resolves features differently from the workspace build it shares `target/` with, and would compile `cmdr-fs`
without the `testing` feature every other crate's tests are built on. Numbers, and the one place a real win is still
sitting: `docs/notes/rust-lane-input-narrowing-2026-08-23.md`.

## Output format

Each check outputs a single line:

```
Desktop: Rust / clippy... OK (1.23s) - No warnings
```

Status can be: `OK` (green), `warn` (yellow), `SKIPPED` (yellow), `FAILED` (red), `BLOCKED` (yellow).

### The auto-fix notice

The formatters (`oxfmt`, `rustfmt`, prettier, `eslint --fix`) rewrite the tree on a local run and only CHECK under
`--ci`. So a reformat that lands AFTER the last commit reads green locally and red in CI: the working tree holds the
fix, the commit doesn't. That reddened CI on 2026-08-18, and the per-check `SuccessWithChanges` line was too easy to
miss in a fifty-check run.

`runChecks` snapshots `git status --porcelain -z` before the run and again after, and prints the set difference as the
LAST line of the run, on both the green and the red path: "This run rewrote N committed files … Commit:", each named.

**Decision**: the notice names only files that were clean when the run started. **Why**: mid-edit files are already
dirty and always will be, so listing every modification would be noise a reader learns to skip. The set difference is
exactly "an auto-fixer touched something that was committed", which is exactly the state that fails CI. Skipped under
`--ci`, where nothing auto-fixes, and silent outside a git work tree.

### The inconsistent-`target/` notice

`target/` can hold fingerprints from one moment and `.rlib`s from another. Cargo trusts the fingerprint, rustc gets an
artefact that disagrees with it, and the run dies on crates nobody touched: `error[E0463]` (can't find the crate) and
`error[E0460]` (found one that disagrees with what a dependent was built against), pointing into
`~/.cargo/registry/src/`.

The usual cause is a fresh worktree. `cp -Rc` walks `target/` for ~80 s, so anything compiling in the main clone during
that walk lands in the copy twice, at two versions. An interrupted build, a killed `cargo-sweep`, or a disk-full write
give the same shape. The commit-staleness guard in `new-worktree.sh` doesn't catch it: the commits agree, and the clone
goes ahead.

`printTornTargetNotice` scans every failed lane's output and prints last, after the auto-fix notice, naming the
implicated registry packages and the recovery: `cargo clean --profile dev`. It cost a session ~15 minutes of diagnosis
on 2026-09-09 and nearly cost a correct dependency change, which is the harm it exists to prevent: the wall of compiler
output arrives right after whatever the author last edited, so reverting a good change is the obvious next move.

**Decision**: fires only when BOTH an `E0460`/`E0463` code and a registry path are present. **Why**: the code alone
fires on an author's own missing crate, which must stay reported as their mistake; a registry path alone shows up in
ordinary output constantly (a deprecation inside a dependency, a backtrace frame). Together they mean the unreconcilable
crate was built from source the author never edited, so their change can't be the cause. Matched on the error CODE, a
stable identifier, never on the sentence around it (same reasoning as `.claude/rules`' string-matching ban).

**Decision**: names the problem, never auto-recovers. **Why**: lanes run concurrently, so a mid-run `cargo clean` pulls
`target/` out from under lanes still building, and a post-run one silently spends a cold rebuild (12 minutes, measured
2026-09-09) on a run the reader thinks is finishing.

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
  raced each other. `StackOrchestrator` (`scripts/check/stack_orchestrator.go`) lifts lifecycle one level up — at runner
  init, after `selectChecks()` resolves the planned set, it brings up the union of every check's `NeedsContainers` pairs
  (`SmbCore` for integration tests, `SmbE2E` for e2e) once, and tears each down once at runner exit. Checks declaring
  `NeedsContainers` assume the containers are up and call `waitForSmbContainers` as a cheap mid-run zombie-guard. The
  service set behind each mode lives in that stack's `modeServices` table in `stacklease/registry.go` and must stay in
  lock-step with the fixture's `start.sh`; SMB's `core` carries `smb-consumer-unicode` because it's the only fixture
  with non-ASCII share names, and without it nothing in CI can catch a regression in the escaping macOS requires of
  every mount URL (`network/mount.rs::build_smb_mount_url`).
- _Cross-process / cross-worktree_: two `check.sh` runs (or a `check.sh` plus a manual `start.sh`) in different
  worktrees have independent orchestrators, so the in-process map can't stop them racing the same containers. The
  orchestrator therefore takes a **machine-wide lease per stack** via the `stacklease` library (holder-id = its own
  `check.sh` PID). `EnsureStarted` calls `Stack.Acquire` (adopt-or-reconcile under that stack's flock); `Stop` calls
  `Stack.Release` on each held stack (down only at zero holders, lock held across the down). The orchestrator imports
  the lib in-process — no subprocess — because it's already Go in the same module.

The standalone scripts (`start.sh`, `e2e-linux-fixtures.sh::start_smb_containers`) take their **own** leases (`manual`
for `start.sh`, `$$` for `e2e-linux.sh`), so a manual run alongside a `check.sh` run just registers as a second holder
and neither tears the other's stack down. The SIGINT handler in `main.go` captures the orchestrator via shared variable
so a Ctrl+C also releases every held lease (with a banner) before exiting 130. See the `stacklease/` module map above
for the lock/lease/policy model, and § "Two fixture stacks, two lease namespaces" for how a second protocol plugs in.

**Decision**: bring-up and teardown are silent on the common path; only the decisions that cost real wall-clock time
print. **Why**: adopting an already-serving stack is the overwhelmingly common case (every worktree racing to reuse the
same containers), so a fixed-cost banner on every run drowned the one thing worth noticing: a stack that actually had to
`up -d` or `compose down`. Two mechanisms split this:

- `stacklease.Logf` carries every `WARN:`-prefixed line and always prints (a leaked stack, a config-hash mismatch under
  a foreign lease); `stacklease.InfoLogf` carries the routine decision log (adopt confirmations, swept dead leases,
  reconcile/release rationale) and is silenced by `SetVerbose(false)`, which `main.go` calls whenever `flags.quiet` is
  set (i.e. no `-v`/`--verbose`, and not CI). Every OTHER caller of the package (the `stack-lease` CLI that
  `start.sh`/`stop.sh`/`e2e-linux.sh` shell out to, `go run`) never calls `SetVerbose`, so it keeps seeing what it
  always has.
- `stacklease.OnReconcileStart` and `OnTeardown` are hooks `Acquire`/`Release` call exactly when they decide `up -d` or
  `compose down` is actually happening (under the stack's lock, before the slow command runs). `NewStackOrchestrator`
  wires both to print one line per stack, at that same moment (`📦 Starting <stack> fixtures…` /
  `🧹 Stopping <stack> fixtures…`) — the one thing worth telling a human about regardless of verbosity. A release error
  still always prints, independent of both mechanisms.

**Decision**: cmdr's SMB stack binds a dedicated host-port range (11480+), not smb2's default (10480+). **Why**: cmdr
runs a _vendored copy_ of smb2's `consumer` compose under its own project name (`smb-consumer`), while smb2's own test
harness runs the same compose under project `consumer` on 10480+. Same ports + different project = mutually exclusive: a
stack leaked by an interrupted smb2 run (its `Drop` teardown doesn't fire on SIGKILL) squats 10480+ and blocks every
cmdr `check.sh` with `port is already allocated`, cascading until manually cleaned. The port table lives on the SMB
registry entry (`hostPorts` in `stacklease/registry.go`), and the lease overrides smb2's per-service
`SMB_CONSUMER_*_PORT` env on every compose call it makes, whatever the caller exported. So every bring-up path
(`start.sh`, this runner, `e2e-linux.sh`, `contention-check.sh`) lands on 11480+, and the config hash reads the same
pinned view, so a stack one path started is adopted by the others. Pinning in the compose file instead isn't an option:
it's vendored, and its defaults are every other smb2 consumer's contract. The runner's read side is
`checks.ApplySmbPortEnv()` (`checks/smb_ports.go`), which exports the same table as `SMB_CONSUMER_*_PORT` (the Rust
integration tests' `guest_port()`) and `SMB_E2E_*_PORT` (the macOS E2E app). The Linux Docker E2E is unaffected (it
talks to containers over the Docker network on internal `:445`, set explicitly in its `docker run -e`).
`TestSmbPinnedPortsCoverEveryVendoredService` fails when a re-vendor adds a service the table misses. Net: cmdr and
smb2's harnesses coexist, and smb2's defaults and `guest_port()` contract stay untouched.

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

### Two fixture stacks, two lease namespaces

`stacklease` leases any Docker Compose fixture stack, not just SMB. Every protocol-shaped thing is a field on `Stack`
(`stacklease/registry.go`): compose project, `/tmp` lock file, `/tmp` lease dir, the optional `/tmp` keys dir a stack's
containers bind-mount, compose dir + files, mode → service table, the services that ship no `HEALTHCHECK`, the port-env
prefix that folds into the config hash, and the optional pinned host-port table (`hostPorts`, SMB only: its vendored
compose defaults to smb2's range, while SFTP's and WebDAV's own compose defaults already carry theirs). The
adopt-or-reconcile policy, the dead-PID sweep, and the down-at-zero teardown are one implementation over that value.

- **Separate namespaces are the point.** Each stack has its own flock target and its own lease dir, so one stack's
  holders are invisible to the other and downing one at zero can never touch the other's containers. The runner uses the
  same holder-id (its `check.sh` PID) in each, which counts once per stack.
- **SMB's `/tmp` paths are frozen** at `cmdr-smb.lock` and `cmdr-smb-leases`, pinned by a test. SFTP and WebDAV follow
  the pattern (`cmdr-sftp.lock` + `cmdr-sftp-leases` on 12480+, `cmdr-webdav.lock` + `cmdr-webdav-leases` on 13480+,
  `cmdr-s3.lock` + `cmdr-s3-leases` on 14480+). A sibling worktree on older code holds its lease at those exact paths;
  moving them would make a live holder invisible and re-open the teardown race the library exists to close.
- **A stack's HOST state is machine-wide too, all of it.** SMB mounts nothing from the host; SFTP's two key-auth
  services bind-mount `/tmp/cmdr-sftp-keys/<service>`, a third machine-wide path beside the lock and the lease dir. ❌
  Never a path relative to the compose file: compose resolves a relative bind source against the compose file's own
  directory, so a per-checkout path bakes the **starting** worktree into containers that sibling worktrees then adopt,
  and deleting that worktree breaks key auth for all of them at once. `Stack.EnsureKeysDir` creates the leaves before
  bring-up (Docker would auto-create a missing bind source root-owned on Linux, which then fails the container's own
  write) and exports `CMDR_SFTP_KEYS_DIR` so compose binds what this process resolved. The resolved dir folds into the
  config hash next to the ports, so adopting a stack bound to a different one reconciles instead. Why, and the five
  copies of the default: `apps/desktop/test/sftp-servers/README.md` § Keys.
- **Host state can go missing under a stack that still reports healthy**, and `Stack.healKeyMaterial` is the only thing
  that notices. The keys dir is a bind SOURCE under `/tmp`, which macOS empties on reboot while the containers come back
  holding the `authorized_keys` they wrote before it; running, healthy, and config-hash-matching all still say "fine".
  So `Acquire` and `Reconcile` stat each leaf's private key before handing the stack over, restart exactly the services
  whose leaf is empty (re-running an entrypoint is the only thing that can put the two halves back in agreement), and
  wait for the pair to reappear. It reports rather than returning to a caller whose key-auth cells would all fail.
- **A killed client can leave state in a container that outlives it**, and `soloResets` (`stacklease/soloreset.go`)
  clears it: a script per service that `Acquire` execs when, after the dead-PID sweep, no other lease remains. ❌ Never
  under another holder, since the reset may cut what a live suite relies on. Best-effort: a failed reset warns and the
  run goes on. SMB's one entry restarts the guest's Samba `notifyd` when it holds leaked change-notify watches (Samba
  4.23.8 never drops a watch whose smbd child was killed, and each one turns every later write on `public` into three
  failed sends); smbd respawns `notifyd` without closing port 445. A forgotten `manual` lease means the reset never
  fires, the benign direction. Measurements: the `soloreset.go` comment.
- **A stack with FIRST-PARTY images declares `buildContextsRel`**, which folds every context's contents into the config
  hash and puts `--build` on `up`. ❗ Both, or an edited entrypoint never reaches a running container: `up -d` neither
  rebuilds nor recreates a healthy one. SFTP declares one context, WebDAV two (its httpd image and its Nextcloud one),
  S3 two (its VersityGW and Garage wrappers); SMB's images are vendored and it declares none. The hash carries each
  context's own name beside each file's, so two contexts holding a `Dockerfile` can't cancel each other out.
- **Both Playwright lanes lease the SFTP and WebDAV stacks in `e2e` mode**, one server each (`sftp-fixture-openssh`,
  `webdav-fixture-apache`), for the `server-ops-*` specs. The Linux lane also lists `SmbE2E`; the macOS lane doesn't,
  since `smb.spec.ts` is skipped there. CI runs `e2e-linux.sh` directly, so that script takes its own lease on all three
  with holder `$$`, exactly as it always has for SMB.
- **A check declares `NeedsContainers []StackMode`**, so it can ask for several stacks. Both strings resolve against the
  registry, and `TestEveryDeclaredStackModeResolves` (`stack_orchestrator_test.go`) turns a typo into a millisecond
  failure rather than one minutes into a run, after planning and `pnpm install`.
- **An unknown mode is an error.** The table used to fall back to SMB's `core` set for anything unrecognized, so a typo
  brought up the wrong containers and then waited for services nobody asked for.
- **An unresolvable compose dir is an error too**, rather than a warning plus docker's default file lookup — which would
  bring up whatever compose file sat near the cwd under our project name.
- **The SFTP stack's compose file sits directly in `apps/desktop/test/sftp-servers/`**, not under a `.compose/` marker
  dir. That dir is SMB's marker for a tree vendored out of the `smb2` crate, with a cmdr-owned override layered on top;
  SFTP's fixture is first-party, so there is one compose file and no `-f` layering.
- **Its `modeServices` table and `start.sh`'s own case table have to agree.** Two lists, one truth: a server in one and
  not the other shows up as a cell with no server, which reads as a backend bug rather than as a fixture one. Adding a
  server means editing both, plus `sftpServiceHostPorts` (`checks/sftp_ports.go`), which the lane's readiness guard
  derives its expected-service list from.
- **SFTP publishes to `127.0.0.1` for the same reason SMB does** (the decision above), through a
  `${SFTP_BIND_ADDR:-127.0.0.1}` prefix on each `ports:` entry. Its compose file is first-party, so the prefix lives
  there directly rather than surviving a re-vendor, and `TestSftpFixturePortsBindToLoopback` fails the run if a mapping
  loses it. The credentials are in a public repo, which makes a LAN-reachable sshd worse here than an anonymous share.
- **`servicesWithoutHealthcheck` is empty for SFTP** because its one image bakes a healthcheck that reads the listening
  socket out of `netstat`. ❗ Not `nc -z`, which SMB's vendored images use and busybox does not implement: it answers 1
  unconditionally, which reads as a container that never comes up.

### How the integration lane selects fixture cells

`fixtureIntegrationFilter` (`checks/fixture-lane-coverage.go`) builds the nextest expression from one fixture table, and
`desktop-fixture-lane-coverage` guards the same table. Each `laneFixture` names three things: the infrastructure
identifiers an `#[ignore]` reason uses (a start script's path, a compose project's service prefix), the test-name prefix
the lane selects that fixture's APP-crate cells by, and the backend crate whose whole ignored surface is Docker cells.

- **The two halves exist because the suites sit on two sides of a crate boundary.** In the app crate the name prefix is
  the only signal, and it has to stay one (`smb_soak_copy_loop` and the concurrency bench are `#[ignore]`d there too and
  belong in no gating lane). In a backend crate there is no other reason to ignore a test, so the whole package
  qualifies.
- ❗ **A `package(x)` clause for a crate that doesn't exist takes the lane down**, because `cargo nextest` fails to
  _parse_ the filterset rather than matching nothing (verified against `cargo-nextest` 0.9.136, 2026-08-22:
  `error: operator didn't match any packages`). So a backend crate's clause joins the filter only once
  `crates/<name>/Cargo.toml` is on disk. A `test(prefix)` clause matching nothing is harmless, so the name half lands
  ahead of its cells.
- **The guard pairs marker with prefix.** A cell gated on the SFTP fixture has to carry `sftp_integration_`; wearing the
  SMB prefix is a finding even though the lane would run it, because it names the wrong fixture to every reader. A cell
  that belongs outside the lane says so with `// allowed-out-of-lane-fixture-cell: <why>`, and an orphaned opt-out
  fails.

**Decision**: `IsFast` field on `CheckDefinition` and a curated `--fast` pre-commit lane. **Why**: A pre-commit run
should finish in ~10s so it actually gets used. The list is editorially curated, not derived from CSV timings: warm
average is what matters, but cold-cache outliers (`cargo-audit` spiking to ~3 min on advisory DB refresh) would silently
make the lane unreliable on the first run of the day. Mirrors the `IsSlow` / `CIOnly` field pattern (negative-sense
boolean default, same colocated style). Mutually exclusive with `--include-slow` / `--only-slow` to keep the semantics
unambiguous: "give me the fast lane" and "give me the slow lane" can't both be true. Named check invocations bypass the
filter (same escape hatch as `IsSlow` and `CIOnly`).

**Decision**: `CIOnly` field on `CheckDefinition` (mirrors `IsSlow`). **Why**: Keeps "this check runs only in CI"
colocated with the check definition rather than as a hardcoded list elsewhere. `FilterCIOnlyChecks` in `registry.go`
drops them outside `--ci`, with a named-check escape hatch so devs can verify locally before pushing. Orthogonal to
`IsSlow`: `--include-slow` and `--only-slow` do NOT pull in CI-only checks (you'd otherwise lose the ability to run "all
slow checks locally without the CI-only ones"). Negative-sense default (`false` = runs locally) matches the other gating
fields.

## Gotchas

**`--only-slow` needs ~20 min timeout.** Slow checks (E2E tests, `rust-tests-linux`) take significantly longer than the
default checks. When running `--only-slow` via an agent or CI, set the timeout to at least 20 minutes (1,200,000 ms).

**Concurrent SMB-touching runs across worktrees now coexist.** Two `pnpm check` invocations in different worktrees (or a
`check.sh` alongside a manual `start.sh` / `pnpm test:e2e:linux`) each take a machine-wide `stacklease` lease and share
the same `smb-consumer` stack. Whichever finishes first releases its lease but sees a non-zero refcount, so it does
**not** down the stack — the other run keeps serving. The stack downs only when the last holder leaves. The old
`Cannot reach smb-consumer-X` cascade (one run's teardown killing another's mid-test) is the exact failure the lease
closes.

A leaked or lingering stack (a forgotten manual `start.sh`, or a numeric holder whose PID got recycled) is the benign
direction: it stays up until a human reaps it. Check state with `(cd scripts/check && go run ./stack-lease status)`
(every stack) or `... status smb` (one); force SMB down with
`rm -rf /tmp/cmdr-smb-leases && apps/desktop/test/smb-servers/stop.sh`. See `apps/desktop/test/smb-servers/README.md` §
"Shared stack across worktrees" and `stacklease/`.

## Dependencies

`golang.org/x/term`, `golang.org/x/sys` (transitive). Go 1.25.
