# Check authoring

Every check lives in this directory as a single Go file, registered in `registry.go`'s `AllChecks` slice. For the full
authoring walkthrough (`CheckDefinition` field semantics, helpers, allowlist mechanics, decisions), see
[DETAILS.md](DETAILS.md). For the runner architecture (parallel executor, dependency graph, CLI flags, freestyle.sh),
see [`../CLAUDE.md`](../CLAUDE.md).

## Key files

- `common.go`: core types (`CheckDefinition`, `CheckResult`, `CheckContext`, `CheckFunc`) and shared utils
  (`RunCommand`, `EnsureGoTool`, `CommandExists`, `runPrettierCheck`, `runESLintCheck`, `indentOutput`,
  `trimBuildNoise`).
- `registry.go`: `AllChecks`, the canonical ordered list, plus lookup/filter functions (`FilterSlowChecks`,
  `FilterCIOnlyChecks`, `FilterFastChecks`, `ValidateCheckNames`).
- `{app}-{name}.go`: one file per check (`desktop-rust-*`, `desktop-svelte-*`, `website-*`, `api-server-*`,
  `scripts-go-*`).
- `inputs.go`: shared `Inputs` building blocks. `allowlist.go` / `directives.go`: allowlist shrink-wrap + opt-out
  tracking plumbing.
- Warn-only scanners with their JSON allowlists: `file-length.go`, `e2e-durations.go`, `website-bundle-size.go`.

## Must-knows

- **Every check MUST declare `Inputs`** (the path globs it reads), or `TestEveryCheckDeclaresInputs` fails the suite. An
  empty list fingerprints on global inputs alone, so the check gets cache-skipped even when its own files change: a
  correctness hole. Reuse a shared set from `inputs.go`, and **be conservative**: when unsure whether the check reads a
  path, include it. Too-wide only costs cache speed; too-narrow costs correctness. Don't list the auto-added globals
  (`.mise.toml`, `scripts/check/**`).
- **Wire every check into CI** (a step in `.github/workflows/ci.yml` / `slow-checks.yml`, or a `NotInCI` reason on the
  definition). The `ci-coverage` check fails the suite until you do one or the other, both ways: a check neither invoked
  nor excused fails, and a check that has a `NotInCI` reason but IS invoked fails as a stale excuse. There's no
  "registered but runs nowhere" state.
- **Length-based truncation is forbidden.** If 200 tests fail, all 200 panic bodies pass through. Filter by structure
  (section delimiters, line-anchored regexes for harness noise), never by max-line count. See DETAILS.md "E2E failure
  output" and "cargo test output" decisions for the section-aware patterns to follow.
- **Pin every tool install.** `EnsureGoTool` `installPath` pins `@vX.Y.Z` (never `@latest`); `cargo install` pins both
  `--version` and `--locked`. Every operational `cargo` command in a check passes `--locked`. Unpinned installs let a
  compromised tool repo auto-propagate to every fresh checkout.
- **Wire allowlist staleness from day one.** If a check grows an allowlist or an opt-out comment, dead entries must
  auto-remove or fail, and orphaned opt-out comments must fail. Reuse `directiveTracker` / `writeJSONAllowlist`. Agents
  never add or raise an allowlist entry (file-length, e2e-duration, bundle-size baseline) without David's OK.
- **Error output uses `indentOutput()`**: `fmt.Errorf("check failed\n%s", indentOutput(output))`. Success messages carry
  useful stats ("12 tests passed"), not generic "OK". Return `Skipped(reason)` when a check can't run,
  `SuccessWithChanges` when it made local fixes (CI mode must still error on the same drift).
- After authoring, run `pnpm check go-vet staticcheck` (staticcheck is strict about idiomatic Go), and update the "Apps
  and check counts" table in DETAILS.md plus `AGENTS.md`'s fast-lane list if the check is `IsFast`.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
