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
- `apply-ai-filters.ts`: shared `applySizeFromAi` / `applyDateFromAi` / `applyTypeFromAi` over a `QueryFilterState`;
  both wrappers call them, so the AI-result-to-chip mapping is in one place.
- `ai-summary.ts`: pure `buildAiSummary()` → the `AiPromptStrip`'s human-readable mirror of the produced pattern +
  Size/Modified/Type. The strip is a MIRROR; the live chips stay the editable truth.

## Must-knows

- **Three pieces of state are QueryDialog's alone; consumer callbacks MUST NOT write them:** (1) `state.lastDialogEvent`
  (drives `deriveEnterAction` + the `⏎` swap), (2) `state.lastAiPrompt` / `lastAiCaveat` (orchestrator sets the prompt
  before `translateAi`), (3) `state.results` / `totalCount` / `cursorIndex` (set after `runQuery` resolves). `runQuery`
  returns `{ entries, totalCount }`; `translateAi` returns `{ caveat, highlightedFields }`. Writing these from a
  consumer breaks the ⏎ ownership swap.
- **AI translation errors surface once, in QueryDialog, for both consumers.** The consumer's `translateAi` must let the
  typed `AiTranslateError` throw (don't swallow it). QueryDialog catches and calls `showAiTranslateErrorToast`. A `null`
  return means a benign empty translation, distinct from a throw. Don't re-add a per-consumer catch.
- **`createQueryFilterState()` owns ONLY cross-consumer fields.** When adding a field, ask "would Selection care?" Yes →
  core factory. No → the consumer's extras module (`createSearchExtrasState()` etc.). Don't share via the core when
  semantics diverge (`lastAiLabel` is the textbook "no").
- **`typeFilter: 'both' | 'file' | 'folder'`** (core, default `'both'`) maps to the existing IPC
  `SearchQuery.isDirectory: Option<bool>` in `buildBaseSearchQuery` (`both → null`, `file → false`, `folder → true`): no
  new IPC field, no engine change. Selection's matcher reads it via `getIsDirFor`; it round-trips as
  `HistoryFilters.isDirectory` (additive `#[serde(default)]`, no schema bump).
- **`recordAiTranslation` (core) writes ONLY `handTyped[mode]`.** The Search-only label/pattern slots live in the extras
  and are written separately. Don't fold them into the core method.
- **`stopPropagation()` on every dialog `keydown`** (shields the file explorer behind it; without it, keys trigger
  quick-search/nav). All `use:trapFocus` listeners run in the capture phase so this can't starve the trap.
- **Don't wipe state from `onDestroy` / any lifecycle hook.** The dialog mounts on open and unmounts on close; state
  survives unmount by design. The ONLY sanctioned reset is `⌘N` (the consumer's clear hook). Wiping on unmount turns
  every close+reopen into lost work.
- **⌘⏎ and ⇧⏎ are explicit no-ops** (swallowed with `preventDefault`); bare Enter is the only key that runs a search or
  opens the cursor row, dispatched via `enterAction`. `⌘N` is captured before the dialog's `stopPropagation` so it
  doesn't reach the route-level new-tab handler.
- **Path pills are mouse-only, `tabindex="-1"`** (keyboard equivalents `⌥←` / `⌥→`); making them tabbable breaks the
  row's arrow-down flow in the virtualized list. The `nested-interactive` axe rule is deliberately disabled on the
  populated-results a11y test with a comment pointing at the rationale; don't "fix" it by retabbing.
- **Status bar stays empty whenever the content area shows a state message** (Searching / No files match / Loading).
  When you add a content-area state in `QueryResults`, make `getStatusText()` return `''` for it, or it reads as broken.
- **Content chip is visible-disabled with NO shortcut** (`⌘4` reserved): wiring a shortcut to a disabled control is
  hostile UX. When Content ships it claims `⌘3` and Regex moves to `⌘4`.
- **AI mode never auto-applies** (cost); filename/regex auto-apply behind `search.autoApply` (default on, 1,000 ms
  debounce), gated also by IME composition. The split lives in `scheduleSearch()`'s early-return chain.
- **The AI translation overwrites `query` + `mode`.** The original prompt lives in `lastAiPrompt`; use
  `getLastAiPrompt()`, don't assume `query` still holds natural-language input after an AI run.
- **The `AiPromptStrip` is a human-readable MIRROR, never the source of truth.** `buildAiSummary()` (`ai-summary.ts`)
  turns the current chip state (pattern + Size/Modified/Type) into the strip's "Here's what the agent did:" lines; the
  live chips stay the editable representation. The first-person agent voice is a SANCTIONED exception to the
  no-first-person copy rule (David-decided), alongside onboarding / About.
- **The spinner covers the AI translate round-trip.** `runAiSearch` sets `isSearching` true BEFORE the cloud translate
  (the slow part) and leaves it on through `executeQuery` (which clears it in `finally`); its early-returns reset it so
  it can't stick. `QueryResults` shows `.spinner` off `isSearching`, and `getStatusText()` returns `''` for it. The
  global `.spinner` honors `prefers-reduced-motion` in `app.css`.
- **Type-in-AI is leave-alone-if-null; size/date are reset-first. Don't "consistency-fix" this.** The AI RECEIVES the
  current type as context (`translateSearchQuery` / `translateSelectionQuery` take a `currentType` arg, via
  `typeFilterToIsDirectory`) and may set or stay silent. `applyTypeFromAi` writes only on a non-null `isDirectory`; a
  `null` keeps the user's choice. Unlike `applySizeFromAi` / `applyDateFromAi` (callers reset to `any` first), callers
  must NOT pre-reset `typeFilter`. The `'type'` highlight flashes the toggle via a wrapper span (`ToggleGroup` has no
  `highlighted` prop).

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
