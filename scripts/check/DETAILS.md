# Check runner details

Pull-tier docs for `scripts/check/`: architecture, flows, and decision rationale. Must-know invariants and gotchas live
in [CLAUDE.md](CLAUDE.md). For check authoring (how to add a new check, `CheckDefinition` shape, naming/`CLIName()`
rules, common helpers, the allowlist), see [`checks/CLAUDE.md`](checks/CLAUDE.md).

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
- **`--verbose`**: Show detailed output
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
- **`-h`, `--help`**: Show help message

`--graph` honors the same selectors (positional or flag form), so `pnpm check rust --graph` graphs only the Rust checks.
It renders before the slow/fast/CI filters, so every lane shows with its size badge. Each node also shows
`~<median wall-time>` from the recent (last 20) passing runs in `~/cmdr-check-log.csv`, so the graph doubles as a perf
dashboard — pairing the CPU-weight (how heavy) with the typical duration (how long) for spotting the next optimization
target. Missing log (CI / `--no-log` / fresh machine) just omits the times. `mermaid` output pastes into a Markdown
```mermaid block or https://mermaid.live; `dot` pipes to Graphviz (`pnpm check --graph --graph-format dot | dot -Tpng -o
checks.png`).

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
- **`stats.go`**: CSV stats logging (`logCheckStats`): appends one row per check to `~/cmdr-check-log.csv`
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

**Slow checks:** `IsSlow: true` marks checks excluded by default (currently: `rust-tests-linux`, `desktop-e2e-linux`,
`desktop-e2e-playwright`). Naming a check (positionally or via `--check`) implicitly includes slow checks
(`includeSlow = len(checkNames) > 0`); group/app selectors don't.

**Fast lane (`--fast`):** `IsFast: true` marks the curated pre-commit check set: ~28 checks that finish in roughly 10s
on a warm cache, intended to run before every commit. It's an editorial pick, not a timing-derived list (see Key
decisions below). Named check invocations bypass the filter so `pnpm check --fast svelte-check` still runs svelte-check.
Mutually exclusive with `--include-slow` / `--only-slow` — combining them errors out, since the lanes are intentionally
separate.

**CI-only checks:** `CIOnly: true` marks checks that run only in `--ci` mode (currently: `cargo-udeps`). They're
silently dropped from local runs (no SKIPPED line) and are not pulled in by `--include-slow` or `--only-slow`. Escape
hatch: an explicit `pnpm check cargo-udeps` always runs, so you can verify locally before pushing.

**Self-contained E2E checks:** `desktop-e2e-playwright` manages the full lifecycle (build binary once, create per-shard
fixtures, start N Tauri instances, run N Playwright processes in parallel, cleanup). Each shard runs in its own isolated
`CMDR_DATA_DIR` with its own Unix socket and MCP port (9429 + shard offset), plus a per-shard `CMDR_INSTANCE_ID` of the
form `e2e-<short>-<pid>` (for example, `e2e-mtp-12345`, `e2e-nonmtp1-12345`). The instance ID drives the macOS Keychain
`SERVICE_NAME` suffix (`Cmdr-e2e-<short>-<pid>`) so two parallel shards can never collide on credentials, and reshapes
the Dock label into `Cmdr (E2E <short>)` so cleanup scripts can target with `pgrep -f 'Cmdr (E2E '`. One shard is
dedicated to MTP specs (serialized; the virtual MTP backing dir at `/tmp/cmdr-mtp-e2e-fixtures` is shared by every Tauri
instance). Stale processes on each port are killed before starting. Per-shard logs go to
`/tmp/cmdr-e2e-playwright-<shard>-<timestamp>.log`. See `docs/tooling/instance-isolation.md` § "How E2E gets isolated
per shard".

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
