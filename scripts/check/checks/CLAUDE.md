# Check authoring

Each check is one Go file here, registered in `registry.go`'s `AllChecks`. Full authoring walkthrough: `DETAILS.md`.
Runner architecture: `../CLAUDE.md`.

## Module map

- `common.go`: core types (`CheckDefinition`, `CheckResult`, `CheckContext`, `CheckFunc`) and shared utils
  (`RunCommand`, `EnsureGoTool`, `CommandExists`, `runPrettierCheck`, `runESLintCheck`, `indentOutput`,
  `trimBuildNoise`).
- `registry.go`: `AllChecks`, the canonical ordered list, plus lookup/filters (`FilterSlowChecks`, `FilterCIOnlyChecks`,
  `FilterFastChecks`, `ValidateCheckNames`).
- `{app}-{name}.go`: one file per check (`desktop-rust-*`, `desktop-svelte-*`, `website-*`, `api-server-*`,
  `scripts-go-*`).
- `inputs.go`: shared `Inputs` blocks. `allowlist.go` / `directives.go`: allowlist shrink-wrap + opt-out tracking.
- Warn-only scanners with JSON allowlists: `file-length.go`, `claude-md-length.go`, `e2e-durations.go`,
  `website-bundle-size.go`. Error-level link checks: `docs-reachable.go` (+ shared `docs_graph.go`) fails when a
  `CLAUDE.md` / `DETAILS.md` / `docs` file isn't reachable from the repo-root `CLAUDE.md`; `docs-dead-links.go` on a
  link or backtick doc-path with no target; `docs-link-text.go` on link text repeating its own target.

## Must-knows

- **Every check MUST declare `Inputs`** (the path globs it reads), or `TestEveryCheckDeclaresInputs` fails the suite. An
  empty list fingerprints on the globals alone, so the check is cache-skipped when its own files change: a correctness
  hole. Reuse a set from `inputs.go` and **be conservative** — too-wide costs cache speed, too-narrow costs correctness.
  Don't list the auto-added globals (`.mise.toml`, `scripts/check/**`).
- **Wire every check into CI** (a step in `.github/workflows/ci.yml` / `slow-checks.yml`, or a `NotInCI` reason).
  `ci-coverage` enforces it both ways: neither invoked nor excused fails, and an excuse on an invoked check fails as
  stale. No "registered but runs nowhere" state.
- **Length-based truncation is forbidden.** If 200 tests fail, all 200 panic bodies pass through. Filter by structure
  (section delimiters, line-anchored regexes), never by line count. DETAILS.md §§ "E2E failure output", "cargo test
  output".
- **Pin every tool install**, or a compromised tool repo auto-propagates to every fresh checkout. `EnsureGoTool`
  `installPath` pins `@vX.Y.Z` (never `@latest`); `cargo install` pins `--version` and `--locked`; a prebuilt binary
  pins its sha256 (`desktop-third-party-notices.go`); every operational `cargo` command passes `--locked`. Toolchains
  count: `cargo-udeps` runs on `desktop-rust-cargo-udeps.go`'s dated `nightlyToolchain`, the single source CI reads via
  `check --print-nightly`. Bumping it: DETAILS.md § "Bumping the pinned nightly".
- **A Rust check never hardcodes a source path.** Cargo lanes take their package selection from `HostCargoSelectionArgs`
  (`CargoSelectionArgs(members, "linux")` in a container); scanners take their roots from `ScannerRoots` /
  `ScannerMemberKinds`. `workspace-member-coverage` fails on an unclassified Rust check or an unreached member.
  DETAILS.md §§ "Workspace geometry", "Workspace member coverage".
- **A new cargo check that COMPILES declares `Exclusive: ResourceCargoBuildDir`** (`common.go`), or it blocks on cargo's
  build-directory lock while holding CPU weight. Metadata-only cargo commands don't take that lock.
- **Wire allowlist staleness from day one.** Dead entries auto-remove or fail; orphaned opt-out comments fail. Reuse
  `directiveTracker` / `writeJSONAllowlist`. Never add or raise an entry without David's OK.
- **Error output uses `indentOutput()`**: `fmt.Errorf("check failed\n%s", indentOutput(output))`. Success messages carry
  stats ("12 tests passed"), not a generic "OK". Return `Skipped(reason)` when a check can't run, `SuccessWithChanges`
  when it made local fixes (CI mode must still error on the same drift).
- **`svelte-tests` coverage runs in a per-invocation temp `reportsDirectory`** (via `VITEST_COVERAGE_DIR`): a fixed path
  lets concurrent runs clobber each other's in-flight v8 worker files (`ENOENT`). DETAILS.md § "svelte-tests coverage
  isolation".
- **A red Rust lane goes through `resolveRustFailure`**, which re-runs failures alone before believing them. Lanes
  inject only WHERE: the Docker lane execs into its still-live container, so don't collapse that back into one
  `docker run`. DETAILS.md § "The contention re-run".
- After authoring, run `pnpm check go-vet staticcheck` (strict about idiomatic Go) and update DETAILS.md § "Apps and
  check counts". `--fast` membership is `IsFast` in `registry.go`, curated by hand.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
