# Check authoring details

Pull-tier docs for `scripts/check/checks/`: architecture, flows, and decision rationale. Must-know invariants and
gotchas live in [CLAUDE.md](CLAUDE.md). For the runner architecture (parallel executor, dependency graph, CLI flags,
freestyle.sh remote execution), see [`../CLAUDE.md`](../CLAUDE.md).

## Key files

- **`common.go`**: Core types (`CheckDefinition`, `CheckResult`, `CheckContext`, `CheckFunc`), shared utils
  (`RunCommand`, `EnsureGoTool`, `CommandExists`, `runPrettierCheck`, `runESLintCheck`, `indentOutput`,
  `trimBuildNoise`)
- **`registry.go`**: `AllChecks`: canonical ordered list of all check definitions. Lookup and validation functions
  (`FilterSlowChecks`, `FilterCIOnlyChecks`, `FilterFastChecks`, `ValidateCheckNames`).
- **`registry_test.go`**: Collision detection, `CLIName()` tests
- **`desktop-rust-*.go`**: One file per Rust check
- **`desktop-svelte-*.go`**: One file per Svelte/TS check
- **`website-*.go`, `api-server-*.go`, `scripts-go-*.go`**: One file per check
- **`file-length.go`**: Informational file-length scanner (warn-only, never fails). Supports an allowlist and
  shrink-wraps it on local runs.
- **`file-length-allowlist.json`**: Allowlist for file-length check:
  `{ "exempt": { "path": reason }, "files": { "path": lineCount } }`. See § File-length allowlist.
- **`claude-md-length.go`**: Warn-only push-tier scanner: warns when a `CLAUDE.md` exceeds 600 words (`DETAILS.md` is
  the unlimited pull tier, not scanned). Allowlist with the same shrink-wrap semantics as file-length. See § CLAUDE.md
  length.
- **`claude-md-length-allowlist.json`**: Allowlist for claude-md-length: `{ "files": { "path": wordCount } }`. Same
  ratchet/consent rules as file-length.
- **`docs_graph.go`**: Shared doc-discoverability graph: reachability from the repo-root `CLAUDE.md` over references
  between docs. Powers both the `docs-reachable` check and the `--docs-graph` renderer. See § Docs reachable.
- **`docs-reachable.go`**: Errors (not warn-only) when any `CLAUDE.md` / `DETAILS.md` / `docs/` file can't be reached
  from the root `CLAUDE.md`. Allowlist with the same shrink-wrap/consent semantics as file-length. See § Docs reachable.
- **`docs-reachable-allowlist.json`**: Allowlist for docs-reachable: `{ "files": { "path": reason } }` of docs
  intentionally unreachable. Goal is empty. Shrink-wraps gone/now-reachable entries; adding one needs David's OK.
- **`docs-dead-links.go`**: Errors (not warn-only) when any Markdown doc has a relative link whose target file or
  directory doesn't exist. Companion to docs-reachable (orphan vs broken link). No allowlist. See § Docs dead links.
- **`claude-md-details-sibling.go`**: Errors (not warn-only) when any non-root `CLAUDE.md` lacks a sibling `DETAILS.md`
  in its directory or doesn't reference a `DETAILS.md`. Mandates the C/D pair so the "should this area have a
  `DETAILS.md`?" decision never recurs. No allowlist. See § CLAUDE.md / DETAILS.md sibling.
- **`resident-doc-budget.go`**: Warn-only metric capping the unconditionally-resident agent-doc bundle (the repo-root
  `CLAUDE.md`, its transitive `@`-imports, and `.claude/rules/*.md`). The cap is a hardcoded constant that ratchets down
  only. See § Resident doc budget.
- **`e2e-durations.go`**: E2E test duration flagger (warn-only): parses the Playwright JSON reports after each E2E run
  and flags tests over the 2 s budget. Embedded in both E2E checks, not a registry check. See § E2E test duration
  flagger.
- **`e2e-duration-allowlist.json`**: Per-platform (`macos` / `linux`) allowlist for the duration flagger:
  `{ "<spec>::<describe chain>::<title>": reason }`. Entries need a reason; new entries need David's OK.
- **`website-bundle-size.go`**: Warn-only website `dist/` size budget: warns when the total grows >10% over the
  committed baseline. Self-skips without `dist/`. See § Website bundle-size baseline.
- **`website-bundle-size-baseline.json`**: Committed baseline for `website-bundle-size`: total bytes + hash-normalized
  top assets. Ratchets down automatically; raising it is manual (delete + regenerate) with David's OK.
- **`allowlist.go`**: Shared allowlist shrink-wrap plumbing: `writeJSONAllowlist` (stable JSON rewrite),
  `reformatWithOxfmt`, `fileExists`. Verdict logic stays per-check.
- **`directives.go`**: `directiveTracker`: records `allowed-*` opt-out comment sites per file and which excused a
  violation; unused ones are reported as orphans (the "unused eslint-disable" equivalent).
- **`changelog-commit-links.go`**: Validates every `https://github.com/vdavid/cmdr/commit/<sha>` URL in `CHANGELOG.md`
  resolves, via a single `git cat-file --batch-check` process.

## Check definition shape

```go
CheckDefinition{
    ID:                "desktop-svelte-eslint", // unique, always accepted as a CLI selector
    Nickname:          "",                      // short alias, also accepted as a selector (optional)
    DisplayName:       "eslint",                // shown in output
    App:               AppDesktop,
    Tech:              "🎨 Svelte",
    IsSlow:            false,
    IsFast:            false, // true = included in --fast (curated pre-commit lane)
    CIOnly:            false, // true = run only in --ci mode (or when named explicitly)
    FreestyleIncompat: true,  // can NOT run on freestyle.sh VMs (Rust, Docker)
    CpuWeight:         2,      // avg cores busy; 0/unset = 1. Governs concurrent admission.
    Inputs:            svelteInputs, // path globs this check reads (for the input-fingerprint cache)
    DependsOn:         []string{"desktop-svelte-prettier"},
    Run:               RunDesktopESLint,
}
```

### Field semantics

- **`ID`** is the canonical name (`pnpm check <id>` always works; `--check <id>` is an alias).
- **`Nickname`** is an optional short alias, accepted everywhere the ID is. `CLIName()` returns nickname if set, else
  ID. `ValidateCheckNames()` runs at startup and fatals on any ID/nickname collision, including collisions with the
  reserved positional group/app keywords (`desktop`, `website`, `api-server`, `scripts`, `rust`, `svelte`, `go`).
- **`IsSlow: true`** excludes the check from the default run; included by `--include-slow`/`--only-slow` or by naming
  the check. Use for E2E suites, full eslint with type-aware rules, etc.
- **`IsFast: true`** opts the check into the curated `--fast` pre-commit lane. The lane is editorially picked, not
  derived from timings — only check this if the check is genuinely cheap on a warm cache _and_ unlikely to spike on a
  cold one.
- **`CIOnly: true`** runs the check only under `--ci` (or when named explicitly). Useful for the slow-but-authoritative
  variant of a check whose fast local variant lives elsewhere (e.g. `cargo-udeps` paired with `cargo-machete`).
- **`FreestyleIncompat: true`** opts out of freestyle.sh remote VM runs. Set for any Rust-compiling check or anything
  that needs Docker. Negative-sense default (`false` = compatible) keeps the field absent in the common case.
- **`DependsOn`** is a flat slice of IDs. Formatters before linters, linters before tests, type checkers before tests.
  Blocked checks (dep failed) get `StatusBlocked` automatically.
- **`CpuWeight`** is the average number of CPU cores the check keeps busy while running (cold/working profile, rounded).
  The runner admits checks so the sum of concurrent weights stays within `NumCPU`, so two CPU-heavy checks don't
  oversubscribe the machine. `0` (unset) counts as `1` (light). Weights are Docker-VM-aware (`rust-tests-linux` /
  `e2e-linux` burn cores in the VM the host process never shows). Calibrate from the isolation sweep in
  `docs/notes/check-cpu-contention.md`; visualize with `pnpm check --graph`. Only the measured non-fast checks carry
  explicit weights today; fast/formatters default to 1.
- **`NotInCI`** documents WHY a check intentionally has no step in any GitHub workflow (for example, the Playwright E2E
  suite needs a macOS window server). The `ci-coverage` check enforces it both ways: a check that's neither invoked by a
  workflow nor carrying a reason fails the suite, and a check that has a reason but IS invoked also fails (stale
  excuse). Empty (the default) = the check must appear in a workflow. See `docs/tooling/ci.md` § "The registry ↔ CI
  contract".
- **`Inputs`** is the list of path globs (relative to repo root) this check reads, for the input-fingerprint cache
  (`pnpm check` skips a check when its inputs are unchanged since its last pass). **Every check MUST declare Inputs**
  (`TestEveryCheckDeclaresInputs` fails the suite otherwise): an empty list fingerprints on the global inputs alone, so
  the check would be cache-skipped even when its own files change — a correctness hole. Reuse a shared set from
  `inputs.go` (`rustInputs`, `svelteInputs`, `websiteInputs`, `apiServerInputs`, `goScriptsInputs`, `workflowsInputs`,
  `desktopAppInputs()`), or `wholeRepoInputs` (`**`) for a whole-tree scanner. **Be conservative: when unsure whether
  the check reads a path, include it.** A too-wide list only costs cache speed; a too-narrow one costs correctness. The
  global inputs (`.mise.toml`, `scripts/check/**`) are added automatically — don't list them. `ci-coverage` rule 4 fails
  if any static path prefix in your Inputs doesn't exist on disk.

## Adding a new check

1. Create `{app}-{name}.go` with a `func RunSomething(ctx *CheckContext) (CheckResult, error)`. Use `website-build.go`
   or `website-docker.go` as templates; they're the simplest.
2. Register it in `AllChecks` in `registry.go` (ID, App, Tech, DependsOn, Run, plus any flag fields). **Declare
   `Inputs`** (the paths it reads) — reuse a shared set from `inputs.go`; the suite fails if you forget. See the
   `Inputs` field semantics above.
3. Return `Success("message")` on pass, `fmt.Errorf(...)` on fail, `Skipped("reason")` to skip.
4. Add a test file if the check has non-trivial logic (`{app}-{name}_test.go`).
5. If the check grows an allowlist or an opt-out comment, wire staleness detection from day one (see § Allowlist
   shrink-wrap): dead entries must auto-remove or fail, and orphaned opt-out comments must fail. Reuse
   `directiveTracker` / `writeJSONAllowlist`.
6. **Wire it into CI**: add a workflow step in `.github/workflows/ci.yml` (or `slow-checks.yml` for slow/weekly checks),
   or set a `NotInCI` reason on the definition. The `ci-coverage` check fails the suite until you do one or the other —
   there's no third option of "registered but runs nowhere".
7. Run `pnpm check go-vet staticcheck` to verify (staticcheck is strict about idiomatic Go).
8. Update the "Apps and check counts" table below and `AGENTS.md`'s fast-lane coverage list (if the check is `IsFast`).

### Return values

- `Success(message)` on success with a short, informative message
- `Warning(message)` for non-fatal issues
- `Skipped(reason)` when the check can't run (for example, missing config)
- `CheckResult{}, error` on failure
- `SuccessWithChanges(message)` when the check made local fixes (auto-fix mode); CI mode should still error

### Success messages

Include useful stats: "12 tests passed", "Checked 42 files", "No lint errors". Avoid generic "OK".

### Error messages

Include the command output using `indentOutput()`:

```go
return CheckResult{}, fmt.Errorf("check failed\n%s", indentOutput(output))
```

### Length-based truncation is forbidden

If 200 tests fail, all 200 panic bodies must pass through. Filter by structure (section delimiters, line-anchored
regexes for harness noise), never by max-line count. See the "E2E failure output" and "cargo test output" decisions
below for the section-aware patterns to follow.

## Common helpers

- **`RunCommand(ctx, name, args...)`** — wraps `exec.Cmd` with the runner's working dir, captured output, and timeout
  hooks.
- **`CommandExists(name)`** — checks `PATH` before invoking.
- **`EnsureGoTool(name, installPath)`** — checks `PATH` first, then `go install`s; returns the full binary path. Used
  for staticcheck, nilaway, etc. `installPath` MUST pin a specific version (`@vX.Y.Z` or a pseudo-version), never
  `@latest`. Same rule applies to `cargo install` calls inside checks: pin both `--version` and `--locked`.
- **`runPrettierCheck(ctx, ...)`** / **`runESLintCheck(ctx, ...)`** — auto-fix locally, check-only under `--ci`.
  Centralizes the dual-mode behavior so individual checks don't reinvent it.
- **`indentOutput(s)`** — indents captured stdout/stderr for error messages.
- **`trimBuildNoise(s)`** — cuts everything before the last `Compiling …` line; when no such line exists (build failed
  before cargo ran), returns input verbatim.

## File-length allowlist

`file-length-allowlist.json` has two sections:

```json
{
  "exempt": {
    "apps/desktop/src/lib/ipc/bindings.ts": "generated by tauri-specta; length not actionable"
  },
  "files": {
    "apps/desktop/src/lib/foo/bar.ts": 412
  }
}
```

- `files` entries suppress the warning up to the recorded line count plus a 10% growth buffer; beyond that the file is
  reported with current and allowed counts plus growth percentage.
- `exempt` entries never warn (generated files whose length is not actionable); each needs a reason.
- New files not in either section are reported normally.
- If the allowlist file is missing, all long files are reported (backwards-compatible).

**Shrink-wrap**: on local (non-CI) runs the check rewrites the allowlist to drop staleness — dead entries (file gone)
and satisfied entries (file back under the 800-line threshold) are removed, and entries with more than 10% slack are
ratcheted down to the file's current count. In CI it only reports what a local run would shrink. The 10% ratchet buffer
mirrors the growth buffer so routine small edits don't churn the JSON.

See `.claude/rules/file-length-allowlist.md` (repo-level) for when an entry may be raised vs lowered without user
consent.

## CLAUDE.md length

`claude-md-length` (warn-only, `IsFast`) keeps the push tier lean: it warns when any `CLAUDE.md` exceeds 600 words. Each
`CLAUDE.md` is auto-injected into every agent session that touches its directory, so words there cost tokens repeatedly;
depth belongs in the colocated `DETAILS.md` (the pull tier), which is deliberately unlimited and NOT scanned. The check
reuses `findClaudeMdFiles` (same walk as `claude-md-reminder`, so only files named `CLAUDE.md` count) and
`strings.Fields` for the word count (matches `wc -w`, so seeded counts and the check agree).

`claude-md-length-allowlist.json` has one `files` section mapping path → accepted word count, with the exact same
shrink-wrap and consent discipline as file-length: a file is suppressed up to its recorded count plus a 10% buffer;
local runs drop dead/under-threshold entries and ratchet >10%-slack entries down; CI reports what a local run would
change. Adding or raising an entry needs David's OK (`.claude/rules/file-length-allowlist.md`); the fix for an oversized
`CLAUDE.md` is to move depth into `DETAILS.md`, not bump the number.

## Docs reachable

`docs-reachable` (`IsFast`, an **error** not a warn: the doc tree must stay connected) enforces that every `CLAUDE.md`,
`DETAILS.md`, and `docs/` file is discoverable from the repo-root `CLAUDE.md` by link-walking, so a reader entering at
the real entry point can find every doc. `docs_graph.go` builds the graph (shared with the `--docs-graph` renderer in
`../docs_graph_render.go`); `docs-reachable.go` is the check shell + allowlist.

How reachability is decided (`BuildDocGraph`):

- **One root, the repo-root `CLAUDE.md`.** It's the true entry point: Claude Code loads it first, and it `@import`s
  `AGENTS.md` + the core docs. A doc is reached when a doc already reached from the root references it. BFS, so each doc
  is placed under its closest-to-root reference (a cycle just hits an already-reached node and stops). The root itself
  is never an orphan.
- **A reference is any mention, syntax-agnostic:** Markdown link, `@import`, backtick path, or bare path token are all
  equal. We watch intent, not form. Matching is generous (relative-to-source, repo-root-relative, and ≥2-segment path
  suffix), because over-connecting only hides a would-be orphan, while a false orphan would be a noisy CI failure.
- **The CLAUDE.md asymmetry:** a `DETAILS.md` or `docs/` file must be named, but a `CLAUDE.md` also counts as reached
  when a reachable doc mentions its _directory_ (`architecture.md` lists most subsystems as `` `some/dir/` ``, and
  Claude Code auto-injects a `CLAUDE.md` from its directory regardless). Such edges are tagged `ViaDir`; the renderer
  shows "(dir reference)".
- **Everything under `docs/` is enforced, including `docs/specs` and `docs/notes`.** Those dirs are periodically-wiped
  scratch, but they must still be discoverable while they exist: specs hang off `docs/specs/index.md`, and a note is
  expected to be linked from the colocated `CLAUDE.md` / `DETAILS.md` whose work it informs.
- **Candidates come from git, not a raw walk.** `findMarkdownDocs` lists
  `git ls-files --cached --others --exclude-standard` (tracked plus untracked-but-not-ignored), so a `.gitignore`d
  scratch dir (`_ignored/`) or a vendored tree can't fail the check on a local working tree even though it never reaches
  CI's clean checkout. A brand-new uncommitted doc still counts. Outside a git work tree it falls back to a filesystem
  walk.

`docs-reachable-allowlist.json` maps a doc path → the reason it's intentionally unreachable. The goal is an empty list:
connect docs rather than exempt them. Shrink-wrap drops entries whose file is gone or which became reachable; adding or
keeping one needs David's OK (`.claude/rules/file-length-allowlist.md`). To inspect the whole tree and spot
deeply-nested or orphaned docs visually, run `pnpm check --docs-graph`.

## Docs dead links

`docs-dead-links` (`IsFast`, an **error** like `docs-reachable`: the doc tree must stay intact) is the companion to
docs-reachable. Where docs-reachable fails on a doc nothing links to (an orphan), this fails on a link pointing at a
target that doesn't exist (a dead link). It scans every first-party Markdown doc, extracts each Markdown link target,
and fails if a local target resolves to no file or directory. External URLs (`https:`, `mailto:`, protocol-relative
`//`), in-page `#anchors`, and links inside code (fenced blocks and inline spans, so documented examples don't count)
are skipped. A target is tried both relative to the linking doc's directory (standard Markdown) and repo-root-relative;
a `../`-heavy path that escapes the repo root is treated as unverifiable and skipped rather than flagged. No allowlist:
a dead link is always a fix (correct the path or drop the link), never an exemption. Reuses `findMarkdownDocs` and the
link regex from `docs_graph.go`.

## CLAUDE.md / DETAILS.md sibling

`claude-md-details-sibling` (`IsFast`, an **error** like `docs-reachable`: the C/D pair is structural) enforces that
every non-root `CLAUDE.md` both has a sibling `DETAILS.md` in its directory and references a `DETAILS.md` (a Markdown
link or a backtick path, syntax-agnostic like `docs-reachable`). This makes the "should this area have a `DETAILS.md`?"
decision a one-time yes so it never recurs per area: the pull tier always exists, and the push-tier doc acknowledges it.
The repo-root `CLAUDE.md` is exempt: it's the `@`-import manifest (its only content is the `@`-imports), not an area
doc, and has no area `DETAILS.md`. The reference check accepts a link to any `DETAILS.md`, not strictly the sibling: the
sibling-exists half is the structural guarantee, the reference half only confirms the author knows the pull tier exists.
No allowlist: a missing `DETAILS.md` is always fixable by creating the file (the depth lives somewhere, so write it
down), so there's nothing to exempt. Reuses `findClaudeMdFiles` (the same walk as `claude-md-length` and
`claude-md-reminder`).

## Resident doc budget

`resident-doc-budget` (warn-only, `IsFast`) caps the unconditionally-resident agent-doc bundle: the repo-root
`CLAUDE.md`, every file it transitively `@`-imports, and every project rule in `.claude/rules/*.md`. Unlike a colocated
`CLAUDE.md` (resident only in sessions that touch its directory), this bundle loads in **every** session, worktree, and
subagent, so each word is paid on every turn of every session. The check sums word counts (via `countWords`, matching
`wc -w`) and warns when the total exceeds `residentDocBudgetWords`, a hardcoded constant seeded at the measured total at
creation time. The cap must only ever ratchet **down** as the docs are trimmed, never up; raising it needs explicit user
consent (same discipline as the allowlists). `@`-imports are resolved against the filesystem (relative to the importing
file's dir first, then the repo root), which naturally drops `@`-prefixed non-imports that share the syntax: npm package
names (`@iconify-json/lucide`), JSDoc tags (`@param`), and emails (`@example.com`). No allowlist file: a single constant
is the whole contract, and the fix for over-budget is to trim a doc, not to bump the number.

## E2E test duration flagger

The E2E suites were hard-won down to under 2 s per test; `e2e-durations.go` defends that. After a successful E2E run,
both E2E checks (`desktop-e2e-playwright`, `desktop-e2e-linux`) call `applyE2EDurationWarnings`, which parses the run's
Playwright JSON reports (`/tmp/cmdr-e2e-report-{mtp,nonmtp1,nonmtp2}.json` for the macOS shards,
`/tmp/cmdr-e2e-report-linux.json` for Docker — the same files `scripts/e2e-test-timings` reads) and flags every test
whose worst single attempt exceeded `e2eSlowTestThresholdMs` (2000). **Warn-only by contract**: a slow test converts the
check's green `OK` into a yellow `warn` line but never fails the suite, and a failed E2E run skips the analysis entirely
(the failure output stays focused).

**Decision**: the analysis is embedded in the two E2E checks, not registered as a separate check with `DependsOn`.
**Why**: the JSON reports are per-run `/tmp` artifacts. Dependencies outside the selected run set count as satisfied, so
a standalone check would run in default (non-slow) suites too and warn about a stale previous run's data. Embedding also
means zero new CI-contract surface (both E2E checks already carry `NotInCI` reasons).

`e2e-duration-allowlist.json` policy (same consent rules as file-length: agents add/raise nothing without David's OK):

- Sections are per platform (`macos` / `linux`) because the same test can be slow only on Docker; each check judges only
  its own section, so a macOS run never flags a Linux-only entry as stale.
- Key format: `<spec file>::<describe chain joined with " › ">::<title>`; duplicate titles collapse to the slowest.
- **Dead entries** (key absent from the run — the report enumerates the full suite, skipped tests included):
  auto-removed locally, report-only in CI. Skipped when any report failed to parse, so a missing shard report can't
  mass-remove entries.
- **Satisfied entries** are only _reported_ for an agent to judge, never auto-removed, and only once the test drops
  below the threshold minus a 25% margin (1.5 s). Wider than file-length's 10% because wall-clock durations oscillate
  run to run; a test hovering at 1.9 s must not cause remove/re-add churn.

## Website bundle-size baseline

`website-bundle-size` (warn-only, `IsFast`, self-skips without `dist/` like `html-validate`) compares the built
website's `dist/` total against `website-bundle-size-baseline.json` and warns when it grows more than 10%, listing the
largest assets with their baseline sizes. Asset names are content-hash-normalized (`About.DvK3R9p1.css` → `About.*.css`)
so rebuilds compare stably. The baseline follows the file-length ratchet discipline: local runs rewrite it downward when
`dist/` shrinks past the 10% band; raising it is always deliberate — delete the baseline file and run
`pnpm check bundle-size` against a fresh build (needs David's OK). A missing baseline is created on the spot locally and
reported as a warning in CI.

## Allowlist shrink-wrap

Checks that own an allowlist verify their own entries are still needed; the helpers live in `allowlist.go` and
`directives.go`. **Decision**: staleness detection lives inside each check, not in a separate meta-check. **Why**: "is
this entry needed?" IS the check's domain logic (line counts, coverage percentages, test-file existence, grep hits), and
the freshest data lives inside the check's own run — coverage staleness, for example, is only knowable from the
`coverage-summary.json` the svelte-tests run just produced. A meta-check would either duplicate each check's core logic
or read stale artifacts.

Policy by staleness class:

- **Dead entries** (file gone, or E2E test gone from the run): auto-removed locally, report-only in CI (same dual-mode
  convention as the formatters). Done by `file-length`, `svelte-tests`, and the E2E duration flagger; `a11y-coverage`
  and `log-error-macro` fail instead (their lists are small/hardcoded).
- **Satisfied entries with a reason** (coverage now ≥ threshold+5% margin; exempt component that has a valid a11y test;
  allowlisted E2E test now under 1.5 s): reported for an agent to judge — the reason may say "tested elsewhere", and the
  margin band stays silently allowlisted to avoid removal/re-add churn.
- **Numeric slack** (file-length, the website bundle-size baseline): auto-ratcheted; the entries carry no reason text,
  so the rewrite loses nothing.
- **Orphaned opt-out comments** (`allowed-bare-poll` / `allowed-lock-poison` / `allowed-error-string-match` /
  `allowed-btn-restyle` / `allowed-rustup-add`): the scanners track which directives excused a violation and fail on the
  unused rest. Prose that merely mentions a directive (a comment line not starting with it) is not a site. Source-code
  comments are never auto-edited.

External tools enforce the same principle natively: knip (`treatConfigHintsAsErrors` in `knip.json`), stylelint
(`reportNeedlessDisables` in `.stylelintrc.mjs`), cargo-deny (`unused-allowed-license = "deny"` in `deny.toml`), and the
slow eslint lane (`reportUnusedDisableDirectives`). The one list nothing can verify automatically is `audit.toml`'s
RUSTSEC ignores — that's a quarterly task in `docs/maintenance.md`.

## Apps and check counts

| App        | Tech     | Checks                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| ---------- | -------- | -------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Desktop    | Rust     | rustfmt, clippy, cargo-audit, cargo-deny, cargo-machete, cargo-udeps (CI-only), jscpd, log-error-macro, error-string-match, lock-poison, bindings-fresh, ipc-enum-camelcase, tests, integration-tests (Docker SMB), tests-linux (slow)                                                                                                                                                                                                                                                                                                                                                                                                                                                                         |
| Desktop    | Svelte   | prettier, eslint, svelte-kit-sync, eslint-typecheck-svelte, eslint-typecheck-typescript, stylelint, css-unused, a11y-contrast, btn-restyle, bare-poll, svelte-check, import-cycles, knip, type-drift, tests, e2e-linux-typecheck, e2e-linux (slow), e2e-playwright (slow)                                                                                                                                                                                                                                                                                                                                                                                                                                      |
| Website    | Astro    | prettier, eslint, typecheck, build, html-validate, bundle-size (warn-only), e2e                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Website    | Docker   | docker-build                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                   |
| API server | TS       | oxfmt, eslint, typecheck, tests                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                |
| Scripts    | Go       | gofmt, go-vet, staticcheck, ineffassign, misspell, gocyclo, nilaway, deadcode, go-tests, govulncheck                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                           |
| Other      | Metrics  | file-length (warn-only), CLAUDE.md-reminder (warn-only), claude-md-length (warn-only), resident-doc-budget (warn-only; caps the always-resident root-CLAUDE.md + @-imports + rules bundle), docs-reachable (errors when a CLAUDE.md/DETAILS.md/docs file isn't reachable from the root CLAUDE.md), docs-dead-links (errors on a doc link whose local target doesn't exist), claude-md-details-sibling (errors when a non-root CLAUDE.md lacks/doesn't reference a sibling DETAILS.md), docs-no-two-col-tables (errors on any 2-column table in agent-facing docs), changelog-commit-links, workflows-rustup (forbids `rustup target/component add` in workflows), ci-coverage (registry-to-workflows contract) |
| Other      | Security | workflows-hardening (SHA-pinning, no `pull_request_target`, job-scoped `id-token: write`)                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                      |

## Key decisions

**Decision**: every operational `cargo` command in checks passes `--locked`. **Why**: without it, cargo silently
re-resolves `Cargo.lock` whenever upstream metadata shifts (a yank, a new transitive dep version). For a 1028-crate
lockfile, that resolution window is wide and lets a freshly-published malicious version land mid-build. `--locked` fails
loudly if the lockfile would change. Applies to `cargo clippy`, `cargo nextest run` (in both `desktop-rust-tests` and
`desktop-rust-integration-tests`), and `cargo +nightly udeps`. Audit/deny/machete read `Cargo.lock` without updating it,
so `--locked` is moot for them, but the install of those tools still uses `--locked` to lock the tool's own dep tree.

**Decision**: every tool install pins `--version` and `--locked` (cargo) or `@vX.Y.Z` (Go). **Why**: an unpinned tool
install (`cargo install cargo-audit` or `EnsureGoTool(..., "@latest")`) means each fresh checkout pulls whatever's
latest. A wave-1-2-class compromise of any of these tool repositories would auto-propagate. Pinning is the Go-side
equivalent of the pnpm `minimum-release-age` defense (a fresh version can't land without a deliberate bump).

**Decision**: `cargo-deny` advisories check disabled; use `cargo-audit` instead. **Why**: Tauri's transitive
dependencies (gtk3-rs, unic-\*, fxhash, proc-macro-error, etc.) trigger unmaintained-crate advisories we can't control.
`cargo-audit` still catches critical security vulnerabilities. License, bans, and sources checks in `cargo-deny` remain
active. See comment in `src-tauri/deny.toml`.

**Decision**: `workflows-hardening` check enforces three GitHub Actions invariants and acts as a regression guard.
**Why**: cmdr's workflows are already correctly hardened (every third-party action is SHA-pinned with a comment, no
`pull_request_target` triggers, no workflow-scoped `id-token: write`). Without an automated guard, a future PR or a
Renovate misconfiguration could silently regress any of those without anyone noticing in review. The check fails on
tag/branch-pinned third-party actions, on `pull_request_target` triggers (wave-4's entry vector), and on workflow-scoped
`id-token: write` (must be job-scoped per the wave-4 OIDC-token-extraction lesson). Local actions (`./...`) are exempt.

**Decision**: `govulncheck` runs against every Go module. **Why**: cargo-audit covers Rust deps; nothing covered Go
until now. `govulncheck` is static-analysis-based, so it only flags vulns actually reachable from the code (low false
positive rate). Most of cmdr's Go modules are dep-free tooling scripts but still call into the Go stdlib, which gets its
own CVEs; the check found 7 real reachable stdlib vulns the first time it ran (fixed by bumping mise's Go pin). Mirrors
the cargo-audit role on the Rust side.

**Decision**: `cfg-gate` check to catch ungated macOS-only crate imports. **Why**: Rust code using macOS-only crates
(from `[target.'cfg(target_os = "macos")'.dependencies]`) compiles fine on macOS but fails on Linux if the `use` isn't
wrapped in `#[cfg(target_os = "macos")]`. CI catches this after push, but the check catches it locally and instantly. It
parses `Cargo.toml` for macOS-only crate names, detects module-level gating (for example,
`#[cfg(target_os = "macos")] mod foo;` in `lib.rs` makes everything inside `foo` inherently safe), and scans remaining
files for ungated `use` statements.

**Decision**: `bare-poll` check to catch silently-passing E2E tests. **Why**: Cmdr's `pollUntil` helper (and its
wrappers `pollFs`, `pollUntilValue`, `pollActiveMode`, `pollOverlayGone`, `pollFocusedPane`) returns `false` on timeout
instead of throwing. A bare `await pollUntil(...)` statement therefore reduces to "wait up to N seconds, then quietly
proceed" — if the polled condition never holds, the test passes green so long as no later `expect` happens to catch it.
A repo-wide grep turned up 187 bare-poll sites across 20 specs; several spec files contained tests with zero `expect()`
calls whose entire assertion was a bare `await pollUntil(...)` — those tests literally couldn't fail. The check is a
fast-lane Go scanner (`apps/desktop/test/`, ~9 ms warm) modeled on `error-string-match`. Same line-anchored grep
pattern: `^\s*await\s+(pollUntil|pollFs|…)\s*\(` only matches the bare-expression-statement shape, so
`expect(await pollUntil(…)).toBe(true)` / `if (!(await pollUntil(…)))` / `return await pollUntil(…)` /
`const ok = await pollUntil(…)` all pass through. Opt out for genuine best-effort cleanups (dismissing an overlay that
might or might not be there) with `// allowed-bare-poll: <reason>` on the line above or as a trailing comment. The
preferred migration target is Playwright's `expect.poll(() => …).toBeTruthy()`, which fuses the wait with the assertion
so the bug class is structurally impossible.

**Decision**: `lock-poison` check to force a deliberate poison-handling choice at every std-lock acquisition. **Why**: A
bare `.lock().unwrap()` / `.read().unwrap()` / `.write().unwrap()` aborts the whole app when the lock is poisoned (a
background thread panicked while holding it), and records no intent — a reader can't tell a considered abort from a
thoughtless one. The policy (recover-by-default for value stores via `lock_ignore_poison()`; abort only for
invariant-guarding locks, marked by an `.expect("… poison …")` whose message names poison) lives in the module doc of
`apps/desktop/src-tauri/src/ignore_poison.rs`. The check is a fast-lane Go scanner (`apps/desktop/src-tauri/src/`,
modeled on `error-string-match`) that flags bare unwraps and non-poison `.expect(…)`. Its matcher requires empty parens
(`.lock()` / `.read()` / `.write()` with nothing between) immediately followed by `.unwrap()` / `.expect(`, so
`io::Read::read(&mut buf).unwrap()`, `io::Write::write(buf).unwrap()`, and tokio's `mutex.lock().await` all pass
through; `try_lock` / `try_read` / `try_write` are out of scope by name. Opt out with `// allowed-lock-poison: <reason>`
on the line above or as a trailing comment. Unlike `error-string-match`, it skips in-file `#[cfg(test)]` mods (tracked
by brace depth): a poisoned lock in a test means the test already panicked, so aborting there is harmless.

**Decision**: Split `desktop-svelte-eslint` into fast (non-type-aware) and slow (full) checks. **Why**: Type-aware rules
(`no-floating-promises`, `no-unsafe-*`, etc.) take ~45% of lint time due to TypeScript project service startup. The fast
check sets `ESLINT_NO_TYPECHECK=1`, which `eslint.config.js` reads to use `tseslint.configs.strict` (no type info) and
suppress `reportUnusedDisableDirectives` (since disable comments for type-aware rules would look unused). The slow check
(`IsSlow: true`) runs the full config with all rules and `reportUnusedDisableDirectives` on, so stale disable comments
are still caught.

**Decision**: `desktop-svelte-kit-sync` runs before every check that needs the TypeScript program. **Why**:
`apps/desktop/tsconfig.json` extends the gitignored, generated `.svelte-kit/tsconfig.json`, and on a fresh tree (new
clone or worktree) nothing else creates it before the checks run. Without it, typescript-eslint's projectService can't
build a program: every imported type resolves to "could not be resolved", type-aware rules go silent, their
`eslint-disable` directives look unused, and the local `--fix` deletes them — this once stripped directives from 7
source files in a fresh worktree. The sync check (~1 s) is the single serialized syncer: `eslint-typecheck-svelte`,
`eslint-typecheck-ts`, and `svelte-check` depend on it. `RunSvelteCheck` calls `check:no-sync` (not `pnpm check`) so it
doesn't rewrite `.svelte-kit/` while the parallel eslint passes read it; humans keep using `pnpm check`, which still
syncs. As defense in depth, `runScopedESLintTypecheck` refuses to run when `.svelte-kit/tsconfig.json` is missing
(relevant for targeted `--check eslint-typecheck-*` runs, where the dependency is treated as satisfied if not selected),
so a degraded projectService can never strip directives again. `apps/desktop` also has `"prepare": "svelte-kit sync"` so
plain installs and IDE flows generate the file.

**Decision**: Clippy runs the enforcing pass (`-D warnings`) first, and only invokes `cargo clippy --fix` if that fails
(and we're not in CI). **Why**: Running `--fix` speculatively before every check doubled wall time on the happy path (no
warnings = no fix to apply). The enforcing pass is the one that actually decides pass/fail anyway, and `--fix` ignores
`-D warnings` so it can't be combined into a single invocation. The trade is one extra re-check on the rare warning
path; cuts ~50% off clippy in the common clean case.

**Decision**: `bindings-fresh` is hash-cached. **Why**: A naive `pnpm bindings:regen` on every invocation takes ~2 min
for a test-mode compile of the full crate just to confirm the output didn't change. Instead, we hash every `.rs` file
under `src-tauri/src` plus `Cargo.lock` and `Cargo.toml`, plus the current `bindings.ts`, and store both hashes in
`<CARGO_TARGET_DIR or workspace target>/.bindings-fresh-marker` after each successful run. If both match next time, we
return OK in <100 ms. The bindings.ts hash in the marker protects against manual edits; if someone hand-tweaks the file
the marker no longer matches and we run the full regen. The marker lives inside cargo's actual target dir (honoring
`CARGO_TARGET_DIR`), so `cargo clean` and wholesale `target/` deletion auto-invalidate it. Same shared-fate pattern as
`node_modules/.pnpm-install-marker`. Hashing all `.rs` files (rather than only those with `#[tauri::command]` /
`specta::Type`) costs ~tens of ms here and removes any "added the attr to a new file but the watch list didn't pick it
up" footgun.

**Decision**: `bindings-fresh` auto-regens outside `--ci`. **Why**: When the hash check detects drift, the check runs
`pnpm bindings:regen` and, in non-CI mode, keeps the regenerated `bindings.ts` (returns `SuccessWithChanges`). Same
philosophy as `oxfmt`, `gofmt`, and `clippy --fix`: locally fix what's mechanical so the dev reviews and commits the
diff alongside the Rust change that caused it, instead of being told to run a separate command and re-run the checker.
In `--ci` mode the regenerated output is byte-compared against the original (which is then restored, so CI never
modifies the tree) and the check fails on drift, with a "Run `pnpm bindings:regen` from `apps/desktop/`" hint. The
marker is updated either way so the next run short-circuits.

**Decision**: `cargo-machete` runs locally; `cargo-udeps` is CI-only. **Why**: Both detect unused dependencies but trade
off speed against precision. udeps compiles the whole crate with nightly (~2 min cold) and is authoritative. machete
greps source files for `use foo;` patterns (~0.5 s on this codebase, no compile) and catches the common case (removed
the last `use` but forgot to drop the dep) plus a class udeps misses ("transitively-used" deps where your Cargo.toml
lists serde but only a transitive dep actually uses it). machete's blind spot is deps used only inside macro expansions
or build.rs codegen; opt those out via `[package.metadata.cargo-machete] ignored = ["foo"]` in the relevant Cargo.toml.
Local dev gets instant feedback from machete; CI runs udeps for the long-tail check.

**Decision**: E2E failure output uses section-aware filtering, not a pattern denylist. **Why**: The checker's contract
with agents is that output is concise enough to read in full: no `head`/`tail`/`grep` needed. Raw Playwright + Tauri +
Docker output is 1000+ lines on a failure (test pass markers, app stdout log, post-ELIFECYCLE build dump). The captured
output has four stable sections (setup, per-test progress, numbered failure blocks, post-ELIFECYCLE dump), split by
fixed delimiters (`Starting Tauri app...`, `\d+\) \[tauri\]`, `[ELIFECYCLE]`). `extractE2ETestOutput` in
`desktop-svelte-e2e-playwright.go` keeps the failure blocks verbatim, drops the post-ELIFECYCLE dump, and in the
progress section keeps `✘` markers with their preceding annotation lines (like `[SMB diag] MCP port: …`) while dropping
`✓`/`-` markers with theirs. The untouched output stays in the timestamped log file the error message links to. Both
`desktop-e2e-linux` and `desktop-e2e-playwright` call the same helper.

If the run died before reaching the test phase, none of `Starting Tauri app...`, a `\d+) [tauri]` failure-block header,
or a `\d+ (passed|failed|flaky|skipped)` tally line will be present. `isPreTestFailure` checks all three; only if all
three are absent does the filter prepend `note: tests did not reach the run phase` and drop the verbose
`docker compose ps` table (anchored on its `NAME IMAGE COMMAND` header so prose containing `Up <N>` survives). Checking
the tally and failure block (not just the Tauri marker) avoids false positives on macOS playwright shards, where Tauri
is started by the Go check and its stdout goes to a per-shard log file, so the marker never appears in Playwright stdout
regardless of success.

**Decision**: `cargo test` / `cargo nextest` failure output is filtered by dropping pass/skip verdict lines only.
**Why**: A 1786-test run produces ~1800 noise lines around 2 real failures. The harness format is stable enough that a
single per-line regex (`^test … ... (ok|ignored…)$` for `cargo test`, `^\s+(PASS|SKIP) [...] …$` for `cargo nextest`)
can drop the noise without risking false positives on panic-message bodies (start-of-line anchor protects quoted test
phrases). `trimRustTestProgress` in `desktop-rust-tests-linux.go` runs after `trimBuildNoise`. Everything else
(`running N tests` header, FAIL/FAILED/LEAK/TIMEOUT verdicts, the `failures:` block, the `test result:` / `Summary`
tally, `error:` lines, bench results) passes through unchanged.

**Decision**: nextest binary is arch-aware. **Why**: `https://get.nexte.st/latest/linux` serves the x86_64-musl build by
default; on an arm64 container (e.g. Apple Silicon under OrbStack) cargo's rustup-shim happily syncs the aarch64
toolchain, then execs the x86 nextest binary and OrbStack crashes with
`Dynamic loader not found: /lib64/ld-linux-x86-64.so.2`. The fix is `dpkg --print-architecture` → `linux` for amd64 and
`linux-arm` for arm64, matching the Go tarball selection.

**Decision**: silence apt/dpkg at the source, not via a post-hoc denylist. **Why**: `provisionScript` redirects both apt
commands to a log file under `DEBIAN_FRONTEND=noninteractive` + `-qq`, so on a successful provision the check's stdout
gets zero apt lines. The log lives on a per-run host directory (`/tmp/cmdr-rust-tests-linux-<unix-ts>/provision.log`)
bind-mounted into the container at `/cmdr-logs`, so it survives the container's `--rm` and is discoverable from the
check's Success/failure message. On apt failure, the script dumps the full log to stderr (captured by Go) so the user
sees what went wrong without having to fish for the file. Redirection at source is bulletproof and zero-maint vs a
denylist treadmill: every Debian version adds new dpkg verbs (`Setting up`, `Unpacking`, `Processing triggers`, …),
continuation lines from multi-line apt prompts have no stable shape, and `apt-get -qq` alone doesn't propagate to dpkg's
per-package chatter. `trimBuildNoise` now only cuts everything before the last `Compiling …` line; when no such line
exists (provisioning died before cargo ran), the output is returned verbatim.
