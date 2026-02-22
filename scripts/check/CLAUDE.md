# Check runner

Go CLI that runs all code quality checks for the Cmdr monorepo (~40 checks across 4 apps) in
parallel with dependency ordering. Invoked via `./scripts/check.sh` at the repo root.

## Key files

| File | Purpose |
|------|---------|
| `main.go` | Entry point: flag parsing, root dir discovery, check selection, pnpm gating, runner delegation |
| `runner.go` | Parallel executor: goroutine pool, dependency graph, fail-fast, live TTY status line |
| `checks/common.go` | Core types (`CheckDefinition`, `CheckResult`, `CheckContext`, `CheckFunc`), shared utils (`RunCommand`, `EnsureGoTool`, `runPrettierCheck`, `runESLintCheck`) |
| `checks/registry.go` | `AllChecks`: canonical ordered list of all check definitions. Lookup and validation functions. |
| `checks/registry_test.go` | Collision detection, `CLIName()` tests |
| `colors.go` | ANSI color constants |
| `utils.go` | `findRootDir()` (walks up until `apps/desktop/src-tauri/Cargo.toml` is found) |
| `checks/desktop-rust-*.go` | One file per Rust check |
| `checks/desktop-svelte-*.go` | One file per Svelte/TS check |
| `checks/website-*.go`, `checks/license-server-*.go`, `checks/scripts-go-*.go` | One file per check |

## Architecture

```
./scripts/check.sh [flags]
  → go run ./scripts/check [flags]
    → ValidateCheckNames()          # startup: catch ID/nickname collisions
    → parseFlags()                  # --rust/--svelte/--go/--app/--check/--ci/--verbose/--fail-fast
    → findRootDir()                 # walk up to repo root
    → selectChecks()                # filter AllChecks by flags
    → FilterSlowChecks()            # drop IsSlow=true unless --include-slow or --check used
    → ensurePnpmDependencies()      # pnpm install once at root (skipped for Rust-only runs)
    → Runner.Run():
        goroutine pool (NumCPU semaphore)
        for each pending check: canStart() checks DependsOn deps
          → dep pending/running: wait
          → dep failed/blocked: mark StatusBlocked, print BLOCKED
          → all deps done: launch goroutine → runCheck() → completedCh
        status line goroutine (200ms tick, TTY only): "Waiting for: foo, bar..."
    → print summary, exit 0/1
```

## Check definition shape

```go
CheckDefinition{
    ID:          "desktop-rust-clippy",  // unique, always accepted by --check
    Nickname:    "clippy",               // short alias, also accepted by --check (optional)
    DisplayName: "clippy",              // shown in output
    App:         AppDesktop,
    Tech:        "🦀 Rust",
    IsSlow:      false,
    DependsOn:   []string{"desktop-rust-rustfmt"},
    Run:         RunClippy,
}
```

## Key patterns

**IDs vs nicknames:** `--check` accepts either. `CLIName()` returns nickname if set, else ID.
`ValidateCheckNames()` runs at startup and fatals on any collision.

**Dependency graph:** Flat `DependsOn` slice per check. Blocked checks get `StatusBlocked` on dep
failure and are counted as failed. Dependencies not in the selected run set are treated as satisfied.

**Auto-fix vs CI mode:** `--ci` disables auto-fixing. Formatters/linters fix files locally, report
only in CI. `runPrettierCheck` and `runESLintCheck` in `common.go` handle both modes.

**Slow checks:** `IsSlow: true` marks checks excluded by default (currently: `rust-tests-linux`).
Named `--check` invocations implicitly include slow checks (`includeSlow = len(checkNames) > 0`).

**Go tool auto-install:** `EnsureGoTool(name, installPath)` checks PATH first, then runs
`go install` and returns the full binary path. Used for staticcheck, nilaway, etc.

**TTY detection:** `golang.org/x/term.IsTerminal` gates the live status line — CI logs stay clean.

## Apps and check counts

| App | Tech | Checks |
|-----|------|--------|
| Desktop | Rust | rustfmt, clippy, cargo-audit, cargo-deny, cargo-udeps, jscpd, tests, tests-linux (slow) |
| Desktop | Svelte | prettier, eslint, stylelint, css-unused, svelte-check, knip, type-drift, tests, e2e, e2e-linux-typecheck, e2e-linux |
| Website | Astro | prettier, eslint, typecheck, build, e2e |
| License server | TS | prettier, eslint, typecheck, tests |
| Scripts | Go | gofmt, go-vet, staticcheck, ineffassign, misspell, gocyclo, nilaway, govulncheck, deadcode, go-tests |
| Other | pnpm | pnpm-audit |

## Dependencies

`golang.org/x/term`, `golang.org/x/sys` (transitive). Go 1.25.
