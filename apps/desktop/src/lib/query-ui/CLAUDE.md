# Query UI (shared filter-and-act-on primitives)

Primitives shared by Search and Selection. Chips: `filter-chips/CLAUDE.md`. Consumers: `../search/CLAUDE.md`,
`../selection-dialog/CLAUDE.md`.

## Module map

- `QueryDialog.svelte` + `query-dialog-config.ts`: the shared orchestrator (a `ModalDialog`, one `QueryDialogConfig`
  prop per consumer), wiring and layout only. Logic sits in four tested siblings: `query-runner.svelte.ts`,
  `recent-popover.svelte.ts`, `query-shortcuts.ts`, `result-actions.ts`, plus `query-stream.ts`. Their two silent traps
  (`getConfig` stays a GETTER; `highlightedFields` is ONE mutated `SvelteSet`): DETAILS § The controller split.
- UI pieces (`QueryBar`, `ModeChips`, `AiPromptStrip`, `QueryResults`, `EmptyState`, `PathPills`, `recent-items/*`),
  pure helpers (`name-column-width.ts`, `ai-summary.ts`, `apply-ai-filters.ts`), `query-filter-state.svelte.ts`, and
  `filter-chips/`. Subsets and adapters: DETAILS § Files.

## Must-knows

- **Three state fields are QueryDialog's alone; consumer callbacks MUST NOT write them**: `lastDialogEvent` (the `⏎`
  swap), `lastAiPrompt` / `lastAiCaveat`, `results` / `totalCount` / `cursorIndex`. Callbacks only RETURN data; own
  content goes in the two snippet slots.
- **AI translation errors surface once, in QueryDialog**: `translateAi` must let the typed `AiTranslateError` throw (a
  `null` return is a benign empty result).
- **`createQueryFilterState()` owns ONLY cross-consumer fields.** Adding one, ask "would Selection care?" Yes → core, no
  → the extras. `recordAiTranslation` writes ONLY `handTyped[mode]`.
- **`stopPropagation()` on every dialog `keydown`**, or keys reach the explorer and trigger quick-search.
- **All chrome is `ModalDialog`'s; ❌ never re-add it here.** Opt-ins: `align="top"`, `fillBody`, `resizable`,
  `ownsKeyboard`, `closeOnOverlayClick`, `overlayClass="search-overlay"`. `config.width` is the OPENING width; ❌ no
  `max-width`, it caps the drag.
- **Three silent-failure traps**: count-only OFF re-runs via `runFromButton()`, ❌ not `scheduleSearch()` (a stale count
  stays); ❌ never swallow a `runQuery` rejection (it reads as "nothing matched"); ❌ never wipe state from a lifecycle
  hook (it survives unmount by design, `⌘N` being the ONLY sanctioned reset).
- **Reopen re-derives results, not the empty state**: a restored NON-AI session sets `runOnMount`, AI must NOT (cost).
- **The query field is a hand-assembled combobox, NOT a house/Ark `Combobox`** (Ark filters on one input; this needs
  two). Every key `RecentItemsPopover` claims must `stopPropagation()`; picking LOADS, never runs.
- **The Name track is MEASURED; ONE inline `grid-template-columns` feeds header AND rows** (two grids resolve `ch`
  differently). Measure `entry.name`, ❌ never DOM text, and ❌ never read `nameTrack` in that effect (a
  measure→render→measure loop). DETAILS § Name column.
- **A `config.streamingSource` consumer answers over TIME, and `isSearching` stays true the whole run.** The run is the
  RUNNER's (run id, generation guard, append, cursor by path, one re-rank at the end); the source owns only the wire. ❌
  Never gate the list on `isSearching` alone, or a live run hides every row it finds. Auto-apply takes `runQuery`, never
  the stream; Escape STOPS a run before closing, and `cancelLive()` answers ONCE per run, or a run whose terminal event
  never comes traps the dialog. DETAILS § Streaming.
- **AI mode never auto-applies** (cost); filename/regex do, behind `search.autoApply` (1,000 ms, IME-gated). An AI
  translation overwrites `query` + `mode`, so `getLastAiPrompt()` holds what the user typed.
- **Type-in-AI is leave-alone-if-null; size/date reset first. ❌ Don't "consistency-fix" this**: each AI run resets
  `sizeFilter` / `dateFilter` or a prior filter leaks, while `applyTypeFromAi` writes only on non-null `isDirectory`.
- **The `AiPromptStrip` MIRRORS chip state, never the truth**; its first-person voice is a SANCTIONED
  no-first-person-copy exception.

Nothing-to-run gating, the two `QueryResults` render gates, path-pill focus, architecture, flows, and decisions:
`DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
