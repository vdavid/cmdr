# Check runner

Go CLI that runs all code quality checks for the Cmdr monorepo (~50 checks across 5 apps) in parallel with dependency
ordering. Invoked via `pnpm check` at the repo root. Check authoring: `checks/CLAUDE.md`.

## Module map

- `main.go` (entry point), `runner.go` (parallel executor: weighted admission, exclusive resources, dependency graph,
  fail-fast, TTY status line), `plan.go` + `checks/fingerprint.go` + `checks/cache.go` (the input-fingerprint cache).
- `checks/inputs.go` (shared `Inputs` blocks) and `checks/cargo-workspace.go` (the geometry every Rust check scopes
  from). `smb_orchestrator.go` + `smblease/` run SMB Docker behind a machine-wide lease.
- `freestyle.go` (remote-VM runs), `graph.go` / `docs_graph_render.go` (the `--graph` / `--docs-graph` renderers),
  `stats.go` (the two CSV logs).

## Must-knows

- **Run from repo root via `pnpm check`.** Positional args select checks/apps/groups in any mix; named checks run even
  if slow/CI-only, app/group selectors keep the default lanes. `ValidateCheckNames` fails startup on a name that would
  shadow a reserved group/app keyword.
- **Checks refuse to run in the main clone** (the auto-fixers reformat tracked files, which only belongs in a worktree).
  CI is exempt via `--ci`; override with `--allow-main` / `-m`.
- **Cache ordering is load-bearing.** Planning runs BEFORE `pnpm install` and SMB/Docker bring-up, so an all-hits run
  installs no deps and starts no container. ❌ Don't move planning after them. A corrupt cache or non-git tree degrades
  to "run everything", never an error.
- **CI is the authoritative backstop against a wrong `Inputs` list**: `--ci` runs fresh and never writes the cache, so a
  too-narrow `Inputs` masks a regression locally but never ships one. Only `StatusOK` is cached; warns, failures, and
  skips drop any stale entry.
- **A cargo lane that COMPILES declares `Exclusive: ResourceCargoBuildDir`** (cargo locks its build dir per command, so
  those lanes are serial anyway, and declaring it stops a blocked one from sitting on 6-8 CPU weight).
- **Every host cargo lane asks cargo the SAME question** (`HostCargoLaneArgs`); own flags cost 20-100 s of rebuild per
  flip. `checks/DETAILS.md` § "One feature set across the cargo lanes".
- **The Rust and frontend lanes are blind to `CLAUDE.md` / `DETAILS.md`** (`agentDocExclusions`), so a docs-only edit is
  a cache hit. ❌ No other `!` exclusion is allowed. DETAILS § Exclusions.
- **Lane flags**: `--only-slow` needs a ~20 min timeout (1,200,000 ms); `--fast` errors out combined with
  `--include-slow` / `--only-slow`. Named checks bypass lane filters.
- **Concurrent SMB-touching runs across worktrees coexist** via per-run machine-wide `smblease` leases on the shared
  `smb-consumer` stack, which downs only when the last holder leaves. ❌ Don't reintroduce per-check or per-process
  teardown.
- **cmdr's SMB stack binds host ports 11480+, not smb2's default 10480+**, so both harnesses coexist.
  `checks.ApplySmbPortEnv()` sets this before bring-up; ❌ don't revert to the default range.
- **Two CSV logs, never merged**: `~/cmdr-check-log.csv` per check run, `~/cmdr-test-log.csv` per test. A tenth column
  breaks every reader of the first's ~98 000 rows.

Flow diagram, CLI options, freestyle.sh execution, exclusive resources, the per-test log, and decisions: `DETAILS.md`.
Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
