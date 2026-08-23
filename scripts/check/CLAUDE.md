# Check runner

Go CLI that runs all code quality checks for the Cmdr monorepo (~50 checks across 5 apps) in parallel with dependency
ordering. Invoked via `pnpm check` at the repo root. Check authoring: `checks/CLAUDE.md`.

## Module map

- `main.go` (entry point), `runner.go` (parallel executor: weighted admission, exclusive resources, dependency graph,
  fail-fast, TTY status line), `plan.go` + `checks/fingerprint.go` + `checks/cache.go` (the input-fingerprint cache).
- `checks/inputs.go` (shared `Inputs` blocks) and `checks/cargo-workspace.go` (the geometry every Rust check scopes
  from). `stack_orchestrator.go` + `stacklease/` run each Docker fixture stack behind its own machine-wide lease.
- `freestyle.go` (remote-VM runs), `graph.go` / `docs_graph_render.go` (the `--graph` / `--docs-graph` renderers),
  `stats.go` (the two CSV logs), `autofix_notice.go` (names the committed files a run's auto-fixers rewrote).

## Must-knows

- **Run from repo root via `pnpm check`.** Positional args mix checks/apps/groups; a named check runs even if
  slow/CI-only, an app/group selector keeps the default lanes. `ValidateCheckNames` fails startup on a name shadowing a
  group/app keyword.
- **Checks refuse to run in the main clone** (the auto-fixers reformat tracked files, which only belongs in a worktree).
  CI is exempt via `--ci`; override with `--allow-main` / `-m`.
- **Cache ordering is load-bearing.** Planning runs BEFORE `pnpm install` and Docker bring-up, so an all-hits run
  installs nothing and starts no container. ❌ Don't move planning after them. A corrupt cache or non-git tree degrades
  to "run everything".
- **CI is the authoritative backstop against a wrong `Inputs` list**: `--ci` runs fresh and never writes the cache, so a
  too-narrow `Inputs` masks a regression locally but never ships one. Only `StatusOK` is cached; warns, failures, and
  skips drop any stale entry.
- **A cargo lane that COMPILES declares `Exclusive: ResourceCargoBuildDir`** (cargo locks its build dir per command, so
  those lanes are serial anyway; declaring it stops a blocked one from holding 6-8 CPU weight).
- **Every host cargo lane asks cargo the SAME question** (`HostCargoLaneArgs`); own flags cost 20-100 s of rebuild per
  flip, and ❌ no per-package `-p` lanes. `checks/DETAILS.md` § "One feature set across the cargo lanes".
- **The Rust and frontend lanes are blind to `CLAUDE.md` / `DETAILS.md`** (`agentDocExclusions`), so a docs-only edit is
  a cache hit. ❌ No other `!` exclusion is allowed. DETAILS § Exclusions.
- **Lane flags**: `--only-slow` needs a ~20 min timeout (1,200,000 ms); `--fast` errors out with `--include-slow` /
  `--only-slow`. Named checks bypass lane filters.
- **A check names its Docker fixtures in `NeedsContainers []StackMode`** (`stacklease` registry: `smb`, `sftp`). Each
  stack has a machine-wide lease, so concurrent worktree runs coexist and a stack downs at its last holder. ❌ No
  per-check teardown, and ❌ don't move SMB's `/tmp/cmdr-smb.lock` or `/tmp/cmdr-smb-leases`: a sibling worktree on
  older code holds a lease there.
- **The lane's filter comes from one fixture table** (`checks/fixture-lane-coverage.go`), guarded by
  `desktop-fixture-lane-coverage`. ❌ Never name a `package(x)` for a crate not yet on disk: nextest fails to PARSE the
  filterset and the lane dies. An unmatched `test(prefix)` is fine.
- **cmdr's SMB stack binds host ports 11480+, not smb2's default 10480+**, so both harnesses coexist
  (`checks.ApplySmbPortEnv()`); ❌ don't revert to the default range.
- **An auto-fixer rewriting a COMMITTED file is a green local run and a red CI one** (CI only checks formatting). The
  run's last line names them; commit them. `autofix_notice.go`, DETAILS § "The auto-fix notice".
- **Two CSV logs, never merged**: `~/cmdr-check-log.csv` per check run, `~/cmdr-test-log.csv` per test. A tenth column
  breaks every reader of the first's ~148 000 rows.

Flow diagram, CLI options, freestyle.sh execution, exclusive resources, the per-test log, and decisions: `DETAILS.md`.
Read it before any non-trivial work here.
