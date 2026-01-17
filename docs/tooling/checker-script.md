# Checker script

The checker script (`scripts/check`) runs code quality checks for all apps in the monorepo. It supports parallel
execution, dependency management between checks, and various filtering options.

## Quick start

```bash
# Run all checks (excludes slow checks by default)
go run ./scripts/check

# Run checks for a specific app
go run ./scripts/check --app desktop

# Run a specific check
go run ./scripts/check --check desktop-rust-clippy

# Run multiple specific checks
go run ./scripts/check --check desktop-rust-rustfmt --check desktop-rust-clippy

# Include slow checks
go run ./scripts/check --include-slow

# CI mode (no auto-fixing, stop on first failure)
go run ./scripts/check --ci --fail-fast
```

## Command-line options

| Option                        | Description                                      |
| ----------------------------- | ------------------------------------------------ |
| `--app NAME`                  | Run checks for a specific app                    |
| `--rust`, `--rust-only`       | Run only Rust checks (desktop)                   |
| `--svelte`, `--svelte-only`   | Run only Svelte checks (desktop)                 |
| `--check ID`                  | Run specific checks by ID (repeatable)           |
| `--ci`                        | Disable auto-fixing (for CI)                     |
| `--verbose`                   | Show detailed output                             |
| `--include-slow`              | Include slow checks (excluded by default)        |
| `--fail-fast`                 | Stop on first failure                            |
| `-h`, `--help`                | Show help message                                |

## Available apps

- `desktop` - The Tauri desktop app (Rust + Svelte)
- `website` - The marketing website (Astro)
- `license-server` - The license server (Cloudflare Worker)

## How it works

### Parallel execution

Checks run in parallel by default, using a number of workers equal to the CPU count. The script respects dependencies
between checks—if check B depends on check A, B won't start until A completes successfully.

### Dependencies

Checks can declare dependencies on other checks. For example:
- `desktop-rust-clippy` depends on `desktop-rust-rustfmt` (formatting should happen before linting)
- `desktop-svelte-eslint` depends on `desktop-svelte-prettier`
- `desktop-rust-tests` depends on `desktop-rust-clippy`

If a dependency fails, dependent checks are marked as "BLOCKED" and don't run.

### Continue on failure

By default, the script continues running independent checks even if some fail. Use `--fail-fast` to stop on the first
failure (useful in CI).

### Slow checks

Some checks are marked as "slow" (for example, `desktop-rust-tests-linux` which runs tests in Docker). These are
excluded by default. Use `--include-slow` to run them, or specify them directly with `--check`.

## Output format

Each check outputs a single line:

```
• Desktop: 🦀 Rust / clippy... OK (1.23s) - No warnings
```

Format: `• {App}: {Tech} / {CheckName}... {Status} ({Duration}) - {Message}`

Status can be:
- `OK` (green) - Check passed
- `warn` (yellow) - Check passed with warnings
- `SKIPPED` (yellow) - Check was skipped (for example, missing config file)
- `FAILED` (red) - Check failed
- `BLOCKED` (yellow) - Check couldn't run because a dependency failed

A status line at the bottom shows currently running checks.

## File structure

```
scripts/check/
├── main.go           # Entry point, CLI parsing
├── runner.go         # Parallel execution engine
├── registry.go       # Check lookup helpers
├── colors.go         # ANSI colors and output helpers
├── utils.go          # Utility functions
├── types.go          # Type re-exports (mostly empty)
└── checks/           # Check implementations
    ├── common.go     # Shared types and utilities
    ├── registry.go   # Check definitions with metadata
    └── *.go          # Individual check files
```

## Adding a new check

1. Create a new file in `scripts/check/checks/` following the naming convention `{app}-{tech}-{checkname}.go`:

```go
package checks

import (
    "fmt"
    "os/exec"
    "path/filepath"
)

// RunMyCheck does something useful.
func RunMyCheck(ctx *CheckContext) (CheckResult, error) {
    cmd := exec.Command("some-tool", "some-args")
    cmd.Dir = filepath.Join(ctx.RootDir, "apps", "desktop")
    output, err := RunCommand(cmd, true)
    if err != nil {
        return CheckResult{}, fmt.Errorf("check failed\n%s", indentOutput(output))
    }
    return Success("Checked 42 files"), nil
}
```

2. Add the check definition to `scripts/check/checks/registry.go`:

```go
{
    ID:          "desktop-mytech-mycheck",
    DisplayName: "mycheck",
    App:         AppDesktop,
    Tech:        "🔧 MyTech",
    IsSlow:      false,
    DependsOn:   []string{"desktop-mytech-formatter"}, // optional
    Run:         RunMyCheck,
},
```

3. Test your check:

```bash
go run ./scripts/check --check desktop-mytech-mycheck
```

## Check implementation guidelines

### Return values

- Return `Success(message)` on success with a short, informative message
- Return `Warning(message)` for non-fatal issues
- Return `Skipped(reason)` when the check can't run (for example, missing config)
- Return `CheckResult{}, error` on failure

### Success messages

Include useful stats in success messages:

- ✅ `12 tests passed`
- ✅ `Checked 42 files`
- ✅ `No lint errors`
- ❌ `OK` (too generic)

### Error messages

Include the command output in error messages using `indentOutput()`:

```go
return CheckResult{}, fmt.Errorf("check failed\n%s", indentOutput(output))
```

### CI vs local mode

Use `ctx.CI` to change behavior:

```go
if ctx.CI {
    cmd = exec.Command("tool", "--check")  // Just check, don't fix
} else {
    cmd = exec.Command("tool", "--fix")    // Auto-fix locally
}
```

### Dependencies

Set `DependsOn` to ensure checks run in the right order:

- Formatters should run before linters
- Linters should run before tests
- Type checkers should run before tests

## Troubleshooting

### Check is blocked

A check shows "BLOCKED" when its dependency failed. Fix the dependency first.

### Check is slow

Add `IsSlow: true` to the check definition if it takes more than a few seconds. Users can include it with
`--include-slow`.

### Check needs a tool installed

Use `CommandExists()` to check if a tool is installed, and auto-install if possible:

```go
if !CommandExists("some-tool") {
    installCmd := exec.Command("cargo", "install", "some-tool")
    if _, err := RunCommand(installCmd, true); err != nil {
        return CheckResult{}, fmt.Errorf("failed to install some-tool: %w", err)
    }
}
```
