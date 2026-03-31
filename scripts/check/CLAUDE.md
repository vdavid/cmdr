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
| `stats.go`                                                                | CSV stats logging (`logCheckStats`) — appends one row per check to `~/cmdr-check-log.csv`                                                                     |
| `colors.go`                                                               | ANSI color constants                                                                                                                                          |
| `utils.go`                                                                | `findRootDir()` (walks up until `apps/desktop/src-tauri/Cargo.toml` is found)                                                                                 |
| `checks/desktop-rust-*.go`                                                | One file per Rust check                                                                                                                                       |
| `checks/desktop-svelte-*.go`                                              | One file per Svelte/TS check                                                                                                                                  |
| `checks/website-*.go`, `checks/api-server-*.go`, `checks/scripts-go-*.go` | One file per check                                                                                                                                            |
| `checks/file-length.go`                                                   | Informational file-length scanner (warn-only, never fails)                                                                                                    |

## Key patterns

**IDs vs nicknames:** `--check` accepts either. `CLIName()` returns nickname if set, else ID. `ValidateCheckNames()`
runs at startup and fatals on any collision.

**Dependency graph:** Flat `DependsOn` slice per check. Blocked checks get `StatusBlocked` on dep failure and are
counted as failed. Dependencies not in the selected run set are treated as satisfied.

**Auto-fix vs CI mode:** `--ci` disables auto-fixing. Formatters/linters fix files locally, report only in CI.
`runPrettierCheck` and `runESLintCheck` in `common.go` handle both modes.

**Slow checks:** `IsSlow: true` marks checks excluded by default (currently: `rust-tests-linux`, `desktop-e2e-linux`).
Named `--check` invocations implicitly include slow checks (`includeSlow = len(checkNames) > 0`).

**Go tool auto-install:** `EnsureGoTool(name, installPath)` checks PATH first, then runs `go install` and returns the
full binary path. Used for staticcheck, nilaway, etc.

**TTY detection:** `golang.org/x/term.IsTerminal` gates the live status line — CI logs stay clean.

**CSV stats logging:** Each check run appends a row to `~/cmdr-check-log.csv` with timestamp, app, check name, duration,
result (pass/fail/skip/blocked), and optional counts (total, issues, changes). `CheckResult` has `Total`, `Issues`,
`Changes` fields (`-1` = N/A, rendered as `N/A` in CSV). Disabled by `--no-log` or `--ci`. Implementation in `stats.go`.

## Check definition shape

```go
CheckDefinition{
    ID:                "desktop-svelte-eslint",  // unique, always accepted by --check
    Nickname:          "",                       // short alias, also accepted by --check (optional)
    DisplayName:       "eslint",                 // shown in output
    App:               AppDesktop,
    Tech:              "🎨 Svelte",
    IsSlow:            false,
    FreestyleIncompat: true,                    // can NOT run on freestyle.sh VMs (Rust, Docker)
    DependsOn:         []string{"desktop-svelte-prettier"},
    Run:               RunDesktopESLint,
}
```

## Adding a new check

1. Create `checks/{app}-{name}.go` with a `func RunSomething(ctx *CheckContext) (CheckResult, error)`. Use
   `website-build.go` or `website-docker.go` as templates — they're the simplest.
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

| App        | Tech    | Checks                                                                                                                                                        |
| ---------- | ------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Desktop    | Rust    | rustfmt, clippy, cargo-audit, cargo-deny, cargo-udeps, jscpd, tests, tests-linux (slow)                                                                       |
| Desktop    | Svelte  | prettier, eslint, eslint-typecheck (slow), stylelint, css-unused, svelte-check, import-cycles, knip, type-drift, tests, e2e-linux-typecheck, e2e-linux (slow) |
| Website    | Astro   | prettier, eslint, typecheck, build, html-validate, e2e                                                                                                        |
| Website    | Docker  | docker-build                                                                                                                                                  |
| API server | TS      | oxfmt, eslint, typecheck, tests                                                                                                                               |
| Scripts    | Go      | gofmt, go-vet, staticcheck, ineffassign, misspell, gocyclo, nilaway, deadcode, go-tests                                                                       |
| Other      | Metrics | file-length (warn-only)                                                                                                                                       |

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

**Decision**: Skip `pnpm install` when lockfile is unchanged. **Why**: `pnpm install` takes ~20s and pegs all CPUs even
when deps haven't changed. A marker file (`node_modules/.pnpm-install-marker`) stores `pnpm-lock.yaml`'s mtime after
each successful install. On the next run, if the mtime matches, install is skipped. The marker lives inside
`node_modules/` so it's automatically invalidated if `node_modules` is deleted. Always runs in CI (`--ci`).

## Freestyle.sh remote execution

Two modes for offloading checks to a freestyle.sh VM:

- `--only-freestyle`: runs only freestyle-compatible checks on the VM, skips the rest entirely.
- `--prefer-freestyle`: runs freestyle-compatible checks on the VM and the rest locally, in parallel. This is the "run
  everything as fast as possible" mode — Rust checks run on your Mac while Node/Go checks run on the VM simultaneously.

**How it works:** Creates a temporary git commit of the full working tree (without modifying the local index/worktree),
pushes it to a temp branch, fetches on the VM, runs checks, cleans up the branch.

**What's freestyle-compatible:** Node/TS checks (Svelte, Astro, API server), Go checks, and metrics — any check without
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

## Dependencies

`golang.org/x/term`, `golang.org/x/sys` (transitive). Go 1.25.
