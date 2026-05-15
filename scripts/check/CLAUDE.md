# Check runner

Go CLI that runs all code quality checks for the Cmdr monorepo (~41 checks across 4 apps) in parallel with dependency
ordering. Invoked via `./scripts/check.sh` at the repo root.

## Quick start

```bash
# Run all checks (excludes slow checks by default)
go run ./scripts/check

# Run checks for a specific app
go run ./scripts/check --app desktop

# Run a specific check (accepts ID or nickname)
go run ./scripts/check --check clippy

# Run multiple specific checks
go run ./scripts/check --check rustfmt --check clippy

# Include slow checks
go run ./scripts/check --include-slow

# Run only slow checks
go run ./scripts/check --only-slow

# CI mode (no auto-fixing, stop on first failure)
go run ./scripts/check --ci --fail-fast

# Run compat checks on freestyle VM, incompat checks locally, in parallel
go run ./scripts/check --prefer-freestyle

# Run only freestyle-compatible checks on the VM (skip Rust, Docker)
go run ./scripts/check --only-freestyle
```

## Command-line options

| Option                      | Description                                             |
| --------------------------- | ------------------------------------------------------- |
| `--app NAME`                | Run checks for a specific app                           |
| `--rust`, `--rust-only`     | Run only Rust checks (desktop)                          |
| `--svelte`, `--svelte-only` | Run only Svelte checks (desktop)                        |
| `--check ID`                | Run specific checks by ID or nickname (repeatable)      |
| `--ci`                      | Disable auto-fixing (for CI)                            |
| `--verbose`                 | Show detailed output                                    |
| `--include-slow`            | Include slow checks (excluded by default)               |
| `--only-slow`               | Run only slow checks                                    |
| `--only-freestyle`          | Run freestyle-compatible checks on a VM (skip the rest) |
| `--prefer-freestyle`        | Run compat checks on VM + the rest locally in parallel  |
| `--fail-fast`               | Stop on first failure                                   |
| `--no-log`                  | Disable CSV stats logging                               |
| `-h`, `--help`              | Show help message                                       |

## Architecture

```
./scripts/check.sh [flags]
  -> go run ./scripts/check [flags]
    -> ValidateCheckNames()          # startup: catch ID/nickname collisions
    -> parseFlags()
    -> findRootDir()                 # walk up to repo root
    -> handleFreestyleFlags():
        --prefer-freestyle:          # parallel: VM (compat) + local (incompat)
          goroutine: freestyleRun()  #   push sync branch, run on VM
          local: Runner.Run()        #   FreestyleIncompat checks only
          wait + reconcile results
        --only-freestyle:            # VM only, skip incompat
          freestyleRun()
    -> selectChecks()                # filter AllChecks by flags
    -> FilterSlowChecks()            # drop IsSlow=true unless --include-slow or --check used
    -> FilterCIOnlyChecks()          # drop CIOnly=true unless --ci or --check named it
    -> ensurePnpmDependencies()      # pnpm install once at root (skipped for Rust-only runs)
    -> Runner.Run():
        goroutine pool (NumCPU semaphore)
        for each pending check: canStart() checks DependsOn deps
          -> dep pending/running: wait
          -> dep failed/blocked: mark StatusBlocked, print BLOCKED
          -> all deps done: launch goroutine -> runCheck() -> completedCh
        status line goroutine (200ms tick, TTY only): "Waiting for: foo, bar..."
    -> print summary, exit 0/1
```

## Key files

| File                                                                      | Purpose                                                                                                                                                       |
| ------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `main.go`                                                                 | Entry point: flag parsing, root dir discovery, check selection, pnpm gating, runner delegation                                                                |
| `runner.go`                                                               | Parallel executor: goroutine pool, dependency graph, fail-fast, live TTY status line                                                                          |
| `checks/common.go`                                                        | Core types (`CheckDefinition`, `CheckResult`, `CheckContext`, `CheckFunc`), shared utils (`RunCommand`, `EnsureGoTool`, `runPrettierCheck`, `runESLintCheck`) |
| `checks/registry.go`                                                      | `AllChecks`: canonical ordered list of all check definitions. Lookup and validation functions.                                                                |
| `checks/registry_test.go`                                                 | Collision detection, `CLIName()` tests                                                                                                                        |
| `stats.go`                                                                | CSV stats logging (`logCheckStats`): appends one row per check to `~/cmdr-check-log.csv`                                                                      |
| `colors.go`                                                               | ANSI color constants                                                                                                                                          |
| `utils.go`                                                                | `findRootDir()` (walks up until `apps/desktop/src-tauri/Cargo.toml` is found)                                                                                 |
| `checks/desktop-rust-*.go`                                                | One file per Rust check                                                                                                                                       |
| `checks/desktop-svelte-*.go`                                              | One file per Svelte/TS check                                                                                                                                  |
| `checks/website-*.go`, `checks/api-server-*.go`, `checks/scripts-go-*.go` | One file per check                                                                                                                                            |
| `checks/file-length.go`                                                   | Informational file-length scanner (warn-only, never fails). Supports an allowlist.                                                                            |
| `checks/file-length-allowlist.json`                                       | Allowlist for file-length check: `{ "files": { "path": lineCount } }`. Files at or below their allowlisted count are suppressed.                              |
| `checks/changelog-commit-links.go`                                        | Validates every `https://github.com/vdavid/cmdr/commit/<sha>` URL in `CHANGELOG.md` resolves, via a single `git cat-file --batch-check` process.              |

## Key patterns

**IDs vs nicknames:** `--check` accepts either. `CLIName()` returns nickname if set, else ID. `ValidateCheckNames()`
runs at startup and fatals on any collision.

**Dependency graph:** Flat `DependsOn` slice per check. Blocked checks get `StatusBlocked` on dep failure and are
counted as failed. Dependencies not in the selected run set are treated as satisfied.

**Auto-fix vs CI mode:** `--ci` disables auto-fixing. Formatters/linters fix files locally, report only in CI.
`runPrettierCheck` and `runESLintCheck` in `common.go` handle both modes.

**Slow checks:** `IsSlow: true` marks checks excluded by default (currently: `rust-tests-linux`, `desktop-e2e-linux`,
`desktop-e2e-playwright`). Named `--check` invocations implicitly include slow checks
(`includeSlow = len(checkNames) > 0`).

**CI-only checks:** `CIOnly: true` marks checks that run only in `--ci` mode (currently: `cargo-udeps`). They're
silently dropped from local runs (no SKIPPED line) and are not pulled in by `--include-slow` or `--only-slow`.
Escape hatch: an explicit `--check cargo-udeps` always runs, so you can verify locally before pushing.

**Self-contained E2E checks:** `desktop-e2e-playwright` manages the full lifecycle (build binary once, create per-shard
fixtures, start N Tauri instances, run N Playwright processes in parallel, cleanup). Each shard runs in its own isolated
`CMDR_DATA_DIR` with its own Unix socket and MCP port (9429 + shard offset). One shard is dedicated to MTP specs
(serialized; the virtual MTP backing dir at `/tmp/cmdr-mtp-e2e-fixtures` is shared by every Tauri instance). Stale
processes on each port are killed before starting. Per-shard logs go to
`/tmp/cmdr-e2e-playwright-<shard>-<timestamp>.log`.

`RUST_LOG` is forwarded to the app (via inherited `os.Environ()`), so trace-level output is one shell-prefix away:

```bash
RUST_LOG=cmdr_lib::file_system::volume::mtp=trace ./scripts/check.sh --check desktop-e2e-playwright
```

The chosen `RUST_LOG` value is echoed at the top of the timestamped log so it's obvious from a glance which level was
captured. When unset, the log starts with `=== RUST_LOG unset (default warn level) ===`.

**Go tool auto-install:** `EnsureGoTool(name, installPath)` checks PATH first, then runs `go install` and returns the
full binary path. Used for staticcheck, nilaway, etc.

**TTY detection:** `golang.org/x/term.IsTerminal` gates the live status line; CI logs stay clean.

**CSV stats logging:** Each check run appends a row to `~/cmdr-check-log.csv` with timestamp, app, check name, duration,
result (pass/fail/skip/blocked), and optional counts (total, issues, changes). `CheckResult` has `Total`, `Issues`,
`Changes` fields (`-1` = N/A, rendered as `N/A` in CSV). Disabled by `--no-log` or `--ci`. Implementation in `stats.go`.

**File-length allowlist:** `checks/file-length-allowlist.json` maps relative paths to accepted line counts. Files at or
below their allowlisted count are silently suppressed. Files that grow beyond their allowlisted count are reported with
both the current and allowed line counts. New files not in the allowlist are reported normally. When the allowlist
suppresses all long files, the check shows "No new long files (N allowlisted)". If the allowlist file is missing, all
long files are reported (backwards-compatible). To allowlist a file, add `"relative/path": lineCount` to the `files`
object.

## Check definition shape

```go
CheckDefinition{
ID:                "desktop-svelte-eslint", // unique, always accepted by --check
Nickname:          "",                      // short alias, also accepted by --check (optional)
DisplayName:       "eslint", // shown in output
App:               AppDesktop,
Tech:              "🎨 Svelte",
IsSlow:            false,
CIOnly:            false, // true = run only in --ci mode (or explicit --check)
FreestyleIncompat: true, // can NOT run on freestyle.sh VMs (Rust, Docker)
DependsOn:         []string{"desktop-svelte-prettier"},
Run:               RunDesktopESLint,
}
```

## Adding a new check

1. Create `checks/{app}-{name}.go` with a `func RunSomething(ctx *CheckContext) (CheckResult, error)`. Use
   `website-build.go` or `website-docker.go` as templates; they're the simplest.
2. Register it in `AllChecks` in `registry.go` (ID, App, Tech, DependsOn, Run).
3. Return `Success("message")` on pass, `fmt.Errorf(...)` on fail, `Skipped("reason")` to skip.
4. Add a test file if the check has non-trivial logic (`checks/{app}-{name}_test.go`).
5. Run `./scripts/check.sh --go` to verify (staticcheck is strict about idiomatic Go).
6. Update the table below and `AGENTS.md`'s `--check` list.

### Return values

- Return `Success(message)` on success with a short, informative message
- Return `Warning(message)` for non-fatal issues
- Return `Skipped(reason)` when the check can't run (for example, missing config)
- Return `CheckResult{}, error` on failure

### Success messages

Include useful stats: "12 tests passed", "Checked 42 files", "No lint errors". Avoid generic "OK".

### Error messages

Include the command output using `indentOutput()`:

```go
return CheckResult{}, fmt.Errorf("check failed\n%s", indentOutput(output))
```

### Dependencies

Set `DependsOn` to ensure checks run in the right order: formatters before linters, linters before tests, type checkers
before tests.

## Apps and check counts

| App        | Tech    | Checks                                                                                                                                                                                              |
| ---------- | ------- | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Desktop    | Rust    | rustfmt, clippy, cargo-audit, cargo-deny, cargo-machete, cargo-udeps (CI-only), jscpd, log-error-macro, error-string-match, bindings-fresh, ipc-enum-camelcase, tests, integration-tests (Docker SMB), tests-linux (slow) |
| Desktop    | Svelte  | prettier, eslint, eslint-typecheck (slow), stylelint, css-unused, a11y-contrast, svelte-check, import-cycles, knip, type-drift, tests, e2e-linux-typecheck, e2e-linux (slow), e2e-playwright (slow) |
| Website    | Astro   | prettier, eslint, typecheck, build, html-validate, e2e                                                                                                                                              |
| Website    | Docker  | docker-build                                                                                                                                                                                        |
| API server | TS      | oxfmt, eslint, typecheck, tests                                                                                                                                                                     |
| Scripts    | Go      | gofmt, go-vet, staticcheck, ineffassign, misspell, gocyclo, nilaway, deadcode, go-tests                                                                                                             |
| Other      | Metrics | file-length (warn-only), CLAUDE.md-reminder (warn-only), changelog-commit-links                                                                                                                     |

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

**Decision**: Go instead of Bash for the check script. **Why**: Cross-platform support (especially Windows), type-safe,
better error handling, and ability to build complex logic (parallel checks, dependency graph, colored output). Go is
already in the toolchain via mise.

**Decision**: `cargo-nextest` instead of `cargo test`. **Why**: Faster test execution (parallel by default), better
output formatting, clearer failure messages. Auto-installed by the check script if missing.

**Decision**: `cargo-deny` advisories check disabled; use `cargo-audit` instead. **Why**: Tauri's transitive
dependencies (gtk3-rs, unic-\*, fxhash, proc-macro-error, etc.) trigger unmaintained-crate advisories we can't control.
`cargo-audit` still catches critical security vulnerabilities. License, bans, and sources checks in `cargo-deny` remain
active. See comment in `src-tauri/deny.toml`.

**Decision**: `cfg-gate` check to catch ungated macOS-only crate imports. **Why**: Rust code using macOS-only crates
(from `[target.'cfg(target_os = "macos")'.dependencies]`) compiles fine on macOS but fails on Linux if the `use` isn't
wrapped in `#[cfg(target_os = "macos")]`. CI catches this after push, but the check catches it locally and instantly. It
parses `Cargo.toml` for macOS-only crate names, detects module-level gating (for example,
`#[cfg(target_os = "macos")] mod foo;` in `lib.rs` makes everything inside `foo` inherently safe), and scans remaining
files for ungated `use` statements.

**Decision**: Auto-fix locally, check-only in CI. **Why**: Developers get instant fixes locally (less friction), CI
ensures code is properly formatted before merge. Controlled by the `--ci` flag.

**Decision**: Split `desktop-svelte-eslint` into fast (non-type-aware) and slow (full) checks. **Why**: Type-aware rules
(`no-floating-promises`, `no-unsafe-*`, etc.) take ~45% of lint time due to TypeScript project service startup. The fast
check sets `ESLINT_NO_TYPECHECK=1`, which `eslint.config.js` reads to use `tseslint.configs.strict` (no type info) and
suppress `reportUnusedDisableDirectives` (since disable comments for type-aware rules would look unused). The slow check
(`IsSlow: true`) runs the full config with all rules and `reportUnusedDisableDirectives` on, so stale disable comments
are still caught.

**Decision**: Clippy runs the enforcing pass (`-D warnings`) first, and only invokes `cargo clippy --fix` if that
fails (and we're not in CI). **Why**: Running `--fix` speculatively before every check doubled wall time on the happy
path (no warnings = no fix to apply). The enforcing pass is the one that actually decides pass/fail anyway, and
`--fix` ignores `-D warnings` so it can't be combined into a single invocation. The trade is one extra re-check on
the rare warning path; cuts ~50% off clippy in the common clean case.

**Decision**: `bindings-fresh` is hash-cached. **Why**: The check used to run `pnpm bindings:regen` on every invocation
(~2 min for a test-mode compile of the full crate) just to confirm the output didn't change. We now hash every `.rs`
file under `src-tauri/src` plus `Cargo.lock` and `Cargo.toml`, plus the current `bindings.ts`, and store both hashes in
`<CARGO_TARGET_DIR or workspace target>/.bindings-fresh-marker` after each successful run. If both match next time, we
return OK in <100 ms. The bindings.ts hash in the marker protects against manual edits; if someone hand-tweaks the file
the marker no longer matches and we run the full regen. The marker lives inside cargo's actual target dir (honoring
`CARGO_TARGET_DIR`), so `cargo clean` and wholesale `target/` deletion auto-invalidate it. Same shared-fate pattern as
`node_modules/.pnpm-install-marker`. Hashing all `.rs` files (rather than only those with `#[tauri::command]` /
`specta::Type`) costs ~tens of ms here and removes any "added the attr to a new file but the watch list didn't pick it
up" footgun.

**Decision**: `cargo-machete` runs locally; `cargo-udeps` is CI-only. **Why**: Both detect unused dependencies but
trade off speed against precision. udeps compiles the whole crate with nightly (~2 min cold) and is authoritative.
machete greps source files for `use foo;` patterns (~0.5 s on this codebase, no compile) and catches the common case
(removed the last `use` but forgot to drop the dep) plus a class udeps misses ("transitively-used" deps where your
Cargo.toml lists serde but only a transitive dep actually uses it). machete's blind spot is deps used only inside
macro expansions or build.rs codegen; opt those out via `[package.metadata.cargo-machete] ignored = ["foo"]` in the
relevant Cargo.toml. Local dev gets instant feedback from machete; CI runs udeps for the long-tail check.

**Decision**: `CIOnly` field on `CheckDefinition` (mirrors `IsSlow` and `FreestyleIncompat`). **Why**: Keeps "this
check runs only in CI" colocated with the check definition rather than as a hardcoded list elsewhere. `FilterCIOnlyChecks`
in `registry.go` drops them outside `--ci`, with a `--check <name>` escape hatch so devs can verify locally before
pushing. Orthogonal to `IsSlow`: `--include-slow` and `--only-slow` do NOT pull in CI-only checks (you'd otherwise lose
the ability to run "all slow checks locally without the CI-only ones"). Negative-sense default (`false` = runs locally)
matches the other gating fields.

**Decision**: Skip `pnpm install` when lockfile is unchanged. **Why**: `pnpm install` takes ~20s and pegs all CPUs even
when deps haven't changed. A marker file (`node_modules/.pnpm-install-marker`) stores `pnpm-lock.yaml`'s mtime after
each successful install. On the next run, if the mtime matches, install is skipped. The marker lives inside
`node_modules/` so it's automatically invalidated if `node_modules` is deleted. Always runs in CI (`--ci`).

**Decision**: E2E failure output uses section-aware filtering, not a pattern denylist. **Why**: The checker's contract
with agents is that output is concise enough to read in full: no `head`/`tail`/`grep` needed. Raw Playwright + Tauri +
Docker output is 1000+ lines on a failure (test pass markers, app stdout log, post-ELIFECYCLE build dump). The captured
output has four stable sections (setup, per-test progress, numbered failure blocks, post-ELIFECYCLE dump), split by
fixed delimiters (`Starting Tauri app...`, `\d+\) \[tauri\]`, `[ELIFECYCLE]`). `extractE2ETestOutput` in
`desktop-svelte-e2e-playwright.go` keeps the failure blocks verbatim, drops the post-ELIFECYCLE dump, and in the
progress section keeps `✘` markers with their preceding annotation lines (like `[SMB diag] MCP port: …`) while dropping
`✓`/`-` markers with theirs. The untouched output stays in the timestamped log file the error message links to. Both
`desktop-e2e-linux` and `desktop-e2e-playwright` call the same helper.

If the run died before reaching the test phase, none of `Starting Tauri app...`, a `\d+) [tauri]` failure-block header,
or a `\d+ (passed|failed|flaky|skipped)` tally line will be present. `isPreTestFailure` checks all three; only if all
three are absent does the filter prepend `note: tests did not reach the run phase` and drop the verbose
`docker compose ps` table (anchored on its `NAME IMAGE COMMAND` header so prose containing `Up <N>` survives). Checking
the tally and failure block (not just the Tauri marker) avoids false positives on macOS playwright shards, where Tauri
is started by the Go check and its stdout goes to a per-shard log file, so the marker never appears in Playwright stdout
regardless of success.

**Decision**: `cargo test` / `cargo nextest` failure output is filtered by dropping pass/skip verdict lines only.
**Why**: A 1786-test run produces ~1800 noise lines around 2 real failures. The harness format is stable enough that a
single per-line regex (`^test … ... (ok|ignored…)$` for `cargo test`, `^\s+(PASS|SKIP) [...] …$` for `cargo nextest`)
can drop the noise without risking false positives on panic-message bodies (start-of-line anchor protects quoted test
phrases). `trimRustTestProgress` in `desktop-rust-tests-linux.go` runs after `trimBuildNoise`. Everything else
(`running N tests` header, FAIL/FAILED/LEAK/TIMEOUT verdicts, the `failures:` block, the `test result:` / `Summary`
tally, `error:` lines, bench results) passes through unchanged.

**Decision**: SMB Docker container lifecycle is owned by a runner-level orchestrator, not per-check. **Why**: Multiple
checks (`desktop-rust-integration-tests`, `desktop-e2e-linux`) need the shared `smb-consumer` Docker Compose project.
Before, each owned the lifecycle: start in entry, `defer ./stop.sh` in cleanup. When both ran in parallel under
`--include-slow`, whichever finished first would tear down containers the other was still using, producing
`Cannot reach smb-consumer-X` cascades. `SmbOrchestrator` (`scripts/check/smb_orchestrator.go`) lifts lifecycle one
level up: at runner init, after `selectChecks()` resolves the planned set, the orchestrator brings up the union of
`NeedsSmb` modes (`SmbModeCore` for integration tests, `SmbModeE2E` for e2e). At runner exit (normal, `--fail-fast`, or
SIGINT) it tears down once. Checks marked `NeedsSmb` no longer manage their own lifecycle: they assume the containers
are up and call `waitForSmbContainers` as a cheap mid-run zombie-guard. The smaller scripts (`start.sh`,
`e2e-linux.sh::start_smb_containers`) keep working standalone for `pnpm test:e2e:linux` invocations outside the check
runner; under check.sh their start.sh invocation just sees the orchestrator's containers already running and probes are
idempotent. The SIGINT handler in `main.go` captures the orchestrator via shared variable so a Ctrl+C also triggers
`./stop.sh` with a banner before exiting 130.

**Decision**: silence apt/dpkg at the source, not via a post-hoc denylist. **Why**: `provisionScript` redirects both apt
commands to a log file under `DEBIAN_FRONTEND=noninteractive` + `-qq`, so on a successful provision the check's stdout
gets zero apt lines. The log lives on a per-run host directory (`/tmp/cmdr-rust-tests-linux-<unix-ts>/provision.log`)
bind-mounted into the container at `/cmdr-logs`, so it survives the container's `--rm` and is discoverable from the
check's Success/failure message. On apt failure, the script dumps the full log to stderr (captured by Go) so the user
sees what went wrong without having to fish for the file. A previous attempt to scrub the verbose output with a
`packageManagerNoiseRE` denylist was a treadmill: every Debian version adds new dpkg verbs (`Setting up`, `Unpacking`,
`Processing triggers`, `Get:N`, `Hit:N`, `Selecting previously unselected package`, `Created symlink`,
`update-alternatives:`, `procps:`, etc.), continuation lines from multi-line apt prompts have no stable shape, and
`apt-get -qq` alone doesn't propagate to dpkg's per-package chatter. Redirection at source is bulletproof and
zero-maint. `trimBuildNoise` now only cuts everything before the last `Compiling …` line; when no such line exists
(provisioning died before cargo ran), the output is returned verbatim. Length-based truncation is forbidden everywhere;
if 200 tests fail, all 200 panic bodies pass through.

**Decision**: nextest binary is arch-aware. **Why**: `https://get.nexte.st/latest/linux` serves the x86_64-musl build by
default; on an arm64 container (e.g. Apple Silicon under OrbStack) cargo's rustup-shim happily syncs the aarch64
toolchain, then execs the x86 nextest binary and OrbStack crashes with
`Dynamic loader not found: /lib64/ld-linux-x86-64.so.2`. The fix is `dpkg --print-architecture` → `linux` for amd64 and
`linux-arm` for arm64, matching the Go tarball selection.

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

**`--only-slow` needs ~20 min timeout.** Slow checks (E2E tests, eslint-typecheck) take significantly longer than the
default checks. When running `--only-slow` via an agent or CI, set the timeout to at least 20 minutes (1,200,000 ms).

## Dependencies

`golang.org/x/term`, `golang.org/x/sys` (transitive). Go 1.25.
