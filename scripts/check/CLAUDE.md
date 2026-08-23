# Check runner

Go CLI running the monorepo's ~50 checks in parallel with dependency ordering, via `pnpm check` at the repo root. Check
authoring: `checks/CLAUDE.md`.

## Module map

- `main.go` (entry point), `runner.go` (parallel executor: weighted admission, exclusive resources, dependency graph,
  fail-fast, TTY status line), `plan.go` + `checks/fingerprint.go` + `checks/cache.go` + `checks/runner-sources.go` (the
  input-fingerprint cache).
- `checks/inputs.go` (shared `Inputs` blocks), `checks/cargo-workspace.go` (the geometry every Rust check scopes from),
  `stack_orchestrator.go` + `stacklease/` (each Docker fixture stack behind a machine-wide lease).
- `freestyle.go` (remote-VM runs), `graph.go` / `docs_graph_render.go` (the `--graph` / `--docs-graph` renderers),
  `stats.go` (the two CSV logs), `autofix_notice.go` (names the committed files auto-fixers rewrote).

## Must-knows

- **Run from repo root via `pnpm check`.** Positional args mix checks/apps/groups; a named check runs even if
  slow/CI-only, an app or group selector keeps the default lanes. `ValidateCheckNames` fails startup on a name shadowing
  one of those keywords.
- **Checks refuse to run in the main clone** (the auto-fixers reformat tracked files, which belongs in a worktree). CI
  is exempt via `--ci`; override with `--allow-main` / `-m`.
- **A check fingerprints the runner CORE (`GlobalInputs`) plus the files its own `Run` reaches**
  (`checks/runner-sources.go`, from the AST at plan time), and fails closed to the whole tree. ❌ Don't put a helper the
  EXECUTOR calls in a check file. DETAILS § "The runner's own source".
- **Cache ordering is load-bearing.** Planning runs BEFORE `pnpm install` and Docker bring-up, so an all-hits run
  installs nothing and starts no container; ❌ don't move it after them. A corrupt cache or non-git tree degrades to
  "run everything".
- **CI is the backstop against a wrong `Inputs` list**: `--ci` runs fresh and never writes the cache, so a too-narrow
  `Inputs` masks a regression locally but never ships one. Only `StatusOK` is cached; warns, failures, and skips drop
  any stale entry.
- **A cargo lane that COMPILES declares `Exclusive: ResourceCargoBuildDir`** (cargo locks its build dir per command, so
  those lanes are serial anyway; declaring it stops a blocked one holding 6-8 CPU weight).
- **Every host cargo lane asks cargo the SAME question** (`HostCargoLaneArgs`); own flags cost 20-100 s of rebuild per
  flip, and ❌ no per-package `-p` lanes. `checks/DETAILS.md` § "One feature set across the cargo lanes".
- **The Rust and frontend lanes are blind to `CLAUDE.md` / `DETAILS.md`** (`agentDocExclusions`), so a docs-only edit is
  a cache hit. ❌ No other `!` exclusion. DETAILS § Exclusions.
- **Lane flags**: `--only-slow` needs a ~20 min timeout (1,200,000 ms); `--fast` errors out with `--include-slow` /
  `--only-slow`. Named checks bypass them.
- **A check names its Docker fixtures in `NeedsContainers []StackMode`** (`stacklease` registry: `smb`, `sftp`). A
  machine-wide lease per stack lets concurrent worktrees coexist; the stack downs at its last holder. ❌ No per-check
  teardown, and ❌ don't move SMB's `/tmp/cmdr-smb.lock` or `/tmp/cmdr-smb-leases` (a sibling worktree on older code
  holds a lease there) or its 11480+ host ports (`checks.ApplySmbPortEnv()`, so cmdr and smb2 coexist).
- **The lane's filter comes from one fixture table** (`checks/fixture-lane-coverage.go`), guarded by
  `desktop-fixture-lane-coverage`. ❌ Never name a `package(x)` for a crate not yet on disk: nextest can't PARSE the
  filterset and the lane dies. An unmatched `test(prefix)` is fine.
- **An auto-fixer rewriting a COMMITTED file is a green local run and a red CI one** (CI only checks formatting). The
  run's last line names them; commit them. DETAILS § "The auto-fix notice".
- **Two CSV logs, never merged**: `~/cmdr-check-log.csv` per run, `~/cmdr-test-log.csv` per test. A tenth column breaks
  every reader of the first's ~148 000 rows.

Flow diagram, CLI options, freestyle.sh, exclusive resources, the per-test log, and decisions: `DETAILS.md`. Read it
before any non-trivial work here.
