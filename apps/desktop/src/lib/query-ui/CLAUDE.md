# Query UI (shared filter-and-act-on primitives)

Primitives shared between the Search dialog (`lib/search/`) and the Selection dialog (`lib/selection-dialog/`).
Filter-chip internals: `filter-chips/CLAUDE.md`. Consumer-specific decisions: `../search/CLAUDE.md`,
`../selection-dialog/CLAUDE.md`.

## Module map

- `QueryDialog.svelte` + `query-dialog-config.ts`: shared orchestrator (a `ModalDialog`, keyboard contract, IME guard,
  auto-apply gates, `lastDialogEvent` ownership), driven by one `QueryDialogConfig` prop per consumer.
- UI pieces (`QueryBar`, `ModeChips`, `AiPromptStrip`, `QueryResults`, `EmptyState`, `PathPills`, `SearchRowMenu`,
  `recent-items/*`), pure helpers, the `query-filter-state.svelte.ts` cross-consumer factory, `apply-ai-filters.ts`,
  `ai-summary.ts` (the `AiPromptStrip` mirror), and the `filter-chips/` subsystem. Consumer-subset differences and the
  `recent-items` adapter shape: DETAILS.md § Files.

## Must-knows

- **Three state fields are QueryDialog's alone; consumer callbacks MUST NOT write them:** `lastDialogEvent` (drives the
  `⏎` swap), `lastAiPrompt` / `lastAiCaveat`, and `results` / `totalCount` / `cursorIndex`. Callbacks return
  `{ entries, totalCount }` / `{ caveat, highlightedFields }` only. DETAILS.md § Ownership contracts.
- **AI translation errors surface once, in QueryDialog.** `translateAi` must let the typed `AiTranslateError` throw;
  a `null` return is a benign empty translation. Don't re-add a per-consumer catch.
- **`createQueryFilterState()` owns ONLY cross-consumer fields.** Adding a field, ask "would Selection care?" Yes → core
  factory; no → the consumer's extras module (`createSearchExtrasState()` etc.). `lastAiLabel` is the textbook "no".
  Relatedly, `recordAiTranslation` (core) writes ONLY `handTyped[mode]`; the Search-only label/pattern slots live in the
  extras and are written separately.
- **`stopPropagation()` on every dialog `keydown`** (shields the explorer behind it; otherwise keys trigger
  quick-search/nav). All `use:trapFocus` listeners run in the capture phase so this can't starve the trap.
- **All chrome is `ModalDialog`'s; never re-add it here.** Opt-ins: `align="top"`, `fillBody`, `padded={false}`,
  `ownsKeyboard` (we own Enter + the popover-aware Escape), `closeOnOverlayClick`, `overlayClass="search-overlay"` (the
  E2E hook). DETAILS.md § Chrome.
- **Two silent-failure traps.** Turning count-only OFF must re-run (`runFromButton()`, never `scheduleSearch()`) or the
  stale count stays; and never swallow a `runQuery` rejection — `executeQuery` toasts the backend's own message, else a
  refused run reads as "nothing matched".
- **Don't wipe state from any lifecycle hook.** State survives unmount by design; the ONLY sanctioned reset is `⌘N`.
  Wiping on unmount turns every close+reopen into lost work.
- **Reopen re-derives results, not the empty state.** A restored NON-AI session sets `runOnMount` to re-run; AI sessions
  must NOT (cloud cost). Don't loosen the `mode !== 'ai'` gate. DETAILS.md § `runOnMount` consumer.
- **Path pills are mouse-only, `tabindex="-1"`**: tabbable pills break the row's arrow-down flow, and `⌥←` / `⌥→` stay
  native move-by-word. The `nested-interactive` axe rule is deliberately disabled on the populated-results test; don't
  "fix" it by retabbing.
- **Status bar stays empty while the content area shows a state message** (Searching / No files match / Loading):
  `getStatusText()` returns `''` for any new content-area state, or it reads as broken.
- **`showingRows` gates two things**: `.results-container`'s `role="listbox"` (a `results.length > 0` gate trips axe
  `aria-required-children` on reopen) and the column header (labels over a spinner or a bare count describe a table
  that isn't there). Pinned by `QueryResults.a11y.test.ts` + `QueryResults.states.svelte.test.ts`.
- **AI mode never auto-applies** (cost); filename/regex auto-apply behind `search.autoApply` (default on, 1,000 ms
  debounce, IME-gated), in `scheduleSearch()`'s early-return chain.
- **AI translation overwrites `query` + `mode`.** Use `getLastAiPrompt()` for the original prompt.
- **The `AiPromptStrip` is a human-readable MIRROR, never the source of truth** (`buildAiSummary()` renders chip state;
  live chips stay editable). Its first-person agent voice is a SANCTIONED exception to the no-first-person copy rule.
- **Type-in-AI is leave-alone-if-null; size/date are reset-first. Don't "consistency-fix" this.** Each AI run resets
  `sizeFilter` / `dateFilter` to `'any'` first, or a prior run's filter leaks; `applyTypeFromAi` writes only on non-null
  `isDirectory`, so callers must NOT pre-reset `typeFilter`. Contract in `apply-ai-filters.ts`.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
