# Filter chips (size, modified, scope, pattern)

The chip strip below the mode-chip row inside the shared `QueryDialog`. It leads with a one-click
`Both | Files | Folders` type toggle, then surfaces Pattern, Size, Modified, and Search in as chips that open popovers
with the dense controls (all filters always visible, no "+ Add filter"). Owned by the shared query UI; consumed by both
Search and Selection. See [`../CLAUDE.md`](../CLAUDE.md) for the orchestrator and cross-consumer state factory.

## Module map

- `FilterChips.svelte`: the strip. Leading type `ToggleGroup`, then the four chips. Owns open-chip state and the three
  keyboard routers (popover openers, scope shortcuts, clear keys).
- `SizeFilterPopover.svelte` / `DateFilterPopover.svelte` / `ScopeFilterPopover.svelte`: the popover bodies, each
  wrapping `$lib/ui/FilterPopover` (a `$lib/ui/Popover` + labelled header). The chips are `$lib/ui/Chip`
  (`variant="filter"`), mounted directly. Shared popover CSS in `filter-popover.css`.
- `filter-chip-state.ts` / `filter-popover-helpers.ts`: pure helpers (chip summaries, size/date presets, comparator
  gating, `buildDatePresets`), testable without mounting Svelte.

## Must-knows

- **Chip-popover focus contract: Esc inside an open filter-chip popover closes only the popover.** The dialog's Escape
  handler runs in capture phase on `window`, so it would otherwise fire before the popover's bubble handler. The dialog
  checks `dialogElement.querySelector('.ui-popover')` (the class `Popover` renders) and, when a popover is present,
  returns without closing the dialog. The popover's own keydown handler then runs on the bubble, closes itself, and
  calls `stopPropagation` so nothing else fires. Without this guard, Escape inside a popover closes the whole dialog and
  loses the user's place. Pinned in `FilterChips.svelte.test.ts`.
- **The type filter is a `ToggleGroup`, not a chip.** It binds the core `typeFilter` state directly (cross-consumer, so
  both dialogs show it) and maps to the existing IPC `isDirectory`. Don't reshape it into a chip+popover.
- **`=` (the `eq` size comparator) is a UI/chip-summary concern ONLY, never reaching the matcher's `SizePredicate` or
  any Rust type.** Below the chip it's `between` with `sizeMin == sizeMax`: `applySizeQuery` pins both bounds,
  `readSizeFilters` emits `{ sizeMin: x, sizeMax: x }`, and `applyHistoryFilters` rehydrates a stored
  `size_min == size_max` as `eq` (not `between`) by deliberate decision (the two are identical; `= x` is the friendlier
  label, so a stored `between 5–5` returns as `= 5`). `applySizeFromAi` sets `eq` when the AI returns `min == max`.
  Don't add an `eq` kind to `SizePredicate` / `HistoryFilters`.
- **`parseSizeToBytes('0', unit)` is `0`, not `undefined`.** The grid lets the user explicitly pick 0 as a lower or
  upper bound, so the helper honors it. `deriveSizeChip` likewise treats a `0` bound as configured (the guard is `>= 0`,
  not `> 0`); an empty input stays unconfigured because `parseFloat('')` is `NaN`. So "= 0 B" / "≥ 0 B" render as real
  filters.
- **Size unit is `'B' | 'KB' | 'MB' | 'GB'`.** The "byte(s)" cell is selectable from the unit column manually; the AI
  translator's `bytesToDisplaySize` still produces `KB | MB | GB`.
- **macOS Option-key shortcuts match on `event.code`, not `event.key`.** The Option key remaps `event.key` to
  typographic glyphs (Option+S → `ß`, Option+M → `µ`), so the `⌥S` / `⌥M` / `⌥I` popover openers match on `KeyS` /
  `KeyM` / `KeyI` first and fall back to `event.key` for synthesized test events. `⌥I` (Search in) is Search-only;
  Selection passes `scopeChipVisible: false` and suppresses it.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
