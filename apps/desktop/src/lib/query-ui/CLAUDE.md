# Query UI (shared filter-and-act-on primitives)

Primitives shared by the Search dialog (`lib/search/`) and the Selection dialog (`lib/selection-dialog/`). Filter-chip
internals: `filter-chips/CLAUDE.md`. Consumer decisions: `../search/CLAUDE.md`, `../selection-dialog/CLAUDE.md`.

## Module map

- `QueryDialog.svelte` + `query-dialog-config.ts`: the shared orchestrator (a `ModalDialog`; keyboard contract, IME
  guard, auto-apply gates, `lastDialogEvent` ownership), driven by one `QueryDialogConfig` prop per consumer.
- UI pieces (`QueryBar`, `ModeChips`, `AiPromptStrip`, `QueryResults`, `EmptyState`, `PathPills`, `SearchRowMenu`,
  `recent-items/*`), pure helpers, the `query-filter-state.svelte.ts` factory, `apply-ai-filters.ts`, `ai-summary.ts`
  (the `AiPromptStrip` mirror), and `filter-chips/`. Consumer subsets + the adapter shape: DETAILS.md § Files.

## Must-knows

- **Three state fields are QueryDialog's alone; consumer callbacks MUST NOT write them:** `lastDialogEvent` (the `⏎`
  swap), `lastAiPrompt` / `lastAiCaveat`, `results` / `totalCount` / `cursorIndex`. Callbacks only RETURN
  `{ entries, totalCount }` / `{ caveat, highlightedFields }`.
- **AI translation errors surface once, in QueryDialog.** `translateAi` must let the typed `AiTranslateError` throw (a
  `null` return is a benign empty translation); don't re-add a per-consumer catch.
- **`createQueryFilterState()` owns ONLY cross-consumer fields.** Adding one, ask "would Selection care?" Yes → core; no
  → the consumer's extras (`lastAiLabel` is the textbook "no"). `recordAiTranslation` (core) writes ONLY
  `handTyped[mode]`; the label/pattern slots are the extras'.
- **`stopPropagation()` on every dialog `keydown`** (else keys reach the explorer and trigger quick-search/nav).
  `use:trapFocus` listeners run in the capture phase, so this can't starve the trap.
- **All chrome is `ModalDialog`'s; never re-add it here.** Opt-ins: `align="top"`, `fillBody`, `padded={false}`,
  `ownsKeyboard` (Enter + the popover-aware Escape), `closeOnOverlayClick`, `overlayClass="search-overlay"`. Every strip
  pads itself at `--spacing-dialog`, matching the title bar.
- **Two silent-failure traps.** Turning count-only OFF must re-run via `runFromButton()` (not `scheduleSearch()`), else
  a stale count stays; and never swallow a `runQuery` rejection (`executeQuery` toasts it), else a refusal reads as
  "nothing matched".
- **Don't wipe state from any lifecycle hook.** State survives unmount by design; `⌘N` is the ONLY sanctioned reset.
- **Reopen re-derives results, not the empty state.** A restored NON-AI session sets `runOnMount` to re-run; AI must NOT
  (cloud cost). Don't loosen the `mode !== 'ai'` gate.
- **The query field is a hand-assembled combobox over recent items, NOT a house/Ark `Combobox`** (Ark filters on the
  control's own input; this needs two fields). `RecentItemsPopover`: no wrap, `↑` at top exits to the field, every
  claimed key `stopPropagation()`s, `↓` opens only with no results to walk. Picking LOADS, never runs (an AI entry
  would re-bill). DETAILS.md § Recent items.
- **Path pills are mouse-only, `tabindex="-1"`**: tabbable pills break the row's arrow-down flow, and `⌥←` / `⌥→` stay
  native move-by-word. `nested-interactive` is deliberately off on the populated-results test.
- **Two `QueryResults` render gates.** The status bar stays empty whenever the content area shows a state message
  (`getStatusText()` returns `''`, else it reads as broken); and `showingRows` — never `results.length > 0`, which trips
  axe `aria-required-children` on reopen — gates both the `role="listbox"` and the column header.
- **AI mode never auto-applies** (cost); filename/regex do, behind `search.autoApply` (default on, 1,000 ms debounce,
  IME-gated) in `scheduleSearch()`'s early-return chain. AI translation overwrites `query` + `mode`, so reach for
  `getLastAiPrompt()` when you want what the user typed.
- **Nothing to run is not a run**: `hasRunnableQuery()` (query non-empty OR size/date/type off default) gates
  `executeQuery`; false → `resetToEmptyState()`, no IPC. An empty pattern WITH a filter IS runnable.
- **The `AiPromptStrip` is a MIRROR of chip state, never the truth**; its first-person agent voice is a SANCTIONED
  no-first-person-copy exception.
- **Type-in-AI is leave-alone-if-null; size/date are reset-first. Don't "consistency-fix" this.** Each AI run resets
  `sizeFilter` / `dateFilter` to `'any'` first or a prior filter leaks; `applyTypeFromAi` writes only on non-null
  `isDirectory`, so callers must NOT pre-reset it.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here.
