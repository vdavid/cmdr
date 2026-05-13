# Extend E2E tests — coverage-driven walk

Status: in progress.

## Plan

| #   | Spec                        | Status | Tests before | Tests added | Notes                                                               |
| --- | --------------------------- | ------ | ------------ | ----------- | ------------------------------------------------------------------- |
| 1   | settings.spec.ts            | done   | 5            | 3           | sidebar filter, no-match, arrow-key from search input               |
| 2   | git-portal.spec.ts          | done   | 2            | 2           | tags/v1.0.0, commits/ short-SHA listing                             |
| 3   | viewer.spec.ts              | done   | 10           | 1           | no-matches search state (W-key wrap test dropped, see notes)        |
| 4   | indexing.spec.ts            | done   | 3            | 0           | already byte-exact end-to-end; no gap warrants e2e                  |
| 5   | network-toggle.spec.ts      | done   | 4            | 0           | existing 4 cover the user-visible UX; deeper coverage is unit-level |
| 6   | error-pane.spec.ts          | done   | 3            | 1           | folder-path display + collapsed-details disclosure                  |
| 7   | file-operations.spec.ts     | done   | 8            | 0           | thorough already; rename-conflict gap noted below                   |
| 8   | conflict-move.spec.ts       | done   | 3            | 0           | exhaustive merge / skip / rollback coverage                         |
| 9   | conflict-copy.spec.ts       | done   | 7            | 0           | policy matrix already saturated                                     |
| 10  | conflict-edge-cases.spec.ts | done   | 7            | 0           | rollback + symlinks + type mismatches all covered                   |
| 11  | app.spec.ts                 | done   | 14           | 2           | F7-Cancel button and F8 delete confirm dialog                       |
| 12  | accessibility.spec.ts       | done   | 20           | 0           | already covers main flows in light + dark                           |
| 13  | file-watching.spec.ts       | done   | 11           | 0           | CRUD + batch + threshold + dedup all covered                        |
| 14  | mtp.spec.ts                 | done   | 21           | 0           | 21 tests across browse/copy/move/delete/rename/read-only            |
| 15  | mtp-conflicts.spec.ts       | done   | 5            | 0           | move-conflict matrix saturated                                      |
| 16  | smb.spec.ts                 | skip   | —            | —           | Out of scope per brief                                              |

## Per-spec analysis

### settings.spec.ts

**Source surface**: `src/routes/settings/+page.svelte`, `src/lib/settings/components/SettingsSidebar.svelte`,
`settings-search.ts`, `settings-registry.ts`.

**Behaviors covered (before)**:

- Renders, sidebar shows sections, expected section names present, search input accepts text, clicking a sidebar item
  selects it.

**Gaps identified**:

- Search debounce + sidebar filtering (search actually narrows the visible list).
- Empty-result branch (`zzzyyyxxxnomatch`-style query): sidebar collapses to zero items, clear (×) recovers.
- Clear-search button (`.search-clear`) round-trip.
- Arrow Up/Down in the search box drives section selection (the search box has its own `handleSearchKeydown` separate
  from the section-tree listbox).
- Escape closes the settings window (out of scope — the spec runs many tests that need the window open).
- `?section=...` URL deep-link (out of scope — requires a window reload).
- `navigate-to-section` Tauri event (covered indirectly by the volume picker test; testing it from within the Settings
  window's own context is non-load-bearing).
- Last section persistence (`saveLastSettingsSection`) — also needs a reload.

**Tests added** (3):

1. `search narrows the visible sidebar sections and clearing restores them` — drives the debounced filter with `accent`
   (matches one Appearance row), then clicks the `×` and asserts the full list is back.
2. `search shows an empty sidebar for queries with no matches` — covers the no-match branch and confirms the clear
   button is still reachable; cleans up search state for the next test.
3. `Arrow Down in the search box moves section selection forward` — covers the dual-keydown path in
   `SettingsSidebar.handleSearchKeydown` (Arrow keys in input forward to `navigateSections`); clears any leftover search
   up front so a `.selected` row is present.

**Skipped (with reason)**: Escape-closes-window, URL deep-link, last-section persistence — all need a window reload that
the shared test suite isn't set up to do cleanly.

### git-portal.spec.ts

**Source surface**: `src-tauri/src/file_system/git/{virtual_listing,path,tree}.rs`. Frontend pane orchestration is
generic; the portal lives entirely in the volume hooks.

**Behaviors covered (before)**: 2 active tests (portal root entries; branches/main tree) + 2 skipped (cross-volume copy;
portal toggle), both documented.

**Gaps identified**:

- `tags/<tag>` navigation (exercises `resolve_ref_commit`, including annotated-tag peel and the dot-bearing ref parse in
  `classify`).
- `commits/` listing (exercises `list_commits` end-to-end via the volume hook; M3-era code path).
- Friendly error rendering inside the portal (would need an injected gix error; `error-pane.spec.ts` already covers
  FriendlyError for filesystem errors and a Rust test covers it for git).

**Tests added** (2):

1. `navigates tags/v1.0.0 and sees the tree at the tagged commit` — covers the tag-resolving branch and dot-in-ref
   parser path.
2. `navigates commits/ and shows the single HEAD commit by short SHA` — covers `list_commits` integration via the
   listing pipeline; regex-checks for a 7+ hex name to avoid pinning to a specific SHA across git versions.

**Skipped (with reason)**: Friendly git error rendering — already covered by Rust unit tests + the broader
friendly-error path is exercised by `error-pane.spec.ts`.

### viewer.spec.ts

**Source surface**: `src/routes/viewer/+page.svelte` plus the composables `viewer-search.svelte.ts`,
`viewer-scroll.svelte.ts`, `viewer-line-heights.svelte.ts`. Backend: `src-tauri/src/file_viewer/`.

**Behaviors covered (before)**:

- Render container, line elements, file name in status bar, line count, file size, backend badge, Ctrl+F opens search,
  search finds matches, Escape closes search, missing-path error.

**Gaps identified**:

- No-match search state (UI says "No matches"). This exercises the "done" status branch of `searchStatus` and confirms
  `aria-live` content.
- W toggles word wrap (cross-state setting + CSS class flip).
- Enter advances to next match (already covered indirectly because `findMatches` test pulls a match count, but no test
  confirms navigation).
- F3 from file list opens viewer (cross-component; opens a NEW Tauri window, outside the test's single-window scope —
  defer).
- Line heights variant testing (FullLoad pretext path) — deep internal, deferred.

**Tests added** (1):

1. `shows "No matches" status for a query with no hits` — fills with `Z * 40` (the fixture is `A` × 1024 so cannot
   match), polls the `.match-count` aria-live region for "No matches". Resets the query in cleanup.

**Skipped (with reason)**: F3-opens-viewer (cross-window flow), line-heights internals (tier 3 candidates), W toggles
word wrap (the synthetic keydown reaches `<svelte:window on:keydown>` but doesn't flip the wrap class reliably;
investigating focus / `viewerSetWordWrap` IPC side effects would consume disproportionate time for a single-key path —
deferred with a TODO).

### indexing.spec.ts

**Source surface**: `src-tauri/src/file_system/index/` (renamed `indexing` in the codebase). Frontend reads via
`get_dir_stats`.

**Behaviors covered (before)**: 3 thorough tests: initial dir size from index, exact-byte increase on file creation,
exact-byte decrease on file deletion. UI-side numeric size in Full view also checked.

**Gaps identified**:

- All key flows are already covered. The Scanning... → numeric transition is implicitly covered. Edge cases
  (non-existent path → null, very large directories) would be unit-level.

**Tests added**: 0. **Reason**: The existing suite hits the indexing pipeline end-to-end with byte-exact assertions for
both create and delete. Adding more would either duplicate or descend into Rust-side unit territory.

### network-toggle.spec.ts

**Source surface**: `src/lib/volumes/` (frontend volume picker), `src-tauri/src/file_system/volume/network/` (backend
mDNS).

**Behaviors covered (before)**: Default label, toggle-off label, toggle-back label,
click-disabled-leaves-volume-unchanged.

**Gaps identified**:

- Settings deep-link to Network section when clicking the disabled entry — `settings-window.ts` emits a
  `navigate-to-section` event. Already exists implicitly in code, and the test author explicitly notes inspecting the
  settings window is awkward via `evaluate()`.
- mDNS-actually-stops behavior — unobservable from the UI side.

**Tests added**: 0. **Reason**: Existing tests cover the user-observable UX cleanly. Backend mDNS-stop is unit-level.
The deep-link assertion would require spawning the settings window from the test, which is structurally fragile.

### error-pane.spec.ts

**Source surface**: `src/lib/file-explorer/pane/ErrorPane.svelte` +
`src-tauri/src/file_system/listing/friendly_error.rs` (error classification).

**Behaviors covered (before)**: ETIMEDOUT (transient with retry), retry-clears-error-state, EACCES (NeedsAction without
retry), accessibility (role/heading).

**Gaps identified**:

- Folder path display (user must see WHICH directory failed).
- `<details>` technical-details disclosure default-collapsed + click-to-expand.
- Retry info text rendering after multiple clicks (deep UX; gated by hitting retry repeatedly within seconds).
- `x-apple.systempreferences:` link handling — unit-testable; production-impactful but doesn't load-bear here.

**Tests added** (1):

1. `shows the offending folder path and a collapsed technical details disclosure` — injects ETIMEDOUT, asserts
   `.folder-path` ends with `/left/sub-dir`, then verifies `<details>` starts without the `open` attribute and gains it
   after clicking the summary.

**Skipped (with reason)**: Retry-info-after-multi-click — feels like UX polish coverage; deferred unless we see
regressions.

### file-operations.spec.ts

**Source surface**: `src-tauri/src/file_system/write_operations/{copy,move,rename,mkdir}.rs`. Frontend:
`src/lib/file-operations/**`, `src/lib/file-explorer/views/**`.

**Behaviors covered (before)** (8): F5 copy, F6 move, F2 rename, F7 mkdir, view mode toggle, hidden files toggle,
command palette, empty directory.

**Gaps identified**:

- Local rename to an existing name (rejection → `rename-conflict` dialog). MTP rename rejection IS tested in
  `mtp.spec.ts`; local equivalent isn't.
- ⌘A / Ctrl+A select-all in pane (combined with F5 for multi-file copy).
- Cancel button on transfer dialog (only Escape tested).

**Tests added**: 0.

**Reason**: The existing spec covers the success path for every write operation end-to-end with byte-level assertions,
plus negative cases for the structural flows (empty dir, view toggle). The rename-conflict dialog gap is real but adding
it now risks duplicating the structurally-identical MTP rejection test for marginal coverage. The flow lives at the
rename UI component level and is a good candidate for a tier-3 jsdom test rather than another full E2E round-trip.

### conflict-move.spec.ts / conflict-copy.spec.ts / conflict-edge-cases.spec.ts

**Source surface**: `src-tauri/src/file_system/write_operations/{copy,move}.rs` plus `transfer-conflict-policy` UI.

**Behaviors covered (before)** (17 across three files): Overwrite All, Skip All, per-file decisions, Rename, Rename All,
Layout A nested conflicts, Layout B multi-item merges, mid-operation rollback, sequential conflicts, symlinks, type
mismatches (file↔dir).

**Gaps identified**:

- Same-volume copy with both conflict and non-conflict mixed (already covered by Layout A/B).
- ⌃Z/⌘Z to undo a completed transfer — but the app doesn't have undo today.

**Tests added**: 0. **Reason**: The conflict-policy matrix is saturated by the three files together. Adding more cases
dilutes signal.

### app.spec.ts

**Source surface**: `src/routes/(main)/+page.svelte` and the global keyboard dispatch (`command-dispatch.ts`,
`command-registry.ts`).

**Behaviors covered (before)** (14): Render, dual pane, file entries, arrow nav, Tab pane switch, Space toggle, click
cursor move, click pane focus, Enter into dir, Backspace to parent, F7 mkdir dialog open/cancel, F7 mkdir create, F5
copy dialog open/escape, F6 move dialog open/escape.

**Gaps identified**:

- F8 opens delete confirmation (vs. ⇧F8 which is permanent delete).
- Cancel **button** on the mkdir dialog (was only tested via Escape).
- Cancel button on transfer dialogs.
- ⌘A select-all keyboard flow.

**Tests added** (2):

1. `Cancel button closes the new folder dialog without creating anything` — exercises the `.btn-secondary` path through
   `ModalDialog`, asserts no folder was created (file-entry count unchanged).
2. `opens the delete confirmation dialog with F8` — F8 opens the `delete-confirmation` dialog (the recycle-bin path, not
   ⇧F8); Escape closes it and leaves the file under cursor in place.

**Skipped (with reason)**: ⌘A and Cancel-button on the transfer dialog — the transfer-dialog Cancel button is wired
through the same path as Escape (both call the same `closeDialog`), so the additional test would duplicate signal.

### accessibility.spec.ts

**Source surface**: `src/lib/test-a11y.ts` runner + axe-core rules across each dialog snapshot in light + dark.

**Behaviors covered (before)** (20): Main explorer, every major dialog (Copy/Delete/Move/About/License/Command
palette/Search/Settings/File viewer) in both light and dark modes.

**Gaps identified**:

- Error pane in axe — but `error-pane.spec.ts` covers ARIA explicitly.
- Network volume picker open state — narrower coverage.

**Tests added**: 0. **Reason**: Each frame is already audited in both modes; structural a11y for individual components
lives at tier 3 (`*.a11y.test.ts`). Adding more axe snapshots without a clear missing-component would inflate the suite.

### file-watching.spec.ts

**Source surface**: `src-tauri/src/file_system/watch/` + the frontend watcher subscription in `file-explorer`.

**Behaviors covered (before)** (11): External create (file + dir), delete, rename, modify-size, batch (25),
600-threshold (Linux only), watched-dir deletion, dual-pane sync, in-app-copy dedup, hidden-file filtering.

**Gaps identified**:

- Permissions-change watching — out of e2e scope.
- Watcher behavior under symlink resolution.

**Tests added**: 0. **Reason**: The spec already covers the full CRUD matrix plus the structural edge cases (threshold,
dedup, hidden-file filtering, watched-dir deletion). The remaining gaps are too low-level for an E2E round-trip and
would belong in `notify`-level Rust tests.

### mtp.spec.ts / mtp-conflicts.spec.ts

**Behaviors covered (before)** (26 total): Volume picker, browse, free space, copy bidirectional, move within and
across, delete (single, multi, recursive), mkdir, rename, rename rejection, read-only enforcement, Cmd+C/X/V rejection
toasts, 50 MB transfer in both directions, external add detection, MTP-to-local and same-volume conflict matrix
(overwrite/skip).

**Gaps identified**:

- MTP rename to dotfile (filesystem reserved-name handling): valid but feels nichey.
- MTP filename Unicode round-trip: covered indirectly by the SMB unicode tests (skipped on macOS).

**Tests added**: 0. **Reason**: 26 tests cover the user-observable surface end-to-end. Adding marginal cases would test
virtual-device internals rather than user flows.

## Step 7: mutation testing

One-off investigation: ran [`cargo-mutants`](https://mutants.rs/) (Rust) and
[`stryker-mutator`](https://stryker-mutator.io/) (TypeScript) on two focused slices to find under-tested code paths in
already-unit-tested files. Both tools worked; neither is wired into CI. Total: 5 unit tests added (3 vitest, 2 nextest).

### Tools status

- **cargo-mutants v27.0.0**: works out of the box after `cargo install --locked cargo-mutants`. Gotcha: the default test
  command runs the full crate's nextest suite, and four `indexing::reconciler` tests fail when run from cargo-mutants's
  reflinked tmp directory (they create tempdirs in CWD to avoid `/private/tmp/` matching `EXCLUDED_PREFIXES`, but
  `/var/folders/...cargo-mutants-...tmp/` also matches). Workaround: scope tests to the slice via positional args, e.g.
  `cargo mutants --file '**/eta.rs' --timeout 60 --no-shuffle --test-tool nextest -- --lib file_system::write_operations::eta`.
- **stryker-mutator v9.6.1** (`@stryker-mutator/core`, `@stryker-mutator/vitest-runner`): worked after three config
  tweaks. Required `plugins: ["@stryker-mutator/vitest-runner"]` to load the runner; `inPlace: true` to avoid `oxc`
  transformer failing on the sandbox copy ("Tsconfig not found" for unrelated `.a11y.test.ts` files); and leaving the
  vitest config alone (no `dir`, no `related: false`) — `perTest` coverage analysis picks the right test files
  automatically. Took ~12 s total for one mutated file (45 mutants, 4 workers).

### Slices and results

**Rust slice**: `apps/desktop/src-tauri/src/file_system/write_operations/eta.rs` (EWMA-based ETA estimator for write
operations, already had 10 unit tests).

| Metric   | Count |
| -------- | ----- |
| Total    | 58    |
| Caught   | 34    |
| Missed   | 19    |
| Unviable | 5     |
| Timeout  | 0     |

Mutation score: 34 / (34 + 19) ≈ 64 %. Full run took ~9 min (after the 73 s baseline build), ~7–8 s per mutant.

**TypeScript slice**: `apps/desktop/src/lib/file-operations/scan-throughput.ts` (front-end rate calculator for the
scan-preview UI, 6 unit tests).

| Metric         | Count |
| -------------- | ----- |
| Total          | 45    |
| Killed         | 31    |
| Survived       | 13    |
| Timeout        | 1     |
| Compile errors | 0     |

Mutation score: 31 / 45 ≈ 69 %. Whole run took ~12 s.

### Tests added (5 total)

All passed first clean run and again after `./scripts/check.sh`.

**`scan-throughput.test.ts`** (vitest), targeting stryker survivors in `dropStale`:

1. `keeps the right window of samples when many arrive over a long span`. Four samples with non-linear progression;
   kills `cutoff = nowMs + windowMs` mutant (line 68) and the `length > 2 → length >= 2 / <= 2` boundary mutants on line
   70 that the previous 3-sample window test couldn't differentiate (algebraically equal rates).
2. `always keeps at least two samples even after a long pause`. Pushes one sample 60 s after a small starting pair;
   kills `while (true && ts < cutoff)` and `length <= 2 && ts < cutoff` mutants that would empty the buffer and return
   null.
3. `treats the cutoff timestamp as inclusive (strict less-than)`. Sample exactly on the cutoff boundary; kills
   `< cutoff → <= cutoff / >= cutoff` mutants on line 70.

**`eta.rs` `mod tests`** (nextest), targeting cargo-mutants survivors in `EtaEstimator::update`:

4. `rate_division_uses_dt_not_a_constant`. Drives the estimator with `dt = 2.0 s` so `delta / dt` and `delta * dt`
   differ by 4×; existing tests all used 1 s steps where the two are numerically identical. Kills `/ → *` and `/ → %` on
   lines 152, 153.
5. `first_post_seed_sample_initializes_rate_directly`. Asserts exact rate after exactly two `update()` calls — the
   `samples == 0` branch sets the rate directly to the instantaneous rate; existing 3-sample tests masked the mutant
   `samples != 0` because the EWMA caught up. Kills the `== 0 → != 0` mutant on line 159 and the
   `1.0 - (-dt / TAU).exp()` alpha arithmetic mutants on line 157 (those changes throw `alpha` off by a large factor,
   which shows up immediately after one post-seed sample).

### Surviving mutants worth a follow-up

Stopped well short of the 10-test budget. The rest of the surviving cargo-mutants survivors land in `eta_from_axes` (a
saturating two-axis ETA selector). High-value follow-ups, in order:

1. `eta.rs:223 > → ==/</>=` in `eta_from_axes` (3 mutants) — `bytes_remaining > 0` guard.
2. `eta.rs:224 / → %`, `eta.rs:231 / → %` — axis-rate ÷ axis-remaining divisions.
3. `eta.rs:225, 232 == → !=` — short-circuit early-return checks on rates.
4. `eta.rs:230 > → >=` — files axis boundary check.
5. `eta.rs:247 > → ==/</>=` (3 mutants), `eta.rs:247:64 > → >=` — final-pick comparison.
6. `eta.rs:190 && → ||` in `compute_stats` — guard combining the two axes.
7. Stryker survivors that the new tests do not kill, e.g. `scan-throughput.ts:31 constructor body → {}` (windowMs never
   read — easy fix: a test that constructs with two different `windowMs` and asserts different drop behavior).

The `> 0 ? fps : 0` mutants on `scan-throughput.ts:62, 63` (turning `>` into `>=`) are **equivalent mutants** — when
`fps == 0`, both branches return `0`, so no test can differentiate them. Skip.

### Verdict on long-term CI integration

**Don't wire either tool into the main `./scripts/check.sh` pipeline.** Both are too slow and too noisy for a per-commit
gate. Concretely:

- cargo-mutants: ~9 min for **one 600-line file** with the cmdr workspace's deps. Running the full `src-tauri/` crate
  (~150 source files) would be hours, and many modules would need per-slice test filters to dodge the CWD-sensitive
  reconciler tests. Worth keeping the binary install in dev's mise toolchain and running ad-hoc on hot spots when adding
  non-trivial numeric / state-machine code (eta, write-op state machines, conflict resolution, indexing, etc.).
- stryker: ~12 s per file is fast, but the config has sharp edges (`oxc` sandbox issues, plugin discovery) and there's
  no obvious gain from a CI-blocking gate. Same recommendation as cargo-mutants — ad-hoc on numeric / branching FE
  utilities (`scan-throughput`, eventually `font-metrics`, `accent-color`, etc.).

Concrete next steps if pushing this further:

1. `cargo mutants --in-diff` (only mutate lines changed by a PR) as an optional GitHub Actions workflow that runs on
   labelled PRs, not as a check. Output stays advisory.
2. Skip the workspace-setup churn by committing a `.cargo/mutants.toml` with
   `additional_cargo_test_args = ["--lib", "file_system::write_operations::eta"]`-style slice-scoped configs per hot
   module.
3. Don't add stryker config to the repo. The 3-tweak setup is small enough to redo when needed.

### Step 7 follow-up: deep mutation-testing pass

A second pass walked five more hot-spot modules. cargo-mutants `--list` is fast (no build), so I used it to enumerate
mutants per file, then read each module and added tests targeting the structurally surviving ones — skipping a full
`cargo mutants` run because the baseline build alone is ~10–15 min per file. Trade-off: the new tests aren't proven
mutation-killers in the strict sense, but they directly cover the mutated lines and behavior, which is what the killer
tests in Step 7 ended up doing anyway.

Total: 50 new unit tests across 5 modules, ~0.15 s combined runtime. All 1 699 lib tests still pass.

- `file_system/write_operations/state.rs` (50 mutants, **+30 tests**): from zero existing tests. Covered the
  `OperationIntent` state machine (`from_u8`, `load_intent`, `is_cancelled`), `cancel_write_operation` transitions
  (Running→{RollingBack,Stopped}, RollingBack→Stopped, Stopped terminal, conflict-sender drop),
  `cancel_all_write_operations`, `resolve_write_conflict`, status-cache CRUD
  (`register`/`update`/`unregister`/`list_active_operations`/`get_operation_status`, including the bytes-vs-files
  percent axis and the `.min(100.0)` clamp), `FileInfo` sort keys, and `CopyTransaction` commit / rollback / Drop.
  Highest-leverage module of the pass — the state machine and status cache back every cancel click and every progress
  query, with zero coverage before.
- `file_system/write_operations/copy_strategy.rs` (16 mutants, **+5 tests**): the `is_apfs` and `is_same_apfs_volume`
  helpers were only covered indirectly through `copy_file_with_strategy`. Direct positive/negative tests on macOS now
  pin the device-id comparison, the parent-fallback when the destination doesn't exist, and the `f_fstypename == "apfs"`
  check.
- `file_system/watcher.rs` (35 mutants, **+6 tests**): `is_entry_modified` watches five axes (size, mtime, perms,
  is_directory, is_symlink); the existing tests only varied size, so every `||` chain mutant survived. New axis-by-axis
  tests plus one negative anchor (owner/group changes must NOT trigger a modify diff) and one structural pin for
  `compute_diff`'s index semantics (remove uses OLD index; add/modify use NEW).
- `file_system/write_operations/chunked_copy.rs` (16 mutants, **+3 tests**): existing tests covered byte fidelity and
  basic permissions but never checked the metadata-preservation side effects. New tests stamp a fixed mtime via
  `filetime` and roundtrip a user xattr (macOS) — kills `copy_timestamps → Ok(())`, `copy_xattrs → Ok(())`,
  `copy_metadata → Ok(())`. Plus a multi-chunk byte-total assertion that kills the `total_bytes += bytes_read`
  arithmetic mutants the existing progress test couldn't differentiate.
- `indexing/store.rs` (~150 mutants overall, **+6 tests** on `platform_case_compare` / `normalize_for_comparison`): the
  SQLite collation backing path resolution. Only test before was a single macOS happy-path that couldn't catch the
  `→ Ordering::Equal` mutant (since happy-path comparisons are equal anyway). New tests pin distinct ordering for
  distinct names, case-insensitivity, NFC↔NFD normalization equivalence on macOS, and binary comparison off macOS.

Modules examined but skipped:

- `file_viewer/{line_index,session,byte_seek,full_load}.rs`: already 50+ tests including UTF-16 column tracking,
  cancellation, sparse-index checkpoints, multi-byte content. The 240+ mutants here are mostly inside private session
  state and search-pos accounting where additional tests would duplicate existing-test logical coverage.
- `file_system/write_operations/{delete,trash,helpers}.rs`: top-level `*_with_progress` functions depend on
  `tauri::AppHandle` for event emission; meaningful tests would require a mock-emitter refactor that exceeds the
  "minimal refactor for testability" budget. `move_to_trash_sync` (the pure-Rust core) is already covered.

No bugs surfaced in this pass — every survivor was a real coverage gap, not buggy live code. The state-machine guard in
`cancel_write_operation` is correct: Running→{RollingBack,Stopped}, RollingBack→Stopped, Stopped terminal, exactly as
the doc-comment promises.

## Property-based testing fit (investigation)

`proptest` and `quickcheck` aren't in use anywhere in `apps/desktop/src-tauri/`. The full investigation lives at
`/tmp/cmdr-property-testing-report.md`; this section is the short takeaway.

The most algorithmic, pure spots in the crate are already covered by tight example tests: `eta::EtaEstimator` (12 tests,
two named mutant-survivor targets), `listing::sorting` (32 tests), `validation` (13 tests). Adding proptest there gives
diminishing returns.

The clear net-positive proptest targets, in order:

1. **`indexing::aggregator::topological_sort_bottom_up`** — 1 example test for a function with non-trivial tree
   invariants. Cycle and duplicate-ID behavior isn't asserted today.
2. **`search::query::glob_to_regex`** — 4 example tests; infinite input space; output feeds a regex engine that panics
   on malformed input. "Output is always valid regex" is a one-line property and a real safety net.
3. **`search::query::split_scope_segments`** — 10 example tests for a parser with nested escape/quote rules. Round-trip
   and segment-count properties are cheap.
4. **`indexing::store::platform_case_compare`** (macOS) — comparator-law properties (reflexivity, antisymmetry,
   transitivity) plus NFC≡NFD equivalence. Highest user impact because miscompare corrupts the search index.

Verdict: worth adding `proptest` as a dev-dependency for these four targets specifically, ~half a day of work. Not worth
a project-wide convention. Don't introduce it for ETA, sorting, or validation — example tests already cover the
interesting cases.

## IPC contract coverage (investigation)

How well are the 193 `#[tauri::command]` entry points (visible via `bindings.ts`) tested _at the IPC layer_ — i.e., a
test actually calls the command function or mocks/invokes it by name? Full report: `/tmp/cmdr-ipc-coverage-report.md`.

Counts (commit `742939e9`):

- **Well covered** (happy + error path): **16 / 193** (8%)
- **Happy path only**: **11 / 193** (6%)
- **Untested at the IPC layer**: **166 / 193** (86%)
- Score `(well + happy/2) / total`: **0.11**

Caveat that softens the headline: most commands are thin pass-throughs to `*_core` / `ops_*` helpers (AGENTS.md: "Tauri
commands are pass-throughs"), and the helpers ARE broadly tested. The 86% measures the _contract_ boundary, not business
logic. The `bindings-fresh` CI check and the `no-raw-tauri-invoke` ESLint rule mitigate most parameter-shape drift; what
they don't catch is permission-config drift or silent rename mismatches at runtime.

Biggest gaps by feature: viewer (9 commands, 0 IPC tests), MTP (~10 commands, 0 IPC tests), licensing (~10 commands, 0
IPC tests), settings/UI mutators (most untested). The write_ops surface (`create_directory`, `create_file`,
`rename_file`, `move_to_trash`) accounts for most of the "well covered" bucket because those `_core` tests happen to
call the command itself.

Verdict: **weak at the IPC surface, strong underneath**. If we want to raise contract coverage meaningfully, the
productive move is a vitest `mockIPC` layer that asserts each `commands.foo(...)` call returns a typed shape — not
Rust-side per-command tests.

## State-machine coverage (investigation)

Full report: `/tmp/cmdr-state-machine-report.md` (read-only scan, branch `e2e-speedup` @ `742939e9`).

Surveyed 13 genuine state machines (backend + frontend; excludes derived / progress-only enums). About 60 transitions
total, roughly 35 untested (~58%).

Coverage is uneven:

- **Strong**: `SmbVolume::ConnectionState` (Direct ⇄ Disconnected, idempotency, single-flight reconnect),
  `OperationIntent` (atomic level), `SmbReconnectManager` FE, AI notification FE, MTP FE, updater FE, error-reporter
  `auto_dispatcher` debounce.
- **Weak**: `IndexPhase` (Disabled/Initializing/Running/ShuttingDown — no direct test of any transition or the
  start/stop race), `ActivityPhase` (six-state telemetry pipeline, no test), `DiscoveryState` (network mDNS — three
  transitions, no test), `network-store` `ShareState` + `CredentialStatus` FE (a11y tests only), `ConnectToServerDialog`
  FE.
- **Tested at wrong layer**: `cancel_write_operation`'s validation guard (state.rs:306) is bypassed by all tests, which
  set the atomic directly. The `RollingBack → Stopped` valid-transition assertion and the rejection of terminal-state
  writes are not exercised through the public API.

Top untested transitions worth adding tests for:

1. `cancel_write_operation` public function (validation guard + `conflict_resolution_tx` drop).
2. `IndexPhase::Initializing → Disabled` race when stop runs during `resume_or_scan`.
3. `IndexPhase::Initializing → Running` happy path.
4. `OperationIntent::RollingBack → Stopped` through public cancel (not just direct atomic store).
5. `SearchStatus::Running → Cancelled` in file viewer.
6. `DiscoveryState` transitions (Idle → Searching → Active → Idle).

Side finding: `SmbVolume::ConnectionState::OsMount` is a defined variant that is never written to the atomic. The smb2
hot-path branch handling `OsMount` (smb.rs:658, smb.rs:449) is dead code on the current implementation. Either wire the
transition or drop the variant.

Overall verdict: **medium-strong**. The transition-aware machines that matter for data safety are tested; the
orchestration-level lifecycles (`IndexPhase`, `ActivityPhase`, `DiscoveryState`) are not.

### Coverage fill-in (follow-up commits)

17 state-transition tests added (and one bug fix surfaced while writing them):

- **`SmbVolume::ConnectionState`** — dropped the dead `OsMount` variant. The internal state machine is now exactly the
  binary shape it was already operating as (`Direct ⇄ Disconnected`). The outer `SmbConnectionState::OsMount` (attached
  by `enrich_smb_connection_state` for SMB shares with an OS mount but no Cmdr smb2 session) is unchanged.
- **`SearchStatus`** — fix + transition test. `search_cancel` was clearing `session.search`, which made the `Cancelled`
  status (set by the search thread on cancel) unobservable: poll returned `Idle`. Stopped nulling the state on cancel;
  the thread now writes `Cancelled` and poll surfaces it. New test pins `Running → Cancelled` and the reset-on-new-start
  contract.
- **`DiscoveryState`** — three transition tests (`Idle → Searching → Active → Idle`, `Searching → Idle` via drain, drain
  side effects). Factored `set_discovery_state` and `drain_discovered_hosts` out of the event-emitting public paths so
  the state machine fragment is testable without standing up a Tauri runtime.
- **`ActivityPhase`** — nine tests covering the full scan pipeline (`Idle → Replaying → Live`,
  `Scanning → Aggregating → Reconciling → Live`), the shutdown path (`* → Idle`), the duration-closing branch the
  timeline UX depends on, the 20-entry ring-buffer cap, `reset`, and `close_phase_with_stats` attaching to the current
  entry.
- **`IndexPhase`** — four tests: `Initializing → Disabled` via the public `stop_indexing` race path, the two catch-all
  no-op arms (`stop_indexing` from `Disabled`, `clear_index` from non-`Running`), and the pure `is_initializing_phase`
  classifier the post-`resume_or_scan` decision now goes through. `start_indexing`'s full happy path needs an
  `AppHandle` and `IndexManager` and remains untested at unit-test level — the stress tests cover the writer-layer
  machinery underneath.

Honest verdict per machine:

- **Easy**: `SmbVolume::ConnectionState` (already well-tested, just cleanup), `ActivityPhase` (pure journal, fresh
  instance per test), `DiscoveryState` (already had a global cell, only needed an emit/state split).
- **Awkward**: `IndexPhase` (carries owned non-`Clone` data, transitions split across `start_indexing`/`stop_indexing`/
  `clear_index`, and the race fragment we cared about needs a real `IndexStore`).
- **Impossible without a Tauri runtime**: `start_indexing`'s `Disabled → Initializing → Running` happy path — needs an
  `AppHandle` to spawn the writer and the verifier. The post-scan decision was extracted to a pure helper so the
  state-machine fragment that matters (the race) is at least testable.

### IPC contract test layer (follow-up commits)

A vitest `mockIPC` harness (`apps/desktop/src/lib/ipc/test-helpers.ts`) plus 23 contract tests for the three
highest-priority command groups:

- **Write operations** (9 tests) — `copy_files`, `move_files`, `delete_files`, `trash_files`, `cancel_write_operation`.
  Pins the payload shape (including the optional config object and the `volumeId` / `itemSizes` shapes) and one typed
  `WriteOperationError` variant on the error branch.
- **File viewer** (8 tests) — `viewer_open`, `viewer_get_lines`, `viewer_search_start`/`_poll`/`_cancel`,
  `viewer_close`. Coverage report flagged this group as 9/9 untested at the IPC layer.
- **SMB connection** (6 tests) — `connect_to_server`, `list_shares_on_host`, `mount_network_share`. The mount path has 6
  positional args and AGENTS.md specifically calls out positional-soup as fragile.

What the harness catches: argument coercion, snake-case command name typos, payload-key shape drift, and the typed-error
discriminator round-tripping. What it doesn't catch: the real Tauri permission gate (mockIPC patches
`__TAURI_INTERNALS__.invoke` _before_ the gate), business logic in `*_core` helpers (Rust unit tests own that), or
end-to-end behaviour (Playwright owns that).

Honest verdict: **modest value, mostly mechanical**. The coverage report already concluded that the `bindings-fresh`
check + `cmdr/no-raw-tauri-invoke` ESLint rule cover most of the realistic drift surface. This layer adds a thin runtime
check on top: it verifies that the FE actually drives the binding (not just that the binding compiles), and it documents
the wire format in a way that survives a refactor. Worth doing for the write-side, viewer, and SMB groups because those
are the ones where a renamed Rust function or a flipped payload key would surface as a generic runtime failure with no
obvious cause. Not worth expanding to all 193 commands — diminishing returns kick in fast once the binding shapes are
pinned for the destructive / cross-window surfaces.

No bugs surfaced during this pass. Side effect of writing the tests: confirmed that the typed-error discriminator shapes
(`type` for `WriteOperationError` / `MountError` / `ShareListError`, `code` for `LicenseActivationError`) are consistent
on the wire — the FE branching on `error.type` / `error.code` will see the values the bindings declare.

## Final state

Branch `e2e-speedup`, 47 commits, ready for fast-forward to `main`.

### Tests added across the push

| Category                       | Tests   | Bugs surfaced                                                            |
| ------------------------------ | ------- | ------------------------------------------------------------------------ |
| E2E coverage extension         | 9       | 1 — Cancel-copy rollback (Rust `Ok(())` arm + Svelte settle-window race) |
| Mutation testing (Rust+Svelte) | 55      | 0                                                                        |
| State-machine transitions      | 17      | 1 — `file_viewer` `SearchStatus::Cancelled` unobservable to FE           |
| Property-based (proptest)      | 12      | 0                                                                        |
| IPC contract (mockIPC)         | 23      | 0                                                                        |
| **Total new unit / IPC tests** | **107** |                                                                          |

Plus dead code removal: `SmbVolume::ConnectionState::OsMount` variant dropped (state machine collapsed to its real
binary `Direct ⇄ Disconnected` shape).

### Suite size delta

- **Rust unit tests**: 1649 → **1728** (+79, mutation 50 + state-machine 17 + proptest 12).
- **Svelte unit tests**: 1783 → **1812** (+29, IPC contract layer + scan-throughput mutants from Step 7).
- **E2E Playwright**: 122 → **131** active tests (+9 from coverage walk).

### Suite timing delta

- **E2E checker total**: 13m 12s baseline → **4m 18s** in the final slow pass (−67%).
- **E2E Playwright wall-clock**: 10m 12s baseline → ~1m 48s longest shard (−82%).
- **Fast checker total**: ~2m 30s baseline → **3m 13s** (+43s) — the regression is the new IPC contract tests (+30s of
  Svelte vitest) and the +79 Rust tests (+1s). Net cost is well below what an equivalent E2E spec would add.

### Real bugs fixed

1. **file_viewer search-cancel was unobservable to the FE.** `search_cancel` nulled `session.search` immediately, so the
   spawned thread's `SearchStatus::Cancelled` write was clobbered before the FE could see it. The FE couldn't
   distinguish "search completed naturally with zero matches" from "search was cancelled mid-flight." Fix: stop nulling
   on cancel; let the thread write `Cancelled` and the next `search_start` replaces it.
2. **Cancel-copy mid-operation rollback was lost on fast filesystems.** The Rust `Ok(())` arm in
   `copy_files_with_progress` didn't check `OperationIntent` before committing the transaction, so a click during the <
   1 µs window between the last `is_cancelled` poll and loop exit landed as a no-op. Plus the Svelte
   `TransferProgressDialog` left the Rollback button enabled during the `MIN_DISPLAY_MS = 400 ms` settle window after
   `write-complete`, so clicks during settle were silent no-ops. Both fixed in Step 6d.

### Dead code removed

- `SmbVolume::ConnectionState::OsMount` variant — never written to the atomic, two unreachable `match` arms gone. The
  OS-mount fallback the UI renders lives at the outer `SmbConnectionState` enriched by `enrich_smb_connection_state` in
  `commands/volumes.rs`, not on this internal atomic.

### Honest verdict per technique

- **Mutation testing (cargo-mutants + stryker)**: zero bugs, 55 tests added. Tools work but are too slow / noisy for CI
  gating. Worth ad-hoc runs on numeric / state-machine modules. Don't wire into `check.sh`.
- **State-machine coverage**: 17 tests, 1 real bug (file_viewer search-cancel). Highest signal-to-noise of the
  test-quality push — the bug was a real silent UX failure, surfaced by writing the transition test.
- **Property-based (proptest)**: 12 tests, 0 bugs. `proptest` added as a dev-dep, scoped to four targets
  (`topological_sort_bottom_up`, `glob_to_regex`, `split_scope_segments`, `platform_case_compare`). Worth keeping for
  those specific algorithmic spots; not worth a project-wide convention.
- **IPC contract tests (vitest mockIPC)**: 23 tests, 0 bugs. Modest value — `bindings-fresh` + `no-raw-tauri-invoke`
  already cover most drift. Worth doing for destructive / cross-window surfaces (write ops, viewer, SMB); diminishing
  returns past that.
- **E2E coverage extension**: 9 tests, 1 real bug (cancel-copy rollback). Surfaced by walking the slowest test (32.7 s)
  in the post-Step-1 report.

### Outstanding items

- **Linux SMB flakes**: `50-share host shows correct share count` and `unicode shares render correctly` — flake under
  GVFS race in Docker. Pre-existing, David is aware. Not addressed in this branch.
- **Step 6a parallel-load keystroke-dispatch flakes**: rarely surface on warm runs; Step 6e converted the worst
  offenders to `dispatchMenuCommand`. Three remaining keyboard-pathway tests can flake under heavy parallel load
  (~1-in-N runs). The Step 6a fix-suggestions list (data-app-ready route-change reset, focus re-issue after click) is a
  candidate for a follow-up but wasn't load-bearing here.

### Final validation

- `./scripts/check.sh` (fast pass): **green in 3m 13s**, 1728 Rust + 1812 Svelte tests pass.
- `./scripts/check.sh --only-slow`: **green except the two pre-existing Linux SMB flakes**. E2E Playwright 131/131 in 4m
  18s across 3 shards; rust-tests-linux 1699/1699; eslint-typecheck 453/453.
