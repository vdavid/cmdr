# Check authoring details

Pull-tier docs for `scripts/check/checks/`: architecture, flows, and decision rationale. Must-know invariants and
gotchas live in `CLAUDE.md`. For the runner architecture (parallel executor, dependency graph, CLI flags, freestyle.sh
remote execution), see `../CLAUDE.md`.

## Key files

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md` § Module
map. Each scanner's own semantics (thresholds, verdicts, allowlist shapes, consent rules) live in its § below, and the
recipe for adding one is § "Adding a new check". Only the layout rules live here:

- **One file per check, `{app}-{name}.go`, with its test as `{app}-{name}_test.go` beside it**: `desktop-rust-*`,
  `desktop-svelte-*`, `website-*`, `api-server-*`, `scripts-go-*`. `registry.go` holds the single canonical ordered
  `AllChecks` list plus the filter/validation helpers; `common.go` holds the core types and command plumbing every check
  builds on; `inputs.go` holds the shared `Inputs` path sets.
- **An allowlist is a sibling JSON named `<check>-allowlist.json`**, and it's NEVER hand-edited: the owning check
  shrink-wraps it on local runs, so you run the check and commit its rewrite (`.claude/rules/file-length-allowlist.md`).
  The shared staleness policy, and why it lives inside each check rather than a meta-check, is § "Allowlist
  shrink-wrap". Ten exist today; `a11y-coverage-allowlist.json` and `ui-primitive-coverage-allowlist.json` are the two
  with no § of their own (both are exempt-with-reason lists whose checks FAIL on a dead or redundant entry rather than
  auto-removing it).
- **Not every file here is a registry check.** `e2e-durations.go` is embedded in the two E2E checks (§ "E2E test
  duration flagger" has the why), `docs_graph.go` is a shared library behind both `docs-reachable` and the
  `--docs-graph` renderer in `../docs_graph_render.go`, and `e2e-build.go` owns producing the Playwright lane's binary
  (compile, find, sign, freshness stamp) so `desktop-svelte-e2e-playwright.go` is left with running the suite against
  it. None appears in `AllChecks`.
- **`changelog-commit-links.go` resolves every commit hash in `CHANGELOG.md` through ONE `git cat-file --batch-check`
  process**, not a process per reference. The recognition rule is § "CHANGELOG commit refs" below.

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
- **`CIOnly: true`** runs the check only under `--ci` (or when named explicitly). Two uses: the slow-but-authoritative
  variant of a check whose fast local variant lives elsewhere (`cargo-udeps` paired with `cargo-machete`), and a check
  whose cost per catch doesn't justify a place in the local loop (`jscpd-rust`, `groq-smoke`).
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
  if any static path prefix in your Inputs doesn't exist on disk. A `!`-prefixed entry EXCLUDES matching paths from the
  set (including from the globals); it's the only way to make a set too narrow, so read `../DETAILS.md` § "Exclusions"
  before adding one, and give it a test.

### ⚠️ A fresh worktree inherits the cache, so a green check can be a stale answer

The fingerprint cache lives under `target/`, and `new-worktree.sh` APFS-clones `target/` from an existing worktree. So a
brand-new worktree starts with somebody else's cache entries, and a check whose inputs happen to match one reports a
cached OK **without ever running against the files actually in that tree**.

That is fine while the clone source and the new branch agree, and misleading the moment they don't — which is exactly
the parallel-agent case every worktree here is. Two shapes it takes, both observed:

- **A stale PASS**: the tree contains a violation, the inherited entry says the check passed, and nothing runs. It stays
  green until an unrelated edit invalidates the fingerprint, then fails "suddenly" on code that was never touched.
- **A stale SCANNER**: the branch predates a merge that fixed a predicate, so the check runs the OLD scanner against the
  NEW layout and fails on a file that is fine on `main`. Rebasing is the fix, not an opt-out comment.

**`pnpm check <name> --fresh` is the escape hatch**: it bypasses the fingerprint, runs everything selected, and then
refreshes the cache. Reach for it before trusting a green check in a young worktree, and before concluding that `main`
is broken — verify against the main clone first, where the answer is not clone-shaped.

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
8. Update the "Apps and check counts" table below.

### Return values

- `Success(message)` on success with a short, informative message
- `CheckResult{Code: ResultWarning, Message: ...}` for a non-fatal warning (there is no `Warning()` constructor; build
  the struct directly, as the length and coverage scanners do)
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

- The scanner enumerates git-tracked files only (`git ls-files -z` from the repo root), so gitignored or untracked
  generated output (a built `dist-analytics/` page, say) is excluded for free without any per-path skip rule. Outside a
  git work tree (a throwaway test dir) it falls back to a filesystem walk with the `fileLengthSkipDirs` exact-name skip
  set. So `exempt` is for generated files that ARE tracked; a gitignored generated file needs no entry at all.
- `files` entries suppress the warning up to the recorded line count plus a 10% growth buffer; beyond that the file is
  reported with current and allowed counts plus growth percentage.
- `exempt` entries never warn (tracked generated files whose length is not actionable); each needs a reason.
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

## Invariant density

`invariant-density` (warn-only, `IsFast`, ~140 ms) gauges how many `❌` "never do X" rules each subsystem's agent docs
carry. A rule in a doc is an invariant with no enforcement, so the count is a bill: read the whole thesis in
`docs/doc-system.md` § The rule budget. Read the gauge with `pnpm check invariant-density -v` (a passing check's message
is suppressed in the default quiet mode).

What it counts:

- **Docs**: `CLAUDE.md`, `DETAILS.md`, and `AGENTS.md`, via `findMarkdownDocs` (so gitignored and vendored trees are out
  for free). `AGENTS.md` is in because the root `CLAUDE.md` is a bare `@AGENTS.md` import, so leaving it out would let
  the repo's most-read rule list grow untracked. `.claude/rules/` is deliberately out: those are agent-workflow policy
  rather than code invariants, and `resident-doc-budget` already caps them.
- **Markers**: every occurrence of the `❌` rune in prose, and **not** inside fenced blocks or inline code spans (use
  versus mention: a rule is imposed in prose, a marker in backticks is being talked about, and without the strippers a
  doc explaining this convention would be billed for it). Counting the marker rather than the prose keeps the check out
  of parsing English; an unmarked prohibition is undercounted, which is one more reason to keep marking them. `⚠️` is
  counted alongside (the base rune, so the variation selector doesn't matter) but never gated.
- **Denominator**: git-tracked source files under the subsystem, by the `fileLengthSourceExtensions` set, counted with
  the same `countLines` `file-length` uses.

**Decision**: a subsystem is the nearest ancestor directory holding a `Cargo.toml` or a `package.json` (longest prefix
wins), everything else falling into a repo-root `.` bucket. **Why**: a directory that declares itself a build unit is a
boundary somebody owns, its name survives refactors inside it, and it comes with a natural denominator. Deriving the
buckets from the repo's own manifests means a new crate or app teaches the check about itself with no Go edit, and
longest-prefix keeps a Rust member nested inside a JS package (`apps/desktop/src-tauri` inside `apps/desktop`) as its
own bucket rather than folding into the package around it. Per-`CLAUDE.md`-directory buckets were the alternative and
lose on both counts: ~380 rows is not a legible gauge, and every module rename churns the allowlist.

**Decision**: `invariantExtraSubsystemRoots` in `invariant-density.go` promotes a hand-named directory to a subsystem
root even though it holds no manifest, and longest-prefix then routes files to it exactly as it does a manifest root.
Two entries today, both under `apps/desktop`: `src` (the Svelte frontend) and `test` (the Vitest, Playwright, and Linux
Docker harness). **Why**: a build unit is sometimes coarser than the boundary somebody owns. One
`apps/desktop/package.json` covers the frontend, the test harness, and the app's build scripts, so a single row averaged
285,000 lines of UI code with 29,000 lines of harness and answered no question about either. Split, each row names a
tree a reader would name out loud, and the residual `apps/desktop` bucket (scripts, ESLint plugins, packaging, the
app-level `C+D.md`) stops being "everything not listed above".

The list is a Go constant rather than a section of the allowlist JSON on purpose: that file is a self-rewriting record
of accepted counts, and bucket geometry is not something a shrink-wrap pass may move. An entry whose directory is gone
is inert (its bucket stays empty and shrink-wrap drops the allowlist entry on the next local run), so a rename costs a
stale line, never a wrong number. It stays a two-entry list rather than a general config system; a third entry wants a
reason as concrete as these two.

Splitting a bucket re-seeds the allowlist by hand, which is the one time adding entries is right. Seed each new bucket
with the count it carried when the parent's entry was last accurate (`git show <commit>:<doc>` over that subtree), never
with today's count: the numbers must sum to the parent's old entry, so an outstanding breach stays visible instead of
being absorbed by the split. The `apps/desktop` split seeded 246 / 19 / 2 against a parent entry of 267, which kept the
frontend's then-outstanding +4 exactly where it was.

**Decision**: the allowlist records the ABSOLUTE rule count per subsystem; the per-kloc density is reported but not
gated. **Why**: adding a rule is the regression, adding code isn't. Gating the ratio would warn when a subsystem
_deletes_ code, and would let a growing subsystem add rules for free.

**Decision**: no slack buffer, unlike the two length allowlists. **Why**: a line or word count drifts on nearly every
edit, so a buffer keeps the JSON from churning. A rule count only moves when somebody writes or deletes a rule, so
there's nothing for a buffer to absorb, and a 10% buffer on `crates/cmdr-index` would silently allow 37 new rules. Any
rise warns; any fall gets ratcheted in, so the improvement lands in the diff.

`invariant-density-allowlist.json` has one `subsystems` section mapping a subsystem root to its accepted `❌` count.
Shrink-wrap on local runs drops a subsystem that's gone or has reached zero and ratchets every entry down to the current
count; CI only reports what a local run would change. A subsystem missing from the allowlist warns rather than being
auto-added: entries are added deliberately, with David's OK
([the same consent contract](../../../.claude/rules/file-length-allowlist.md) the length allowlists carry). A warn names
the three rule-heaviest docs of the regressed subsystem, so it points somewhere.

**⚠️ Known limitation: the count conflates two different things, so a rise on its own never means "this change made the
code worse".** One rule at a type definition and ten scattered prose warnings a reader must remember both count as `+1`
each. Two measured cases, both from the effort that introduced this check:

- The operation-lifecycle work replaced `OperationStatus.is_running` (a presence test that read `true` for queued,
  running, and paused alike, routed around by roughly ten prose warnings) with one typed field plus one guardrail at its
  definition. The count went **up 2**, while the places a reader must hold the rule went from ten to one, now enforced
  by the compiler.
- The indexing-ownership work's first three milestones ran the count **up 11**, because collapsing mechanisms means
  adding a mechanism first and documenting honestly why it can't be tidied away.

So the number answers "how many prohibitions does this subsystem assert", not "how hard is this subsystem to hold in
your head". Those diverge exactly when a rule moves from prose into a type, which is the outcome worth wanting. Treat a
rise as a prompt to look, and ❌ never shave a doc to make it fall. A future refinement worth considering (David's call,
not shipped): weight a rule by whether the compiler already enforces it, or count rule SITES a reader must visit rather
than rule instances.

## Copy-paste detection (jscpd)

Two warn-only lanes over one shared core (`jscpd.go`): `jscpd-rust` (`desktop-rust-jscpd.go`) and `jscpd-frontend`
(`desktop-svelte-jscpd.go`). Both shell out to the pinned jscpd CLI, read its JSON report, and report the CLONES: which
two files say the same thing, at which lines.

**Decision**: the clone list is the product, the percentage is a footnote. **Why**: nobody refactors a percentage. A
lane that keeps only the aggregate can't name a target, which is what a duplication check is for.

**Decision**: two checks, each owning one lane. **Why**: they carry different `Inputs` (a TypeScript edit shouldn't
invalidate the Rust lane's cache), different `Tech` (the runner groups its output by it), different sensitivity floors,
and separate CI steps, so one lane's warn doesn't hide the other's. They share every line of logic through `jscpd.go`;
each `{app}-{name}.go` file is a `jscpdLane` value plus a one-line entry point.

**Decision**: in the default local lane, not `--fast` and not CI-only. **Why**: a copy-paste is cheapest to undo at the
milestone where somebody wrote it; the same warn three weeks later in CI is archaeology. The cost is ~11 s (Rust) and ~5
s (frontend), both cheap enough for a per-milestone run and too slow for the pre-commit lane.

### Thresholds

Both floors are measured, not guessed; re-measure before moving one.

- **Rust, `--min-tokens 100`** (~25 lines): 91 clones in 53 file pairs. 75 gives 200 clones, 50 gives 555, and the extra
  ones are mostly short match arms and builder chains that read as idiom rather than copy-paste.
- **Frontend, `--min-tokens 75`**: 28 clones in 22 file pairs, median 20 lines, and every pair names two things that
  plainly do the same job (`NewFolderDialog` ↔ `NewFileDialog`, `SettingCheckbox` ↔ `SettingSwitch`). 50 gives 101
  clones with a 14-line median, over half of them short CSS blocks that read as house style.
- Both lanes exclude test code, which is intentionally repetitive. Rust also excludes `tests/` module directories, where
  most of this repo's unit tests live; the frontend excludes `*.test.ts`, `*.test-*.ts`, `test-*.ts`, `__mocks__/`, and
  `__fixtures__/`. `dialog-gallery/fixtures/` stays in — that gallery ships.

### Svelte is covered; there's just no `svelte` row

jscpd tokenizes a `.svelte` file into three sub-formats: `typescript` for the script block, `css` for the style block,
and `markup` for the template. So Svelte clones are reported under those names and the statistics carry no `svelte` row
— which reads exactly like "Svelte didn't parse", and makes dropping `svelte` from the lane's `--format` list look like
a cleanup. Dropping it is what would create the blind spot: jscpd then skips every `.svelte` file outright. The
guardrail against that sits on the `formats` field in `desktop-svelte-jscpd.go`, where the edit would happen. (Verified
on jscpd 4.2.3, 2026-08-18: `getFormatByFile('a.svelte')` returns `svelte`, and 16 of the frontend lane's 22 pairs are
`.svelte` files.)

The frontend lane scans `apps/desktop/src` only. `apps/website`, `apps/api-server`, and `apps/analytics-dashboard` are a
deliberate blind spot: 436,000 lines against roughly 12,000 each, so three more lanes would buy coverage of 3% of the
frontend line count. Revisit when one of them grows.

### The allowlist keys on a file PAIR, valued in duplicated LINES

`jscpd-rust-allowlist.json` and `jscpd-frontend-allowlist.json` each hold one `pairs` section mapping
`"path/a.rs ↔ path/b.rs"` (sorted, so report order can't mint a second entry; a single path for a clone with both ends
in one file) to the duplicated line count that pair may carry.

**Decision**: the key is the file pair, never an individual clone. **Why**: a churning allowlist is worse than no
allowlist. A `file:line` key moves the moment anything above the clone changes, and a content hash of the fragment
changes the moment somebody renames a variable in both copies — both would rewrite the JSON on edits that changed no
duplication at all. A pair of paths only moves when a file is renamed or deleted. What a pair key gives up is "the clone
moved to a different part of the same two files", which was never a regression.

**Decision**: the value is duplicated lines rather than a clone count. **Why**: one number then catches both
regressions. A new duplicated block between the same two files raises it, and an existing block growing raises it too —
a clone count would miss the second entirely.

**Decision**: no slack buffer, unlike the two length allowlists. **Why**: a line or word count drifts on nearly every
edit, so those need one. A duplicated-line count only moves when duplication is added or removed, so there's nothing for
a buffer to absorb, and a buffer would let a clone grow for free.

Shrink-wrap on local runs drops a pair with no duplication left and ratchets every entry down to what's actually there;
CI only reports what a local run would change. A pair missing from the allowlist warns rather than being auto-added,
under the same consent contract as the length allowlists (`.claude/rules/file-length-allowlist.md`). The allowlist also
doubles as the complete inventory: every duplicated file pair in the repo is a line in it.

### What each verdict prints

- **Pass**: the headline (clones, duplicated lines, percentage, file pairs) plus the ten worst pairs, each with the file
  and lines of that pair's widest clone. Visible under `pnpm check -v`. This is the standing map of where duplication
  lives.
- **Warn**: only the delta — every pair over its number, with EVERY clone behind it as `file:line ↔ file:line` — then
  the one-line headline. The standing inventory is deliberately absent here: burying three new lines under ten standing
  ones is how a check teaches people to skip it.

`jscpdSpan` orders the two ends of a location because jscpd reports a few intra-file clones backwards (`start` 105,
`end` 51, with the byte positions agreeing), which printed verbatim reads like the tool is broken.

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

## CHANGELOG commit refs

`changelog-commit-links` (`IsFast`, nickname `changelog-links`) is the canonical owner of how `CHANGELOG.md` carries
commit references. Everything below is the shared contract; the two renderers (the app's What's new parser in
`apps/desktop/src-tauri/src/whats_new/`, and the website's linkifier in `apps/website/src/lib/changelog.ts`) implement
the same rule and point here.

**The form is a bare trailing group**: `- Some change (b626d7a4, 2d41cc14)`. The changelog stores hashes, never markdown
links, and each renderer linkifies (website) or strips (What's new popup) on the way out. The third consumer, the GitHub
release body that `release.yml` seds out of the section, leans on GitHub's own autolinking of a bare same-repo SHA
(documented behavior, not yet observed on a real Cmdr release: confirm on the first release after 2026-08-03). Dropping
the URLs cut the file by ~43% (206 KB to 117 KB); it was ~40% URL boilerplate, which every agent reading the file paid
for and which wrapped entries across three or four lines. The check **fails on any `…/commit/<sha>` URL** so the linked
form can't creep back.

**Recognition is structural, not positional-guess:** rebuild each bullet entry from its wrapped source lines, then
require that the entry ENDS with a parenthetical whose every comma-separated item is 6-40 lowercase hex chars. Anchoring
to the end of the entry is what keeps prose safe: entries routinely close on `(~40x speed-up!)`, `(smb2 0.8.0)`, or
`(photo.JPG to photo.jpg)`, and a hex-looking word mid-sentence is never even considered.

**Every ref must be exactly 8 characters**, which is what `.claude/commands/release.md` produces
(`git log --format='%h' --abbrev=8`) and what the whole file is normalized to. A wrong length is a finding at every site
it appears, not once per unique hash, so one pass fixes them all.

**Recognize loosely, enforce the length strictly.** The recognition pattern deliberately stays `{6,40}` rather than
becoming `{8}`. Narrowing it would make a stray 7-character ref stop being a ref at all: it'd be read as prose, silently
skip SHA validation, and quietly fail to render in anything matching the convention. Recognizing wide and then failing
on the length is the loud version of the same rule.

**Why the recognition floor is 6 and not lower:** 6 is where the false-positive risk turns real. Plenty of short English
words are hex-only ("added", "faced", "beaded"), so a 5-char group is read as prose and ignored rather than flagged. The
trade: a genuine 5-char hash silently escapes validation. That's the right side to err on. A 6-char hex WORD in a
trailing parenthetical ("decade") is read as a hash and fails the length rule, which is loud and one rephrase away from
fixed.

**The two renderers stay permissive at `{6,40}` on purpose.** `apps/desktop/src-tauri/src/whats_new/` strips hash groups
out of user-facing release notes, and a shipped app version renders whatever changelog it's handed, including older
ones; a group a stricter matcher failed to recognize would reach users as raw hex. The website's linkifier renders only
the current file, so tightening it buys nothing.

Each unique SHA is then resolved through one `git cat-file --batch-check` process and required to be **reachable from
HEAD**, not merely present in the object DB: an abbreviated SHA of a rebased-away commit still resolves locally via the
reflog, but CI's clean clone has no reflog and would fail there instead. Findings cite the line the hash actually sits
on, which for a wrapped group is a continuation line, not the entry's first.

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
Playwright JSON reports (`/tmp/cmdr-e2e-report-{mtp,nonmtp1,nonmtp2}-<pid>.json` for the macOS shards,
`/tmp/cmdr-e2e-report-linux-<pid>.json` for Docker — the same files `scripts/e2e-test-timings` reads) and flags every
test whose worst single attempt exceeded `e2eSlowTestThresholdMs` (2000). **Warn-only by contract**: a slow test
converts the check's green `OK` into a yellow `warn` line but never fails the suite, and a failed E2E run skips the
analysis entirely (the failure output stays focused).

**Decision**: the analysis is embedded in the two E2E checks, not registered as a separate check with `DependsOn`.
**Why**: the JSON reports are per-run `/tmp` artifacts. Dependencies outside the selected run set count as satisfied, so
a standalone check would run in default (non-slow) suites too and warn about a stale previous run's data. Embedding also
means zero new CI-contract surface (both E2E checks already carry `NotInCI` reasons).

`e2e-duration-allowlist.json` policy (same consent rules as file-length: agents add/raise nothing without David's OK):

- Sections are per platform (`macos` / `linux`) because the same test can be slow only on Docker; each check judges only
  its own section, so a macOS run never flags a Linux-only entry as stale.
- Key format: `<spec file>::<describe chain joined with " › ">::<title>`; duplicate titles collapse to the slowest.
- **Value form**: a bare reason string suppresses the test at any duration over the 2 s budget (legacy, unbounded — for
  inherently-heavy tests). An object `{ "maxMs": N, "reason": "..." }` raises that test's budget to N ms: suppressed at
  or under N, re-flagged past N (a separate "over the raised cap" warn). The cap form grants contention headroom (the
  `--include-slow` lane runs every E2E shard + Linux Docker + Linux Rust at once, inflating wall-clock) without going
  fully unbounded. `MaxMs == 0` round-trips back to the bare-string form, so untouched legacy entries keep their shape.
  Staleness is still judged against the 2 s budget, not the raised cap (the entry exists to suppress the 2 s warn).
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
  so the rewrite loses nothing. `invariant-density` and the two jscpd lanes ratchet the same way but carry no buffer —
  their numbers only move when somebody writes a rule or duplicates a block, so there's no drift to absorb.
- **Orphaned opt-out comments** (`allowed-bare-poll` / `allowed-lock-poison` / `allowed-error-string-match` /
  `allowed-dropping-timeout` / `allowed-btn-restyle` / `allowed-rustup-add`): the scanners track which directives
  excused a violation and fail on the unused rest. Prose that merely mentions a directive (a comment line not starting
  with it) is not a site. Source-code comments are never auto-edited.

External tools enforce the same principle natively: knip (`treatConfigHintsAsErrors` in `knip.json`), stylelint
(`reportNeedlessDisables` in `.stylelintrc.mjs`), cargo-deny (`unused-allowed-license = "deny"` in `deny.toml`), and the
slow eslint lane (`reportUnusedDisableDirectives`). The one list nothing can verify automatically is `audit.toml`'s
RUSTSEC ignores — that's a quarterly task in `docs/maintenance.md`.

## svelte-tests coverage isolation

**Decision**: `svelte-tests` gives each invocation a private coverage `reportsDirectory` under the OS temp dir
(`newCoverageRun` mints one with `os.MkdirTemp`, exports it as `VITEST_COVERAGE_DIR` which `vitest.config.ts` reads, and
removes it when done). It reads `coverage-summary.json` back from that temp dir, not from `apps/desktop/coverage/`.
**Why**: the v8 provider writes intermediate per-worker files to `reportsDirectory/.tmp/coverage-N.json` and wipes that
directory at run boundaries. Two concurrent `pnpm check svelte-tests` runs (common with multiple agents/sessions
checking at once) sharing the fixed `./coverage` path meant one run's cleanup deleted the other's in-flight worker
files, crashing it with `ENOENT` and phantom test failures. Isolation (not serialization) lets both run green. Nothing
depends on the canonical path persisting — no CI artifact upload, and thresholds live in this Go check, not in vitest
config — so isolation applies everywhere, with no CI split. Manual `pnpm test:coverage` (no env var set) still writes
`./coverage`.

## Nothing a shard owns is shared between runs

**Decision**: every resource an E2E run touches is scoped to the RUN, not to the shard name. The MCP ports come from the
OS (`reserveMcpPorts` binds `127.0.0.1:0` once per shard, holding every listener open until the last is reserved, so the
kernel can't hand the same port back twice); everything else carries the pid: socket path, data dir, fixture dir,
instance ID, Playwright output dir, JSON report, shard log, build log, the Linux lane's report, and the run's virtual
MTP backing dir. The pre-flight no longer kills whoever holds a port; it only removes its own pid-scoped socket file.

A timestamp is NOT a scope. Two suites can start in the same second, and `os.Create` on a shard log truncates whatever
the other run was writing, so the timestamped paths carry the pid too.

**Why**: two suites run at once whenever two worktrees are busy, which is most of the time. The MCP port used to be
`9429 + shard offset`, and the pre-flight SIGTERM'd whatever was listening on it — so a suite starting at 16:03:30
killed a suite that had been running since 15:58, mid-test. The victim's shard reported one 15 s timeout (the in-flight
`webview.eval()` hanging on a socket whose process had gone) and 37 cascading `ECONNREFUSED` failures, with no panic and
no crash report to say the app had been signalled. It reads exactly like a product bug and cost a full triage cycle. The
output dir moved for the second half of that cost: the concurrent suite had already overwritten the failing run's
recordings and error contexts, so the only pictures of the failure were of the run that passed.

The JSON report and the MTP backing dir came last, and they were the expensive two:

- **The report is the run's evidence.** `e2e-test-log.go` turns it into the per-test log, `e2e-durations.go` flags slow
  specs from it, and `e2e-flaky.go` counts retry-passes out of it. At a fixed path, a concurrent suite answered all
  three questions about a run it never took part in, and the per-test log recorded the wrong run's names under this
  run's timestamp — instrumentation quietly lying, which is worse than no instrumentation. Nothing reads it from a known
  path any more: the lane hands readers the path, and `scripts/e2e-test-timings` takes the newest glob match.
- **The MTP backing dir is wiped at MTP-shard startup and between tests.** Shared machine-wide, a starting suite deleted
  the tree a running suite's MTP spec was asserting against, and the victim reported a missing file it had created
  itself. The app takes the run's root from `CMDR_VIRTUAL_MTP` and `mtp-fixtures.ts` from `CMDR_MTP_FIXTURE_ROOT`, so
  both sides of a spec agree on one path per run.

Still shared, and deliberately: the fixture hardlink cache (`/tmp/cmdr-e2e-fixtures-cache/`) is content-addressed and
built by tmp-dir + atomic rename, so sharing is the point and a torn read is structurally impossible; per-worktree
copies would cost 170 MB each and buy nothing. The `smblease` lease dir is machine-wide for the same kind of reason.

Reports, logs, and recordings deliberately OUTLIVE their run (they're what a post-mortem reads), so age collects them
instead: `sweepStaleE2EArtifacts` runs at the start of both E2E lanes and removes run-scoped leftovers older than a
week. The patterns are narrow on purpose — `e2e-tmp-sweep.go` must never match `cmdr-e2e-fixtures-cache` or a hand-made
`cmdr-e2e-data-<name>`, and `TestE2EArtifactIsSweepable` pins both directions.

`TestPlanShardsSharesNothingBetweenConcurrentRuns` pins the whole rule against two plans from different pids that share
a timestamp.

## The Playwright lane's binary is fingerprinted, because its build isn't incremental

**Decision**: `buildTauriBinary` stamps the release binary with a fingerprint of everything it was compiled from
(`e2eBinaryInputs`, in `e2e-build.go`) and skips the compile when the binary on disk already carries that stamp.

**Why**: `pnpm test:e2e:playwright:build` does not get cheaper when nothing changed. Measured back-to-back on a warm
`target/` with an untouched tree (macOS, rustc 1.97.1, 2026-08-12): the second build took **172 s**, of which cargo
spent **2 m 42 s** recompiling `cmdr` alone. The `beforeBuildCommand` runs `vite build` unconditionally (6.6 s) and
rewrites `apps/desktop/build/`, which the app crate embeds, so cargo's fingerprint for `cmdr` invalidates on every
invocation no matter what changed. "Cargo is incremental, so a no-op build is free" is simply false here. An
instrumented session paid this three times.

The stamped set is narrower than the lane's `Inputs` on purpose: it drops `apps/desktop/test/**`. Playwright reads its
specs, its config, and the shared fixture helpers from disk when the suite runs, so editing one changes what the suite
asserts, never what it asserts against — and an E2E debugging loop edits exactly those files.
`TestE2EBinaryInputsCoverTheBuildAndNothingElse` pins both directions of that boundary.

Every uncertainty rebuilds: a missing binary, a missing or unreadable stamp, or a fingerprint pass that failed (which
hands over an empty string, and `recordE2EBuild` refuses to write one). That bias matters more here than elsewhere,
because this lane carries `NotInCI` — nothing downstream would catch a suite that passed against a stale binary.

The stamp carries the binary's size and mtime alongside the fingerprint, so it vouches for one specific file rather than
for whatever sits at that path. `target/<triple>/release/Cmdr` isn't ours exclusively: a plain `pnpm tauri build` in the
same worktree writes the same path without the `playwright-e2e` feature, and an unbound stamp would hand that binary to
a harness that can't drive it.

`ctx.ReuseArtifacts` is the escape hatch, false under `--ci`, `--fresh`, and `CMDR_CHECK_NO_CACHE`. It deliberately
stays true for a NAMED check, unlike the check-level cache's "named ⇒ run fresh" rule: naming the slow lane is how you
run it at all, and running the suite against an up-to-date binary is running it for real. `cacheBypassed` in `plan.go`
is the one predicate both consumers read.

## One feature set across the cargo lanes

Cargo keys its artifacts by the exact question you asked it: which packages, which features. Two lanes sharing one
`target/` that ask differently don't share anything; they take turns rebuilding `cmdr` and everything above it. Measured
on a warm tree: re-running an identical `cargo build --workspace --tests` costs 1.3 s, the same build with
`cmdr/virtual-mtp` dropped costs 92 s, and going back costs 20 s. A package-scoped run (`cd src-tauri && cargo …`)
resolves dependency features for one package instead of the workspace and rebuilds the four first-party crates for 99.6
s. Full runs: `docs/notes/cargo-lane-feature-thrash.md`.

So **`HostCargoLaneArgs` (`cargo-workspace.go`) is the one answer**, and every host lane that compiles builds its
command line from it: `desktop-rust-tests`, `desktop-rust-integration-tests`, `desktop-rust-groq-smoke`, and (spelled
out by hand, see below) `pnpm bindings:regen`. It returns the workspace selection plus `SharedTargetFeatureArgs()` =
`--features cmdr/virtual-mtp`. A lane that needs a genuinely different feature set needs its own `CARGO_TARGET_DIR`, not
its own flags.

Who stays out, and why it's not an oversight:

- **`desktop-rust-clippy`** needs no alignment: its workspace units go through `clippy-driver` and land in their own
  fingerprints, so running it either way left the test build at 0.7 s / 0 crates. (It also means the `virtual-mtp` code
  is compiled by the test lane and linted by nothing.
  `cargo clippy --workspace --all-targets --features cmdr/virtual-mtp` passes clean today, so closing that is available
  whenever someone wants it.)
- **`desktop-rust-tests-linux`** builds in its container's own `CARGO_TARGET_DIR`, and deliberately omits the feature:
  that lane is already tight against the 8 s per-test cap on a slower VM, and MTP's virtual-device coverage doesn't
  differ by platform.
- **`desktop-rust-rustdoc`** owns a private target dir. **`desktop-rust-cargo-udeps`** runs on the pinned nightly, whose
  artifacts can't be shared with stable anyway.

### Why the feature set is `virtual-mtp`

The MTP tests that drive a virtual device (`backends/mtp_test`, `mtp_archive_test`, `mtp_read_range_test`,
`mtp_scan_oracle_tests`, `connection/path_cache_sync_test` — ~29 tests) only COMPILE under it. Without it
`cargo nextest` silently filters them out, so they protected nothing while looking like coverage. It's test-only (never
enters a production build) and costs ~2-4 s on a ~27 s suite, so it's the cheapest set that keeps the test lane honest.
It MUST stay package-qualified (`cmdr/virtual-mtp`): a bare `--features virtual-mtp` changes meaning once more than one
package is selected.

Prerequisites these tests rely on (per-test temp backing root, watcher off, `virtual_device_test_lock()`):
`apps/desktop/src-tauri/src/mtp/DETAILS.md` § "Rust tests that drive the device".

### The bindings regen is the one invocation outside Go

`desktop-bindings-fresh` shells out to `pnpm bindings:regen`, so `package.json` repeats the lane args by hand. Two
things hold that together:

- `TestBindingsRegenAsksCargoTheSameQuestionAsTheOtherLanes` compares the script against
  `CargoSelectionArgs(members, "macos")` + `SharedTargetFeatureArgs()`, and fails on a re-introduced `cd src-tauri`.
  macOS is the right target to compare against because the committed `bindings.ts` is the macOS surface (that's the
  check's `NotInCI` reason).
- The regen only survives the shared feature set because the exported surface no longer moves with it: the manifest's
  `typed unless cfg(test)` group in `ipc.rs` holds the three virtual-MTP commands back while the crate compiles its own
  tests, which is where `ipc::tests::export_bindings_test` writes the file. Without that, regenerating with the feature
  would commit three commands a real build doesn't answer. Pinned by
  `ipc::tests::the_exported_surface_leaves_out_the_test_only_virtual_mtp_commands`.

Net effect on the two lanes, same sequence measured before and after: `bindings-fresh` on a marker miss went 28.8 s →
2.3 s, and the `rust-tests` run right after it went 70 s → 27.7 s.

## Rust test diagnostics: retry-passes and deadline classes

`rust-test-diagnostics.go` parses cargo-nextest output for all three Rust lanes (`desktop-rust-tests`,
`desktop-rust-tests-linux`, `desktop-rust-integration-tests`). Two jobs, both aimed at making a run's outcome
self-explaining rather than something a reader has to re-derive.

The Playwright equivalent of the first job is `e2e-flaky.go` (retry-passes become a warn on both E2E lanes, read from
the structured JSON report rather than the `list` reporter's text).

**Retry-passes become a warn, not a pass.** `.config/nextest.toml` grants `retries` to named real-FSEvents tests, and
nextest exits 0 when a retry rescues the run. `ParseFlakyTests` reads the `FLAKY n/m` summary lines (and `TRY n PASS`,
deduped against them) and the lane returns `ResultWarning` naming each test and the rescuing attempt. That turns the
retry budget into a standing flake-rate meter: a rising rate shows up in ordinary runs instead of needing a dedicated
measurement pass. Policy and the three conditions for granting retries at all: `docs/testing.md`.

**Failures are sorted by which deadline blew.** `ClassifyRustFailures` splits failing tests into `ClassNextestCap` (a
`TIMEOUT` line: nextest killed the process at `slow-timeout`, no panic exists to read), `ClassInTestDeadline` (a
`wait_until` timeout panic below the cap, with the wait's description captured), `ClassLeak`, and `ClassOther`.
`DiagnoseRustFailures` renders that above the raw output. The distinction is the point: the two timeout classes look
identical in raw nextest output but need opposite fixes, and raising `slow-timeout` does nothing for an in-test
deadline.

Gotchas for anyone touching this:

- **The parsers are pinned to real nextest 0.9.136 output**, captured from a probe crate rather than written from
  memory. The fixtures in `rust-test-diagnostics_test.go` are verbatim, including the `(2/4)` progress counter and the
  `(───)` placeholder. Keep them verbatim when the pinned nextest version moves.
- **`TRY n FAIL` lines are not failures.** They're retried attempts; counting them double-reports every flake. The
  `FAIL` regex is line-anchored so it can't match them.
- **The summary block repeats every `FAIL`/`TIMEOUT` line**, so classification dedupes by (binary, test) and keeps the
  first occurrence, which is the one carrying the panic body.
- **`ClassInTestDeadline` recognition depends on a string Rust owns**: `timed_out()` in `crates/cmdr-fs/src/testing.rs`.
  Nothing but `TestWaitUntilPanicFormatStillMatchesTheClassifier` ties the two languages together; without it, rewording
  the panic would silently downgrade every `wait_until` timeout to `ClassOther`. Don't delete that test.
- **Leaks are a PASS, not a failure.** nextest counts a leaky test in its "N passed (M leaky)" tally, so `RealFailures`
  drops them before anything re-runs or counts failures. Treating a leak as a failure both overstates a red run and
  sends the contention re-run chasing a test that passed.
- **The progress counter can contain spaces**, because nextest right-aligns the index to the total's width: a 4 802-test
  run prints `(  42/4802)`. Every status regex matches it as `\([^)]*\)`, never `\(\S+\)`. Reading it as one non-space
  token silently dropped every failure numbered under 1000 out of the classifier, so those got no diagnosis and no
  contention re-run (verified against a real container run, 2026-08-02). The small-total fixtures can't catch this on
  their own; `TestClassifyRustFailures_PaddedProgressCounter` and `TestTrimRustTestProgress_PaddedProgressCounter` carry
  the padded form.
- **`TRY n FAIL` lines are not failures.** They're retried attempts; counting them double-reports every flake. The
  `FAIL` regex is line-anchored so it can't match them.
- **The summary block repeats every `FAIL`/`TIMEOUT` line**, so classification dedupes by (binary, test) and keeps the
  first occurrence, which is the one carrying the panic body.
- **`ClassInTestDeadline` recognition depends on a string Rust owns**: `timed_out()` in `crates/cmdr-fs/src/testing.rs`.
  Nothing but `TestWaitUntilPanicFormatStillMatchesTheClassifier` ties the two languages together; without it, rewording
  the panic would silently downgrade every `wait_until` timeout to `ClassOther`. Don't delete that test.
- **Leaks are a PASS, not a failure.** nextest counts a leaky test in its "N passed (M leaky)" tally, so `RealFailures`
  drops them before anything re-runs or counts failures. Treating a leak as a failure both overstates a red run and
  sends the contention re-run chasing a test that passed.

## The contention re-run (`rust-test-contention.go`)

No Rust lane believes a red run until it has re-run the failures alone. Rationale, the four verdicts, and the reporting
contract are in `docs/testing.md`; this section is the mechanics.

All three lanes funnel a failure through the one `resolveRustFailure` in `desktop-rust-tests.go`, which takes the runner
and the load sampler as parameters. That injection is the contract: a lane may say WHERE a re-run happens, never what a
red run means. `desktop-rust-tests` and `desktop-rust-integration-tests` pass `nextestContentionRunner` (shells out to
`cargo` on the host) and `LoadPerCore`; `desktop-rust-tests-linux` passes a container-backed pair (below).

Two stages, both serialized (`test-threads = 1`), driven by two nextest profiles in `.config/nextest.toml`:

- **`contention-probe`** `inherits = "default"`, so every deadline (including the per-test overrides) is exactly what
  the failing run had. Only the parallelism changes. That's what makes a pass here mean "starved" rather than "given
  more time".
- **`contention-retry`** runs only what stage 1 couldn't clear, with a 40 s cap.

Gotchas:

- **`contention-retry` deliberately has no `inherits`.** An inherited per-test override BEATS a profile-level
  `slow-timeout`: verified against nextest 0.9.136 (2026-07-29), a test carrying a 4 s override still died at 4 s under
  a profile declaring 30 s. Inheriting would silently keep the tight caps this stage exists to lift.
- **Don't read the per-test caps in `.config/nextest.toml` as runtimes.** They're hang backstops, often 20-50x the real
  thing, and mistaking one for a runtime is how the 40 s retry cap first looked too small for the SMB lane. Measured
  2026-07-29 on an idle M3 Max: `smb_integration_concurrent_streaming_writes_no_deadlock` carries a 130 s cap and runs
  in **2.8 s**; the whole 53-test integration suite is **5.3 s** wall-clock. 40 s is ~14x the slowest real test across
  every lane.
- **The integration lane passes `--run-ignored only` through `baseArgs`** so the re-run inherits it. Every
  `smb_integration_*` test is `#[ignore]`-gated, so a re-run without it selects nothing and the probe would read as
  "everything passed alone", turning every real SMB failure into a contention warn.
- **A non-zero cargo exit during a re-run is expected**, because failing tests are the whole point. `nextestRanRE`
  (nextest's `Summary [` line) distinguishes "tests ran and some failed" from "cargo couldn't run at all". Only the
  latter is a runner error, and it marks every verdict real rather than inventing an excuse for a red run.
- **A runner error never softens a verdict.** Both `ClassifyContention` error paths fall through to `VerdictReal`. The
  re-run may only ever make a red run _more_ explained, never quietly greener.
- **The 15-test cap is disclosed, not silent** (`ContentionSkippedNote`), per the "no length-based truncation" rule
  above applied to sampling: a bounded look must never read as a full one.
- Worst case cost is bounded at roughly 15 × (8 s + 40 s) ≈ 12 minutes, which only happens if 15 tests genuinely hang.
  Measured reality: a 10-failure saturated run resolved in 2m51s total against a 2m0s idle baseline.

### The Docker lane re-runs inside its own container

`desktop-rust-tests-linux` starts its container **detached** (PID 1 is a bounded `sleep`) and execs each phase into it:
provision, test run, then any contention re-run. That's the whole reason for the detached shape. A re-run in a fresh
`docker run` would re-provision and recompile the workspace from a cold `CARGO_TARGET_DIR`, costing tens of minutes and
making the mechanism unaffordable; execing back into the live container costs seconds.

- **The container is removed on every exit path** (deferred `docker rm -f`, bounded by `dockerControlTimeout`). The
  `sleep` cap (`containerKeepAlive`, 4 h) exists only for the case where the check runner is hard-killed and never runs
  its defers. Don't replace it with `sleep infinity`.
- **The deadlines inside the container are deliberately identical to the host's.** The container mounts the repo, so it
  reads the same `.config/nextest.toml`; nothing grants it extra slack. A Docker-only cap bump would hide genuine
  Linux-only slowness (the one thing this lane exists to catch), and it would have to be enormous to help anyway: the
  starvation this handles was a 114× stretch (0.07 s natively, past 8 s in the container, at host load ~105 on 16
  cores). Deadlines answer "did it hang"; the isolated re-run answers "was it starved".
- **The re-run carries the failing run's package selection** (`containerRerunArgs`). Same trap as the integration lane's
  `--run-ignored only`: a re-run that selects nothing reads as "everything passed alone" and turns every real failure
  into a warn.
- **Load is sampled on BOTH sides of the VM boundary and the worse one wins** (`dockerLoadSampler`). On macOS the host's
  load average sees the Linux VM as a few vCPU threads, so a container saturated from inside barely moves it; the
  container's own `/proc/loadavg` is blind to the Playwright shards and cargo processes competing outside the VM. An
  unreadable load reads as 0 (quiet), which keeps a run red rather than softening it.
- **The container's `cargo-nextest` is pinned** (`containerNextestVersion`) to the same version the host lanes install.
  It classifies the same output with the same profile semantics, so a container drifting to `latest` would quietly
  change what a verdict means.
- **`docker exec` failing is a runner error, not evidence.** A dead container or wedged daemon flows through
  `ClassifyContention`'s error paths to `VerdictReal`, so the run stays red.

## Workspace geometry: which members a Rust check reaches

`cargo-workspace.go` reads the root `Cargo.toml`'s `[workspace] members` (globs expanded) plus each member's own
manifest, and hands every Rust check the same picture. It parses the manifests directly instead of shelling out to
`cargo metadata`, because the source scanners otherwise need no toolchain and a cold `cargo metadata` costs more than
all of them together.

A member describes ITSELF in `[package.metadata.cmdr]`, so a new crate teaches every lane and scanner about itself with
no Go edit:

- **`kind`** decides which scanners have jurisdiction. `app` (the default) is first-party code linked into the Cmdr
  process. `tool` is a first-party standalone developer CLI: its own process, its own stdout, its own SQLite connection,
  so the rules written for the app process describe a situation it isn't in (`crates/index-query`). `vendored` is a
  third-party fork whose value is that it still matches upstream (`crates/fsevent-stream`). An unknown value is an
  error, not a silent default.
- **`platforms`** is the target-OS allowlist the member claims for itself, in cargo's `target_os` spelling. Empty means
  portable. `CargoSelectionArgs` turns it into `--workspace --exclude <name>` for every lane whose target can't build
  it.

**Gotcha: `--workspace` moves a macOS-only member from the dependency graph into the SELECTION set, where its target
gate no longer applies.** `cmdr-fsevent-stream` was only ever a `cfg(target_os = "macos")` dependency of `cmdr`, so no
non-macOS lane ever touched it. Selected directly it fails at `cargo check` — not at link — with
`E0455: link kind 'framework' is only supported on Apple targets` (reproduced in a `rust:latest` container, 2026-07-30).
That's every compiling lane, including the ones named for macOS: CI's "Desktop (Rust)" job runs `desktop-rust-clippy`,
`desktop-rust-tests`, and `desktop-rust-cargo-machete` on ubuntu.

**Gotcha: the Docker lane computes its selection for `linux`, not for the host.** `desktop-rust-tests-linux` runs cargo
inside a container from a Mac, so `HostCargoSelectionArgs` would answer for the wrong OS.
`TestProvisionScriptSelectsForTheContainerNotTheHost` pins it.

Feature specs are package-qualified (`cmdr/virtual-mtp`). A bare `--features virtual-mtp` changes meaning once more than
one package is selected.

**The Linux lane is the one most likely to be starved rather than broken**, which is why it re-runs its failures alone
inside its own container (mechanics: § "The Docker lane re-runs inside its own container"). Its cores are a slice of a
host that may also be running three Playwright shards, a second container, and four `cargo-nextest` processes. Context
for reading its output:

- **A moving failure set across runs means starvation, not a defect.** Measured 2026-07-30 on one unchanged tree: 47
  timeouts while the full `--include-slow` suite ran (three Playwright shards plus a SECOND Docker container plus four
  `cargo-nextest` processes), 11 timeouts — of a nearly disjoint set — alone on a still-busy host, and zero alone on a
  settled one. Nothing was wrong with the tree. The re-run now says this for you, but a warn naming a different test
  each run is the same signal.
- **The tests that die are the trivial ones**, because nextest batches many fast tests at once and a stall takes the
  whole batch. `read_connections_get_a_smaller_page_cache_than_write_connections` (a `tempfile::tempdir()`, two SQLite
  opens, two PRAGMA reads — no threads, no sleeps, no waits) timing out at 8 s is a machine symptom by construction.
  Same shape, measured 2026-07-31: `sqlite_util::tests::cached_pages_come_from_the_shared_slab` timed out at 8 s in the
  container across three consecutive runs while taking 0.07 s natively.
- **A "too-slow" or "real" verdict from the re-run is not a load excuse.** Those keep the lane red on purpose; reach for
  a control run (the same lane, alone, on a commit before the change) rather than re-running until it's green.
- **`pkill -f check.sh` does NOT stop a run.** The runner is a compiled Go binary under the `go-build` cache parented by
  `go run .`, and killing the wrapper orphans the whole tree of Playwright, Docker, and cargo children, which then keep
  competing with whatever you start next. Kill `go-build.*/check` and `go run \. --include-slow`, then confirm with `ps`
  before drawing conclusions from the next run.

## Workspace member coverage

`workspace-member-coverage` (`IsFast`, error-level, app scope `crates`) is what stops the next crate from re-opening the
hole this whole area was built to close: before it, every cargo lane was `cmd.Dir`-scoped to the app package and every
Rust scanner walked a hardcoded `apps/desktop/src-tauri/src`, so anything under `crates/` was never tested, linted,
formatted, or scanned. Nothing was red about that — a member no lane selects compiles fine and reports nothing at all.

It asserts three things:

- **Every member is reachable.** A member no lane's target OS can build is a member whose tests never run. A member of a
  `kind` no scanner governs is a member whose sources nothing reads.
- **Every Rust check is classified**, into exactly one of `rustCargoLanes` (drives cargo), `rustScannerJurisdictions`
  (walks source trees; coverage comes from the declared kinds), or `rustMetaChecks` (reasons about the workspace rather
  than compiling or scanning it). Adding a Rust check without classifying it fails, which is the shape of "someone added
  a scanner and hardcoded a path inside it". Each cargo lane records HOW it reaches the workspace, so a targeted
  invocation can't pass for a sweep — `desktop-rust-groq-smoke` runs one `--lib` test against a live endpoint and says
  so.
- **No stale or empty classification**: an entry naming a check that no longer exists fails, the same way `ci-coverage`
  refuses to let an excuse outlive its check. So does a jurisdiction that declares neither member kinds nor
  `AppTreeOnly` — that one makes `ScannerRoots` hand back no roots, and a scanner with no roots scans nothing and
  passes. The default breadth is app + tool; anything narrower needs a `Why`.

`rustScannerJurisdictions` isn't documentation: each scanner resolves its own roots through `ScannerRoots` /
`ScannerMemberKinds`, and an undeclared check ID is an error rather than an empty list (an empty list reads as "scanned
nothing" and passes). Anything narrower than every first-party member carries a `Why`. The narrowings today:

- **`desktop-rust-log-error-macro`** is `AppTreeOnly`. `log_error!` is a crate-root `macro_rules!` no separate crate can
  invoke, so pointing it at `crates/` would make every diagnostic `log::error!` there a hard failure with no legal
  alternative. Crates raise errors as typed values the app re-raises.
- **`desktop-rust-mtp-dropping-timeout`** and **`desktop-rust-mtp-no-transport-reset`** are `AppTreeOnly`: both are
  scoped to `src/mtp/`, one app-side USB subsystem.
- **`desktop-rust-sqlite-open-direct`** is `app` only. The page-cache slab is process-wide, so a standalone CLI opening
  the first connection in its own process has nothing to protect.
- **`desktop-rust-write-ops-agent-isolation`** is `AppTreeOnly`. It fences `file_system/write_operations/`, which only
  exists in the app tree; no other member has a write engine to keep out of the agent module.
- **`desktop-pluralize-noun`** is `app` only. `pluralize` is a private module of the app crate, so a tool crate can't
  reach the helper the check directs it to.
- **`desktop-rust-cfg-gate`** governs every kind, because it pairs each member's OWN manifest with that member's tree
  and skips members that are already macOS-only at the crate level.

`claude-md-length` needed no re-rooting: it enumerates via `git ls-files` across the whole repo already.

## Apps and check counts

Checks by app and tech:

- **Desktop / Rust**: rustfmt, clippy, rustdoc (`cargo doc --all-features --document-private-items` over every
  first-party member, with every doc lint in `rustdocDeniedLints` denied and any leftover warning failing the check too;
  the vendored fork is skipped because `--all-features` turns on two mutually exclusive arms there), cargo-audit,
  cargo-deny, cargo-machete, cargo-udeps (CI-only), jscpd (warn-only; the clone list, on a per-file-pair ratchet),
  log-error-macro, sqlite-open-direct (every SQLite connection opens through `crate::sqlite_util`, so the process-wide
  shared page cache is always installed before SQLite initializes), error-string-match, write-ops-isolation (the write
  engine may not name the `agent` module: an approved operation is an ordinary operation, and an engine that can see the
  agent grows a second execution path; per-source outcomes reach a caller through the injected `OperationEventSink`
  instead), lock-poison (two lanes: an error-level one for an acquisition that records no poison-handling choice, and a
  warn-only one for a failure that's silently discarded, on a per-file ratchet), test-sleep (flags a fixed
  `thread::sleep` / `tokio::time::sleep` in test code, where a condition-based `wait_until` belongs; opt out a genuine
  sleep-is-the-subject site with `// allowed-test-sleep: <reason>`), fixed-temp-dir (flags a test fixture built on
  `std::env::temp_dir()`, where every process on the machine shares the path and two suite runs delete each other's live
  fixtures; the sanctioned fixture is `crate::test_support::TestDir`, and a site where the temp root is load bearing
  opts out with `// allowed-fixed-temp-dir: <reason>`), no-hand-rolled-fixture (bans a struct literal of
  `CachedScanResult` / `SourceHint` / `VolumePreflight` in test code, so a fixture can only be one of the shapes a named
  constructor actually builds; it ships with ZERO findings on purpose and is a regression fence rather than a finder —
  the shapes are already clean, and the point is that the next test author can't undo that by copy-pasting an old
  literal), derive-default-justified (every `#[derive(..., Default, ...)]` under `file_system/` and `cmdr-fs` carries a
  `// DEFAULT-OK: <why>` line, because a zero value on a fact-carrying type isn't "no information", it's a claim about
  the disk that nobody made), probe-unwrap-justified (flags `\.is_directory(…).await.unwrap_or(…)` in production
  `file_system/` code, where a probe that COULDN'T answer gets collapsed into a confident "no" and picks the branch that
  deletes; opt out with `// allowed-probe-unwrap: <why the guess is truthful>`), discarded-outcome (a function that
  returns NOTHING while dropping a typed answer from the free function it delegates to; three of these shipped before it
  existed, and each ended as an IPC command or MCP tool inventing a success. `Result` and `Option` returns are
  deliberately out of scope: `Result` is `#[must_use]`, so the compiler already warns, and an `Option` discard is the
  map/set idiom. That leaves exactly the gap the compiler can't see, a bare `bool` or a named outcome type. Every
  ambiguity resolves to "don't flag" — an unresolvable name, two definitions disagreeing on their return type, a method
  call — because a check people learn to ignore is worse than none. Opt out with
  `// allowed-discarded-outcome: <why nobody above needs the answer>`), mtp-dropping-timeout, mtp-no-transport-reset,
  bindings-fresh, ipc-enum-camelcase, tests, integration-tests (Docker SMB), tests-linux (slow)

The last three share one region tracker, `rustTestModState` / `advanceTestModRegion` (`desktop-rust-test-sleep.go`), in
opposite polarities: test-sleep and fixed-temp-dir scan ONLY inside an inline test module, derive-default and
probe-unwrap scan only OUTSIDE one. It arms on both test-gating `cfg` forms (`#[cfg(test)]` and the
`#[cfg(any(test, feature = "testing"))]` the `cmdr-fs` host stubs need), which `isTestGatedCfg` decides and
`TestTestModRegion_ArmsOnBothTestGatedCfgForms` pins. A tracker that only knew the literal form would read six test
doubles as production code.

- **Crates / Rust**: workspace-member-coverage (every workspace member is reachable by the cargo lanes and the source
  scanners, and every Rust check has declared which of the two it is), index-crate-isolation (no guarded crate —
  `cmdr-index`, `cmdr-fs`, `cmdr-archive` — reaches `tauri`, `tauri-specta`, or `cmdr` anywhere in its `cargo metadata`
  tree, plus a per-bucket public-surface ceiling on `cmdr-index` and `cmdr-archive`. `cmdr-fs` is deliberately uncapped:
  it's shared vocabulary whose job is to be named from everywhere. See
  `crates/cmdr-index/src/indexing/handle/DETAILS.md` for what each index number means, the crate's own entry in
  `index-crate-isolation.go` for the archive ones, and why raising either needs David's say-so). The crates' code is
  also covered by the desktop Rust lanes above, which all run workspace-wide; this scope is for checks about the crate
  boundary itself.
- **Desktop / Svelte**: prettier, eslint, svelte-kit-sync, eslint-typecheck-svelte, eslint-typecheck-typescript,
  stylelint, css-unused, a11y-contrast, a11y-coverage (every primitive has a tier-3 a11y test), ui-primitive-coverage
  (every top-level `lib/ui/*.svelte` primitive has a Debug > Components catalog section), dialog-gallery-coverage (every
  `SOFT_DIALOG_REGISTRY` id has a row in the Debug > Soft dialogs gallery, and every row names a registered id),
  btn-restyle, bare-poll, svelte-check, import-cycles, jscpd (warn-only; the frontend clone list, TypeScript and
  Svelte), message-keys-fresh (regenerate-and-diff `keys.gen.ts` from the message catalogs), message-key-naming (the
  `area.feature.leaf` shape + known-area first segment), message-keys-unused (catalog keys never referenced in `src/`;
  error-level, with a closed dynamic-prefix allowlist for runtime-built keys), message-screenshots-fresh (warn-only;
  drift between the committed i18n capture report and the catalogs' `@key.screenshot` couplings; runs the coupler's
  `--check`, reads no PNGs), i18n-stale (warn-only; a non-`en` translation whose `@key.sourceHash` no longer matches the
  English value), i18n-parity (ERROR; each locale key's `{placeholder}`+`<tag>` set, or raw `{token}` set for
  `errors.*`, must equal English's, since a mismatch crashes at runtime), i18n-icu (ERROR; every non-`errors.*` locale
  message must compile via `intl-messageformat`), i18n-tag-param-collision (ERROR; a message naming a `<tag>` and a
  `{param}` alike renders the param as a stringified handler, because `Trans` lets the tag win the merged lookup),
  i18n-trans-snippets (ERROR; a message `<tag>` with no matching `snippets={{ … }}` key at the call site renders as
  nothing, so its inner text silently vanishes; catches a rename finished on only one side), i18n-plural (ERROR; each
  plural covers its locale's required CLDR categories, gated on the English source's plural shape), i18n-coverage
  (ERROR; keys missing from a locale, or byte-identical to English without a `@key.sameAsSourceJustification`, either of
  which ships a half-translated locale), i18n-dont-translate (warn-only; a curated brand/system token English carries
  but the locale dropped), knip, type-drift, tests, e2e-linux-typecheck, e2e-linux (slow), e2e-playwright (slow)
- **Desktop / Docs**: pluralize-noun, third-party-notices (regenerate-and-diff `THIRD-PARTY-NOTICES.md` from
  `Cargo.lock` + `pnpm-lock.yaml` via cargo-about and `pnpm licenses list`; the accepted-license list is derived from
  `deny.toml` rather than duplicated, the output is pinned to be identical on macOS and Linux, and the runner's input
  fingerprint is what keeps it off unrelated runs)
- **Website / Astro**: prettier, eslint, typecheck, build, html-validate, bundle-size (warn-only), e2e
- **Website / Docker**: docker-build
- **API server / TS**: oxfmt, eslint, typecheck, tests
- **Analytics dashboard / Svelte**: svelte-kit-sync, eslint, stylelint, svelte-check, import-cycles, knip, tests, build.
  Stylelint, knip, and import-cycles run through the same `runStylelintCheck` / `runKnipCheck` / `runImportCyclesCheck`
  helpers the desktop lanes use, parameterized by app dir. `dashboard-build` is NOT redundant with
  `dashboard-svelte-check`: the `$lib/server` boundary guard trips only at build time, so it's the only check standing
  between a stray runtime import and shipping a server-side API key into the browser bundle. The dashboard deliberately
  has no css-unused (Tailwind supplies every utility class, so "undefined class" would fire on all of them), no
  a11y-contrast (that tool models desktop's accent matrix and light/dark token pairs; the dashboard is dark-only), no
  type-drift (no Rust), and no bare-poll (no Playwright helpers).
- **Scripts / Go**: gofmt, go-vet, staticcheck, ineffassign, misspell, gocyclo, nilaway, deadcode, go-tests, govulncheck
- **Other / Metrics**: file-length (warn-only), CLAUDE.md-reminder (warn-only), claude-md-length (warn-only),
  invariant-density (warn-only; `❌` rules per subsystem, absolute and per 1,000 source lines, on a strict ratchet),
  resident-doc-budget (warn-only; caps the always-resident root-CLAUDE.md + @-imports + rules bundle), docs-reachable
  (errors when a CLAUDE.md/DETAILS.md/docs file isn't reachable from the root CLAUDE.md), docs-dead-links (errors on a
  doc link, or a bare backtick path naming a doc, whose local target doesn't exist), docs-link-text (errors on a
  Markdown link whose text is its own target path), claude-md-details-sibling (errors when a non-root CLAUDE.md
  lacks/doesn't reference a sibling DETAILS.md), docs-table-hygiene (errors on any 2-column table or any table column
  wider than 100 chars in agent-facing docs), changelog-commit-links, workflows-rustup (forbids
  `rustup target/component add` in workflows), ci-coverage (registry-to-workflows contract)
- **Other / Security**: workflows-hardening (SHA-pinning, no `pull_request_target`, job-scoped `id-token: write`)

## Key decisions

**Decision**: every operational `cargo` command in checks passes `--locked`. **Why**: without it, cargo silently
re-resolves `Cargo.lock` whenever upstream metadata shifts (a yank, a new transitive dep version). For a 1028-crate
lockfile, that resolution window is wide and lets a freshly-published malicious version land mid-build. `--locked` fails
loudly if the lockfile would change. Applies to `cargo clippy`, `cargo nextest run` (in both `desktop-rust-tests` and
`desktop-rust-integration-tests`), and `cargo udeps` (which runs on the pinned nightly). Audit/deny/machete read
`Cargo.lock` without updating it, so `--locked` is moot for them, but the install of those tools still uses `--locked`
to lock the tool's own dep tree.

**Decision**: every tool install pins `--version` and `--locked` (cargo) or `@vX.Y.Z` (Go). **Why**: an unpinned tool
install (`cargo install cargo-audit` or `EnsureGoTool(..., "@latest")`) means each fresh checkout pulls whatever's
latest. A wave-1-2-class compromise of any of these tool repositories would auto-propagate. Pinning is the Go-side
equivalent of the pnpm `minimum-release-age` defense (a fresh version can't land without a deliberate bump).

**Decision**: `third-party-notices` pins the license file for crates that ship more than one, and verifies the pin
landed. **Why**: cargo-about reads whichever candidate file the filesystem enumerates first, and APFS and ext4 don't
enumerate alike, so `libmimalloc-sys` and `miniz_oxide` produced different notices on a Mac than on the Linux runner:
the check could not be green in both places at once, and wasn't from the day it landed. `licenseClarifications` names
the file instead of leaving the choice to the host. The checksums cargo-about wants alongside a pin don't guard
themselves: a stale one yields a warning, exit code 0, and a silent fall back to scanning, which is the exact behavior
being removed. So `verifyClarifications` compares each pinned crate's resolved `source_path` against the pin and fails
when they differ, naming the `shasum` command that fixes it. Every text also carries its `Text from:` file into the
generated notices, so the next crate to develop this ambiguity surfaces as a diff line naming a file rather than as a
license count that moved for no visible reason.

**Gotcha**: a clarification REPLACES the crate's declared license expression, so `crateClarification.license` repeats
that expression verbatim (`miniz_oxide` is `MIT OR Zlib OR Apache-2.0`, not the single `MIT` whose text is pinned).
Shortening it to the pinned license silently narrows what the in-app Acknowledgements dialog reports. Pinning several
files for one crate is normal and sometimes required: `libmimalloc-sys` ships the Rust wrapper's MIT and compiles the
vendored mimalloc C sources' MIT into the binary, and both copyright holders have to be credited, which letting the
filesystem pick never did.

**Decision**: `cargo-about` installs from its checksum-pinned prebuilt release tarball, falling back to `cargo install`
only on a host with no published asset. **Why**: the source build runs ~3 min and dominated this check on any machine
without the tool, which in CI meant every single run: rust-cache's save step is
`post-if: success() || CACHE_ON_FAILURE`, so while the check was failing the job saved no cache at all, and every run
rebuilt the tool (and the whole Tauri tree) before failing again. CI now passes `cache-on-failure: true`, and the
download costs ~2 s. The sha256 pins carry exactly the weight `--version --locked` carries for a `cargo install`: this
binary gets executed, so an asset that changed underneath us must fail loudly rather than run. The installed binary's
`--version` is verified rather than assumed from presence, because a stale local cargo-about harvests license files by
its own rules and hands David a diff CI can't reproduce.

**Decision**: `cargo-udeps` runs on a dated nightly pinned in `desktop-rust-cargo-udeps.go`, not on floating `+nightly`.
**Why**: udeps needs nightly, but a floating one makes an unrelated upstream lint change break the scheduled "Slow
checks" job at an arbitrary time (a `unused_imports` tightening did exactly that), and a compromised nightly would land
transparently. Same reasoning as the stable pin in `rust-toolchain.toml`. The check installs the pin itself when rustup
lacks it (reading `rustup toolchain list`, rather than classifying a cargo failure by its message), and CI's "Install
nightly toolchain" step asks the check tool for the version via `./scripts/check/check --print-nightly`, so the date
exists in exactly one place. Renovate can't track it: dated Rust nightlies aren't a Renovate datasource (there's no
registry of nightly dates to query, and the `rust`/`rust-version` datasources cover stable releases only), so the bump
is a maintenance task instead, listed in `docs/maintenance.md`.

### Bumping the pinned nightly

1. Pick a nightly at least 3 days old (the same stability window `minimumReleaseAge` gives dependencies). Confirm it
   exists: `curl -sI https://static.rust-lang.org/dist/<YYYY-MM-DD>/channel-rust-nightly.toml` returns `200`.
2. Edit `nightlyToolchain` in `checks/desktop-rust-cargo-udeps.go`. That's the only place the date appears.
3. Run `pnpm check cargo-udeps` (it installs the toolchain if needed) and fix whatever new lints the newer nightly
   surfaces. Nightly lints are usually genuine (the `unused_imports` tightening flagged real redundant imports), so fix
   the code rather than reaching for an `allow`.

**Decision**: `cargo-deny` advisories check disabled; use `cargo-audit` instead. **Why**: Tauri's transitive
dependencies (gtk3-rs, unic-\*, fxhash, proc-macro-error, etc.) trigger unmaintained-crate advisories we can't control.
`cargo-audit` still catches critical security vulnerabilities. License, bans, and sources checks in `cargo-deny` remain
active. See comment in `deny.toml`.

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
files for ungated references.

Three things that detection has to get right, each learned from a miss:

- **Any `crate::` reference counts, not just `use` lines.** A `use`-only scan reads `unsafe { libc::geteuid() }` as
  nothing at all, which is how a `#[cfg(unix)]` test in `cmdr-fs` reached macOS-only `libc` and broke the Linux lane
  with a green local run. Trailing `//` comments are stripped first so a SAFETY note explaining a gated call isn't
  itself reported.
- **A crate declared unconditionally too isn't macOS-only.** `tar` is macOS-only for production extraction and an
  all-target `[dev-dependencies]` so the tarball-building test compiles everywhere; counting it would report five
  perfectly fine test lines.
- **The gate walk balances brackets rather than pattern-matching continuation lines.** The gate can sit above a
  multi-line `#[cfg_attr(feature = "testing", allow(...))]`, whose inner `)` lines match no enumerable shape; stopping
  there reports the most carefully gated code in the tree (`thread_qos.rs`).

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
`crates/cmdr-fs/src/ignore_poison.rs`. The check is a fast-lane Go scanner (every Rust source root, modeled on
`error-string-match`) running TWO lanes over one scan: an error-level intent lane, and a warn-only swallow lane (§ "The
lock-poison swallow lane" below). The intent lane flags bare unwraps and non-poison `.expect(…)`. Its matcher requires
empty parens (`.lock()` / `.read()` / `.write()` with nothing between) immediately followed by `.unwrap()` / `.expect(`,
so `io::Read::read(&mut buf).unwrap()`, `io::Write::write(buf).unwrap()`, and tokio's `mutex.lock().await` all pass
through; `try_lock` / `try_read` / `try_write` are out of scope by name. Opt out of EITHER lane with
`// allowed-lock-poison: <reason>` on the line above or as a trailing comment. Unlike `error-string-match`, it skips
in-file `#[cfg(test)]` mods (tracked by brace depth): a poisoned lock in a test means the test already panicked, so
aborting there is harmless. Whole test files are skipped via `isRustTestPath` (shared with `test-sleep`), so a
`tests.rs` split into themed modules under a `tests/` directory keeps passing.

### The lock-poison swallow lane

**Decision**: a second, warn-only lane in the same check for an acquisition whose failure is silently DISCARDED, on a
per-file ratchet in `lock-poison-allowlist.json`. **Why**: two bugs shipped from this shape, and neither was visible to
the intent lane — `if let Ok(guard) = cache().lock()` emptied three recents lists, and
`match known.lock() { Err(_) => return }` killed the Linux volume watcher for a whole session. Both READ as handled
while doing something worse than the panic the intent lane bans: no log line, no recovery, and a facility that stays
dead. A third (the MTP device watcher) and 48 more sites came out of the first run.

The lane classifies each acquisition by what CONSUMES its `Result`, so the four shapes fall out of one rule rather than
four matchers:

- A `let Ok(…) = <lock>()` binding: an `if let` with no `else`, or a let-else / `else` branch that records nothing.
- A `match <lock>() { … }` whose `Err` arm (or the wildcard standing in for it) returns, breaks, or yields a default.
- A combinator chain ending in `.ok()` or the `unwrap_or` / `map_or` family.

A handler is accepted when it does one of the three things the policy sanctions: recover (`into_inner` /
`*_ignore_poison`), abort loudly (`panic!` / `expect`), or hand the failure to the caller (an `Err(…)`). Anything else
substitutes a default value out of thin air, which IS the bug class.

Parsing notes, all of them load-bearing: the consuming construct must sit at the same bracket depth as the acquisition
(in `match watched.and_then(|w| w.lock().ok())` the `match` reads what the closure returned, so the closure's `.ok()` is
what discards); comments are BLANKED rather than removed, so a brace in prose can't unbalance the depth count and a
`panic!` in prose can't read as intent, while byte offsets stay comparable across the scanners; a chain is read to the
end of its statement, so a `.map(…).unwrap_or_default()` wrapped over three lines is still seen whole; and a shape the
parser can't resolve is left alone rather than guessed at.

**Decision**: the allowlist keys on the FILE, valued in its site count. **Why**: same reasoning as the jscpd lanes — a
`file:line` key moves the moment anything above the site changes, while a count only moves when somebody writes or
removes a swallow. No slack buffer, for the same reason. Shrink-wrap ratchets an entry down to the current count and
drops a file that reaches zero, so the number only goes down; raising one needs David's OK
(`.claude/rules/file-length-allowlist.md`).

**Decision**: `mtp-dropping-timeout` check to keep wall-clock timeouts and task aborts away from mtp-rs calls. **Why**:
A PTP transaction is command → data → response over one bulk pipe, so dropping its future mid-data-phase leaves the
device expecting bytes nobody will send — that's the single trigger behind every phone wedge we've reproduced, and the
software recovery isn't guaranteed to get the device back. mtp-rs bounds each USB transfer itself and fails cleanly, so
an outer deadline (whose clock starts earlier) can only preempt a clean failure with a wedge. The check is a fast-lane
Go scanner over `apps/desktop/src-tauri/src/mtp/` (modeled on `lock-poison`, reusing its `#[cfg(test)]`-mod skip) that
flags `tokio::time::timeout(` and `.abort()`. Opt out with `// allowed-dropping-timeout: <reason>` when the dropped
future genuinely holds nothing on the wire; the two current exceptions are the device-lock wait and the event loop's
interrupt-endpoint poll. Rationale in full: `apps/desktop/src-tauri/src/mtp/connection/DETAILS.md` § "No dropping
timeouts".

**Decision**: `mtp-no-transport-reset` check, with NO opt-out directive. **Why**: The Still Image Class `DEVICE_RESET`
control request looks like the missing "unwedge the pipe" step in session-reset recovery, and it will keep looking like
one to every future reader. On Android it's a kill switch: `MtpServer` answers it by dropping its FunctionFS endpoints
and never re-arming them, while the USB controller stays `configured`, so the phone keeps enumerating and answering
nothing until it's physically replugged (verified on a Pixel 9 Pro XL via `adb logcat`, 2026-07-21). The check is a
fast-lane Go scanner over `apps/desktop/src-tauri/src/mtp/` flagging `reset_by_serial(` / `reset_by_location(` /
`reset_first(` in any file, tests included. It has no directive on purpose: reintroducing a reset means deleting the
check, and that deliberate act is the whole point. Rationale in full:
`apps/desktop/src-tauri/src/mtp/connection/DETAILS.md` § "No transport reset in recovery".

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

**Decision**: `jscpd-rust` and `groq-smoke` are CI-only. **Why**: Cost per catch. Measured over 24 days of
`~/cmdr-check-log.csv`, jscpd burned 20 122 CPU-seconds across 837 local runs for one real finding, by a wide margin the
worst ratio in the suite; copy-paste detection is a periodic sweep, and a duplicate that lands on Monday is just as
findable on Friday. groq-smoke burned 9 211 CPU-seconds across 106 runs (70 s median) for four, and what it validates is
a third-party provider's live contract rather than our code, so it can only ever go red on Groq's schedule. Both keep
their existing CI steps, so neither stops being enforced. groq-smoke keeps `IsSlow` alongside `CIOnly`: that's what
holds it out of CI's default lane, leaving its one dedicated step in the nightly slow-checks workflow as the only place
it runs.

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

**Decision**: every rustdoc lint the project holds itself to is DENIED, and a warning that survives anyway fails the
check. **Why**: a warn-level doc lint is a defect nobody sees. `cargo doc`'s output is buffered and thrown away on a
green run, so ~90 of them (redundant explicit link targets, `<usage>` angle brackets read as HTML, a mislabeled
` ```ignore ` block) sat unnoticed for months and only surfaced when an interrupted run dumped the raw stream. The
contract is now binary: `rustdocDeniedLints` is the list, anything on it is an `error`, and anything else that warns is
unowned (a new lint class, or a doc problem outside the list) and fails with the warning shown, so it either joins the
list or gets fixed.

**Decision**: `--document-private-items`, with `private_intra_doc_links` explicitly ALLOWED. **Why**: none of these
crates are published, so rustdoc here is an internal artifact and a public item's doc naming the internal it delegates
to (`ArchiveIndex` explaining that its read handles live behind `EntryStore`) should RESOLVE rather than render as plain
text. The lint doesn't disappear under the flag, it changes meaning ("resolves only because you passed
`--document-private-items`"), which is a warning about a build nobody runs — hence `-A`, stated next to the denials
instead of filtered out of the output. The flag also puts private items' own docs under the link gate, which is where
~70 genuinely broken links were hiding.

**Decision**: rustdoc output is filtered per LINE-anchored diagnostic header, never per blank-line-separated paragraph.
**Why**: cargo runs its `Documenting <crate>` progress line straight into the first diagnostic with no blank line, and
rustdoc glues `error: could not document <crate>` to the warning count above it. A paragraph split therefore hands a
diagnostic whatever preceded it, so a diagnostic that opens or closes the stream gets swallowed. `rustdocDiagnostics` in
`desktop-rust-rustdoc.go` starts a new block at every column-zero `error…:` / `warning…:`, keeps every following line
(the `-->` locator, source excerpt, `= note`, `= help`, and any trailing `help:` suggestion) with it, and ends a block
at cargo's own progress lines. Both severities survive, since both fail the check. `rustdocFailureOutput` falls back to
the raw stream when nothing parsed, so a toolchain failure or a killed process still reports a reason. The fixture in
`desktop-rust-rustdoc_test.go` is shaped like real cargo output for exactly this reason; a tidy blank-line-separated one
passes while the real thing fails.

**Gotcha**: a module documented TWICE (an outer `///` on the `mod x;` declaration plus the file's own `//!` header)
makes rustdoc resolve BOTH fragments in the PARENT's scope, so every link the module writes about its own items breaks
and the diagnostics come back with no source span (only "the link appears in this line"). Ten `Index::*` links in
`cmdr-index` failed this way. Document a module in its own header; the parent's module list shows that first line
anyway.

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
