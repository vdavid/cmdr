# Check runner

Go CLI that runs all code quality checks for the Cmdr monorepo (~41 checks across 4 apps) in
parallel with dependency ordering. Invoked via `./scripts/check.sh` at the repo root.

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

# CI mode (no auto-fixing, stop on first failure)
go run ./scripts/check --ci --fail-fast
```

## Command-line options

| Option | Description |
| --- | --- |
| `--app NAME` | Run checks for a specific app |
| `--rust`, `--rust-only` | Run only Rust checks (desktop) |
| `--svelte`, `--svelte-only` | Run only Svelte checks (desktop) |
| `--check ID` | Run specific checks by ID or nickname (repeatable) |
| `--ci` | Disable auto-fixing (for CI) |
| `--verbose` | Show detailed output |
| `--include-slow` | Include slow checks (excluded by default) |
| `--fail-fast` | Stop on first failure |
| `--no-log` | Disable CSV stats logging |
| `-h`, `--help` | Show help message |

## Architecture

```
./scripts/check.sh [flags]
  -> go run ./scripts/check [flags]
    -> ValidateCheckNames()          # startup: catch ID/nickname collisions
    -> parseFlags()                  # --rust/--svelte/--go/--app/--check/--ci/--verbose/--fail-fast/--no-log
    -> findRootDir()                 # walk up to repo root
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

| File | Purpose |
|------|---------|
| `main.go` | Entry point: flag parsing, root dir discovery, check selection, pnpm gating, runner delegation |
| `runner.go` | Parallel executor: goroutine pool, dependency graph, fail-fast, live TTY status line |
| `checks/common.go` | Core types (`CheckDefinition`, `CheckResult`, `CheckContext`, `CheckFunc`), shared utils (`RunCommand`, `EnsureGoTool`, `runPrettierCheck`, `runESLintCheck`) |
| `checks/registry.go` | `AllChecks`: canonical ordered list of all check definitions. Lookup and validation functions. |
| `checks/registry_test.go` | Collision detection, `CLIName()` tests |
| `stats.go` | CSV stats logging (`logCheckStats`) — appends one row per check to `~/cmdr-check-log.csv` |
| `colors.go` | ANSI color constants |
| `utils.go` | `findRootDir()` (walks up until `apps/desktop/src-tauri/Cargo.toml` is found) |
| `checks/desktop-rust-*.go` | One file per Rust check |
| `checks/desktop-svelte-*.go` | One file per Svelte/TS check |
| `checks/website-*.go`, `checks/license-server-*.go`, `checks/scripts-go-*.go` | One file per check |
| `checks/file-length.go` | Informational file-length scanner (warn-only, never fails) |

## Key patterns

**IDs vs nicknames:** `--check` accepts either. `CLIName()` returns nickname if set, else ID.
`ValidateCheckNames()` runs at startup and fatals on any collision.

**Dependency graph:** Flat `DependsOn` slice per check. Blocked checks get `StatusBlocked` on dep
failure and are counted as failed. Dependencies not in the selected run set are treated as satisfied.

**Auto-fix vs CI mode:** `--ci` disables auto-fixing. Formatters/linters fix files locally, report
only in CI. `runPrettierCheck` and `runESLintCheck` in `common.go` handle both modes.

**Slow checks:** `IsSlow: true` marks checks excluded by default (currently: `rust-tests-linux`,
`desktop-e2e-linux`). Named `--check` invocations implicitly include slow checks
(`includeSlow = len(checkNames) > 0`).

**Go tool auto-install:** `EnsureGoTool(name, installPath)` checks PATH first, then runs
`go install` and returns the full binary path. Used for staticcheck, nilaway, etc.

**TTY detection:** `golang.org/x/term.IsTerminal` gates the live status line — CI logs stay clean.

**CSV stats logging:** Each check run appends a row to `~/cmdr-check-log.csv` with timestamp, app,
check name, duration, result (pass/fail/skip/blocked), and optional counts (total, issues, changes).
`CheckResult` has `Total`, `Issues`, `Changes` fields (`-1` = N/A, rendered as `N/A` in CSV).
Disabled by `--no-log` or `--ci`. Implementation in `stats.go`.

## Check definition shape

```go
CheckDefinition{
    ID:          "desktop-rust-clippy",  // unique, always accepted by --check
    Nickname:    "clippy",               // short alias, also accepted by --check (optional)
    DisplayName: "clippy",              // shown in output
    App:         AppDesktop,
    Tech:        "Rust",
    IsSlow:      false,
    DependsOn:   []string{"desktop-rust-rustfmt"},
    Run:         RunClippy,
}
```

## Adding a new check

1. Create `checks/{app}-{name}.go` with a `func RunSomething(ctx *CheckContext) (CheckResult, error)`.
   Use `website-build.go` or `website-docker.go` as templates — they're the simplest.
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

Set `DependsOn` to ensure checks run in the right order: formatters before linters, linters before
tests, type checkers before tests.

## Apps and check counts

| App | Tech | Checks |
|-----|------|--------|
| Desktop | Rust | rustfmt, clippy, cargo-audit, cargo-deny, cargo-udeps, jscpd, tests, tests-linux (slow) |
| Desktop | Svelte | prettier, eslint, stylelint, css-unused, svelte-check, import-cycles, knip, type-drift, tests, smoke, e2e-linux-typecheck, e2e-linux (slow) |
| Website | Astro | prettier, eslint, typecheck, build, html-validate, e2e |
| Website | Docker | docker-build |
| License server | TS | prettier, eslint, typecheck, tests |
| Scripts | Go | gofmt, go-vet, staticcheck, ineffassign, misspell, gocyclo, nilaway, deadcode, go-tests |
| Other | pnpm | pnpm-audit |
| Other | Metrics | file-length (warn-only) |

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

## Dependencies

`golang.org/x/term`, `golang.org/x/sys` (transitive). Go 1.25.
