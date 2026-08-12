# Check authoring

Each check is one Go file here, registered in `registry.go`'s `AllChecks`. Full authoring walkthrough: `DETAILS.md`.
Runner architecture: `../CLAUDE.md`.

## Module map

- `common.go`: core types (`CheckDefinition`, `CheckResult`, `CheckContext`, `CheckFunc`) and shared utils
  (`RunCommand`, `EnsureGoTool`, `CommandExists`, `runPrettierCheck`, `indentOutput`, `trimBuildNoise`).
- `registry.go`: `AllChecks`, the canonical ordered list, plus lookup/filters (`FilterSlowChecks`, `FilterCIOnlyChecks`,
  `FilterFastChecks`, `ValidateCheckNames`).
- `{app}-{name}.go`: one file per check (`desktop-rust-*`, `desktop-svelte-*`, `website-*`, `api-server-*`,
  `scripts-go-*`).
- `inputs.go`: shared `Inputs` blocks. `allowlist.go` / `directives.go`: allowlist shrink-wrap + opt-out tracking.
- `test-log.go`: per-test record vocabulary; parsers in `rust-test-diagnostics.go`, `vitest-test-log.go`,
  `e2e-test-log.go`. `e2e-build.go`: producing the Playwright lane's binary (compile, find, sign, freshness stamp);
  `desktop-svelte-e2e-playwright.go` runs the suite against it.
- Warn-only scanners with JSON allowlists: `file-length.go`, `claude-md-length.go`, `e2e-durations.go`,
  `website-bundle-size.go`. Error-level doc-graph checks: `docs-reachable.go` (+ shared `docs_graph.go`),
  `docs-dead-links.go`, `docs-link-text.go`.

## Must-knows

- **Every check MUST declare `Inputs`** (the path globs it reads), or `TestEveryCheckDeclaresInputs` fails the suite. An
  empty list fingerprints on the globals alone, so the check is cache-skipped when its own files change. Reuse a set
  from `inputs.go` and be conservative: too-wide costs cache speed, too-narrow costs correctness. Don't list the
  auto-added globals (`.mise.toml`, `scripts/check/**`). Code lanes inherit `agentDocExclusions`, so they never see a
  `CLAUDE.md` / `DETAILS.md` edit; a check that reads one needs `wholeRepoInputs`.
- **Wire every check into CI** (a step in `.github/workflows/ci.yml` / `slow-checks.yml`, or a `NotInCI` reason).
  `ci-coverage` enforces it both ways, so there's no "registered but runs nowhere" state.
- **Length-based truncation is forbidden.** If 200 tests fail, all 200 panic bodies pass through. Filter by structure
  (section delimiters, line-anchored regexes), never by line count. DETAILS.md §§ "E2E failure output", "cargo test
  output".
- **A test lane calls `ctx.RecordTests(...)` BEFORE its pass/fail branch** (`test-log.go`), or a red run never says
  WHICH test failed. One shared mechanism; an unparsable report records nothing and changes no verdict.
- **Pin every tool install**, or a compromised tool repo auto-propagates to every fresh checkout. `EnsureGoTool` pins
  `@vX.Y.Z` (never `@latest`), `cargo install` pins `--version` + `--locked`, a prebuilt binary pins its sha256
  (`desktop-third-party-notices.go`), and every operational `cargo` passes `--locked`. Toolchains count too:
  `cargo-udeps` runs on the dated `nightlyToolchain` CI reads via `check --print-nightly`. Bumping it: DETAILS.md §
  "Bumping the pinned nightly".
- **A Rust check never hardcodes a source path, its own features, or a `cmd.Dir`.** Cargo lanes take selection AND
  features from `HostCargoLaneArgs` (the container lane computes for `linux`); scanners take roots from `ScannerRoots` /
  `ScannerMemberKinds`. A lane asking something different makes the others rebuild `cmdr`: 20-100 s per flip.
  `workspace-member-coverage` fails on an unclassified check or unreached member. DETAILS.md §§ "Workspace geometry",
  "One feature set across the cargo lanes".
- **A new cargo check that COMPILES declares `Exclusive: ResourceCargoBuildDir`** (`common.go`), or it blocks on cargo's
  build-directory lock while holding CPU weight. Metadata-only cargo commands don't take that lock.
- **Wire allowlist staleness from day one.** Dead entries auto-remove or fail; orphaned opt-out comments fail. Reuse
  `directiveTracker` / `writeJSONAllowlist`. Never add or raise an entry without David's OK.
- **Error output uses `indentOutput()`**: `fmt.Errorf("check failed\n%s", indentOutput(output))`. Success messages carry
  stats ("12 tests passed"), not "OK". Return `Skipped(reason)` when a check can't run, `SuccessWithChanges` when it
  made local fixes (CI must still error on the same drift).
- **`svelte-tests` coverage runs in a per-invocation temp `reportsDirectory`** (via `VITEST_COVERAGE_DIR`): a fixed path
  lets concurrent runs clobber each other's in-flight v8 worker files (`ENOENT`). DETAILS.md § "svelte-tests coverage
  isolation".
- **The Playwright lane's release build is NOT incremental** (a no-op rebuild costs 172 s), so `e2e-build.go` stamps the
  binary with what it was compiled from and skips the build when that matches. The set drops `apps/desktop/test/**`;
  every uncertainty rebuilds. DETAILS.md § "The Playwright lane's binary is fingerprinted".
- **A red Rust lane goes through `resolveRustFailure`**, which re-runs failures alone before believing them. Lanes
  inject only WHERE: the Docker lane execs into its still-live container, so don't collapse that into one `docker run`.
  DETAILS.md § "The contention re-run".
- After authoring, run `pnpm check go-vet staticcheck` and update DETAILS.md § "Apps and check counts". `--fast`
  membership is `IsFast` in `registry.go`, curated by hand.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
