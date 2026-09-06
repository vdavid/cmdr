# Check runner

Go CLI running the monorepo's 131 checks in parallel with dependency ordering, via `pnpm check` at the repo root.
Authoring a check: `checks/CLAUDE.md`.

## Module map

- `main.go` (entry), `runner.go` (parallel executor: weighted admission, exclusive resources, dependency graph,
  fail-fast, TTY status line).
- `plan.go` + `cache.go` + `runner-sources.go` + `checks/fingerprint.go` (the input-fingerprint cache),
  `checks/inputs.go` (shared `Inputs` blocks), `checks/cargo-workspace.go` (the geometry Rust checks scope from).
- `stack_orchestrator.go` + `stacklease/` (Docker fixture stacks), `graph.go` / `docs_graph_render.go` (renderers),
  `stats.go` + `unknown_selector_log.go` (the CSV logs), `autofix_notice.go`.

## Must-knows

- **Run from repo root via `pnpm check`.** Positional args mix checks, apps, and groups; a named check runs even if
  slow/CI-only/disabled, an app or group selector keeps the default lanes.
- **Checks refuse to run in the main clone** (the auto-fixers reformat tracked files, which belongs in a worktree).
  `--ci` is exempt; override with `--allow-main`.
- **A check fingerprints the runner CORE (`GlobalInputs`) plus the files its own `Run` reaches** (read from the AST at
  plan time), and fails closed to the whole tree. ❌ No helper the EXECUTOR calls in a check file.
- **Cache ordering is load-bearing.** Planning runs BEFORE `pnpm install` and Docker bring-up, so an all-hits run
  installs nothing and starts no container; ❌ don't move it after. Only `StatusOK` is cached, and a corrupt cache
  degrades to "run everything".
- **A cargo lane that COMPILES declares `Exclusive: ResourceCargoBuildDir`** (cargo locks its build dir, so those lanes
  serialize anyway) and asks cargo the SAME question as every other host lane (`HostCargoLaneArgs`); own flags cost
  20-100 s of rebuild per flip. ❌ No per-package `-p` lanes.
- **The Rust and frontend lanes are blind to `CLAUDE.md` / `DETAILS.md`** (`agentDocExclusions`), so a docs-only edit is
  a cache hit. ❌ No other `!` exclusion.
- **A check names its Docker fixtures in `NeedsContainers []StackMode`** (`stacklease` registry: `smb`, `sftp`,
  `webdav`). One machine-wide lease per stack lets worktrees coexist; the stack downs at its last holder. ❌ No
  per-check teardown, ❌ never move SMB's frozen `/tmp` lease paths or its 11480+ ports.
- **The lane's nextest filter comes from one fixture table** (`checks/fixture-lane-coverage.go`), guarded by
  `desktop-fixture-lane-coverage`. ❌ Never name a `package(x)` for a crate not yet on disk: nextest can't PARSE the
  filterset. An unmatched `test(prefix)` is fine.
- **An auto-fixer rewriting a COMMITTED file is a green local run and a red CI one.** The run's last line names them;
  commit them.
- **Three CSV logs, never merged**: `~/cmdr-check-log.csv` per run, `~/cmdr-test-log.csv` per test,
  `~/cmdr-unknown-check-log.csv` per rejected selector (a missed name says a check is named wrong; the rows feed a
  naming review, so keep writing them). ❌ Never add a column: it breaks every reader of a log now past 200,000 rows.
- **`--only-slow` needs a ~20 min command timeout** (1,200,000 ms); `--fast` errors out with `--include-slow` /
  `--only-slow`. Named checks bypass both.

Flow diagram, CLI options, exclusive resources, the fixture-stack leases, the per-test and unrecognized-name logs, and
decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
