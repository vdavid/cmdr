# Check runner

Go CLI running the monorepo's 149 checks in parallel with dependency ordering, via `pnpm check` at the repo root.
Authoring a check: `checks/CLAUDE.md`.

## Module map

- `main.go` (entry), `select.go` (selectors), `runner.go` (parallel executor), `plan.go` + `checks/fingerprint.go` (the
  input-fingerprint cache), `stack_orchestrator.go` + `stacklease/` (Docker fixture stacks). The rest: `DETAILS.md` §
  Key files.

## Must-knows

- **Run from repo root via `pnpm check`.** A named check runs even if slow, CI-only, or disabled; an app or group
  selector keeps the default lanes.
- **Checks refuse to run in the main clone** (the auto-fixers rewrite tracked files): `--ci` is exempt, `--allow-main`
  overrides. A run blocks while a fresh worktree is still warming (`.warming-worktree`).
- **A check fingerprints `GlobalInputs` plus the files its own `Run` reaches** (read from the AST), failing closed to
  the whole tree. ❌ No helper the EXECUTOR calls in a check file.
- **Cache planning runs BEFORE `pnpm install` and Docker bring-up**, so an all-hits run starts nothing; ❌ don't move it
  after.
- **A cargo lane that compiles declares `Exclusive: ResourceCargoBuildDir` and asks cargo via `HostCargoLaneArgs`**: own
  flags cost 20–100 s of rebuild per flip. ❌ No per-package `-p` lanes.
- **The Rust and frontend lanes ignore `CLAUDE.md` / `DETAILS.md`** (`agentDocExclusions`), so a docs-only edit is a
  cache hit. ❌ No other `!` exclusion.
- **Docker fixtures go in `NeedsContainers`** (one machine-wide `stacklease` lease per stack). ❌ No per-check teardown,
  ❌ never move SMB's `/tmp` lease paths or its 11480+ ports.
- **The nextest fixture filter comes from `checks/fixture-lane-coverage.go`.** ❌ Never name a `package(x)` for a crate
  not yet on disk: nextest can't parse the filterset.
- **An auto-fixer rewriting a committed file is green locally and red in CI.** The run's last line names them; commit
  them.
- **The four CSV logs live in `~/.local/share/check-runner/cmdr/`.** ❌ Never add a column: it breaks every reader.
- **Output is capped at 40 lines / 8 KB per check**, with every failure saved whole in `output/`; `-v` prints all. ❌
  Don't raise the cap for one noisy check; tighten its output.
- **`--only-slow` needs a ~20 min command timeout.**

Flow diagram, CLI options, exclusive resources, fixture-stack leases, the logs, and decisions: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
