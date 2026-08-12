# Check runner

Go CLI that runs all code quality checks for the Cmdr monorepo (~50 checks across 5 apps) in parallel with dependency
ordering. Invoked via `pnpm check` at the repo root.

Check authoring (adding a check, `CheckDefinition` shape, naming rules, helpers, allowlists): `checks/CLAUDE.md`. Flow
diagram, CLI options, freestyle.sh execution, and decisions: `DETAILS.md`.

## Module map

- `main.go`: entry point (flag parsing, root dir discovery, check selection, pnpm gating, runner delegation).
- `runner.go`: parallel executor (weighted admission gate, exclusive resources, dependency graph, fail-fast, TTY status
  line).
- `plan.go` + `checks/fingerprint.go` + `checks/cache.go`: the input-fingerprint cache (split selected checks into hits
  and misses before pnpm/SMB; record passes after the run).
- `checks/inputs.go`: shared `Inputs` building blocks (mined from ci.yml filters). `checks/cargo-workspace.go`: the
  cargo workspace's geometry, which every Rust check derives its scope from.
- `smb_orchestrator.go` + `smblease/` + `smb-lease/`: runner-level SMB Docker lifecycle behind a machine-wide lease.
- `freestyle.go`: freestyle.sh remote-VM execution. `graph.go` / `docs_graph_render.go` (+ `docs_graph_usage.go`): the
  `--graph` and `--docs-graph` renderers. `stats.go`: the two CSV logs (per check, and per test).

## Must-knows

- **Run from repo root via `pnpm check`.** Positional args select checks/apps/groups in any mix; named checks run even
  if slow/CI-only, app/group selectors keep the default lanes. `ValidateCheckNames` fails startup if a check ID or
  nickname would shadow a reserved group/app keyword (`desktop`, `website`, `api-server`, `dashboard`, `scripts`,
  `rust`, `svelte`, `go`), so resolution order (check → app → group) can't silently change meaning.
- **Checks refuse to run in the main clone** (the auto-fixers reformat tracked files; that only happens in a worktree).
  Detection: `--git-dir` == `--git-common-dir` (`isMainWorkingTree`). CI is exempt via `--ci`; override a deliberate
  local main run with `--allow-main` / `-m`. `tauri-wrapper.ts` carries the same guard for `pnpm dev` (not `build`).
- **Cache ordering is load-bearing.** Planning runs BEFORE `pnpm install` and SMB/Docker bring-up, so a run whose
  node/SMB checks are all cache hits installs no deps and starts no container. Don't move planning after them. A corrupt
  cache or non-git tree degrades to "run everything", never an error.
- **CI is the authoritative backstop against a wrong `Inputs` list.** `--ci` runs fresh and never writes the cache, so a
  too-narrow `Inputs` can mask a regression locally but never ship one. Named checks and `--fresh` /
  `CMDR_CHECK_NO_CACHE=1` also bypass it. Only `StatusOK` is cached; warns, failures, and skips re-run and drop any
  stale entry.
- **A cargo lane that COMPILES declares `Exclusive: ResourceCargoBuildDir`.** Cargo locks its build directory per
  command, so those lanes are serial anyway; declaring it stops a blocked one from sitting on 6-8 CPU weight.
  `DETAILS.md` § "Exclusive resources".
- **Lane flags:** `--only-slow` needs a ~20 min timeout (1,200,000 ms) from an agent or CI, since E2E and
  `rust-tests-linux` run far longer than the default suite; `--fast` is mutually exclusive with `--include-slow` /
  `--only-slow` and errors out when combined. Named checks bypass every lane filter.
- **Concurrent SMB-touching runs across worktrees coexist** via per-run machine-wide `smblease` leases on the shared
  `smb-consumer` stack: it downs only when the last holder leaves, so a finishing run never kills another's mid-test.
  Don't reintroduce per-check or per-process teardown. Inspect and force-down recipes: `DETAILS.md` § Gotchas.
- **Two CSV logs, never merged.** `~/cmdr-check-log.csv` per check run, `~/cmdr-test-log.csv` per test (`DETAILS.md` §
  "The per-test log"). A tenth column breaks every reader of the first's ~98 000 nine-column rows.
- **cmdr's SMB stack binds host ports 11480+, not smb2's default 10480+**, so both harnesses coexist instead of fighting
  over ports. `checks.ApplySmbPortEnv()` sets this before bring-up; don't revert to the default range.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
