# Query UI (shared filter-and-act-on primitives)

Primitives shared between the Search dialog (`lib/search/`) and the Selection dialog (`lib/selection-dialog/`): the
unified query bar, mode chips, AI prompt strip, filter chips strip, virtualized results table, recent-items footer +
popover, and the `createQueryFilterState()` factory. Filter-chip internals live in
[`filter-chips/CLAUDE.md`](filter-chips/CLAUDE.md). Consumer-specific decisions live in
[`lib/search/CLAUDE.md`](../search/CLAUDE.md) and [`lib/selection-dialog/CLAUDE.md`](../selection-dialog/CLAUDE.md).

## Module map

- `QueryDialog.svelte` + `query-dialog-config.ts`: shared orchestrator (overlay, keyboard contract, IME guard,
  auto-apply gates, `lastDialogEvent` ownership), driven by a single `QueryDialogConfig` prop per consumer.
- `QueryBar`, `ModeChips`, `AiPromptStrip`, `QueryResults`, `EmptyState`, `PathPills`, `SearchRowMenu`,
  `recent-items/*`: UI pieces. Pure helpers: `enter-action.ts`, `path-pills-layout.ts`, `recent-chips-layout.ts`.
- `query-filter-state.svelte.ts`: factory owning the cross-consumer state. `filter-chips/`: chip subsystem.
- `apply-ai-filters.ts`: shared `applySizeFromAi` / `applyDateFromAi` / `applyTypeFromAi` over a `QueryFilterState` (one
  place for the AI-result-to-chip mapping). `ai-summary.ts`: pure `buildAiSummary()` → the `AiPromptStrip`'s
  human-readable mirror (a MIRROR; the live chips stay the editable truth).

## Must-knows

- **Three pieces of state are QueryDialog's alone; consumer callbacks MUST NOT write them:** `state.lastDialogEvent`
  (drives `deriveEnterAction` + the `⏎` swap), `state.lastAiPrompt` / `lastAiCaveat` (orchestrator sets these before
  `translateAi`), and `state.results` / `totalCount` / `cursorIndex` (set after `runQuery` resolves). `runQuery` returns
  `{ entries, totalCount }`; `translateAi` returns `{ caveat, highlightedFields }`. Writing these from a consumer breaks
  the ⏎ ownership swap. Full ownership table in DETAILS.md § Ownership contracts.
- **AI translation errors surface once, in QueryDialog, for both consumers.** The consumer's `translateAi` must let the
  typed `AiTranslateError` throw (don't swallow it). QueryDialog catches and calls `showAiTranslateErrorToast`. A `null`
  return means a benign empty translation, distinct from a throw. Don't re-add a per-consumer catch.
- **`createQueryFilterState()` owns ONLY cross-consumer fields.** When adding a field, ask "would Selection care?" Yes →
  core factory. No → the consumer's extras module (`createSearchExtrasState()` etc.). Don't share via the core when
  semantics diverge (`lastAiLabel` is the textbook "no").
- **`typeFilter: 'both' | 'file' | 'folder'`** (core, default `'both'`) maps to the existing IPC
  `SearchQuery.isDirectory: Option<bool>` (`both → null`, `file → false`, `folder → true`): no new IPC field, no engine
  change, no schema bump (`HistoryFilters.isDirectory` is additive `#[serde(default)]`). Mapping detail in DETAILS.md §
  State shape.
- **`recordAiTranslation` (core) writes ONLY `handTyped[mode]`.** The Search-only label/pattern slots live in the extras
  and are written separately. Don't fold them into the core method.
- **`stopPropagation()` on every dialog `keydown`** (shields the file explorer behind it; without it, keys trigger
  quick-search/nav). All `use:trapFocus` listeners run in the capture phase so this can't starve the trap.
- **Don't wipe state from `onDestroy` / any lifecycle hook.** The dialog mounts on open and unmounts on close; state
  survives unmount by design. The ONLY sanctioned reset is `⌘N` (the consumer's clear hook). Wiping on unmount turns
  every close+reopen into lost work.
- **Reopen re-derives results so they show immediately, not the empty state.** `hasSearched` (component-local) is seeded
  from `getLastRunQuery() !== null` so persisted results render on mount. A restored NON-AI session (prior run +
  `hasRestorableQuery()`) sets `runOnMount` to re-run on reopen. AI restored sessions must NOT re-run (cloud cost): the
  `onMount` gate excludes `mode === 'ai'`, so the seeded `hasSearched` renders the persisted results without re-calling
  translate. Don't loosen the `mode !== 'ai'` gate. Full lifecycle (cold-open / hot-prefill / index-not-ready) in
  DETAILS.md § `runOnMount` consumer.
- **⌘⏎ and ⇧⏎ are explicit no-ops** (`preventDefault`); bare Enter is the only key that runs a search or opens the
  cursor row, via `enterAction`. `⌘N` is captured before the dialog's `stopPropagation` so it doesn't reach the
  route-level new-tab handler.
- **Path pills are mouse-only, `tabindex="-1"`, with no keyboard equivalent**; making them tabbable breaks the row's
  arrow-down flow. `⌥←` / `⌥→` are deliberately left native (move-by-word in the focused query input), NOT bound to pill
  nav: don't re-add an `⌥`+arrow folder-nav (DETAILS.md § Path pills). The `nested-interactive` axe rule is deliberately
  disabled on the populated-results a11y test; don't "fix" it by retabbing.
- **Status bar stays empty whenever the content area shows a state message** (Searching / No files match / Loading):
  make `getStatusText()` return `''` for any new content-area state, or it reads as broken.
- **`.results-container` carries `role="listbox"` ONLY when option rows actually render** (the `showingRows` derived,
  not `results.length > 0` alone): on reopen the dialog re-runs with persisted `results` set while the spinner shows, so
  a length-only gate yields `role="listbox"` with no `option` children = axe `aria-required-children` (critical). Pinned
  by the "searching with stale results" test in `QueryResults.a11y.test.ts`.
- **Content chip is visible-disabled with NO shortcut** (`⌘4` reserved): when Content ships it claims `⌘3` and Regex
  moves to `⌘4`.
- **AI mode never auto-applies** (cost); filename/regex auto-apply behind `search.autoApply` (default on, 1,000 ms
  debounce, IME-gated), in `scheduleSearch()`'s early-return chain.
- **The AI translation overwrites `query` + `mode`.** Use `getLastAiPrompt()` for the original prompt; don't assume
  `query` still holds natural-language input after an AI run.
- **The `AiPromptStrip` is a human-readable MIRROR, never the source of truth.** `buildAiSummary()` (`ai-summary.ts`)
  renders the current chip state into the strip's "Here's what the agent did:" lines; the live chips stay editable. Its
  first-person agent voice is a SANCTIONED exception to the no-first-person copy rule (alongside onboarding / About).
- **The spinner covers the AI translate round-trip.** `runAiSearch` sets `isSearching` true BEFORE the cloud translate
  and leaves it on through `executeQuery` (cleared in `finally`; early-returns reset it so it can't stick). Detail in
  DETAILS.md § Shared UI behavior.
- **Type-in-AI is leave-alone-if-null; size/date are reset-first. Don't "consistency-fix" this.** Each AI run resets
  `sizeFilter` / `dateFilter` to `'any'` before applying the translation (the helpers no-op on a null bound, so without
  the reset a prior run's filter leaks). Type is the deliberate asymmetry: `applyTypeFromAi` writes only on a non-null
  `isDirectory`, so the AI staying silent keeps the user's choice. Callers must NOT pre-reset `typeFilter`. Full
  contract in `apply-ai-filters.ts`.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
