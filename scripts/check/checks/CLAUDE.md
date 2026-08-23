# Check authoring

One Go file per check, registered in `registry.go`'s `AllChecks`. Runner: `../CLAUDE.md`.

## Module map

- `common.go` (core types + shared utils), `registry.go` (`AllChecks`, lookup, lane filters), `inputs.go` (shared
  `Inputs` blocks + `GlobalInputs`), `runner-sources.go` (which runner files each check reaches), `fixture-stacks.go`
  (the `NeedsContainers` vocabulary), `allowlist.go` / `directives.go` (shrink-wrap and opt-out tracking).
- One `{app}-{name}.go` per check. `test-log.go` and its parsers hold the per-test record vocabulary; `e2e-build.go`
  produces the Playwright lane's binary.
- Warn-only scanners keep a JSON allowlist beside them (`file-length.go`, `claude-md-length.go`,
  `invariant-density.go`, `e2e-durations.go`, `desktop-rust-module-cycles.go`, `lock-poison.go`, plus the pairs sharing
  `bundle-size-baseline.go` / `jscpd.go`). Doc-graph checks: `docs-reachable.go` (+ `docs_graph.go`),
  `docs-dead-links.go`, `docs-link-text.go`.

## Must-knows

- **Every check MUST declare `Inputs`** (the path globs it reads), or `TestEveryCheckDeclaresInputs` fails. Reuse a set
  from `inputs.go`; too-wide costs cache speed, too-narrow costs correctness. Code lanes inherit `agentDocExclusions`,
  so a check READING a `CLAUDE.md` / `DETAILS.md` needs `wholeRepoInputs`.
- **Your check's own source is fingerprinted for you** (`runner-sources.go` follows `Run` through the package). It
  can't see a DATA file (name a new allowlist JSON via `runnerDataInputs`) or an `init()` that registers rather than
  assigns (which drops every check back to the whole tree). `../DETAILS.md` § "The runner's own source".
- **Wire every check into CI** (a step in `ci.yml` / `slow-checks.yml`, or a `NotInCI` reason); `ci-coverage` enforces
  both ways.
- **Length-based truncation is forbidden.** If 200 tests fail, all 200 panic bodies pass through. Filter by structure
  (delimiters, line-anchored regexes), ❌ never by line count.
- **A test lane calls `ctx.RecordTests(...)` BEFORE its pass/fail branch** (`test-log.go`), or a red run never says
  WHICH test failed. An unparsable report records nothing and changes no verdict.

- **Pin every tool install** (❌ never `@latest`), or a compromised tool repo reaches every fresh checkout. Versions,
  sha256s, `--locked`, and the dated nightly: `DETAILS.md` § "Key decisions".
- **A Rust check never hardcodes a source path, its own features, its `Inputs`, or a `cmd.Dir`.** Cargo lanes take
  selection and features from `HostCargoLaneArgs` and inputs from `rustCompileInputs` plus their tool config; scanners
  take roots from `ScannerRoots` / `ScannerMemberKinds` and inputs from `rustScanInputs(<same kinds>)` or
  `rustAppTreeInputs`. ❌ No `tools/**`. A lane asking something else makes the others rebuild `cmdr` (20-100 s a flip);
  `workspace-member-coverage` fails on an unclassified check or unreached member.
- **A new cargo check that COMPILES declares `Exclusive: ResourceCargoBuildDir`** (`common.go`), or it blocks on
  cargo's build-directory lock while holding CPU weight. Metadata-only commands skip that lock.
- **Wire allowlist staleness from day one**: dead entries auto-remove or fail, orphaned opt-out comments fail. Reuse
  `directiveTracker` / `writeJSONAllowlist`, put the file in the check's `Inputs`, and get David's OK before adding or
  raising an entry (`.claude/rules/file-length-allowlist.md`).
- **Error output goes through `indentOutput()`**; success messages carry stats ("12 tests passed"), not "OK". Return
  `Skipped(reason)` when it can't run, `SuccessWithChanges` when it fixed something.
- **`svelte-tests` coverage runs in a per-invocation temp `reportsDirectory`** (`VITEST_COVERAGE_DIR`): a fixed path
  lets concurrent runs clobber each other's v8 worker files.
- **The Playwright lane's release build is NOT incremental** (a no-op rebuild costs 172 s), so `e2e-build.go` stamps the
  binary with what it was built from and skips the build when that matches. Any uncertainty rebuilds.
- **A red Rust lane goes through `resolveRustFailure`**, which re-runs failures alone before believing them; the Docker
  lane execs into its live container, so ❌ don't collapse that into a `docker run`.
- After authoring, run `pnpm check go-vet staticcheck` and update DETAILS § "Apps and check counts". `--fast` membership
  is `IsFast`, hand-curated.

The authoring walkthrough, output-filtering recipes, the nightly bump, workspace geometry, the Rust input blocks, and
decision detail: `DETAILS.md`. Read it before any non-trivial work here.
