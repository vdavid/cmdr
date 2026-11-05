# The four costliest local check lanes (2026-11-05)

Why `clippy`, `svelte-tests-related`, `eslint-typecheck-svelte`, and `eslint-typecheck-ts` cost ~42 machine-hours of
local `pnpm check` in 14 days, where the time goes inside each, and the ranked cuts. Cuts 1–4 and the catalog bug have
landed: § "Results" has the before/after numbers. Runner background: `scripts/check/DETAILS.md` § "Input fingerprint
cache".

## Method

- **Data**: `~/.local/share/check-runner/cmdr/check-log.csv`, rows from 2026-10-22 to 2026-11-05, imported into SQLite.
  The `check` column holds nicknames.
- **Timings**: this worktree (`worktree-agent-ae96dd9c8b4a8657c` at `f0eac16a60`), M-series Mac, 16 cores, rustc/cargo
  1.99.1, vitest 5.0.5, ESLint 10.13.0, typescript-eslint 8.72.1, TypeScript 6.0.3. The load average swung from 228 down
  to 5 during the session (other agents building); every timing below carries the load it ran under. Repeats agreed
  within ~2 s at load under 25.
- **Probes**: a comment line appended to one file, the lane timed, the file restored and the lane timed again. ESLint
  was profiled with `TIMING=30` plus `--stats -f json`. Vitest's related-file selection was driven through its Node API
  (`createVitest` + `specifications.filterTestsBySource`) and Vite's module graph.

## What the CSV says

14 days, by lane and result (count, mean seconds, hours):

- `clippy`: 610 pass (66 s, 11.2 h), 70 fail (305 s, 5.9 h), 22 cached.
- `svelte-tests-related`: 403 pass (106 s, 11.8 h), 76 fail (203 s, 4.3 h), 52 skip, 0 cached.
- `eslint-typecheck-svelte`: 203 pass (203 s, 11.4 h), 26 fail (255 s, 1.8 h), 9 cached.
- `eslint-typecheck-ts`: 236 pass (117 s, 7.6 h), 70 fail (230 s, 4.5 h), 10 cached.

Percentiles of passes (p10 / p50 / p90, seconds):

- `clippy`: 14 / 47 / 134.
- `svelte-tests-related`: 17 / 99 / 194.
- `eslint-typecheck-svelte`: 104 / 156 / 326. Floor 91 s.
- `eslint-typecheck-ts`: 52 / 87 / 211. Floor 47 s.

Cache misses are structural, not bad luck:

- **Named checks always run fresh** (`DETAILS.md` § "Flags / escape hatches"), and `AGENTS.md` tells agents to name
  `svelte-tests-related` and `clippy` before every commit. `svelte-tests-related` is `NamedOnly`, so it can never hit
  the cache: 0 of 531.
- The two ESLint lanes ran 238 and 316 times while the default-lane `svelte-tests` ran 112 times, so more than half
  their runs are named (the CI failure message itself says to run them by name) or scoped (`pnpm check svelte`).
- Their unnamed runs miss too: `svelte-check`, with the same `svelteInputs`, hit the cache 13 times in 400. By the time
  a full `pnpm check` runs, some frontend file has always changed.
- 94 `svelte-tests-related` passes (2.6 h) logged the same message (same test count, same file count) as the run before,
  within an hour. Most are likely identical inputs re-run.

The ESLint lanes drifted. Daily pass floor, `eslint-typecheck-svelte`: ~30 s in July, ~50 s through 2026-10-02, ~80 s by
10-07, ~100 s from 10-16, 105–174 s in November. `eslint-typecheck-ts`: 22 s → 37 s → ~50 s over the same span. Monthly
means doubled from September to November. § ESLint has the cause.

## `svelte-tests-related`: three runs in four are the whole suite

- 308 of 403 passes ran 3,000+ tests; most ran 17,900–19,700, which is the entire suite (1,392 spec files). They
  averaged 130 s. Passes under 1,000 tests averaged 20 s. One run with a single changed file ran 17,958 tests.
- **Cause: the setup file's import closure.** Vitest 5 selects every spec when a changed file is reachable from a
  `setupFiles` entry (`AffectedModulesResolver.findAffectedInEnvironment`, `vitest/dist/chunks/cli-api.*.js`).
  `src/test-setup.ts` imports `#lib/intl/messages.svelte.js`, and that module's two lazy `import()`s pull in half the
  app's hubs:
  - `test-setup.ts → messages.svelte.ts → logging/logger.ts → ipc/bindings.ts`
  - `test-setup.ts → messages.svelte.ts → settings/index.ts → settings/restricted-settings-bridge.ts → tauri-commands/index.ts`
- The closure is 185 modules, 173 of them ours: all 69 of `tauri-commands/`, 33 of `settings/`, 13 of `commands/`, 8 of
  `shortcuts/`, plus `logging/`, `utils/display-path.ts`, and more. A random sample of 40 source files hit it twice
  (`settings/definitions/ai.ts`, `file-operations/mutation-error.ts`).
- **It's the hot files.** Of 588 commits touching `apps/desktop/src` since 2026-10-22, 163 (28%) touch the closure, and
  `ipc/bindings.ts` alone accounts for 116: any Rust IPC change regenerates it. The lane diffs against the merge-base,
  so one such commit anywhere on the branch turns every later run into a full-suite run.
- **Measured** (load 5–7): editing `multi-rename/counter-token.ts` (23 related spec files) took the lane 23 s; editing
  `utils/display-path.ts` (closure) took 112 s for the full suite. No change: 2 s, a skip.
- Side finding: a message catalog edit (`intl/messages/en/*.json`) selects 0 tests: catalogs arrive through the
  `virtual:cmdr-catalog` plugin, which the graph walk can't see, and `relatedSourceExts` drops `.json` anyway. CI's full
  suite covers it; worth knowing when an agent trusts this lane after a copy edit.

## ESLint: per-file program rebuilds and one expensive rule

**Svelte pass** (437 files; 125 s wall at load 15):

- Parse is 94 s of it, rules 27 s. Parse costs a near-constant ~200 ms per file (p10 169, p50 197, p90 226 ms), after a
  one-time ~7 s for the first file.
- Why: svelte-eslint-parser hands typescript-eslint's `projectService` a virtual TS version of each component, which
  differs from disk, so every file updates the program and gets a fresh type checker. The per-file cost scales with the
  program's size, and the program is the whole `tsconfig.json`: ~2,700 TS files, ~1,300 of them tests no component
  imports. That's the drift: per-file cost × file count, both growing (more components, more tests, the 2026-10-12
  SvelteKit 3 move).
- Proof, same load band: with a program that leaves out `*.test.ts` and `scripts/`, parse drops to 52 s and the pass to
  80 s. Via legacy `parserOptions.project` pointed at a test-free `tsconfig` (`include` src TS + `.svelte`, `exclude`
  tests), parse is 45 s and the pass 73 s: **−42%**, still green, the type-aware rules still running (rule time
  unchanged at 25 s, and the lane's unused-directive reporting stayed silent).
- `@typescript-eslint/no-unsafe-enum-assignment` costs 16.5 s, 62% of all rule time. Turning it off took rules from 26.7
  s to 9.7 s and the pass from 125 s to 108 s. The whole frontend has **one** enum, a `const enum` in
  `src/lib/search/live-ranking.ts`.

**TypeScript pass** (2,728 files; 68–71 s wall at load 9–13):

- Parse 14 s, rules 44–47 s, and ~10 s of program setup. No per-file rebuild problem here.
- The type-aware rules dominate: `no-unsafe-enum-assignment` 8.0 s, `no-floating-promises` 7.1 s, `no-misused-promises`
  6.3 s, `no-duplicate-type-constituents` 5.4 s. (The first rule to ask about a node pays the checker's lazy work, so
  per-rule numbers skew; the off-switch run is the real measure.)
- Enum rule off: 62 s against 68 s.

**Failures roughly double**: a red verdict runs the no-typecheck fix pass and then the verdict again
(`typeAwareFixPasses`), so 70 TS fails averaged 230 s against 117 s for a pass. Type-aware findings never auto-fix, so
on those the second verdict is pure cost.

## `clippy`: cheap when warm, very expensive when red

- Warm, no change: 1.8 s. One-line edit in the app crate: 14–15 s (load 18–33). In `cmdr-fs`, which everything depends
  on: 18–20 s (load 12–16). So the 47 s median is mostly contention and post-rebase rebuilds, not the lane's design.
- **The failure path is the cost**: 70 fails averaged 305 s, 5.9 h. On red, the lane runs `cargo clippy --fix` (without
  `-D warnings`), touches every member's crate root, then re-runs the `-D warnings` verdict. Timed on a clean tree at
  load 6–24: `--fix` took **167 s** (cargo fix forces a re-lint of every workspace member through its own wrapper), and
  the verdict right after it took **61 s** (its fingerprints no longer matched). A plain re-run after that: 2 s.
- **It rarely helps**: every sampled fail message reads "clippy found unfixable issues", and the saved outputs are
  compile errors (`E0425`, `E0063`, …) or lints with no machine-applicable fix (`type_complexity`,
  `allow_attributes_without_reason`). The fix pass can't fix either.
- Second-order cost: `touchWorkspaceCrateRoots` bumps every crate root's mtime, so the next `rust-tests`,
  `bindings-fresh`, and integration build in that worktree recompile every member too (20–100 s each, per
  `cargo-lane-feature-thrash.md`).
- Flags are otherwise aligned: clippy's units live in their own fingerprints and don't thrash the test build
  (`docs/notes/cargo-lane-feature-thrash.md` § "Clippy is not part of this"). The `download-llama-server.go` prelude is
  inside the 1.8 s no-op.

## Proposed cuts, ranked by expected savings

Savings are machine-hours per 14 days at the logged run rates, rounded down.

1. **Cut the vitest setup file's import closure** (`svelte-tests-related`)
   - **Problem**: `test-setup.ts → messages.svelte.ts` reaches 173 of our modules, including `bindings.ts` and all of
     `tauri-commands/`, so 76% of related runs are the full suite.
   - **Impact**: ~6–9 h. Full-suite runs at ~130 s become scoped runs at ~20–25 s whenever the branch's only closure hit
     is a hub like `bindings.ts`.
   - **Solution**: make `messages.svelte.ts` stop importing `logging/logger` and `settings/index` itself: the app's boot
     wires them in through a setter (a logger sink and a locale persister), and `test-setup.ts` installs test doubles.
     Add a regression test that drives `filterTestsBySource` (as in this note's method) and asserts a `tauri-commands/`
     or `bindings.ts` edit selects fewer than all specs, so the closure can't regrow silently.
   - **Size**: S–M (one module's two lazy imports, its boot call site, the setup file, one test).
   - **Clear win or tradeoff?** Clear win. CI is untouched (it runs the full `svelte-tests`).

2. **Give the Svelte ESLint pass a test-free TS program** (`eslint-typecheck-svelte`)
   - **Problem**: every `.svelte` file rebuilds a type checker over a ~2,700-file program, half of it tests.
   - **Impact**: −42% of the lane, ~5 h locally. **CI shares it** (same lane under `--ci`).
   - **Solution**: a `tsconfig.lint-svelte.json` extending `tsconfig.json` with `include` = `src/**/*.ts` +
     `src/**/*.svelte` + `test/fixtures/*.svelte` and `exclude` = `src/**/*.test.ts`, used by the `**/*.svelte` block
     via `parserOptions.project` (legacy mode; `projectService` can't pick a tsconfig per block). Before landing, verify
     parity: plant a cross-file `no-floating-promises` in a component (the check this split was originally verified
     with) and diff findings on the whole tree, old against new.
   - **Size**: S.
   - **Clear win or tradeoff?** Mostly a win. The cost: legacy `project` mode for one block, which typescript-eslint
     keeps supported but no longer recommends.

3. **Gate clippy's fix pass on something fixable** (`clippy`)
   - **Problem**: a red verdict always pays `--fix` (167 s) plus a forced re-lint (61 s+) and invalidates the other Rust
     lanes, though nearly every failure is a compile error or an unfixable lint.
   - **Impact**: ~4 h direct, plus the avoided full-workspace rebuilds in the next Rust lanes.
   - **Solution**: run the verdict with `--message-format=json` and enter the fix branch only when a diagnostic carries
     a `MachineApplicable` suggestion (a typed field in cargo's JSON, so no string matching). Compile errors and
     unfixable lints report at once. When it does fix, keep today's flow.
   - **Size**: S.
   - **Clear win or tradeoff?** Clear win. CI never fixes, so nothing to share.

4. **Drop `no-unsafe-enum-assignment` and ban enums** (both ESLint lanes)
   - **Problem**: 17 s per Svelte pass and ~6 s per TS pass guarding one `const enum`.
   - **Impact**: ~2 h locally (−14% Svelte, −9% TS). **CI shares it.**
   - **Solution**: turn the `as const` object pattern into the house rule: convert `MatchQuality` in `live-ranking.ts`,
     set `erasableSyntaxOnly` (or a `no-restricted-syntax` on `TSEnumDeclaration`), and switch the rule off with a
     comment saying why.
   - **Size**: XS.
   - **Clear win or tradeoff?** Clear win, if you're happy to ban enums (they're already absent everywhere but one
     file).

5. **Let named runs reuse a cached pass for named-only lanes** (`svelte-tests-related`, and optionally the ESLint lanes)
   - **Problem**: "named ⇒ fresh" means the pre-commit ritual can't skip a lane whose inputs haven't changed; 94 related
     passes (2.6 h) look like exact repeats.
   - **Impact**: ~2–3 h on `svelte-tests-related`; more if extended to named ESLint runs (unmeasurable from the CSV,
     which carries no fingerprint).
   - **Solution**: a `NamedOnly` lane consults the cache like an unnamed one; `--fresh` stays the escape hatch. For
     `svelte-tests-related`, the fingerprint should add the merge-base SHA, since the selection depends on it.
   - **Size**: S.
   - **Clear win or tradeoff?** Tradeoff: it softens a documented escape hatch, and an agent that names a lane to watch
     it run again needs `--fresh`.

6. **Skip the ESLint re-verdict when the fix pass changed nothing** (both ESLint lanes)
   - **Problem**: a red run lints twice even when the safe fix pass touched no file, which is the case for every
     type-aware finding.
   - **Impact**: ~2 h locally (fails cost ~2× a pass).
   - **Solution**: compare the lane's files' content hashes (or `git diff --stat` of the scope) before and after the fix
     pass; if unchanged, return the first verdict's output.
   - **Size**: XS.
   - **Clear win or tradeoff?** Clear win.

7. **Lint only what the branch touched, locally** (both ESLint lanes; the big lever, not first)
   - **Problem**: a full `pnpm check` lints all ~3,200 files with type information even when one component changed; it's
     the single largest block (~25 h).
   - **Impact**: up to ~15 h locally after cuts 2 and 4.
   - **Solution**: mirror `svelte-tests-related`: locally, lint the branch's changed files plus their direct importers;
     CI keeps the full lanes. A type-aware finding in an unchanged, non-importing file can then reach CI first (a
     changed return type making a distant call a floating promise).
   - **Size**: M.
   - **Clear win or tradeoff?** Tradeoff: local verdicts stop being exact, and a miss costs one CI cycle (25–35 min).
     Worth deciding after 2 and 4 land and the lanes are re-measured.

Considered and not proposed:

- **Sharding the Svelte pass across processes**: better wall time on an idle Mac, same or more CPU (each shard pays the
  ~7 s program setup), and the Mac is rarely idle.
- **ESLint `--cache`**: unsound with type-aware rules (a file's findings depend on other files' types), and almost every
  run changes some input.
- **Narrowing clippy's flags or adding `-p` lanes**: closed by the measurements in `cargo-lane-feature-thrash.md`; warm
  clippy is already 15–20 s per edit.

## Results

Measured in `worktree-agent-ad744b063f60c1fb9` on 2026-11-05, same Mac and tool versions as § Method, under heavy
contention from other agents (load average 55–170), so wall times are inflated; the spec counts are exact.

1. **Setup closure cut** (`installMessageRuntimeHooks`; guard: `apps/desktop/src/test-setup-closure.test.ts`).
   - Closure: 185 modules (173 ours) before, 8 after (the setup file, the message runtime, its three locale helpers,
     `startup-catalog.ts`, `display-path.ts`, and the layout shim).
   - Every module of the old closure (172 by a static walk from `test-setup.ts`, the logger, and `#lib/settings`), fed
     one at a time to Vitest's `related` selection: before, all 172 selected the full suite (1,392 spec files); after, 8
     do (the new closure), and the median selects 425 (mean 507).
   - Hubs: `ipc/bindings.ts` 1,392 → 691 spec files, `tauri-commands/index.ts` 1,392 → 392, `logging/logger.ts` 1,392 →
     629, `settings/definitions/ai.ts` 1,392 → 485.
   - Wall: the full suite took 311 s (load 55–120); `vitest related` for `settings/definitions/ai.ts` now runs 485 files
     in 124 s (load 100 → 55). At the report's load 5–7 that's roughly 112 s → 40–45 s for a hub edit, and a leaf
     formerly in the closure (most of `settings/`, `logging/`) now runs only its own tests.
2. **Clippy fix gate** (`parseClippyJSON`, `scripts/check/checks/desktop-rust-clippy-json.go`).
   - Planted an `#[allow(dead_code)]` with no reason in `cmdr-sort` (unfixable: `allow_attributes_without_reason`,
     `items_after_test_module`) and timed `pnpm check clippy` on a warm tree: **379 s** with the old lane (load 119 →
     33; `--fix`, the crate-root touch, and the re-verdict), **6 s** with the new one (load ~120), same diagnostics.
   - The old run's crate-root touch then cost the next clippy a rebuild of every member; the new lane touches nothing on
     this path.
   - Warm no-op: 13 s total under load (`--message-format=json` reuses the cached plain verdict: cargo doesn't
     fingerprint the message format).

- **Cut 4, `no-unsafe-enum-assignment` off** (`erasableSyntaxOnly` in `apps/desktop/tsconfig.json` bans enums;
  `MatchQuality` is an `as const` object; eight parameter-property constructors became plain fields).
  - Svelte pass, back to back at load 12–20: **141 s → 117 s** (−17%); rule time 30.6 s → 11.8 s, of which the enum rule
    alone was 19.0 s (62%). Parse unchanged (106 s → 102 s).
  - TypeScript pass, back to back at load 12–20: 90 s → 97 s, rule time 59.0 s → 59.8 s: no measurable gain. The enum
    rule's 9.6 s was the checker's lazy work, which the next type-aware rule now pays (the caveat in § ESLint).
  - An earlier pair at load 60–130 (675 s → 205 s Svelte) swung with the load and isn't comparable.
- **Bug, catalog edits selected nothing** (`catalogReaderTargets`,
  `scripts/check/checks/desktop-svelte-tests-related.go`).
  - An edit to `intl/messages/en/common.json`: **0 → 46 spec files** (every `src/lib/intl/` test plus the parity,
    catalog-tooling, and settings-search tests that read catalogs), counted through Vitest's `related` selection.
  - Found on the way: `vite-locale-catalogs.ts` and `i18n-catalog-lib.ts` are `vitest.config.ts` dependencies, so
    passing either to `vitest related` selects the whole suite. The lane passes the reading tests instead.
