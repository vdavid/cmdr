# Check authoring

One Go file per check, registered in `registry.go`'s `AllChecks`. Runner: `../CLAUDE.md`.

## Module map

- `common.go` (core types + shared utils), `registry.go` (`AllChecks`, lookup, lane filters), `inputs.go` (shared
  `Inputs` blocks + `GlobalInputs`), `runner-sources.go` (which runner files each check reaches), `fixture-stacks.go`
  (the `NeedsContainers` vocabulary), `allowlist.go` / `directives.go` (shrink-wrap and opt-out tracking).
- One `{app}-{name}.go` per check. `test-log.go` and its parsers hold the per-test record vocabulary; `e2e-build.go`
  produces the Playwright lane's binary.
- Ratcheting scanners keep a sibling `<check>-allowlist.json`; not every file here is a registry check. Inventory:
  DETAILS § "Key files".

## Must-knows

- **Every check MUST declare `Inputs`** (the path globs it reads), or `TestEveryCheckDeclaresInputs` fails. Reuse a set
  from `inputs.go`; too-wide costs cache speed, too-narrow costs correctness. Code lanes inherit `agentDocExclusions`,
  so a check READING a `CLAUDE.md` / `DETAILS.md` needs `wholeRepoInputs`.
- **A Go TEST that reads the real repo widens `goTestsInputs`**: declare what it reads in `realTreeReadingTests`, or it
  goes green from cache on the very edit it exists to catch. `TestGoTestsInputsCoverTheRealTreeItsTestsRead` enforces
  it. DETAILS § "The Go lanes split three ways".
- **Your check's own source is fingerprinted** (`runner-sources.go` follows `Run` through the package). It can't see a
  DATA file (name a new allowlist JSON via `runnerDataInputs`) or an `init()` that registers rather than assigns (which
  drops every check back to the whole tree). `../DETAILS.md` § "The runner's own source".
- **Wire every check into CI** (`ci.yml` / `slow-checks.yml`, or a `NotInCI` reason); `ci-coverage` enforces both ways.
- **Length-based truncation is forbidden**: if 200 tests fail, all 200 panic bodies pass through. Filter by structure,
  ❌ never by line count.
- **A test lane calls `ctx.RecordTests(...)` BEFORE its pass/fail branch** (`test-log.go`), or a red run never says
  WHICH test failed.
- **Pin every tool install** (❌ never `@latest`), or a compromised tool repo reaches every fresh checkout;
  `EnsureGoTool` enforces it. Versions, sha256s, and the dated nightly: DETAILS § "Key decisions".
- **Need a Go version? `MiseGoVersion(rootDir)`**, ❌ never a literal; `go-version-single-source` enforces it.
- **A Rust check never hardcodes a source path, its own features, its `Inputs`, or a `cmd.Dir`.** Cargo lanes take them
  from `HostCargoLaneArgs` + `rustCompileInputs`; scanners from `ScannerRoots` / `ScannerMemberKinds` +
  `rustScanInputs(<same kinds>)`. ❌ No `tools/**`; anything else rebuilds `cmdr` for the others (20-100 s).
  `workspace-member-coverage` enforces it. DETAILS § "Workspace geometry".
- **A new cargo check that COMPILES declares `Exclusive: ResourceCargoBuildDir`** (`common.go`), or it blocks on cargo's
  build-directory lock while holding CPU weight.
- **Wire allowlist staleness from day one**: reuse `directiveTracker` / `writeJSONAllowlist`, name the file via
  `runnerDataInputs`, and give every entry a mandatory `reason`.
- **Error output goes through `indentOutput()`**; success messages carry stats ("12 tests passed"), not "OK". Return
  `Skipped(reason)` when it can't run, `SuccessWithChanges` when it fixed something.
- **Vitest lanes:** coverage needs a per-invocation `reportsDirectory` (`VITEST_COVERAGE_DIR`), or concurrent runs
  clobber each other's v8 files; ❌ a timeout's real message is never in the json report. DETAILS § "Vitest failure
  output".
- **The Playwright lane's release build is NOT incremental** (172 s for a no-op), so `e2e-build.go` stamps the binary
  with what built it and skips on a match; any uncertainty rebuilds.
- **A red test lane re-runs its failures ALONE before believing them**; the verdicts are shared
  (`contention-verdict.go`). Rust: `resolveRustFailure`, the Docker lane execing into its live container, ❌ never a
  `docker run`. Playwright: `resolveE2EFailure`, by spec FILE.
- After authoring, run `pnpm check go-vet staticcheck` and update DETAILS § "Apps and check counts". `--fast` membership
  is `IsFast`, hand-curated.

The authoring walkthrough, output filtering, the nightly bump, workspace geometry, and decisions: `DETAILS.md`. Read it
before any non-trivial work here: editing, planning, reorganizing, or advising.
