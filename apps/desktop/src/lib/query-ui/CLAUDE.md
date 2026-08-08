# Query UI (shared filter-and-act-on primitives)

Primitives shared by Search and Selection. Chips: `filter-chips/CLAUDE.md`. Consumers: `../search/CLAUDE.md`,
`../selection-dialog/CLAUDE.md`.

## Module map

- `QueryDialog.svelte` + `query-dialog-config.ts`: the shared orchestrator (a `ModalDialog`, one `QueryDialogConfig`
  prop per consumer), wiring and layout only. Logic sits in four tested siblings: `query-runner.svelte.ts`,
  `recent-popover.svelte.ts`, `query-shortcuts.ts`, `result-actions.ts`, plus `query-stream.ts` (answers over time).
  Their two silent traps (`getConfig` stays a GETTER; `highlightedFields` is ONE mutated `SvelteSet`): DETAILS.md § The
  controller split.
- UI pieces (`QueryBar`, `ModeChips`, `AiPromptStrip`, `QueryResults`, `EmptyState`, `PathPills`, `recent-items/*`),
  pure helpers (`name-column-width.ts`, `ai-summary.ts`, `apply-ai-filters.ts`), `query-filter-state.svelte.ts`, and
  `filter-chips/`. Subsets and adapters: DETAILS.md § Files.

## Must-knows

- **Three state fields are QueryDialog's alone; consumer callbacks MUST NOT write them:** `lastDialogEvent` (the `⏎`
  swap), `lastAiPrompt`/`lastAiCaveat`, `results`/`totalCount`/`cursorIndex`. Callbacks only RETURN data; own content
  goes in the two snippet slots.
- **AI translation errors surface once, in QueryDialog.** `translateAi` must let the typed `AiTranslateError` throw (a
  `null` return is a benign empty result).
- **`createQueryFilterState()` owns ONLY cross-consumer fields.** Adding one, ask "would Selection care?" Yes → core; no
  → the extras. `recordAiTranslation` writes ONLY `handTyped[mode]`; label/pattern slots are the extras'.
- **`stopPropagation()` on every dialog `keydown`** (else keys reach the explorer and trigger quick-search).
- **All chrome is `ModalDialog`'s; never re-add it here.** Opt-ins: `align="top"`, `fillBody`, `resizable`,
  `ownsKeyboard`, `closeOnOverlayClick`, `overlayClass="search-overlay"`. ❌ Strips pad VERTICALLY only. `config.width`
  is the OPENING width; ❌ no `max-width`, it caps the drag.
- **Three silent-failure traps.** Count-only OFF re-runs via `runFromButton()`, not `scheduleSearch()` (else a stale
  count stays); never swallow a `runQuery` rejection (it reads as "nothing matched"); never wipe state from a lifecycle
  hook (it survives unmount by design, `⌘N` being the ONLY sanctioned reset).
- **Reopen re-derives results, not the empty state.** A restored NON-AI session sets `runOnMount`; AI must NOT (cost).
- **The query field is a hand-assembled combobox, NOT a house/Ark `Combobox`** (Ark filters on one input; this needs
  two). Every key `RecentItemsPopover` claims must `stopPropagation()`; picking LOADS, never runs. DETAILS § Recent.
- **Path pills are mouse-only, `tabindex="-1"`**: tabbable pills break the row's arrow-down flow, and `⌥←`/`⌥→` stay
  native.
- **The Name track is MEASURED; ONE inline `grid-template-columns` feeds header AND rows** (two grids resolve `ch`
  differently). Measure `entry.name`, never DOM text; ❌ never read `nameTrack` in that effect (a measure→render→measure
  loop). DETAILS.md § Name column.
- **Two `QueryResults` render gates.** The status bar COLLAPSES via `.is-empty` on `getStatusText()` → `''`, staying
  mounted; its `aria-live` region is the INNER span, so a live run's counters can't flood it. And `showingRows`, not
  `results.length > 0` (trips axe `aria-required-children`), gates `role="listbox"` and the header.
- **A `config.streamingSource` consumer answers over TIME, and `isSearching` stays true the whole run.** The run is the
  RUNNER's (run id, generation guard, append, cursor by path, one re-rank at the end); the source owns only the wire. ❌
  Never gate the list on `isSearching` alone, or a live run hides every row it finds. Auto-apply takes `runQuery`, never
  the stream (Decision 7); Escape STOPS a run before closing. DETAILS.md § Streaming.
- **AI mode never auto-applies** (cost); filename/regex do, behind `search.autoApply` (1,000 ms, IME-gated). An AI
  translation overwrites `query` + `mode`, so `getLastAiPrompt()` holds what the user typed.
- **Nothing to run is not a run**: `hasRunnableQuery()` gates `executeQuery`; false → `resetToEmptyState()`, no IPC. An
  empty pattern WITH a filter still runs.
- **The `AiPromptStrip` MIRRORS chip state, never the truth**; its first-person voice is a SANCTIONED
  no-first-person-copy exception.
- **Type-in-AI is leave-alone-if-null; size/date reset first. Don't "consistency-fix" this.** Each AI run resets
  `sizeFilter`/`dateFilter` to `'any'` or a prior filter leaks; `applyTypeFromAi` writes only on non-null `isDirectory`.

Architecture, flows, decisions: `DETAILS.md`. Read it before any non-trivial work here.
