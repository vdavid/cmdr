# Check runner

Go CLI running the monorepo's 120 checks in parallel with dependency ordering, via `pnpm check` at the repo root.
Authoring a check: `checks/CLAUDE.md`.

## Module map

- `main.go` (entry), `runner.go` (parallel executor: weighted admission, exclusive resources, dependency graph,
  fail-fast, TTY status line), `plan.go` + `checks/fingerprint.go` + `cache.go` + `runner-sources.go` (the
  input-fingerprint cache).
- `checks/inputs.go` (shared `Inputs` blocks), `checks/cargo-workspace.go` (the geometry every Rust check scopes from),
  `stack_orchestrator.go` + `stacklease/` (each Docker fixture stack behind a machine-wide lease).
- `graph.go` / `docs_graph_render.go` (the two renderers), `stats.go` (the CSV logs), `autofix_notice.go`.

## Must-knows

- **Run from repo root via `pnpm check`.** Positional args mix checks/apps/groups; a named check runs even if
  slow/CI-only/disabled, an app or group selector keeps the default lanes.
- **Checks refuse to run in the main clone** (the auto-fixers reformat tracked files, which belongs in a worktree).
  `--ci` is exempt; override with `--allow-main`.
- **A check fingerprints the runner CORE (`GlobalInputs`) plus the files its own `Run` reaches**
  (`checks/runner-sources.go`, from the AST at plan time), and fails closed to the whole tree. ❌ No helper the EXECUTOR
  calls in a check file. DETAILS § "The runner's own source".
- **The ten Go lanes read three different amounts of the Go trees**: the eight that compile take `goSourceInputs`
  (`.go` + module files), `misspell` the whole tree, `scripts-go-tests` the Rust and frontend trees too (its guards read
  them). `checks/DETAILS.md` § "The Go lanes split three ways".
- **Cache ordering is load-bearing.** Planning runs BEFORE `pnpm install` and Docker bring-up, so an all-hits run
  installs nothing and starts no container; ❌ don't move it after. A corrupt cache degrades to "run everything".
- **CI is the backstop against a wrong `Inputs` list**: `--ci` runs fresh and never writes the cache, so a too-narrow
  `Inputs` masks a regression locally but never ships one. Only `StatusOK` is cached; anything else drops the entry.
- **A cargo lane that COMPILES declares `Exclusive: ResourceCargoBuildDir`**: cargo locks its build dir per command, so
  those lanes are serial anyway, and declaring it stops a blocked one holding CPU weight.
- **Every host cargo lane asks cargo the SAME question** (`HostCargoLaneArgs`); own flags cost 20-100 s of rebuild per
  flip, and ❌ no per-package `-p` lanes. `checks/DETAILS.md` § "One feature set across the cargo lanes".
- **The Rust and frontend lanes are blind to `CLAUDE.md` / `DETAILS.md`** (`agentDocExclusions`), so a docs-only edit is
  a cache hit. ❌ No other `!` exclusion.
- **Lane flags**: `--only-slow` needs a ~20 min timeout (1,200,000 ms); `--fast` errors out with `--include-slow` /
  `--only-slow`. Named checks bypass both. A `Disabled` reason drops a check from EVERY lane, runnable by name only
  (`invariant-density`). DETAILS § "Mothballing a check".
- **A check names its Docker fixtures in `NeedsContainers []StackMode`** (`stacklease` registry: `smb`, `sftp`,
  `webdav`). One machine-wide lease per stack lets worktrees coexist; the stack downs at its last holder. ❌ No
  per-check teardown, ❌ never move SMB's frozen `/tmp` lease paths or its 11480+ ports. DETAILS § "Two fixture stacks,
  two lease namespaces".
- **The lane's filter comes from one fixture table** (`checks/fixture-lane-coverage.go`), guarded by
  `desktop-fixture-lane-coverage`. ❌ Never name a `package(x)` for a crate not yet on disk: nextest can't PARSE the
  filterset. An unmatched `test(prefix)` is fine.
- **An auto-fixer rewriting a COMMITTED file is a green local run and a red CI one**. The run's last line names them;
  commit them.
- **Two CSV logs, never merged**: `~/cmdr-check-log.csv` per run, `~/cmdr-test-log.csv` per test. A tenth column breaks
  every reader of the first's ~148 000 rows.

Flow diagram, CLI options, exclusive resources, the per-test log, and decisions: `DETAILS.md`. Read before non-trivial
work here.
