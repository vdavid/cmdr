# Filter chips (size, modified, scope, pattern)

The chip strip that lives below the mode-chip row inside the shared `QueryDialog`. Each chip surfaces one filter
dimension (Size, Modified, Search in, Pattern) and opens a popover with the dense controls. Owned by the shared query
UI; consumed by both Search and Selection through the same `QueryDialog` orchestrator. See
[`../CLAUDE.md`](../CLAUDE.md) for the orchestrator, the unified bar, the results table, and the cross-consumer state
factory.

## Files

| File                        | Purpose                                                                                                                                                                                                                                                                                  |
| --------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `FilterChips.svelte`        | Filter chip strip (Pattern + Size + Modified + Search in) plus Add filter dropdown. Owns the open-chip state and the three keyboard routers. Visibility flags: `scopeChipVisible`, `patternChipVisible`                                                                                  |
| `SizeFilterPopover.svelte`  | Size popover body: the comparator + value + unit list grid, the lower/upper custom-input flags, and the `pickSize*` auto-promote handlers                                                                                                                                                |
| `DateFilterPopover.svelte`  | Modified popover body: the comparator + dynamic-preset grid, the `buildDatePresets`-derived list + first-match selection keys, custom-input flags, `pickDate*` handlers                                                                                                                  |
| `ScopeFilterPopover.svelte` | Search-in popover body: scope textarea, "Hide boring folders" / "Case-sensitive" toggles, and the ⌥C / ⌥V footer buttons                                                                                                                                                                 |
| `filter-popover.css`        | Shared global styles for the popover bodies: `.popover-section`, `.popover-label`, the `.list-grid` / `.list-cell` / `.list-col` grid, and `.popover-input`. Imported by all three popover components (Svelte `<style>` is component-scoped, so shared classes need a global stylesheet) |
| `FilterChip.svelte`         | Single chip: default/configured states, `×` clear, Backspace clear, aria-expanded                                                                                                                                                                                                        |
| `FilterChipPopover.svelte`  | Generic popover shell: frosted-glass, auto-flip, focus trap, Esc closes without disrupting dialog. Wrapped by each `*FilterPopover` body and used directly for the Add filter menu                                                                                                       |
| `filter-chip-state.ts`      | Pure helpers: `deriveSizeChip`, `deriveDateChip`, `deriveScopeChip`, `derivePatternChip` (testable in isolation)                                                                                                                                                                         |
| `filter-popover-helpers.ts` | Pure: `SIZE_PRESETS`, `byteUnitLabel`, `kiloByteLabel`, `isSizeRangeDisabled`, `showsUpperBound`, `isDateRangeDisabled`, `showsDateUpperBound`, `buildDatePresets`                                                                                                                       |

Companion tests (colocated):

| Test                               | Coverage                                                                                                                              |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------- |
| `FilterChips.svelte.test.ts`       | Chip rendering, `×` and Backspace clear, popover open/close, Add filter list, scope behavior, ⌥S/⌥M/⌥I openers, ⌥C/⌥V scope shortcuts |
| `FilterChips.a11y.test.ts`         | Tier-3 axe-core audit across populated chip states                                                                                    |
| `FilterChip.a11y.test.ts`          | Tier-3 axe-core audit across default, configured, disabled, and open states                                                           |
| `FilterChipPopover.svelte.test.ts` | Mount / unmount via `open` prop, Esc → onClose with stopPropagation                                                                   |
| `FilterChipPopover.a11y.test.ts`   | Tier-3 axe-core audit for the popover surface                                                                                         |
| `SizeFilterPopover.a11y.test.ts`   | Tier-3 axe-core audit: closed + open in `between` mode (all columns)                                                                  |
| `DateFilterPopover.svelte.test.ts` | Preset click auto-promote, Custom… cell flow, comparator click, upper-bound column gating                                             |
| `DateFilterPopover.a11y.test.ts`   | Tier-3 axe-core audit: closed, preset mode, and custom-bounds mode (`nested-interactive` disabled there — input inside Custom cell)   |
| `ScopeFilterPopover.a11y.test.ts`  | Tier-3 axe-core audit: closed + open with scope text and toggles                                                                      |
| `filter-chip-state.test.ts`        | Default → configured → cleared rules for each chip's display summary                                                                  |
| `filter-popover-helpers.test.ts`   | Size + date preset rules, comparator gating, dynamic Modified preset labels                                                           |

## Chip state shape

`deriveSizeChip` / `deriveDateChip` / `deriveScopeChip` / `derivePatternChip` each return:

- `configured: boolean` — whether the filter is currently constrained (anything other than `any` / empty).
- `summary: string` — the label shown when configured. Empty when not configured.

The chip component reads its default label (when not configured) from a static prop and the summary string (when
configured) from the derived state. Keeping the rules pure (no `$state` reads inside the helpers) lets
`filter-chip-state.test.ts` pin the table without mounting Svelte.

## Popover anatomy

The popover is a frosted-glass surface anchored to the chip. `FilterChipPopover.svelte` owns positioning, the focus
trap, and the close-on-Escape contract. It's also reused by `RecentItemsPopover.svelte` (see
[`../recent-items/`](../recent-items/)) for the same auto-flip + focus-trap + Esc-scoped close.

**Anatomy** (top to bottom):

1. Header strip with the filter name (Size / Modified / Search in / Pattern).
2. The grid of controls (cells render as `<button>` with `role="radio"` plus `aria-checked` — see "Grid-style popovers"
   below).
3. Optional inline custom input when the user selects `Custom…`.
4. Footer affordances for the Search-in popover (`⌥F` / `⌥D` buttons).

**Positioning**: `FilterChipPopover.svelte` measures its anchor (the chip) and the popover element, then auto-flips
above the chip if there's not enough room below. The flip decision runs once per open and on `ResizeObserver` ticks.

**Focus trap**: Tab and Shift+Tab cycle within the popover; focus returns to the chip on close.

## Chip-popover focus contract

**Esc inside an open filter-chip popover closes only the popover.** The dialog's Escape handler runs in capture phase on
`window`, which would otherwise fire before the popover's bubble handler. The dialog checks
`dialogElement.querySelector('.filter-chip-popover')` and, when a popover is present, returns without closing the
dialog. The popover's own keydown handler then runs on the bubble, closes itself, and calls `stopPropagation` so nothing
else fires. Without this guard, Escape inside a popover would close the whole dialog and lose the user's place. Pinned
in `FilterChips.svelte.test.ts`.

## Grid-style popovers

The Size and Modified popovers render as a multi-column list selector. Their bodies live in `SizeFilterPopover.svelte`
and `DateFilterPopover.svelte`; the shared grid CSS lives in `filter-popover.css`. Tested via
`filter-popover-helpers.test.ts` and `FilterChips.svelte.test.ts` (which mounts `FilterChips` and drives the real
popover children).

**Size popover** (`SizeFilterPopover.svelte`):

- Col 1: `any`, `≥`, `≤`, `between` (one selected at a time).
- Col 2: `0`, `1`, `5`, `10`, `20`, `50`, `100`, `200`, `500`, `Custom…`. Disabled when col 1 = `any`. Selecting
  `Custom…` reveals an inline `<input type="number">`.
- Col 3: unit. The "byte(s)" cell label flips based on the selected value. The "kB/KB" cell follows
  `appearance.fileSizeFormat` (SI → `kB`, binary → `KB`). `MB` and `GB` are constant.
- When col 1 = `between`: cols 4 + 5 mirror cols 2 + 3 for the upper bound.

**Modified popover** (`DateFilterPopover.svelte`):

- Col 1: `any`, `after`, `before`, `between`.
- Col 2: presets `today`, `yesterday`, `this week`, `last week`, `this month`, `last month`, `this year`, `Custom…`
  (Custom reveals `<input type="date">`).
- When col 1 = `between`: col 3 mirrors col 2 for the upper bound. No unit column.

**Cells are buttons**, not radios; they carry `role="radio"` plus `aria-checked` so AT users read the cell set as a
radio group while the click target stays generous. Disabled cells get `disabled={true}` rather than `aria-disabled`, so
the keyboard skip and the mouse not-allowed cursor are both correct without extra handling.

## Shortcut openers

`FilterChips.svelte::handleDialogPopoverOpener`:

- `⌥S` opens the Size popover.
- `⌥M` opens the Modified popover.
- `⌥I` opens the Search-in popover (Search only; Selection passes `scopeChipVisible: false` and the ⌥I shortcut is
  suppressed).

On macOS the Option key remaps `event.key` to typographic glyphs (Option+S → `ß`, Option+M → `µ`), so `altLetter()`
matches on `event.code` (`KeyS`, `KeyM`, …) first and falls back to `event.key` for synthesized test events. Same trick
lives in `SearchDialog.svelte::matchKey` for the mode-chip ⌥A / ⌥F / ⌥R shortcuts.

## Chip-side behavior

- `DateFilterPopover.svelte` keeps `dateIsCustomLower` / `dateIsCustomUpper` in sync via an `$effect` that flips them
  OFF when `dateValue` matches a preset (mirrors the size flow in `SizeFilterPopover.svelte`). The Modified popover
  never shows both a preset AND Custom as selected.
- A Modified preset cell lights up only when its `key` matches `selectedDateLowerKey` / `selectedDateUpperKey` (the key
  of the FIRST preset whose `resolved` date equals the bound), NOT a bare `dateValue === preset.resolved` compare. Two
  presets can resolve to the same ISO date (on a Sunday with a Sunday-first locale, "today" and "this Sunday" both land
  on today; on the 1st of a month, "today" and "1st of <month>" collide), so the bare compare would light up every
  colliding cell at once. The first-match key keeps exactly one cell selected.
- Size > Custom input lives INSIDE the Custom cell (one click selects + focuses).
- Modified presets are dynamic ("today 0:00", "1st of May 0:00", …) — see `filter-popover-helpers.ts::buildDatePresets`.
- Value + unit cells in the Size and Modified popovers stay clickable while comparator = `any`; they render with
  `.is-disabled-look` (dimmed) and clicking auto-promotes the comparator to `gte` / `after` plus applies the clicked
  value.

## Key decisions

**Decision**: Filter chips with popovers instead of inline labelled controls. **Why**: An earlier form-shaped filter row
(label + select + value) competed with the search bar and the results. Chips are calmer (default = name only, configured
= "Size > 100 MB ×"), extensible (the trailing "+ Add filter" chip is the affordance for new filters), and
keyboard-first (Tab cycles chips; Enter opens the popover; Esc closes only the popover via the capture-phase guard
documented above). The popover surface is the right place for the dense single-filter UI that doesn't deserve permanent
screen real estate.

**Decision**: Pattern chip always rendered. **Why**: After moving the AI bar to keep the natural-language prompt
visible, the AI's produced pattern needed a visible home in the dialog. The same chip primitive serves all three modes
for consistency: in filename / regex mode the chip reads from the bar, and in AI mode it reads from `lastAiPattern`.
Clicking × clears the pattern only; the AI transparency strip stays put.

## Gotchas

**Gotcha**: `parseSizeToBytes('0', unit)` is `0`, not `undefined`. The list-style grid lets the user explicitly pick 0
as a lower or upper bound, so the helper honors it.

**Gotcha**: Size unit is `'B' | 'KB' | 'MB' | 'GB'`. The "byte(s)" cell is selectable from the unit column manually; the
AI translator's `bytesToDisplaySize` still produces `KB | MB | GB`.

## Dependencies

- `../query-filter-state.svelte` — `QueryFilterState`, `SizeFilter`, `SizeUnit`, `DateFilter` types and setters
- `$lib/settings/reactive-settings.svelte` — `getFileSizeFormat()` for the `kB/KB` cell label
- `$lib/tooltip/tooltip` — chip tooltips
