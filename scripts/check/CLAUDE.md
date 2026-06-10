# Check runner

Go CLI that runs all code quality checks for the Cmdr monorepo (~42 checks across 4 apps) in parallel with dependency
ordering. Invoked via `pnpm check` at the repo root.

For check authoring (how to add a check, `CheckDefinition` shape, naming rules, helpers, allowlists), see
[`checks/CLAUDE.md`](checks/CLAUDE.md). For the full flow diagram, CLI options, freestyle.sh execution, and decisions,
see [DETAILS.md](DETAILS.md).

## Key files

- `main.go`: entry point (flag parsing, root dir discovery, check selection, pnpm gating, runner delegation).
- `runner.go`: parallel executor (CPU-weighted admission gate, dependency graph, fail-fast, live TTY status line).
- `plan.go` + `checks/fingerprint.go` + `checks/cache.go`: the input-fingerprint cache (split selected checks into hits
  and misses before pnpm/SMB; record passes after the run).
- `checks/inputs.go`: shared `Inputs` building blocks (mined from ci.yml filters).
- `smb_orchestrator.go` + `smblease/` + `smb-lease/`: runner-level SMB Docker lifecycle behind a machine-wide lease.
- `freestyle.go`: all freestyle.sh remote-VM execution. `graph.go`: `--graph` renderer. `stats.go`: CSV stats logging.

## Must-knows

- **Run from repo root via `pnpm check`.** Positional args select checks/apps/groups in any mix; named checks run even
  if slow/CI-only, app/group selectors keep the default lanes. `ValidateCheckNames` fails startup if a check ID or
  nickname would shadow a reserved group/app keyword (`desktop`, `website`, `api-server`, `scripts`, `rust`, `svelte`,
  `go`), so resolution order (check → app → group) can't silently change meaning.
- **Cache ordering is load-bearing.** Planning runs BEFORE `pnpm install` and SMB/Docker bring-up, so a run whose
  node/SMB checks are all cache hits installs no deps and starts no container. Don't move planning after them. A
  corrupt/missing cache or non-git tree degrades to "run everything", never an error.
- **CI is the authoritative backstop against a wrong `Inputs` list.** `--ci` always runs fresh and never writes the
  cache, so a too-narrow `Inputs` can mask a regression locally but never ship one. Named checks (positional or
  `--check`) and `--fresh` / `CMDR_CHECK_NO_CACHE=1` also bypass the cache. Only `StatusOK` results are cached; warns,
  failures, and skips always re-run and drop any stale entry.
- **`--only-slow` needs a ~20 min timeout** (1,200,000 ms). Slow checks (E2E, `rust-tests-linux`) take far longer than
  the default suite; when running via an agent or CI, set the timeout accordingly.
- **`--fast` is mutually exclusive with `--include-slow` / `--only-slow`:** combining them errors out, since the lanes
  are intentionally separate. Named check invocations bypass lane filters (`pnpm check --fast svelte-check` still runs
  it).
- **Concurrent SMB-touching runs across worktrees coexist** via per-run machine-wide `smblease` leases on the shared
  `smb-consumer` stack: the stack downs only when the last holder leaves, so a finishing run never kills another's
  mid-test. Don't reintroduce per-check or per-process teardown. Inspect with
  `(cd scripts/check && go run ./smb-lease status)`; force down with
  `rm -rf /tmp/cmdr-smb-leases && apps/desktop/test/smb-servers/stop.sh`.
- **cmdr's SMB stack binds host ports 11480+, not smb2's default 10480+**, so cmdr's vendored `smb-consumer` compose and
  smb2's own `consumer` harness coexist instead of fighting over ports. `checks.ApplySmbPortEnv()` sets this before
  bring-up; don't revert to the default range.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
