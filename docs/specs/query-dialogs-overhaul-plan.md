# Search + Select dialogs overhaul — implementation plan

Status: draft for execution. Worktree: `.claude/worktrees/query-dialogs-overhaul` (branch `query-dialogs-overhaul`, off
local `main`).

## Why this exists

The Search dialog (`lib/search/`) and the Select/Deselect dialog (`lib/selection-dialog/`) are the two consumers of the
shared `lib/query-ui/` layer. They're new, flagged **Alpha**, and this is the first real pass over them since they
landed. Today they have correctness bugs (size/type filtering doesn't work, Select loses its state on reopen), UX gaps
(no running feedback, opaque AI, off-screen cursor after select), and they don't look like the rest of the app. This
plan fixes all of that end to end.

**Mental model the implementer must hold:** Search and Select are near-identical in the user's mind. The only real
differences: Select operates on the **current folder, flat**; Search operates on the **whole volume or the local
folder** (per setting), both **recursive**. Treat every change as "make both better"; push shared behavior into
`query-ui/` and keep the two wrappers thin. Be DRY, but don't force-share where semantics genuinely diverge (the
existing core/extras split is the precedent: cross-consumer state in the core factory, consumer-only state in an extras
module).

**Design values that drive the choices below** (from `docs/design-principles.md` and `AGENTS.md` § Principles):
rock-solid + always-responsive UI (spinner during work), radical transparency (show what the AI actually did),
keyboard-first, delightful + platform-native, accessible (real AA contrast, respects system text size). Follow
`docs/style-guide.md` for every user-facing string (sentence case, active voice, no "error/failed", en dashes not em).

## Root-cause findings (verified against the code, not assumed)

These are the load-bearing facts the milestones build on. Re-confirm with a failing test before "fixing" — several
surface symptoms have a different root cause than they appear to.

1. **Select loses state on reopen** — Search holds its `QueryFilterState` in a module-level singleton
   (`lib/search/search-state.svelte.ts:42` `const core = createQueryFilterState(...)`), so it survives the dialog
   mount/unmount. Select creates it as a **component-scoped** `const` inside `SelectionDialog.svelte` (~line 96), so a
   fresh instance is born on every open. The shared `query-ui/CLAUDE.md` already documents the contract: "state survives
   unmount by design… wiping on unmount turns every close+reopen into lost work." Select just accidentally violates it.

2. **`≥ 1 MB` returns zero FILE results** — NOT a missing-size bug for files. File `size` is **Tier-1 eager** metadata
   (populated from `stat()` in `list_directory_core`, `reading.rs:229/295`; confirmed in `listing/CLAUDE.md` § metadata
   tiers), so live-folder snapshot entries DO carry file sizes. The real cause is **Select-specific**: Select's
   `buildMatchQuery()` returns `null` when `!pattern.trim()` (`SelectionDialog.svelte` ~line 233), so its JS matcher
   short-circuits to `[]` regardless of the size filter. This is why `*.png` "works" (non-empty pattern) but `≥ 1 MB`
   alone doesn't. **Search already handles filter-only correctly end-to-end** (verified, do NOT change the Rust
   backend): `engine.rs:202-216` computes `has_name/has_size/has_date/has_dir_filter`, compiles
   `compiled_pattern = None` for an empty pattern (the scan just skips the name check), and only rejects an all-empty
   query on a >100k-entry index; and the Enter/auto-apply path (`QueryDialog.executeQuery` → `runQuery`) has no
   empty-query gate. So M2 is a Select fix plus a Search regression-test, not a backend change.

3. **Size `0` reads as "off"** — `filter-chip-state.ts:52` uses `minNumeric > 0`; `0` is a valid bound. Empty input
   stays excluded because `parseFloat('')` is `NaN`. One-line fix (`> 0` → `>= 0`), plus mirror in the `between` branch.

4. **Folder sizes** — directories have `size: None` from disk metadata (only files get `stat().len()`), BUT the frontend
   `FileEntry` already carries `recursiveSize` / `recursivePhysicalSize` (`types.ts:22-23`), enriched from the drive
   index at cache-write time (`listing/CLAUDE.md` § "Enrichment at cache-write time"). So folder sizes are available
   _when the index has them_. Search results get dir sizes via `fill_directory_sizes()` (`search/query.rs:296`). The
   matcher's `getSizeFor` must fall back to `recursiveSize` for directories. When the index hasn't computed a folder's
   size yet, it's `undefined` → that folder can't match a size filter (acceptable; surface nothing misleading).

5. **Cursor doesn't move after select** — `applyIndicesToFocusedPane` → `FilePane.applyIndices` updates the selection
   `SvelteSet` but never moves the cursor (`pane-commands.ts:200`, `FilePane.svelte`). Need an explicit cursor move +
   scroll-into-view on commit.

6. **a11y-contrast is static-CSS analysis, not DOM/axe** (`scripts/check-a11y-contrast/`). It auto-pairs `color` +
   `background` declared in the _same_ selector, and has hand-listed scenarios (`dropdown_states.go`) for cases where fg
   and bg live on different selectors or need opacity modeling. The ToggleGroup `.tg-hint` (`--color-text-tertiary` at
   `opacity: 0.7`) and `.tg-badge` aren't modeled for their real composited contrast, and the dialogs' own text-on-bg
   pairs that span selectors aren't covered. The axe-core tier (`test-a11y.ts`) **disables `color-contrast`** (jsdom
   can't compute it), so the static check is the only contrast gate — extending it is the real TDD lever.

7. **Component primitives already exist, uncatalogued** — `FilterChipPopover.svelte` is already a generic
   frosted-glass/auto-flip/focus-trap/Esc popover (reused by `RecentItemsPopover`). `FilterChip.svelte` is already a
   generic chip (label/value/configured/clear). The recent-selection "pills" are _inline_ `<button>` markup in
   `RecentItemsFooter.svelte` (not a component). `lib/ui/` has **no** generic `Dropdown` or `Chip`. `Button.svelte`
   supports `variant: primary|secondary|danger` and `size: regular|mini`. The footer action buttons use `size="mini"`.

8. **The type filter's backend already exists** — `SearchQuery.is_directory: Option<bool>` (`search/types.rs:17`) is
   already filtered in the engine (`engine.rs:280-284`) and counted in the breadth guard (`engine.rs:205-206`). The
   frontend just **hardwires `isDirectory: null`** in `buildBaseSearchQuery` (`query-filter-state.svelte.ts:305`). So
   the new Type toggle wires into the _existing_ field — no new `SearchQuery` field, no engine change. `HistoryFilters`
   (`search/history.rs:57`) is a Rust `specta::Type` carrying `size_min`/`size_max`; it has `CURRENT_SCHEMA_VERSION = 1`
   and **quarantines+resets the store on any version mismatch** (`history.rs:219`). **Decision (David, beta):** a schema
   bump that wipes everyone's recent-items history is ACCEPTABLE — we have very few beta users, so don't contort the
   design to preserve history; pick the more elegant shape and bump if it's cleaner. (Purely-additive fields with serde
   `#[serde(default)]` still need no bump and stay free, so prefer that when it's equally clean — but a bump is no
   longer a blocker.) Any new IPC field needs `pnpm bindings:regen`.

## Conventions for this work

- TDD where marked **[TDD]**: write the failing test, SEE it fail for the right reason, then implement (per
  `tdd-red-green.md`). Mandatory for every bug fix and the matcher/state logic.
- Keep colocated `CLAUDE.md` (must-knows) + `DETAILS.md` (depth) in sync as you change architecture (project
  `docs-maintenance.md` rule). New gotchas → nearest `CLAUDE.md`.
- Run `pnpm check --fast` every few edits; `pnpm check` before each commit; `pnpm check --include-slow` before declaring
  a milestone done; always finish with `oxfmt` in the run. Never tail/head checker output.
- Commit per milestone (or finer). Lead-with-impact messages, no co-author lines.
- This is a `query-ui` shared layer with extensive `*.a11y.test.ts` + `*.svelte.test.ts` + pure-helper tests. Expect to
  update many; that's the contract working as intended.

---

## Milestone M1 — State hoisting + DRY foundation (bug 2)

**Goal:** Select survives close/reopen with identical mode, term, filters, and re-derived results; the two wrappers
share their common wiring.

**Why first:** Everything downstream (filters, type filter, AI strip) reads/writes this state. Fixing ownership first
means later milestones add fields in one place and both dialogs inherit them.

**Approach:**

- Create `lib/selection-dialog/selection-state.svelte.ts` mirroring `search-state.svelte.ts`: a module-level
  `const core = createQueryFilterState({ defaultMode: 'filename' })` singleton, re-exporting the getters/setters the
  wrapper needs. Selection has no "extras" module (no scope/index), so this is thinner than Search's façade — only the
  core. Move `applySelectionHistoryEntry`'s state writes to read this singleton.
- `SelectionDialog.svelte` stops creating its own instance; imports the singleton.
- **DRY pass:** factor the shared wrapper concerns. Candidates (judge each; only share where it removes real
  duplication, per the "elegance between duplication and overengineering" principle):
  - Share only what genuinely duplicates: the `applySizeFromAi`/`applyDateFromAi` helpers (near-identical to Search's
    filter-write helpers — move to a shared `query-ui` helper taking a `QueryFilterState`) and the recent-items adapter
    shape. **Do NOT** build a `filterChipsExtras` no-op abstraction "for symmetry" (review N5 — that's premature
    abstraction; Selection's no-op scaffold exists only because `FilterChips` reads props unconditionally). Hold the
    line at the real duplication.
  - Do NOT merge the wrappers themselves; they legitimately differ (IPC, snapshot vs index, commit semantics).
  - Note: the `buildAccessors()` extraction (the single source for the matcher's `getSizeFor` / `getIsDirFor`) lands in
    **M4**, when the type filter + folder-size fallback give the two duplicated accessor sites a reason to unify. Don't
    pre-extract it here against accessors that don't yet diverge — that's the premature abstraction this pass warns
    against.
- **Reset-on-reopen question:** persistence is global (like Search), NOT per-folder. Opening Select in folder B after
  setting `*.png` in folder A restores `*.png` and re-runs against B's snapshot. This matches Search's "last query
  sticks" behavior and the user's request ("reopen with the exact same state"). `⌘N` remains the only reset.
- Results: Select recomputes results on reopen by re-running the matcher against the freshly-snapshotted folder (its
  `runOnMount`/auto-apply already fires). So "same results, same order" holds when the folder is unchanged, and
  correctly refreshes when it changed.

**Docs:** Update `lib/selection-dialog/CLAUDE.md` (the "creates its own QueryFilterState (separate factory instance)"
line becomes "imports the module singleton"; note the persistence contract). Update `lib/query-ui/CLAUDE.md` if the DRY
helper lands there.

**Tests:**

- **[TDD]** `SelectionDialog.svelte.test.ts`: mount the REAL component → set mode/term/size filter → unmount → remount →
  assert state restored. The red assertion is the inverse ("state is LOST after remount") and must genuinely fail
  against today's component-scoped `const` (`SelectionDialog.svelte:96`), then flip to "state RETAINED" after the hoist.
  Confirm the existing test harness actually mounts/unmounts the Svelte component (not just re-calls the factory), or
  the red is fake.
- Keep existing `selection-history-state.test.ts` green (apply round-trip now targets the singleton).
- If a shared filter-write helper lands: a colocated pure test for it.

**Checks:** `pnpm check svelte` then `pnpm check`.

---

## Milestone M2 — Filter-only queries work (bug 6a, the headline correctness fix)

**Goal:** A size/type/date filter with an **empty name pattern** matches files (and folders) on the filters alone, in
both dialogs. `≥ 1 MB` with no glob selects every file ≥ 1 MB.

**Why high-priority:** This is the bug the user hit first ("the size filter doesn't even seem to work"). It's the
difference between the filters being decorative and functional.

**Scope correction (from review):** Search already supports filter-only queries end-to-end (backend match-alls on an
empty pattern when filters are present, and the Enter/auto-apply path has no empty-query gate — root-cause finding 2).
So **this milestone is a Select fix plus a Search regression test**, NOT a backend change. Don't touch `engine.rs`.

**Approach:**

- **Select (the actual fix):** `buildMatchQuery()` must return a query when the pattern is empty _but at least one
  filter is active_ (size ≠ any, date ≠ any, or the new type filter ≠ both — the type filter lands in M4, so write the
  predicate to read whatever filters exist now and extend it in M4). In that case the pattern becomes match-all (`*`
  glob / `.*`). Keep "empty pattern AND no filters → null" (nothing to do). The matcher (`selection-matching.ts`)
  already composes pattern AND size AND date, so a match-all pattern + size predicate yields the right set.
- **Run gating (Select):** confirm Select's auto-apply / Enter path re-runs `runQuery` on a filter edit with an empty
  bar (Select's `scheduleSearch` already fires on filter edits). The `runOnMount` `hasFilters` check
  (`QueryDialog.svelte:162-163`) already covers cold-open; make sure the live edit path does too.
- **Search:** add a regression test proving an empty bar + size filter returns files (frontend builds a match-all
  `SearchQuery`, backend returns results). No code change expected — if the test passes as-is, that's the milestone's
  "Search is fine" confirmation.
- **Empty-state vs results:** when only filters are set, the "Press Enter to filter/search" hint and the empty-state
  examples should still make sense (don't show "type a pattern" as if nothing can run).

**Docs:** `lib/selection-dialog/CLAUDE.md` § Match semantics — note filter-only queries match-all on the pattern.
`lib/search/CLAUDE.md` if the backend empty-pattern behavior changes. Add a gotcha to the nearest `CLAUDE.md`:
"filter-only queries are valid; don't reintroduce an empty-pattern early-return."

**Tests:**

- **[TDD]** `selection-matching.test.ts`: a folder snapshot with a 2 MB file + small files + a dir; query
  `{ size: gte 1 MB }` with empty pattern → returns exactly the 2 MB file's index. (Red: today `buildMatchQuery`
  short-circuits; test the wrapper's `buildMatchQuery` or add a matcher-level test that proves match-all+size.)
- **[TDD]** `SelectionDialog.svelte.test.ts`: empty bar + size ≥ 1 MB → preview list non-empty.
- Search: a `search-state` / `build-search-query` test that an empty pattern + size filter builds a match-all query; a
  Rust `search` test (`engine.rs`/`query.rs`) that empty pattern + `min_size` returns files. **[TDD]** for the Rust side
  if the backend currently requires a pattern.

**Checks:** `pnpm check rust svelte` then `pnpm check`.

---

## Milestone M3 — Size filter polish: `0` handling + `=` comparator (bug 1, decision: equals)

**Goal:** `0` is a usable bound; a one-click `=` (equals) comparator exists so "size = 0" (and "= 5 MB") is trivial.

**Approach:**

- Fix `deriveSizeChip` (`filter-chip-state.ts`): `minOk = !isNaN(n) && n >= 0` (both the gte/lte branch and the
  `between` branch's `minOk`/`maxOk`). Mirror nothing else — `parseSizeToBytes('0')` already returns `0` correctly.
- Add `'eq'` to the `SizeFilter` union (`query-filter-state.svelte.ts`). **`eq` is a UI/chip-summary concern only — it
  does NOT touch the matcher or Rust** (review S3): the matcher's `between` already matches exactly one value when
  `min == max` (`selection-matching.ts:94-99`), and `SearchQuery` already carries `min_size`/`max_size`. So `eq` maps to
  `sizeMin == sizeMax == bytes` at the `readSizeFilters()` / `applySizeQuery()` layer, and `SizePredicate` /
  `sizePredicateMatches` stay unchanged. Wire through:
  - `SIZE_COMPARATORS` in `SizeFilterPopover.svelte` (add `{ value: 'eq', label: '=' }`; pick the column order that
    reads best — likely after `lte`, before `between`).
  - `readSizeFilters()` / `applySizeQuery()` / `applyHistoryFilters()`: `eq` sets `sizeMin == sizeMax == bytes`.
  - `deriveSizeChip`: `eq` → summary `= {value} {unit}`.
  - **No `HistoryFilters` Rust change for `eq`** — it round-trips as `size_min == size_max`, which already persists (the
    struct stores bounds only, no comparator kind). **Deliberate decision (review round 2):** on restore,
    `size_min == size_max` always rehydrates as `eq`, NOT `between`. `between x x` and `eq x` are semantically identical
    (both match exactly `x`), and `eq` is the friendlier label, so the collapse is intentional. This is a behavior
    change from today (which rehydrates `min==max` as `between`) — document it so nobody "fixes" the relabel later. A
    user who stored `between 5–5` sees it come back as `= 5`; that's fine. (Rejected alternative: persisting a
    `sizeComparator` field — a Rust/bindings change for marginal value.)
- The auto-promote behavior (clicking a value cell while comparator is `any` promotes to `gte`) stays; `eq` is an
  explicit pick.

**Why `=` as its own comparator, not just "between 0 and 0":** the user explicitly asked for it; "size = 0" is a common
real task (find empty files), and `between 0 0` is unobvious. One-click equals is the delightful path.

**Docs:** `filter-chips/CLAUDE.md` § Size popover (add the `=` column entry and the 0-is-valid gotcha is already there —
keep it, it's now actually honored). `filter-chips/DETAILS.md` if comparator semantics need depth.

**Tests:**

- **[TDD]** `filter-chip-state.test.ts`: `gte`/`lte`/`between` with value `0` → `configured: true`, correct summary (red
  against `> 0`); `eq` summaries.
- **[TDD]** `selection-matching.test.ts`: `eq 0` matches only zero-byte files; `eq 5 MB` matches only exactly-5-MB.
- `filter-popover-helpers.test.ts` / `FilterChips.svelte.test.ts`: `=` comparator renders + selects + applies.
- History round-trip test for `eq`.

**Checks:** `pnpm check svelte` then `pnpm check`.

---

## Milestone M4 — `file | folder | both` type filter + folder sizes (bug 6c, 6b) + remove "+ Add filter" (bug 6)

**Goal:** A one-click ToggleGroup to constrain results to files, folders, or both, on both dialogs; size filters can
match folders by recursive size; the no-value "+ Add filter" chip is gone.

**Reuse the EXISTING backend field (from review C1):** `SearchQuery.is_directory: Option<bool>` already exists, the
engine already filters on it, and `buildBaseSearchQuery` hardwires `isDirectory: null`
(`query-filter-state.svelte.ts:305`). So there is **no new `SearchQuery` field and no engine change** — the Search work
is "stop hardwiring null; populate it from the toggle." Naming: the UI is a 3-way `Both/Files/Folders` toggle, so the
frontend core state is `typeFilter: 'both'|'file'|'folder'` (UI-named, per "name internals after the UI"), mapped at
`buildSearchQuery` time to the existing IPC `isDirectory` (`both → null`, `file → false`, `folder → true`). Document
this trivial enum→Option<bool> mapping where it lives.

**Approach:**

- **State:** add `typeFilter: 'both' | 'file' | 'folder'` (default `'both'`) to the core `createQueryFilterState()`
  (cross-consumer → core). Include it in `clearCore()` and the `⌘N` reset. Map to `isDirectory` in `buildSearchQuery`;
  read directly in Select's matcher.
- **UI:** render a `ToggleGroup` (existing `lib/ui/ToggleGroup.svelte`) in the filter strip area, NOT a chip+popover —
  the user explicitly wants one-click. Placement: leading the chip strip (before Size/Modified) reads naturally ("show
  [files] where size > …"). Labels: `Both` / `Files` / `Folders` (sentence case, plural — run the copy past David per
  "humans to humans"). Lives in the shared `QueryDialog` strip; both dialogs show it.
- **Matcher (Select):** `selection-matching.ts` filters by `isDirectory` when `typeFilter !== 'both'`. **Factor a single
  `buildAccessors()` helper** (fits the M1 DRY pass) and use it at BOTH `MatchAccessors` construction sites in
  `SelectionDialog.svelte` — `runQuery` (~317) AND `commitMatches` (~339). They currently duplicate `getSizeFor`, and
  both need the new `getIsDirFor(i)` + the folder-size fallback below; if only one is updated, preview and commit
  disagree.
- **Folder sizes (Select):** `getSizeFor(i)` returns `entries[i].size` for files and `entries[i].recursiveSize` for
  directories (already on `FileEntry:22`, enriched from the index). When a dir's `recursiveSize` is `undefined` (index
  hasn't computed it), it can't match a size filter — honest, not a bug. **Verify** the live-folder snapshot carries
  `recursiveSize` for dirs (`getEntriesSnapshot` → `getFileRange`). Per `listing/CLAUDE.md`, enrichment happens at
  cache-write time, so it should be present when the index is ready; if `get_file_range` returns un-enriched dirs, call
  the existing `refreshListingIndexSizes` before snapshotting (the file list already does this before
  `fetchListingStats`). Don't recursively walk the tree at snapshot time (expensive, blocks).
- **Folder sizes (Search):** already handled by `fill_directory_sizes()`; the existing `is_directory` + size filters
  already compose backend-side. No backend change.
- **History round-trip (`HistoryFilters` — needs a Rust edit):** to let recent searches/selections restore the type
  filter, add `is_directory: Option<bool>` to the Rust `HistoryFilters` struct (`search/history.rs:57`, shared by both
  histories), `pnpm bindings:regen`, and extend `readHistoryFilters()` / `applyHistoryFilters()`
  (`query-filter-state.svelte.ts:319-393`, today size/date only). **Schema note:** prefer additive `#[serde(default)]`
  so no bump is needed (cleanest, zero data loss). But a bump is no longer a blocker — David accepted wiping
  recent-items history in beta if a more elegant `HistoryFilters` shape wants it (root-cause finding 8). Don't contort
  the design to preserve history.
- **MCP prefill decision (review C4):** `SearchPrefill` / `applySearchPrefill` (`search-state.svelte.ts:232-258`) and
  the MCP `open_search_dialog` tool don't carry a type filter today. **Decision:** add `isDirectory?: boolean | null` to
  `SearchPrefill` + the MCP tool so agents can prefill it (consistency + testability principle; cheap, additive). If
  this balloons, defer it and say so in the milestone notes — but default to wiring it.
- **Remove "+ Add filter":** delete the trailing "+ Add filter" chip/dropdown from `FilterChips.svelte` and its
  handlers. All filters are now always visible (Type toggle + Size + Modified + Pattern). Update tests that asserted the
  Add-filter menu. After removal, watch `knip` (orphaned handlers/exports), `css-unused`, and `import-cycles` — same as
  M8's component moves.

**Why a ToggleGroup, not a chip:** size/date are ranges that deserve a popover; type is a 3-way mutually-exclusive
choice where a popover is friction. One-click matches the keyboard-first + low-friction principle. It also gives the AI
transparency strip (M6) a clean thing to reflect ("Folders only").

**Docs:** `filter-chips/CLAUDE.md` (Type toggle added, "+ Add filter" removed — update the file table + chip list).
`lib/query-ui/CLAUDE.md` (new core field). `selection-dialog/CLAUDE.md` § Match semantics (type + folder-size). Add a
gotcha: "folder size = `recursiveSize` (index-derived); `undefined` until the index computes it."

**Tests:**

- **[TDD]** `selection-matching.test.ts`: `typeFilter: 'folder'` returns only dirs; `'file'` only files; folder + size
  uses `recursiveSize`; the shared `buildAccessors()` is exercised so preview and commit agree.
- New tests for the type toggle rendering + selection; a11y test for the toggle in the dialog.
- `buildSearchQuery` test: `typeFilter` maps to `isDirectory` (`both→null`, `file→false`, `folder→true`). Existing
  engine tests already prove `is_directory` + `min_size` compose — no new Rust test needed unless behavior changes.
- Update `FilterChips.svelte.test.ts` for the removed Add-filter menu + new toggle.
- History round-trip includes the type filter (Rust `HistoryFilters` additive field + bindings regen; assert no schema
  bump). `applyHistoryFilters` / `readHistoryFilters` test extended.

**Checks:** `pnpm check rust svelte` then `pnpm check`.

---

## Milestone M5 — Mode-switch term carry-over (decision) + cursor jump after select (bug 4)

**Goal:** Switching modes carries the typed term when the target mode is empty (never overwrites); after a successful
Select, the active pane's cursor jumps to the first newly-selected file and scrolls it into view.

**Approach — term carry-over:**

- `switchMode(target)` in `query-filter-state.svelte.ts`: today it saves `handTyped[mode] = query` then restores
  `handTyped[target]` (per-mode buffers). Change: after restoring, **if the target buffer is empty, seed it with the
  outgoing term** so the text carries; if non-empty, leave it. Applies across AI↔non-AI too (raw text carried — a glob
  into AI as a prompt, or a prompt into filename as a glob; the user accepted the semantic oddity in exchange for not
  losing their words). Don't overwrite a non-empty target (the user's explicit constraint).
- This is shared, so both dialogs get it. Watch the AI special-casing: AI mode's bar holds the natural-language prompt
  while the pattern lives in `handTyped.filename|regex` via `recordAiTranslation`. The carry-over uses the _bar's_
  current term as the source.

**Approach — cursor jump:**

- On commit (`SelectionDialog.commitMatches` → `onCommit(indices, mode)` → `applyIndicesToFocusedPane(indices, mode)` →
  `FilePane.applyIndices`), after applying the selection, move the focused pane's cursor to the **first selected row**
  and scroll it into view. Reuse the pane's existing cursor-move + scroll-into-view primitive (the same one type-to-jump
  / navigation uses) — don't hand-roll scrolling.
- **Index-space care (review S1):** `indices` are snapshot indices (still including `..` at 0); `dropParentIndex` only
  trims the leading `..` from the _count_, and `applyIndices(idxs, mode, hasParent)` applies the `hasParent` offset
  internally. So the cursor target is NOT raw `indices[0]` — derive it through the **same `hasParent` transform
  `applyIndices` uses** so the cursor lands on the actual first selected file, never on the `..` row. Verify against
  `FilePane.applyIndices` / `selection.applyIndices` and `setCursorIndex`'s expected space.
- Only for `mode === 'add'` (selecting). For `'remove'` (deselect), moving the cursor to a just-deselected row is odd;
  **decision: don't move the cursor on deselect** (nothing was "selected" to reveal). Document this.

**Docs:** `query-ui/DETAILS.md` (switchMode carry-over rule + why). `file-explorer/pane/CLAUDE.md` or `DETAILS.md` (the
post-select cursor jump + the deselect exception + the index-space note).

**Tests:**

- **[TDD]** `query-filter-state.test.ts`: switch with empty target seeds the term; switch with non-empty target does NOT
  overwrite; AI↔filename both directions.
- **[TDD]** a pane-level test (`pane-commands.test.ts` or `FilePane` test) that `applyIndices(add)` moves the cursor to
  the **first selected file's row** (post-`hasParent` transform, never the `..` row) and that `remove` does not. Assert
  scroll-into-view is requested (mockable seam).
- E2E (Playwright) optional smoke: select `*.txt`, assert cursor lands on first match and it's visible. Only if a
  cheaper unit proof isn't convincing.

**Checks:** `pnpm check svelte` then `pnpm check`.

---

## Milestone M6 — AI transparency + running spinner + exact-size AI (features 8, 9, 7)

**Goal:** When an AI query runs, the user sees what the LLM actually did (the produced pattern + the size/date/type
filters it set), laid out between the mode selector and the filter chips. A spinner shows while a search/translation is
in flight. The AI can produce exact-size filters like "size = 0".

**Approach — transparency (`AiPromptStrip.svelte`):**

- Today the strip shows only the echoed prompt + an optional caveat. Extend it to render a concise, human-readable
  summary of the translation result: the produced glob/regex pattern (labelled, e.g. `Pattern: *.{jpg,png,heic}`), and
  the filters the AI set (Size, Modified, Type) in plain language. The data exists: `lastAiPattern` /
  `lastAiPatternKind` (Search extras), the chips' derived summaries, `highlightedFields` from the translate result, and
  `lastAiCaveat`. For Select, the pattern lives in `handTyped.filename|regex`.
- **Voice (David, decided):** the strip MAY speak as the in-app agent in the first person — "Here's what the agent did:"
  is the intended copy. This is a deliberate, sanctioned exception to the "no first-person in app copy" rule (alongside
  onboarding/About): the product's mental model is that an agent lives inside the app and acts on the user's behalf. The
  agentic loop / system prompts aren't built yet, but the language can already reflect that model. Keep it warm and
  honest, not overclaiming. Final wording still gets David's eye ("humans to humans").
- **Structured data is mandatory, not just prose.** The translate result MUST return the structured filters (pattern +
  kind, size min/max, modified after/before, type) so the dialog can **populate the actual filter chips** from it — the
  strip's text is a human-readable mirror of that structured state, never a substitute. The chips become the live,
  editable representation of what the agent set.
- Layout: keep it **between the mode chips and the filter strip** (where the prompt already sits). Radical transparency,
  not oversharing (design principle). The user should understand what ran well enough to explain it.
- **Post-AI editing handoff (David, decided):** after an AI search, the AI-generated filter set IS the current filter
  state. When the user switches to Filename (or Regex) mode, the AI's produced **pattern** must be sitting in the bar
  ready to tweak (it's already stashed in `handTyped[filename|regex]` by `recordAiTranslation`; the M5 term carry-over +
  this buffer guarantee it), and the applied Size/Modified/Type chips stay put (they live in shared, non-per-mode
  state). So the user lands in Filename mode looking at exactly what the agent built, fully editable. Verify this end to
  end — it's the core "tweak what the agent did" loop.
- The existing `highlightedFields` flash already draws the eye to changed chips; the strip's job is to _name_ them so
  the user understands at a glance.
- The disabled "Refine…" affordance stays (coming-soon contract).
- This is shared across both dialogs (Select's AI is cloud-only; the strip only shows after an AI run).

**Approach — spinner (feature 9):**

- `isSearching` state already exists and is set around `executeQuery` / `runAiSearch` (`QueryDialog.svelte:303/322`).
  Surface it: a spinner / progress affordance in the results area or near the bar while `isSearching` (and during AI
  translate, which is the slow part). Reuse `lib/ui/LoadingIcon.svelte` or the standard spinner — don't hand-roll.
  Respect `prefers-reduced-motion`. Ensure `getStatusText()` returns `''` while the spinner-state shows (the
  `query-ui/CLAUDE.md` rule: status bar empty when the content area shows a state message).
- AI translate can take seconds (cloud round-trip) — this is the most important place for feedback.

**Approach — exact-size AI (feature 7):**

- **No IPC type change needed (review N3):** `selection/ai/parser.rs` already has `size_min`/`size_max` as
  `Option<u64>`, and the prompt already emits `size_min`, so `size_min == size_max` is already expressible. This is
  **prompt-wording only**: teach the model it _can_ set them equal for "exactly N". Then `applySizeFromAi` (the shared
  helper from M1), seeing `min == max`, sets the M3 `eq` comparator so the chip reads "= 0 B" rather than "between 0 and
  0". `0` is not `null`, so the existing null-guards are fine.

**Approach — type filter in AI (David, decided):**

- The AI must **receive the current `file | folder | both` setting as input**, and may either set it to something it
  chooses OR leave it alone. This is the first step toward the "agent sees app state" model.
- **Asymmetry from size/date — important:** size/date currently reset to `any` before applying the AI's result (clean
  slate, so a prior run's filter can't leak). Type is DIFFERENT per David: pass the current type in; if the AI returns a
  type, apply it; **if the AI returns nothing for type, leave the user's current setting untouched** (don't reset to
  `both`). Document this deliberate difference so nobody "consistency-fixes" it into a reset.
- **IPC change + bindings regen:** extend the translate IPC (`translateSelectionQuery` / `translateSearchQuery` and the
  Rust `selection/ai` + `search/ai` request/response types) to (a) accept the current type as context and (b) return an
  optional type. Update the prompt so the model knows the field exists and is optional. `pnpm bindings:regen`. Wire the
  returned type into the shared filter-write helper alongside `applySizeFromAi` / `applyDateFromAi`.
- Structuring the IPC to carry the current type as context makes it cheap to pass the broader current filter set later
  (the agent-sees-state direction); scope this milestone to type, but don't design it shut.

**Docs:** `query-ui/CLAUDE.md` (transparency strip now summarizes the translation; spinner during `isSearching`).
`AiPromptStrip` header comment. `search/ai` / `selection/ai` `CLAUDE.md` if the prompt changes for exact size.

**Tests:**

- `AiPromptStrip.svelte.test.ts` + `.a11y.test.ts`: renders pattern + filter summary + caveat; hides when no AI run.
- A QueryDialog test that `isSearching` shows the spinner and clears it; status text empty during the spinner state.
- **[TDD]** size-AI: a translate-result with `sizeMin == sizeMax == 0` lands an `eq 0` filter (shared `applySizeFromAi`
  test). Rust test for the IPC type carrying exact size if changed.

**Checks:** `pnpm check rust svelte` then `pnpm check`.

---

## Milestone M7 — a11y: extend the contrast check, then fix; font-size bump (bug 5)

**Goal:** The `a11y-contrast` check actually covers these dialogs (and any other components it's missing); the "AI"
badge and shortcut hints meet AA; the whole dialog is one font-size step larger, both dialogs.

**Why TDD here is real:** the static check is the only contrast gate (axe disables `color-contrast`). Extending the
check to model the badge/hint/dialog pairs makes it **fail red** on the current tokens, then the token fix turns it
green. That's the genuine red→green the user asked for.

**Approach — extend the check first [TDD]:**

- In `scripts/check-a11y-contrast/`, add scenarios (the `dropdown_states.go` pattern, or a new `query_dialog_states.go`
  / `toggle_group_states.go`) modeling the real composited pairs:
  - `.tg-badge` ("AI") fg on `--color-accent-subtle` bg.
  - `.tg-hint` (`--color-text-tertiary` at `opacity: 0.7`) on `--color-bg-primary` — model the opacity composite.
  - Any dialog text-on-bg pair that spans selectors and is currently unmodeled (the AI strip caveat
    `--color-text-tertiary`, chip summaries, footer hints). Sweep the dialogs for `color` set without a same-selector
    `background` and add the ones over a non-`bg-primary` surface.
  - Audit for OTHER components the check misses while here (the agent flagged `SettingSelect .option-description`
    resting state as a candidate) — fix the check coverage, not just the dialogs.
- Run the check; capture the red findings (these are the real violations).

**Approach — fix the tokens:**

- Raise the offending pairs to AA: pick a darker/ligher token (e.g. `--color-text-secondary` instead of tertiary, drop
  the `opacity: 0.7`, or introduce a dedicated accessible token). The badge "AI" must be legible on its subtle accent
  background across all 9 runtime accents (the check sweeps the accent matrix — let it guide the choice). Shortcut hints
  must be readable without the 0.7 opacity crutch. Keep the visual hierarchy (hints still quieter than labels) while
  clearing 4.5:1 (3:1 if large).

**Approach — font bump:**

- Bump the dialog one font-size step across bar + mode chips + filter chips + results rows + footer, **both dialogs**.
  Use the `--font-size-*` token scale (never raw px for font-size — `frontend.md` rule). The dialog should also respect
  the system text-size watcher (`system-text-size-changed`) like the rest of the app — verify the bump composes with it
  rather than fighting it.
- **First-class verification item (review S5):** the font bump changes Brief-mode column widths (`font-metrics/` ships
  per-char widths to Rust) AND row heights in the **virtualized results list**. A wrong row-height constant breaks
  virtual scrolling (gaps/overlap). Explicitly re-measure and test the virtualized results sizing at the new font step,
  plus the chip-strip wrap. This risk is independent of M8.

**Docs:** `scripts/check-a11y-contrast/` doc / comment on the new scenarios. `ToggleGroup.svelte` comment on why the
hint token changed. `style-guide.md` / `design-system.md` only if a new token is introduced.

**Tests:**

- **[TDD]** the contrast check itself: run `pnpm check a11y-contrast`, see the new red, then green after the token fix.
- Existing `*.a11y.test.ts` (axe tier-3) stay green.
- A visual pass via MCP screenshots (light + dark, a couple of accents) to confirm the font bump + contrast read well —
  this is human-reviewed UI per the "humans to humans" principle.

**Checks:** `pnpm check a11y-contrast css svelte` then `pnpm check`. (`a11y-contrast` is in `--fast`.)

---

## Milestone M8 — Component-library alignment + standard buttons (feature 10, bug 3)

**Goal:** The dialogs' reusable primitives live in `lib/ui/` and appear in Debug > Components so they benefit from the
upcoming standard-component restyle; footer buttons use standard primary/secondary (not `mini`); the dialogs read like
the rest of the app.

**Important scoping note:** the user is doing a separate "component sweep" later today that _restyles_ the standard
components. This milestone's job is to **extract + relocate + register** these primitives (behavior-preserving) so the
sweep reaches them — NOT to invent new styling. Keep each component's current look; just make it a catalogued `lib/ui/`
component.

**Approach:**

- **`Dropdown` (generic popover):** promote `filter-chips/FilterChipPopover.svelte` → `lib/ui/Dropdown.svelte` (generic
  frosted-glass, auto-flip, focus-trap, Esc-scoped close). Keep the existing API (`anchor`, `open`, `onClose`,
  `ariaLabel`, children). Re-point `RecentItemsPopover` and the filter popovers at it. The "Recent selections → All
  selections… ⌘H" trigger uses this generic `Dropdown`.
- **`FilterDropdown` (subtype):** a thin wrapper / variant of `Dropdown` for the size/date/scope filter popovers (the
  labelled grid surface). Could be `Dropdown` + a `variant="filter"` prop or a small `FilterDropdown.svelte` composing
  `Dropdown`. Pick the cleaner one; document the choice.
- **`Chip` (generic):** promote `filter-chips/FilterChip.svelte` → `lib/ui/Chip.svelte` (label/value/configured/clear/
  open/disabled). Replace the **inline** recent-selection pills in `RecentItemsFooter.svelte` with this `Chip` (they're
  currently raw `<button>` markup — this is the "little chips should be a reusable component" the user wants). The
  recent pill needs: label, mode badge, age/tooltip, click, optional remove — make sure `Chip` supports a leading badge
  - optional clear so it covers both the filter chip and the recent pill.
- **Buttons (bug 3):** move the footer action buttons (`SearchFooterActions.svelte` "Go to file" / "Show all in main
  window"; Select's "Select these files" via `primaryAction`) off `size="mini"` + custom inline shortcut-hint spans to
  standard `Button` `variant="primary|secondary"` at `size="regular"`, with the shortcut hint rendered the standard way
  (the app's `ShortcutChip` component, not bespoke spans). The recent-footer trailing "All selections…" button likewise
  uses `Button` (it's currently raw markup).
- **Catalog:** register the new components in `routes/dev/components/`: new sections `Dropdown`, `FilterDropdown`,
  `Chip` (and ensure `ToggleGroup` already shown covers the new type filter). Follow the catalog registration pattern
  (`sections/*.svelte` + `SUB_IDS` + import + render in `+page.svelte`).
- **Overall styling:** align spacing/typography/surfaces to app tokens where the dialog currently diverges (without
  pre-empting the sweep — just remove obvious one-offs, use `--spacing-*` / `--radius-*` / `--font-*`).

**Why extract now (before the sweep) rather than after:** the user wants the sweep to hit these. If they stay
dialog-local, the sweep misses them. Extracting first is the DRY/ideal-end-state choice (`ideal-over-cheap` rule).

**Risk:** `FilterChipPopover`/`FilterChip` are used widely; this is mechanical-but-broad. Do it in its own milestone,
keep the API stable, lean on the existing `*.svelte.test.ts` + `*.a11y.test.ts` to prove no behavior change. Watch the
`knip` (unused exports), `import-cycles`, and `css-unused` checks after the moves.

**Docs:** `lib/ui/CLAUDE.md` + `DETAILS.md` (new `Dropdown`, `FilterDropdown`, `Chip` entries, props tables).
`filter-chips/CLAUDE.md` (now wrap the `lib/ui` primitives). `recent-items` docs (pills are `Chip` now).
`routes/dev/components` index.

**Tests:**

- Move/keep the existing popover + chip tests; add `lib/ui/Dropdown`, `Chip` colocated `*.svelte.test.ts` +
  `*.a11y.test.ts` (the catalog components need tier-3 a11y).
- `SearchFooterActions.svelte.test.ts` + policy test updated for standard buttons.
- A button-restyle check (`btn-restyle`) and `knip` must pass.

**Checks:** `pnpm check svelte css` then `pnpm check`, and `pnpm check --include-slow` (E2E may touch these surfaces).

---

## Cross-cutting: docs, final checks, merge

- **Bindings:** any Rust IPC type change (the M4 `HistoryFilters.is_directory` field, the M4 `SearchPrefill`/MCP
  addition) requires `pnpm bindings:regen`; the generated `bindings.ts` is checked by `bindings-fresh` (and the
  `no-raw-tauri-invoke` rule means call sites go through the typed `commands.*` wrappers). Never hand-edit
  `bindings.ts`.
- After all milestones: full `pnpm check --include-slow` (allow ~20 min for the slow lane: `desktop-e2e-linux`,
  `desktop-e2e-playwright`, `rust-tests-linux`). Finish with `oxfmt`.
- Sweep the colocated docs one more time for drift (the `docs-maintenance` rule). Confirm `AGENTS.md` /
  `architecture.md` subsystem descriptions for `query-ui`, `search`, `selection-dialog` still read true.
- Manual MCP smoke on the running app (both dialogs, light + dark): filter-only size query, `= 0`, type toggle, AI query
  with the transparency strip, spinner, reopen-keeps-state, cursor jump.
- Update `CHANGELOG.md` if these dialogs are user-facing in the current channel (they're Alpha — check the changelog
  conventions; likely yes, impact-focused).
- FF-merge to local `main` after rebasing onto current local `main`; delete the worktree + branch. Do NOT push (user
  pushes on their own cadence).

## Sequencing & parallelism

Run sequentially by default (we're not in a hurry). Hard dependencies:

- **M1 first** (state foundation; M3/M4/M6 add fields to the hoisted core).
- **M2** depends on M1 (filter-only gating reads the shared state) but is otherwise the highest-value early win.
- **M3, M4** depend on M1/M2 (filters + matcher).
- **M6** (AI transparency) depends on M4 (reflects the type filter) and M3 (reflects `eq`).
- **M5** (term carry-over + cursor jump) is largely independent — could go any time after M1.
- **M7** (a11y/font) is independent of the filter logic; do after M8's component moves settle, or before — but the font
  bump interacts with any layout the component sweep touches, so run M7 AFTER M8 to a11y-test the final markup.
- **M8** (component extraction) is broad and mechanical; keep it isolated to reduce churn. Safe to do last.

Only genuinely-independent pair that _could_ overlap: M5 (cursor jump, pane code) and M8 (ui component extraction) touch
disjoint files. Even so, prefer sequential unless time-pressured.

**Parallel subagents → sub-worktrees (David, instruction):** if execution parallelizes work across subagents, each
subagent works in its **own git worktree** (branched off this `query-dialogs-overhaul` branch, not main), with its own
populated `.codegraph/` per the `codegraph-worktree` rule (clone the db with `cp -c`, copy `config.json`, run
`codegraph sync`) before it starts. The lead rebases + integrates each sub-branch back onto `query-dialogs-overhaul`,
re-running the security/data-safety-critical tests itself before merging (the `verify-delegated-work` rule). Given the
heavy shared-state coupling here (most milestones touch `query-filter-state.svelte.ts` and the shared `QueryDialog`),
expect this to be mostly sequential; parallelize only the genuinely-disjoint pieces (e.g. M8's component extraction, or
independent test-writing) to avoid thrashing the shared core.

## Open verification items for the implementer (resolve with a test, don't assume)

1. Whether `get_file_range` returns `recursiveSize`-enriched directories for the live Select snapshot, or whether a
   `refreshListingIndexSizes` call is needed before snapshotting. (M4)
2. Exact strings/labels for the Type toggle and the AI transparency strip — run them past the style guide (sentence
   case, plural, active voice) and past David for the human-facing copy ("humans to humans" principle). (M4/M6)
3. Whether to wire the type filter into MCP `open_search_dialog` prefill now or defer (default: wire it, it's additive).
   (M4)

Resolved during planning (don't re-litigate): Search's backend already match-alls on empty-pattern-with-filters
(`engine.rs:202-216`), so M2 is Select-only. `SearchQuery.is_directory` already exists and is filtered, so the Type
toggle reuses it. `eq` is a chip-summary concern only (`between` min==max in the matcher), so no matcher/Rust change.
