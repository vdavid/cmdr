# Check authoring

One Go file per check, registered in `registry.go`'s `AllChecks`. Runner: `../CLAUDE.md`.

## Module map

- `common.go` (core types + shared utils), `registry.go` (`AllChecks`, lookup, lane filters), `inputs.go` (shared
  `Inputs` blocks + `GlobalInputs`), `runner-sources.go` (which runner files each check reaches), `fixture-stacks.go`
  (the `NeedsContainers` vocabulary), `allowlist.go` / `directives.go` (shrink-wrap and opt-out tracking).
- One `{app}-{name}.go` per check. `test-log.go` and its parsers hold the per-test record vocabulary; `e2e-build.go`
  produces the Playwright lane's binary.
- Warn-only scanners keep a sibling `<check>-allowlist.json`; not every file here is a registry check. Inventory and
  layout rules: DETAILS § "Key files".

## Must-knows

- **Every check MUST declare `Inputs`** (the path globs it reads), or `TestEveryCheckDeclaresInputs` fails. Reuse a set
  from `inputs.go`; too-wide costs cache speed, too-narrow costs correctness. Code lanes inherit `agentDocExclusions`,
  so a check READING a `CLAUDE.md` / `DETAILS.md` needs `wholeRepoInputs`.
- **A Go TEST that reads the real repo widens `goTestsInputs`.** A guard that only re-runs when its own source changes
  goes green from cache on the edit it exists to catch. Declare what it reads in `realTreeReadingTests`;
  `TestGoTestsInputsCoverTheRealTreeItsTestsRead` fails otherwise. DETAILS § "The Go lanes split three ways".
- **Your check's own source is fingerprinted for you** (`runner-sources.go` follows `Run` through the package). It can't
  see a DATA file (name a new allowlist JSON via `runnerDataInputs`) or an `init()` that registers rather than assigns
  (which drops every check back to the whole tree). `../DETAILS.md` § "The runner's own source".
- **Wire every check into CI** (`ci.yml` / `slow-checks.yml`, or a `NotInCI` reason); `ci-coverage` enforces both ways.
- **Length-based truncation is forbidden.** If 200 tests fail, all 200 panic bodies pass through. Filter by structure,
  ❌ never by line count.
- **A test lane calls `ctx.RecordTests(...)` BEFORE its pass/fail branch** (`test-log.go`), or a red run never says
  WHICH test failed.
- **Pin every tool install** (❌ never `@latest`), or a compromised tool repo reaches every fresh checkout. Versions,
  sha256s, and the dated nightly: DETAILS § "Key decisions".
- **Need a Go version? Call `MiseGoVersion(rootDir)`** — ❌ never a literal. `go-version-single-source` enforces it.
- **A Rust check never hardcodes a source path, its own features, its `Inputs`, or a `cmd.Dir`.** Cargo lanes take both
  from `HostCargoLaneArgs` + `rustCompileInputs`; scanners take `ScannerRoots` / `ScannerMemberKinds` +
  `rustScanInputs(<same kinds>)`. ❌ No `tools/**`. Asking something else makes the others rebuild `cmdr` (20-100 s);
  `workspace-member-coverage` fails on an unclassified check or unreached member.
- **A new cargo check that COMPILES declares `Exclusive: ResourceCargoBuildDir`** (`common.go`), or it blocks on cargo's
  build-directory lock while holding CPU weight.
- **Wire allowlist staleness from day one**: reuse `directiveTracker` / `writeJSONAllowlist`, name the file via
  `runnerDataInputs`, and get David's OK before adding or raising an entry (`.claude/rules/file-length-allowlist.md`).
- **Error output goes through `indentOutput()`**; success messages carry stats ("12 tests passed"), not "OK". Return
  `Skipped(reason)` when it can't run, `SuccessWithChanges` when it fixed something.
- **`svelte-tests` coverage needs a per-invocation temp `reportsDirectory`** (`VITEST_COVERAGE_DIR`): a fixed path lets
  concurrent runs clobber each other's v8 files.
- **The Playwright lane's release build is NOT incremental** (172 s for a no-op), so `e2e-build.go` stamps the binary
  with what built it and skips when that matches. Any uncertainty rebuilds.
- **A red Rust lane goes through `resolveRustFailure`**, which re-runs failures alone before believing them; the Docker
  lane execs into its live container, so ❌ never a `docker run`.
- After authoring, run `pnpm check go-vet staticcheck` and update DETAILS § "Apps and check counts". `--fast` membership
  is `IsFast`, hand-curated.

The authoring walkthrough, output-filtering recipes, the nightly bump, workspace geometry, the Rust input blocks, and
decision detail: `DETAILS.md`. Read it before any non-trivial work here.
