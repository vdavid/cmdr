# Query UI (shared filter-and-act-on primitives)

Home for primitives shared between the Search dialog (`lib/search/`) and the upcoming Selection dialog
(`lib/selection-dialog/`). Owns the unified query bar, mode chips, AI prompt strip, filter chips strip (size, modified,
scope, pattern), virtualized results table with path pills and per-row menus, recent- items footer + popover, and the
cross-consumer filter state factory.

See [`docs/specs/selection-dialog-plan.md`](../../../../../docs/specs/selection-dialog-plan.md) for the bigger picture.
`lib/search/CLAUDE.md` keeps Search-specific decisions (snapshot store, virtual volume, MCP open path, "Open in pane",
index lifecycle, "Use current folder" smart fallback).

## Files (M3)

| File                                        | Purpose                                                                                                                                                                                                 |
| ------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `QueryBar.svelte`                           | Unified query input: one `<input>` for AI / filename / regex; placeholder updates per mode; right-gutter run hint + ⏎ button                                                                            |
| `ModeChips.svelte`                          | Mode chip row below the bar. Thin wrapper over `lib/ui/ToggleGroup.svelte` with `semantics="tabs"`. AI / Filename / Content (disabled) / Regex. Search renders all four; Selection (M7+) drops Content. |
| `AiPromptStrip.svelte`                      | Strip below the chip row showing the AI prompt, optional caveat, disabled Refine button                                                                                                                 |
| `FilterChips.svelte`                        | Filter chip strip (Pattern + Size + Modified + Search in) plus Add filter dropdown. Each opens a popover. Visibility flags: `scopeChipVisible`, `patternChipVisible`                                    |
| `FilterChip.svelte`                         | Single chip: default/configured states, `×` clear, Backspace clear, aria-expanded                                                                                                                       |
| `FilterChipPopover.svelte`                  | Generic popover: frosted-glass, auto-flip, focus trap, Esc closes without disrupting dialog                                                                                                             |
| `filter-chip-state.ts`                      | Pure helpers: `deriveSizeChip`, `deriveDateChip`, `deriveScopeChip`, `derivePatternChip` (testable in isolation)                                                                                        |
| `filter-popover-helpers.ts`                 | Pure: `SIZE_PRESETS`, `byteUnitLabel`, `kiloByteLabel`, `isSizeRangeDisabled`, `showsUpperBound`, `isDateRangeDisabled`, `showsDateUpperBound`, `buildDatePresets`                                      |
| `QueryResults.svelte`                       | Column headers + results list + states (loading, empty, populated) + status bar. New `showPathColumn` prop (default `true` for Search; Selection passes `false`)                                        |
| `EmptyState.svelte`                         | Pre-search "Try…" block: three example chips, optional index size hint, optional keyboard hint                                                                                                          |
| `PathPills.svelte`                          | Clickable path-pill strip rendered inside each row's path column. Overflow collapse into a single `…` pill with hidden-segments tooltip                                                                 |
| `path-pills-layout.ts`                      | Pure: `computePathPillsLayout`, `scheduleStableWidthMeasure`                                                                                                                                            |
| `SearchRowMenu.svelte`                      | Per-row `…` button: always visible on every row. Routes to the parent's native context menu via `onOpen`. Name kept verbatim per M3 plan                                                                |
| `query-filter-state.svelte.ts`              | Factory `createQueryFilterState()` producing the cross-consumer state instance                                                                                                                          |
| `enter-action.ts`                           | Pure: `deriveEnterAction({ lastEvent, resultsCount })` returning `'run-search' \| 'go-to-file'`                                                                                                         |
| `recent-chips-layout.ts`                    | Pure: `computeRecentChipsLayout` for the recent-items footer's greedy fit                                                                                                                               |
| `recent-items/RecentItemsFooter.svelte`     | Generic `<E>` chip strip for recent entries plus trailing "All …" affordance. Consumer passes adapter + keyFn                                                                                           |
| `recent-items/RecentItemsPopover.svelte`    | Generic `<E>` fuzzy-searchable popover over the full recent-entries history (ufuzzy)                                                                                                                    |
| `recent-items/recent-items-state.svelte.ts` | Factory `createRecentItemsState({ getRecent })` returning the reactive store                                                                                                                            |
| `recent-items/recent-items-types.ts`        | `RecentItemAdapter<E>`, `RecentItemKey<E>`, `RecentItemView`                                                                                                                                            |
| `recent-items/recent-items-utils.ts`        | Pure helpers `modeBadge`, `modeName`, `formatAge`, `filterSummary`, `chipTooltip`                                                                                                                       |

Component-level tests (`*.svelte.test.ts`) and tier-3 a11y tests (`*.a11y.test.ts`) colocate with the components. The
companion test catalog (mirrors the file table above):

| Test                                     | Coverage                                                                                                                              |
| ---------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `QueryBar.svelte.test.ts`                | Per-mode placeholder, value mirror, `onInput` callback                                                                                |
| `ModeChips.svelte.test.ts`               | Chip set, active marker, click + keyboard activation, focus motion (skipping Content), AI-on/off cardinality, ToggleGroup wiring      |
| `FilterChips.svelte.test.ts`             | Chip rendering, `×` and Backspace clear, popover open/close, Add filter list, scope behavior, ⌥S/⌥M/⌥I openers, ⌥C/⌥V scope shortcuts |
| `AiPromptStrip.svelte.test.ts`           | Renders prompt, renders caveat when set, Refine button is disabled with Coming soon tooltip                                           |
| `QueryResults.a11y.test.ts`              | Tier-3 axe-core audit across result states                                                                                            |
| `QueryResults.states.svelte.test.ts`     | Loading / no-results-criteria / populated branches, status-bar emptiness rule                                                         |
| `PathPills.svelte.test.ts`               | Path-pill split semantics (`/` only), click → onPick wiring, stopPropagation contract                                                 |
| `PathPills.a11y.test.ts`                 | Pins `tabindex="-1"` per pill (not in Tab order); axe-core audit                                                                      |
| `path-pills-layout.test.ts`              | Deterministic layout against mocked widths (chrome budget, first/last preservation, hidden middle)                                    |
| `SearchRowMenu.svelte.test.ts`           | Button rendering, `is-cursor` marker, onOpen + stopPropagation on click                                                               |
| `SearchRowMenu.a11y.test.ts`             | Tier-3 axe-core audit for cursor-row and non-cursor variants                                                                          |
| `FilterChip.a11y.test.ts`                | Tier-3 axe-core audit across default, configured, disabled, and open states                                                           |
| `FilterChipPopover.svelte.test.ts`       | Mount / unmount via `open` prop, Esc → onClose with stopPropagation                                                                   |
| `EmptyState.svelte.test.ts`              | Chip rendering per `aiEnabled`, click → `onPick`                                                                                      |
| `RecentItemsFooter.svelte.test.ts`       | Layout cap, click → onPick, contextmenu → onRemove, "All …" → onOpenAll, Search-shaped + Selection-shaped adapters                    |
| `RecentItemsFooter.label.svelte.test.ts` | D5: the leading label renders                                                                                                         |
| `RecentItemsFooter.a11y.test.ts`         | Zero/one/many/disabled state audits                                                                                                   |
| `RecentItemsPopover.svelte.test.ts`      | Closed/open render, fuzzy filter, empty message, Enter on cursor row, right-click → onRemove, filter resets on reopen                 |
| `RecentItemsPopover.a11y.test.ts`        | Closed + open-with-entries audits                                                                                                     |
| `filter-chip-state.test.ts`              | Default → configured → cleared rules for each filter chip's display summary                                                           |
| `filter-popover-helpers.test.ts`         | Size + date preset rules, comparator gating, dynamic Modified preset labels                                                           |
| `query-filter-state.test.ts`             | Factory defaults, switchMode + per-mode buffers, history filters, recordAi NG3 split                                                  |
| `enter-action.test.ts`                   | Eight-permutation table for `deriveEnterAction`                                                                                       |
| `recent-chips-layout.test.ts`            | Greedy-fit packing against mocked widths                                                                                              |
| `recent-items-utils.test.ts`             | `modeBadge`, `modeName`, `formatAge`, `filterSummary`, `chipTooltip` rules                                                            |

## State shape contract

`createQueryFilterState()` owns ONLY cross-consumer fields. Both Search and Selection share the same shape; one dialog's
instance can never leak into the other.

Fields:

- `query`, `mode` (the unified search input + mode discriminator)
- `sizeFilter` + value/unit, plus the `Max` half for `between` ranges
- `dateFilter` + value, plus `dateValueMax` for `between` ranges
- `caseSensitive`
- `lastAiPrompt`, `lastAiCaveat` (the AI transparency strip's content)
- per-mode `handTyped` buffers (`ai` / `filename` / `regex`)
- `results`, `totalCount`, `cursorIndex`, `isSearching`
- `lastDialogEvent` (drives ⏎ ownership via `deriveEnterAction`)
- `runOnMount`, `lastRunQuery` (one-shot prefill + auto-apply gates)

Search-only fields live next to the Search wrapper in
[`lib/search/search-extras-state.svelte.ts`](../search/search-extras-state.svelte.ts): `scope`, `excludeSystemDirs`,
`isIndexReady`, `indexEntryCount`, `isIndexAvailable`, `lastAiLabel`, `lastAiPattern`, `lastAiPatternKind`. The
whole-drive index is Search-only (Selection matches against an in-memory pane listing), so the index flags live there
even though they look like "session state". The Search wrapper instantiates both factories and composes them;
Selection's wrapper uses only the core. This keeps Selection's runtime state free of fields it never reads, and keeps
the shared factory honest about what's actually shared.

### When to use the factory vs extras

When adding a new field, ask: "would Selection also care about this?"

- **Yes** → add it to `createQueryFilterState()`. Cross-consumer. Selection's instance will carry it whether or not the
  Selection wrapper reads it today.
- **No** → add it to `createSearchExtrasState()` in `lib/search/`. Search-only.
- **No, but Selection has its own variant** → keep both in their respective consumer's "extras" module. Don't try to
  share via the core when the semantics diverge.

The `lastAiLabel` field is the textbook "no" case. Search's snapshot pane needs a short LLM-produced title for the
breadcrumb; Selection has no snapshot pane and no breadcrumb to seed.

### `recordAiTranslation` is split (NG3)

Pre-M2, `recordAiTranslation({pattern, kind, label})` wrote four pieces in one function: `handTyped[mode]`,
`lastAiPattern`, `lastAiPatternKind`, `lastAiLabel`.

M2 splits it because three of the four writes are Search-only:

- **Core's `recordAiTranslation({pattern, kind})`** writes ONLY to `handTyped[mode]` (R3 B2: AI's output overwrites the
  matching mode's hand-typed buffer). Both consumers call this.
- **Extras' `recordAiPatternAndLabel({pattern, kind, label})`** writes ONLY to the Search-only fields. Search's wrapper
  calls this right after the core method; Selection's wrapper skips it.

The Search façade in `lib/search/search-state.svelte.ts` keeps the legacy `recordAiTranslation({pattern, kind, label})`
shape as a convenience that calls both methods in sequence.

## Round 3 polish (R3)

These shipped with the search-fixup round 3 brief but apply to every consumer of the query UI:

- **B1**: `QueryBar.svelte` run button no longer leads with a corner-down-left icon; the `⏎` shortcut sits at the suffix
  slot at `--spacing-xs` from the "Search" label so the rhythm matches "Go to file ⏎" and "All searches… ⌘H" elsewhere.
- **B5**: `FilterChips.svelte` keeps `dateIsCustomLower` / `dateIsCustomUpper` in sync via an `$effect` that flips them
  OFF when `dateValue` matches a preset (mirrors the size flow). The Modified popover never shows both a preset AND
  Custom as selected.
- **U1**: `RecentItemsFooter.svelte` + `recent-chips-layout.ts` use a greedy-fit layout: leading label ("Recent
  searches:" or "Recent selections:") and trailing button ("All searches… ⌘H" or equivalent) are always rendered; the
  middle slot packs as many chips as fit, dropping the rest silently. No horizontal scrolling, no ellipsis chip.
- **U2**: each chip's tooltip leads with the full text so a CSS-ellipsis-truncated chip stays readable on hover.
- **U3**: Size > Custom input lives INSIDE the Custom cell (one click selects + focuses).
- **U4**: Modified presets are dynamic ("today 0:00", "1st of May 0:00", …) — see
  `filter-popover-helpers.ts::buildDatePresets`.
- **U5**: value + unit cells in the Size and Modified popovers stay clickable while comparator = `any`; they render with
  `.is-disabled-look` (dimmed) and clicking auto-promotes the comparator to `gte` / `after` plus applies the clicked
  value.
- **U7**: path column font bumped from `--font-size-xs` to `--font-size-sm` (matching the filename column); row vertical
  padding cut from `--spacing-xs` to `--spacing-xxs` so the row height stays the same.

R3 search-specific items (B2, B3, B4, B6, U6, U8, T1) stay in `lib/search/CLAUDE.md`.

## Round 2 grid-style filter popovers (D10 / D11)

The Size and Modified popovers render as a multi-column list selector. Tested via `filter-popover-helpers.test.ts` and
`FilterChips.svelte.test.ts`.

**Size popover** (`FilterChips.svelte`):

- Col 1: `any`, `≥`, `≤`, `between` (one selected at a time).
- Col 2: `0`, `1`, `5`, `10`, `20`, `50`, `100`, `200`, `500`, `Custom…`. Disabled when col 1 = `any`. Selecting
  `Custom…` reveals an inline `<input type="number">`.
- Col 3: unit. The "byte(s)" cell label flips based on the selected value. The "kB/KB" cell follows
  `appearance.fileSizeFormat` (SI → `kB`, binary → `KB`). `MB` and `GB` are constant.
- When col 1 = `between`: cols 4 + 5 mirror cols 2 + 3 for the upper bound.

**Modified popover** (same component):

- Col 1: `any`, `after`, `before`, `between`.
- Col 2: presets `today`, `yesterday`, `this week`, `last week`, `this month`, `last month`, `this year`, `Custom…`
  (Custom reveals `<input type="date">`).
- When col 1 = `between`: col 3 mirrors col 2 for the upper bound. No unit column.

**Cells are buttons**, not radios; they carry `role="radio"` plus `aria-checked` so AT users read the cell set as a
radio group while the click target stays generous. Disabled cells get `disabled={true}` rather than `aria-disabled`, so
the keyboard skip and the mouse not-allowed cursor are both correct without extra handling.

**Shortcut openers** (`FilterChips.svelte::handleDialogPopoverOpener`):

- `⌥S` opens the Size popover.
- `⌥M` opens the Modified popover.
- `⌥I` opens the Search-in popover (Search only; Selection passes `scopeChipVisible: false` and the ⌥I shortcut is
  suppressed).

On macOS the Option key remaps `event.key` to typographic glyphs (Option+S → `ß`, Option+M → `µ`), so `altLetter()`
matches on `event.code` (`KeyS`, `KeyM`, …) first and falls back to `event.key` for synthesized test events. Same trick
lives in `SearchDialog.svelte::matchKey` for the mode-chip ⌥A / ⌥F / ⌥R shortcuts.

**Gotcha: `parseSizeToBytes('0', unit)` is now 0, not `undefined`.** Round 1 returned undefined for `0`, which silently
dropped a `0`-byte preset. The list-style grid lets the user explicitly pick 0 as a lower or upper bound, so the helper
now honors it.

**Gotcha: Size unit is `'B' | 'KB' | 'MB' | 'GB'`.** Round 1 had no byte unit; round 2 adds it for the "byte(s)" cell.
The AI translator's `bytesToDisplaySize` still produces `KB | MB | GB`; the user can still pick "bytes" from the unit
column manually.

## Round 2 R2: PathPills measurement

The fitting algorithm lives in `path-pills-layout.ts::computePathPillsLayout` (pure, deterministic, unit-tested with
mocked widths). The chrome budget per pill dropped from 16 px to 4 px (matching the real CSS padding) so the strip no
longer collapses when there's free space. The container width comes from a `ResizeObserver` on the strip element, and
`createPretextMeasure` provides pixel-accurate text widths.

## Keyboard shortcuts (in-dialog, hard-coded)

Both Search and Selection inherit these. ⏎ has dynamic ownership (see D8 below).

| Shortcut  | Action                                                                              |
| --------- | ----------------------------------------------------------------------------------- |
| `Enter`   | Dispatched via `enterAction`: "go-to-file" or "run-search" (D8)                     |
| `⌥⏎`      | Show all results in the main window (Search) / no-op (Selection); see consumer      |
| `⌘Enter`  | No-op (R4). Bare Enter is the only path that runs a search or opens the cursor row. |
| `⇧Enter`  | No-op (R4). Same rule as ⌘Enter.                                                    |
| `⌘N`      | Clear all dialog state ("new search" / "new selection")                             |
| `⌘H`      | Toggle the recent-items popover (fuzzy over the full history)                       |
| `⌘1`      | Switch to AI (AI on) or Filename (AI off)                                           |
| `⌘2`      | Switch to Filename (AI on) or Regex (AI off)                                        |
| `⌘3`      | Switch to Regex (AI on); no-op when AI is off                                       |
| `⌘4`      | Reserved for Content when it ships; not wired now                                   |
| `⌥A`      | Mode chip: AI (global inside the dialog; only when AI is enabled)                   |
| `⌥F`      | Mode chip: Filename (global)                                                        |
| `⌥R`      | Mode chip: Regex (global)                                                           |
| `⌥←`      | Navigate the active pane to the cursor row's parent folder                          |
| `⌥→`      | Navigate the active pane to the cursor row's path (descend back)                    |
| `↑` / `↓` | Move the cursor through the results list (loops top<->bottom)                       |
| `←` / `→` | When focus is on a mode chip: move between chips (skip Content)                     |
| `Tab`     | Trapped within the dialog; cycles through interactive elements                      |
| `Escape`  | Close the dialog                                                                    |

Scope-popover shortcuts (`⌥C`, `⌥V`) are Search-only — see `lib/search/CLAUDE.md` § "Scope shortcuts".

### Round 2 D8: `⏎` ownership swap

The factory carries `lastDialogEvent: LastDialogEvent` (one of `opened`, `results-arrived`, `cursor-moved`,
`query-edited`, `filter-edited`). The pure helper `deriveEnterAction({ lastEvent, resultsCount })` returns
`'go-to-file' | 'run-search'`:

- `'go-to-file'` when there are results AND the last event was `results-arrived` or `cursor-moved` (the user just got a
  list back or is browsing it). Pressing ⏎ opens the cursor row in the active pane.
- `'run-search'` otherwise (zero results, freshly opened, query/filter just edited). Pressing ⏎ runs the query.

The bar's run button reads `Search ⏎` only when `enterAction === 'run-search'`; the footer's `Go to file` button reads
`Go to file ⏎` only when `enterAction === 'go-to-file'`. Exactly one of them surfaces the hint at any time. Tests in
`enter-action.test.ts` pin the eight-permutation table.

### Round 2 D6: footer buttons always visible

The policy: footer actions render unconditionally; when there are no results (or the index isn't ready) they render
disabled instead of hidden, so the layout stays still while the user types. The specific Search footer buttons ("Show
all in main window", "Go to file") live in `lib/search/SearchFooterActions.svelte`.

The Content chip is visible-disabled with a "Coming soon" tooltip. It has **no** shortcut. Wiring a shortcut to a
disabled control is hostile UX; reserving `⌘4` is the better contract. When Content ships, it claims `⌘3` and Regex
moves to `⌘4`.

**Esc inside an open filter-chip popover closes only the popover.** The dialog's Escape handler runs in capture phase on
`window`, which would otherwise fire before the popover's bubble handler. The dialog checks
`dialogElement.querySelector('.filter-chip-popover')` and, when a popover is present, returns without closing the
dialog. The popover's own keydown handler then runs on the bubble, closes itself, and calls `stopPropagation` so nothing
else fires. Without this guard, Escape inside a popover would close the whole dialog and lose the user's place. Pinned
in `FilterChips.svelte.test.ts`.

## Mode chips: shared visual primitive, two ARIA shapes

`lib/ui/ToggleGroup.svelte` is the shared segmented-control primitive used by both Settings's toggle groups and the
Query dialog's mode chips. See `lib/ui/CLAUDE.md` § "ToggleGroup" for the primitive's contract. `ModeChips.svelte` is
the Query-side wrapper: `semantics="tabs"`, one option entry per mode, the disabled Content entry carries the
`disabled: true, tooltip: "Coming soon: ..."` flags so the chip stays visible-disabled with the tooltip wired through
the underlying ToggleGroup option cells.

Same external props as the pre-M3 `SearchModeChips`: `mode`, `aiEnabled`, `disabled`, `onSelect`.

## Key shared patterns

**Command palette pattern**: Own fixed overlay + backdrop, not `ModalDialog`. Needs custom keyboard handling (arrow keys
for results, Tab between filters) that would fight `ModalDialog`'s focus management.

**Two-cursor hover model**: `cursorIndex` (keyboard) and `hoveredIndex` (mouse) are independent. Hovering a row writes
`cursorIndex` via `onHover` so mouse + keyboard share one accent-colored cursor.

**Live search with debounce**: 1 s debounce on filename/regex modes only, gated by the `search.autoApply` setting
(default on). AI mode never auto-applies regardless: AI calls cost money and the user must explicitly opt in via Enter /
`⌘Enter` / the `⏎` run button. Constant `SEARCH_AUTO_APPLY_DEBOUNCE_MS = 1000` lives in `query-filter-state.svelte.ts`.

**Auto-apply gates**: `scheduleSearch()` returns early in three cases:

1. `mode === 'ai'`: AI never auto-applies.
2. `search.autoApply === false`: the user runs every search explicitly.
3. IME composition is in progress.

**`⏎` run button**: Always visible on the right end of the bar. Clicking it is equivalent to pressing Enter in the
input. Mouse-first path; keyboard-first path is Enter.

**"Press Enter to search" hint**: Appears in the right gutter of the bar in `--color-text-tertiary` when (a) the query
is non-empty and (b) it has changed since the last actually-issued search and (c) auto-apply won't pick it up
(`mode === 'ai'` OR `search.autoApply === false`). Tracked by `lastRunQuery`.

**IME composition guard**: The dialog tracks `imeComposing` via `oncompositionstart` / `oncompositionend` on the search
bar input. While composing, `scheduleSearch()` is a no-op so we don't fire mid-character on Chinese / Japanese / Korean
input. On `compositionend` the dialog calls `scheduleSearch()` once so the user gets exactly one auto-apply fire after
the composed character lands.

**Deferred loading indicator**: The "Loading drive index..." message only appears when the user has triggered a search
while the index is still loading. On initial open, the results area is empty (no loading message) since the user is
still typing their query.

**State preservation across close + reopen**: The factory's `$state` survives dialog unmount. Closing the dialog (Escape
or overlay click) does NOT wipe query, mode, filters, scope, results, or cursor. The only reset path is `⌘N` inside the
dialog, which calls the consumer's clear hook.

**`⌘N` shortcut**: Hard-coded in the dialog's `handleModifierShortcuts`. Captured before the dialog's global
`stopPropagation` would let it reach the route-level `⌘N` (new tab) handler. The choice of `⌘N` matches the macOS "new
X" idiom.

**`runOnMount` flag**: A one-shot boolean on the core factory. Cleared on `⌘N` (so the shortcut doesn't leave a stale
flag). Set by Search's `applySearchPrefill(prefill)` (and Selection's equivalent) to `prefill.autoRun ?? true`. Consumed
by the `$effect` block in the dialog that fires when the flag is true and the dialog is mounted. Idempotent: the effect
clears the flag first.

**Path pills with overflow collapse**: Each result row's path column renders as a strip of clickable ancestor pills
produced by `PathPills.svelte`. Clicking a pill calls the dialog's `onNavigate(ancestorPath)` callback, which closes the
dialog and navigates the active pane to that ancestor. Pills are **not** in the keyboard Tab order (`tabindex="-1"`):
tabbing through them would break the row's arrow-down keyboard flow inside the virtualized list. The keyboard
equivalents are `⌥←` and `⌥→`. Paths are split strictly on `/`; macOS and Linux only, no `\` handling.

When the path doesn't fit its column, the middle pills collapse into a single `…` pill. Width is measured with
`@chenglou/pretext` (the same canvas-based measurer the rest of the app uses); the first and last segments stay visible.
Hovering the `…` pill shows a tooltip listing the hidden segments as clickable buttons.

The pill's `onclick` calls `e.stopPropagation()` so it doesn't double-fire the row's `onResultClick`. Svelte 5 delegates
events at the document root, so unit tests assert against the `stopPropagation` spy rather than racing a wrapper DOM
listener.

**Per-row `…` menu**: `SearchRowMenu.svelte` renders an ellipsis button on every row, always visible. Both the button
click and a right-click on the row call `onRowMenu(entry)` on the parent, which routes to the existing native
`showFileContextMenu` factory. The column header above the button reads "Actions".

## Key shared decisions

**Decision**: Unified query bar with mode chips, not two separate input rows. **Why**: AI prompts and filename patterns
are two ways to ask the same question. Keeping them in separate inputs made them feel like competing features and
crowded the dialog's top. One `<input>` plus a mode-chip row mirrors Spotlight and Raycast, halves the visual weight,
and lets `⌘1` / `⌘2` / `⌘3` and the placeholder copy carry the mode discriminator. The state-shape collapse (`aiPrompt`
and `namePattern` gone; one `query` plus `mode`) is a permanent simplification, not a transient M2 refactor.

**Decision**: Filter chips with popovers instead of inline labelled controls. **Why**: The previous filter row was
form-shaped (label + select + value), three rows of it competing with the search bar and the results. Chips are calmer
(default = name only, configured = "Size > 100 MB ×"), extensible (the trailing "+ Add filter" chip is the affordance
for new filters), and keyboard-first (Tab cycles chips; Enter opens the popover; Esc closes only the popover via the
capture-phase guard documented above). The popover surface is the right place for the dense single-filter UI that
doesn't deserve permanent screen real estate.

**Decision**: `MAX_HISTORY_PER_TAB = 100`. **Why**: Not search-specific, but landed in this redesign because the
snapshot store needs an authoritative eviction signal. The cap applies to every volume (local, network, MTP,
search-results) uniformly. 100 is enough for power users who navigate deeply and use `⌘[` for orientation; tightening
below would start to hurt them. The cap is enforced inside `navigation-history.ts::push()`, which returns the dropped
entries so callers (the tab-state manager) can release per-entry resources in one step.

**Decision**: AI mode example chips re-run on click. **Why**: AI mode's "explicit user trigger" rule counts a click as a
trigger. The same applies to recent-search AI entries (footer chip click + popover Enter both run). Anything the user
deliberately picks from the dialog is the same kind of "yes, please" as pressing Enter.

**Decision**: `RecentItemsPopover` reuses `FilterChipPopover` for positioning + focus trap + Esc-scoped close. **Why**:
The plan calls for a sub-overlay-of-an-overlay with the same auto-flip, focus-trap, and "Esc closes only the popover"
semantics as the filter chips. Reimplementing those would risk drift; reusing the primitive guarantees the contract
covers both popover kinds via the single `.filter-chip-popover` DOM selector.

**Decision**: Pattern chip always rendered (search-fixup clarification 5). **Why**: After moving the AI bar to keep the
natural-language prompt visible, the AI's produced pattern needed a visible home in the dialog. We use the same chip
primitive for all three modes for consistency: in filename / regex mode the chip reads from the bar, and in AI mode it
reads from `lastAiPattern`. Clicking × clears the pattern only; the AI transparency strip stays put.

**Decision**: Path pills inside result rows are mouse-only and not in the keyboard Tab order. **Why**: Making the pills
tabbable inside virtualized rows would break the row's arrow-down keyboard flow: pressing Down at the end of a row would
land on the next row's first pill instead of the next row's primary cell. Keyboard users navigate the list with arrow
keys (cursor row is the keyboard target) and reach the same operations via `⌥←` / `⌥→`. Axe's `nested-interactive` rule
still flags the structural nesting on the populated-results audit; we disable that one rule explicitly with a comment
pointing here.

**Decision**: AI mode never auto-applies; only Enter / `⌘Enter` / the ⏎ button / chip clicks fire it. **Why**: AI calls
cost money (cloud) or RAM + latency (local). Even a fast model has a per-call cost the user should opt into. Filename
and regex modes auto-apply behind the `search.autoApply` setting (default on, 1,000 ms debounce). The split lives in
`scheduleSearch()`'s early-return chain (mode, setting, IME composition).

## Shared gotchas

**Gotcha**: `stopPropagation()` on every `keydown`. **Why**: Without this, keys propagate to the file explorer behind
the dialog and trigger quick-search or navigation.

**Gotcha**: Don't call the dialog's clear hook from `onDestroy`. **Why**: The dialog's lifecycle (mount on open, unmount
on close) doesn't match the user's mental model of "the search I was working on." Wiping state on unmount turned every
close + reopen into a lost-work moment. The only sanctioned reset path is `⌘N`. If you find yourself wanting to wipe
state from a lifecycle hook, you probably want a user-initiated action instead.

**Gotcha**: status bar stays empty whenever the content area is showing a state message (Searching, No files match,
Loading drive index). The rule: content is the source of truth; duplicating the same string in the status bar reads as
broken. When you add a new content-area state in `QueryResults.svelte`, make sure `getStatusText()` returns `''` for
that state.

**Gotcha**: ⌘⏎ and ⇧⏎ are explicit no-ops in the dialog (R4). The earlier "⌘Enter runs AI" shortcut is gone; bare Enter
is the only key that runs a search or opens the cursor row (dispatched via `enterAction` per D8). The dialog's
`handleModifierShortcuts` swallows both modifier combinations with `preventDefault` so the bare-Enter handler never sees
a modified Enter.

**Gotcha**: The AI's translation overwrites `query` and `mode`. **Why**: We want the bar to show what was searched, not
the natural-language prompt. The original prompt is preserved separately in `lastAiPrompt` (set by `executeAiSearch`
before the IPC call) so the `AiPromptStrip` can render it. Anyone building on top of this should not assume `query`
still contains the user's natural-language input after an AI run; use `getLastAiPrompt()` instead.

**Gotcha**: `nested-interactive` axe rule is explicitly disabled on the populated-results a11y test. **Why**: The row
gains interactive children (path-pill buttons + the `…` menu button) inside the `role="option"` row. Tab order is
suppressed via `tabindex="-1"` per spec, but axe still flags the structural nesting. Cleanly fixing it means either
dropping the row's `role="option"` (and surfacing the cursor via a custom mechanism) or hoisting the buttons out of the
row's grid cell — both are out of redesign scope.

## Dependencies

- `$lib/ui/ToggleGroup.svelte` — segmented-control primitive used by `ModeChips`
- `$lib/tauri-commands` — `getRecentSearches` (Search) / future `getRecentSelections` (Selection) via the recent-items
  factory; `showFileContextMenu` (row context menu)
- `@leeoniya/ufuzzy` — fuzzy filtering inside `RecentItemsPopover`
- `$lib/settings` — `getSetting('ai.provider')` (AI chip visibility, ⌘ shortcut numbering)
- `$lib/tooltip/tooltip` — chip tooltips (Content chip's "Coming soon" copy, recent-items chip tooltips)
- CSS variables from `app.css` (`--z-modal`, `--color-accent-subtle`, `--color-bg-secondary`, etc.)
