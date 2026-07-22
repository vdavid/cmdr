# Query UI (shared filter-and-act-on primitives)

Primitives shared between the Search dialog (`lib/search/`) and the Selection dialog (`lib/selection-dialog/`); the
module map below lists the pieces. Filter-chip internals: `filter-chips/CLAUDE.md`. Consumer-specific decisions:
`../search/CLAUDE.md`, `../selection-dialog/CLAUDE.md`.

## Module map

- `QueryDialog.svelte` + `query-dialog-config.ts`: shared orchestrator (overlay, keyboard contract, IME guard,
  auto-apply gates, `lastDialogEvent` ownership), driven by one `QueryDialogConfig` prop per consumer.
- UI pieces (`QueryBar`, `ModeChips`, `AiPromptStrip`, `QueryResults`, `EmptyState`, `PathPills`, `SearchRowMenu`,
  `recent-items/*`), pure helpers, the `query-filter-state.svelte.ts` cross-consumer factory, `apply-ai-filters.ts`
  (`applySizeFromAi` / `applyDateFromAi` / `applyTypeFromAi`), `ai-summary.ts` (the `AiPromptStrip` mirror), and the
  `filter-chips/` subsystem. Per-file catalog: DETAILS.md § Files.

## Must-knows

- **Three state fields are QueryDialog's alone; consumer callbacks MUST NOT write them:** `lastDialogEvent` (drives the
  `⏎` swap), `lastAiPrompt` / `lastAiCaveat`, and `results` / `totalCount` / `cursorIndex`. `runQuery` returns
  `{ entries, totalCount }` and `translateAi` returns `{ caveat, highlightedFields }` only. Contract: DETAILS.md §
  Ownership contracts.
- **AI translation errors surface once, in QueryDialog.** The consumer's `translateAi` must let the typed
  `AiTranslateError` throw; QueryDialog catches it and toasts. A `null` return is a benign empty translation, not an
  error. Don't re-add a per-consumer catch.
- **`createQueryFilterState()` owns ONLY cross-consumer fields.** Adding a field, ask "would Selection care?" Yes → core
  factory; no → the consumer's extras module (`createSearchExtrasState()` etc.). `lastAiLabel` is the textbook "no".
- **`recordAiTranslation` (core) writes ONLY `handTyped[mode]`.** Search-only label/pattern slots live in the extras,
  written separately; don't fold them into the core method.
- **`stopPropagation()` on every dialog `keydown`** (shields the explorer behind it; otherwise keys trigger
  quick-search/nav). All `use:trapFocus` listeners run in the capture phase so this can't starve the trap.
- **Don't wipe state from `onDestroy` / any lifecycle hook.** State survives unmount by design; the ONLY sanctioned
  reset is `⌘N`. Wiping on unmount turns every close+reopen into lost work.
- **Reopen re-derives results, not the empty state.** A restored NON-AI session sets `runOnMount` to re-run; AI restored
  sessions must NOT re-run (cloud cost). Don't loosen the `mode !== 'ai'` gate. Lifecycle: DETAILS.md § `runOnMount`
  consumer.
- **Path pills are mouse-only, `tabindex="-1"`, no keyboard equivalent**; making them tabbable breaks the row's
  arrow-down flow. `⌥←` / `⌥→` stay native move-by-word, NOT pill nav. The `nested-interactive` axe rule is deliberately
  disabled on the populated-results a11y test; don't "fix" it by retabbing.
- **Status bar stays empty while the content area shows a state message** (Searching / No files match / Loading):
  `getStatusText()` must return `''` for any new content-area state, or it reads as broken.
- **`.results-container` carries `role="listbox"` ONLY when option rows actually render** (the `showingRows` derived,
  not `results.length > 0`): on reopen, persisted `results` are set while the spinner shows, so a length-only gate
  yields `role="listbox"` with no `option` children = axe `aria-required-children` (critical). Pinned by
  `QueryResults.a11y.test.ts`.
- **AI mode never auto-applies** (cost); filename/regex auto-apply behind `search.autoApply` (default on, 1,000 ms
  debounce, IME-gated), in `scheduleSearch()`'s early-return chain.
- **AI translation overwrites `query` + `mode`.** Use `getLastAiPrompt()` for the original prompt; don't assume `query`
  still holds natural-language input after an AI run.
- **The `AiPromptStrip` is a human-readable MIRROR, never the source of truth** (`buildAiSummary()` renders chip state;
  live chips stay editable). Its first-person agent voice is a SANCTIONED exception to the no-first-person copy rule
  (alongside onboarding / About).
- **Type-in-AI is leave-alone-if-null; size/date are reset-first. Don't "consistency-fix" this.** Each AI run resets
  `sizeFilter` / `dateFilter` to `'any'` before applying, or a prior run's filter leaks. `applyTypeFromAi` writes only
  on non-null `isDirectory`, so AI silence keeps the user's choice; callers must NOT pre-reset `typeFilter`. Contract in
  `apply-ai-filters.ts`.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
