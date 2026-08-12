# Check authoring

Every check lives in this directory as a single Go file, registered in `registry.go`'s `AllChecks` slice. Full authoring
walkthrough: `DETAILS.md`. Runner architecture (parallel executor, dependency graph, CLI flags, freestyle.sh):
`../CLAUDE.md`.

## Module map

- `common.go`: core types (`CheckDefinition`, `CheckResult`, `CheckContext`, `CheckFunc`) and shared utils
  (`RunCommand`, `EnsureGoTool`, `CommandExists`, `runPrettierCheck`, `runESLintCheck`, `indentOutput`,
  `trimBuildNoise`).
- `registry.go`: `AllChecks`, the canonical ordered list, plus lookup/filter functions (`FilterSlowChecks`,
  `FilterCIOnlyChecks`, `FilterFastChecks`, `ValidateCheckNames`).
- `{app}-{name}.go`: one file per check (`desktop-rust-*`, `desktop-svelte-*`, `website-*`, `api-server-*`,
  `scripts-go-*`).
- `inputs.go`: shared `Inputs` building blocks. `allowlist.go` / `directives.go`: allowlist shrink-wrap + opt-out
  tracking plumbing.
- Warn-only scanners with their JSON allowlists: `file-length.go`, `claude-md-length.go`, `e2e-durations.go`,
  `website-bundle-size.go`. Error-level link checks: `docs-reachable.go` (+ shared `docs_graph.go`) fails when a
  `CLAUDE.md` / `DETAILS.md` / `docs` file isn't reachable from the repo-root `CLAUDE.md`; `docs-dead-links.go` fails on
  a link or backtick doc-path with no target; `docs-link-text.go` fails on link text repeating its own target.

## Must-knows

- **Every check MUST declare `Inputs`** (the path globs it reads), or `TestEveryCheckDeclaresInputs` fails the suite. An
  empty list fingerprints on the globals alone, so the check is cache-skipped when its own files change: a correctness
  hole. Reuse a set from `inputs.go`, and **be conservative** — too-wide only costs cache speed, too-narrow costs
  correctness. Don't list the auto-added globals (`.mise.toml`, `scripts/check/**`).
- **Wire every check into CI** (a step in `.github/workflows/ci.yml` / `slow-checks.yml`, or a `NotInCI` reason).
  `ci-coverage` enforces it both ways: neither invoked nor excused fails, and an excuse on an invoked check fails as
  stale. There's no "registered but runs nowhere" state.
- **Length-based truncation is forbidden.** If 200 tests fail, all 200 panic bodies pass through. Filter by structure
  (section delimiters, line-anchored regexes), never by max-line count. Patterns: DETAILS.md §§ "E2E failure output",
  "cargo test output".
- **Pin every tool install.** `EnsureGoTool` `installPath` pins `@vX.Y.Z` (never `@latest`); `cargo install` pins
  `--version` and `--locked`; a prebuilt binary pins its sha256 (`desktop-third-party-notices.go`). Every operational
  `cargo` command passes `--locked`. Unpinned installs let a compromised tool repo auto-propagate to every fresh
  checkout. Toolchains count: `cargo-udeps` runs on `desktop-rust-cargo-udeps.go`'s dated `nightlyToolchain`, the single
  source CI reads via `check --print-nightly`. Bumping it: DETAILS.md § "Bumping the pinned nightly".
- **A Rust check never hardcodes a source path.** Cargo lanes take their package selection from `HostCargoSelectionArgs`
  (or `CargoSelectionArgs(members, "linux")` when cargo runs in a container); source scanners take their roots from
  `ScannerRoots` / `ScannerMemberKinds`. `workspace-member-coverage` fails on an unclassified Rust check or a member
  nothing reaches. DETAILS.md §§ "Workspace geometry", "Workspace member coverage".
- **A new cargo check that COMPILES declares `Exclusive: ResourceCargoBuildDir`** (see `common.go`). Without it, it
  fights every other cargo lane for cargo's build-directory lock while holding CPU weight it can't use. Metadata-only
  cargo commands (`metadata`, `about`, `deny`, `machete`) don't take that lock and stay undeclared.
- **Wire allowlist staleness from day one.** Dead entries auto-remove or fail; orphaned opt-out comments fail. Reuse
  `directiveTracker` / `writeJSONAllowlist`. Agents never add or raise an allowlist entry without David's OK.
- **Error output uses `indentOutput()`**: `fmt.Errorf("check failed\n%s", indentOutput(output))`. Success messages carry
  useful stats ("12 tests passed"), not generic "OK". Return `Skipped(reason)` when a check can't run,
  `SuccessWithChanges` when it made local fixes (CI mode must still error on the same drift).
- **`svelte-tests` coverage runs in a per-invocation temp `reportsDirectory`** (via `VITEST_COVERAGE_DIR`), never a
  fixed path: concurrent runs clobber each other's in-flight v8 worker files (`ENOENT`). DETAILS.md § "svelte-tests
  coverage isolation".
- **A red Rust lane goes through `resolveRustFailure`**, which re-runs failures alone before believing them. Lanes
  inject only WHERE: the Docker lane execs into its still-live container, so don't collapse it back into one
  `docker run`. DETAILS.md § "The contention re-run".
- After authoring, run `pnpm check go-vet staticcheck` (strict about idiomatic Go) and update DETAILS.md § "Apps and
  check counts". `--fast` membership is `IsFast` in `registry.go`, editorially curated.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
