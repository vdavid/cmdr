# Query UI (shared filter-and-act-on primitives)

Primitives shared between the Search dialog (`lib/search/`) and the Selection dialog (`lib/selection-dialog/`): the
unified query bar, mode chips, AI prompt strip, filter chips strip, virtualized results table, recent-items footer +
popover, and the `createQueryFilterState()` factory. Filter-chip internals:
[`filter-chips/CLAUDE.md`](filter-chips/CLAUDE.md). Consumer-specific decisions:
[`lib/search/CLAUDE.md`](../search/CLAUDE.md), [`lib/selection-dialog/CLAUDE.md`](../selection-dialog/CLAUDE.md).

## Module map

- `QueryDialog.svelte` + `query-dialog-config.ts`: shared orchestrator (overlay, keyboard contract, IME guard,
  auto-apply gates, `lastDialogEvent` ownership), driven by one `QueryDialogConfig` prop per consumer.
- UI pieces: `QueryBar`, `ModeChips`, `AiPromptStrip`, `QueryResults`, `EmptyState`, `PathPills`, `SearchRowMenu`,
  `recent-items/*`. Pure helpers: `enter-action.ts`, `path-pills-layout.ts`, `recent-chips-layout.ts`.
- `query-filter-state.svelte.ts`: factory owning cross-consumer state. `filter-chips/`: chip subsystem.
- `apply-ai-filters.ts`: shared `applySizeFromAi` / `applyDateFromAi` / `applyTypeFromAi`. `ai-summary.ts`: pure
  `buildAiSummary()` → the `AiPromptStrip` mirror.

## Must-knows

- **Three state fields are QueryDialog's alone; consumer callbacks MUST NOT write them:** `state.lastDialogEvent`
  (drives `deriveEnterAction` + the `⏎` swap), `state.lastAiPrompt` / `lastAiCaveat`, and `state.results` / `totalCount`
  / `cursorIndex`. Writing them from a consumer breaks the ⏎ ownership swap. `runQuery` returns
  `{ entries, totalCount }` and `translateAi` returns `{ caveat, highlightedFields }` only; neither touches state. Full
  table in DETAILS.md § Ownership contracts.
- **AI translation errors surface once, in QueryDialog, for both consumers.** The consumer's `translateAi` must let the
  typed `AiTranslateError` throw; QueryDialog catches it and calls `showAiTranslateErrorToast`. A `null` return is a
  benign empty translation, not an error. Don't re-add a per-consumer catch.
- **`createQueryFilterState()` owns ONLY cross-consumer fields.** Adding a field, ask "would Selection care?" Yes → core
  factory; no → the consumer's extras module (`createSearchExtrasState()` etc.). `lastAiLabel` is the textbook "no".
- **`typeFilter: 'both' | 'file' | 'folder'`** (core, default `'both'`) maps onto the existing IPC
  `SearchQuery.isDirectory: Option<bool>`: no new IPC field or schema change.
- **`recordAiTranslation` (core) writes ONLY `handTyped[mode]`.** The Search-only label/pattern slots live in the extras
  and are written separately. Don't fold them into the core method.
- **`stopPropagation()` on every dialog `keydown`** (shields the explorer behind it; otherwise keys trigger
  quick-search/nav). All `use:trapFocus` listeners run in the capture phase so this can't starve the trap.
- **Don't wipe state from `onDestroy` / any lifecycle hook.** State survives unmount by design (mount on open, unmount
  on close). The ONLY sanctioned reset is `⌘N`. Wiping on unmount turns every close+reopen into lost work.
- **Reopen re-derives results, not the empty state.** A restored NON-AI session sets `runOnMount` to re-run; AI restored
  sessions must NOT re-run (cloud cost). Don't loosen the `mode !== 'ai'` gate. Full lifecycle in DETAILS.md §
  `runOnMount` consumer.
- **⌘⏎ and ⇧⏎ are explicit no-ops** (`preventDefault`); bare Enter is the only key that runs a search or opens the
  cursor row, via `enterAction`. `⌘N` is captured before the dialog's `stopPropagation` so it doesn't reach the
  route-level new-tab handler.
- **Path pills are mouse-only, `tabindex="-1"`, no keyboard equivalent**; making them tabbable breaks the row's
  arrow-down flow. `⌥←` / `⌥→` stay native (move-by-word in the query input), NOT bound to pill nav. The
  `nested-interactive` axe rule is deliberately disabled on the populated-results a11y test; don't "fix" it by
  retabbing.
- **Status bar stays empty whenever the content area shows a state message** (Searching / No files match / Loading):
  make `getStatusText()` return `''` for any new content-area state, or it reads as broken.
- **`.results-container` carries `role="listbox"` ONLY when option rows actually render** (the `showingRows` derived,
  not `results.length > 0`): on reopen, persisted `results` are set while the spinner shows, so a length-only gate
  yields `role="listbox"` with no `option` children = axe `aria-required-children` (critical). Pinned by the "searching
  with stale results" test in `QueryResults.a11y.test.ts`.
- **Content chip is visible-disabled with NO shortcut** (`⌘4` reserved): when Content ships it claims `⌘3` and Regex
  moves to `⌘4`. Wiring a shortcut to a disabled control is hostile UX.
- **AI mode never auto-applies** (cost); filename/regex auto-apply behind `search.autoApply` (default on, 1,000 ms
  debounce, IME-gated), in `scheduleSearch()`'s early-return chain.
- **AI translation overwrites `query` + `mode`.** Use `getLastAiPrompt()` for the original prompt; don't assume `query`
  still holds natural-language input after an AI run.
- **The `AiPromptStrip` is a human-readable MIRROR, never the source of truth** (`buildAiSummary()` renders chip state;
  live chips stay editable). Its first-person agent voice is a SANCTIONED exception to the no-first-person copy rule
  (alongside onboarding / About).
- **Type-in-AI is leave-alone-if-null; size/date are reset-first. Don't "consistency-fix" this.** Each AI run resets
  `sizeFilter` / `dateFilter` to `'any'` before applying (helpers no-op on a null bound, so without the reset a prior
  run's filter leaks). Type is the deliberate asymmetry: `applyTypeFromAi` writes only on a non-null `isDirectory`, so
  AI silence keeps the user's choice. Callers must NOT pre-reset `typeFilter`. Full contract in `apply-ai-filters.ts`.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
