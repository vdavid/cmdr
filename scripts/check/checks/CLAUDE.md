# Check authoring

Each check is one Go file here, registered in `registry.go`'s `AllChecks`. Runner architecture: `../CLAUDE.md`.

## Module map

- `common.go` (core types + shared utils), `registry.go` (`AllChecks`, lookup, lane filters), `inputs.go` (shared
  `Inputs` blocks), `allowlist.go` / `directives.go` (allowlist shrink-wrap and opt-out tracking).
- One `{app}-{name}.go` per check. `test-log.go` and the parsers beside it hold the per-test record vocabulary;
  `e2e-build.go` produces the Playwright lane's binary.
- Warn-only scanners with JSON allowlists: `file-length.go`, `claude-md-length.go`, `invariant-density.go`,
  `e2e-durations.go`, `website-bundle-size.go`, and the two copy-paste lanes over shared `jscpd.go`. Error-level
  doc-graph checks: `docs-reachable.go` (+ `docs_graph.go`), `docs-dead-links.go`, `docs-link-text.go`.

## Must-knows

- **Every check MUST declare `Inputs`** (the path globs it reads), or `TestEveryCheckDeclaresInputs` fails. An empty
  list fingerprints on the globals alone, so the check is cache-skipped when its own files change. Reuse a set from
  `inputs.go`; too-wide costs cache speed, too-narrow costs correctness. Code lanes inherit `agentDocExclusions`, so a
  check that READS a `CLAUDE.md` / `DETAILS.md` needs `wholeRepoInputs`.
- **Wire every check into CI** (a step in `ci.yml` / `slow-checks.yml`, or a `NotInCI` reason). `ci-coverage` enforces
  it both ways, so there's no "registered but runs nowhere" state.
- **Length-based truncation is forbidden.** If 200 tests fail, all 200 panic bodies pass through. Filter by structure
  (section delimiters, line-anchored regexes), ❌ never by line count.
- **A test lane calls `ctx.RecordTests(...)` BEFORE its pass/fail branch** (`test-log.go`), or a red run never says
  WHICH test failed. An unparsable report records nothing and changes no verdict.
- **Pin every tool install**, or a compromised tool repo auto-propagates to every fresh checkout: `EnsureGoTool` pins
  `@vX.Y.Z` (❌ never `@latest`), `cargo install` pins `--version` + `--locked`, a prebuilt binary pins its sha256, and
  every operational `cargo` passes `--locked`. Toolchains count too (`cargo-udeps` on the dated `nightlyToolchain`).
- **A Rust check never hardcodes a source path, its own features, or a `cmd.Dir`**: cargo lanes take selection AND
  features from `HostCargoLaneArgs`, scanners take roots from `ScannerRoots` / `ScannerMemberKinds`. A lane asking
  something different makes the others rebuild `cmdr` (20-100 s per flip), and `workspace-member-coverage` fails on an
  unclassified check or unreached member.
- **A new cargo check that COMPILES declares `Exclusive: ResourceCargoBuildDir`** (`common.go`), or it blocks on cargo's
  build-directory lock while holding CPU weight. Metadata-only cargo commands don't take that lock.
- **Wire allowlist staleness from day one**: dead entries auto-remove or fail, orphaned opt-out comments fail. Reuse
  `directiveTracker` / `writeJSONAllowlist`. ❌ Never add or raise an entry without David's OK.
- **Error output uses `indentOutput()`**; success messages carry stats ("12 tests passed"), not "OK". Return
  `Skipped(reason)` when a check can't run, `SuccessWithChanges` when it made local fixes.
- **`svelte-tests` coverage runs in a per-invocation temp `reportsDirectory`** (via `VITEST_COVERAGE_DIR`): a fixed path
  lets concurrent runs clobber each other's in-flight v8 worker files.
- **The Playwright lane's release build is NOT incremental** (a no-op rebuild costs 172 s), so `e2e-build.go` stamps the
  binary with what it was compiled from and skips the build when that matches. Every uncertainty rebuilds.
- **A red Rust lane goes through `resolveRustFailure`**, which re-runs failures alone before believing them; the Docker
  lane execs into its still-live container, so ❌ don't collapse that into one `docker run`.
- After authoring, run `pnpm check go-vet staticcheck` and update DETAILS § "Apps and check counts". `--fast` membership
  is `IsFast` in `registry.go`, curated by hand.

The authoring walkthrough, the output-filtering recipes, the nightly bump, workspace geometry, and decision detail:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
