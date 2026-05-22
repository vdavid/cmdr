# Query UI (shared filter-and-act-on primitives)

The home for primitives shared between the Search dialog (`lib/search/`) and the upcoming Selection dialog
(`lib/selection-dialog/`). M3 will move the visible components here; M2 (this milestone) lands the cross-consumer state
factory and two pure helpers.

See `docs/specs/selection-dialog-plan.md` for the bigger picture.

## Files (M2)

| File                           | Purpose                                                                                  |
| ------------------------------ | ---------------------------------------------------------------------------------------- |
| `query-filter-state.svelte.ts` | Factory `createQueryFilterState(options)` producing the cross-consumer state instance    |
| `query-filter-state.test.ts`   | Pins the factory: defaults, switchMode + per-mode buffers, history filters, recordAi NG3 |
| `enter-action.ts`              | Pure helper `deriveEnterAction({lastEvent, resultsCount})` and `LastDialogEvent` type    |
| `enter-action.test.ts`         | Eight-permutation table for `deriveEnterAction`                                          |
| `recent-chips-layout.ts`       | Pure helper `computeRecentChipsLayout(...)` for the recent-items footer's greedy fit     |
| `recent-chips-layout.test.ts`  | Pins the greedy fit against mocked widths                                                |

## State shape contract

`createQueryFilterState()` owns ONLY cross-consumer fields. Both Search and Selection share the same shape; one dialog's
instance can never leak into another.

Fields:

- `query`, `mode` (the unified search input + mode discriminator)
- `sizeFilter` + value/unit, plus the `Max` half for `between` ranges
- `dateFilter` + value, plus `dateValueMax` for `between` ranges
- `caseSensitive`
- `lastAiPrompt`, `lastAiCaveat` (the AI transparency strip's content)
- per-mode `handTyped` buffers (`ai` / `filename` / `regex`)
- `results`, `totalCount`, `cursorIndex`, `isSearching`
- `lastDialogEvent` (drives ⏎ ownership via `deriveEnterAction`)
- `runOnMount`, `lastRunQuery` (one-shot prefill + auto-apply gates)

Search-only fields live next to the Search wrapper in
[`lib/search/search-extras-state.svelte.ts`](../search/search-extras-state.svelte.ts): `scope`, `excludeSystemDirs`,
`isIndexReady`, `indexEntryCount`, `isIndexAvailable`, `lastAiLabel`, `lastAiPattern`, `lastAiPatternKind`. The
whole-drive index is Search-only (Selection matches against an in-memory pane listing), so the index flags live here
even though they look like "session state". The Search wrapper instantiates both factories and composes them;
Selection's wrapper uses only the core. This keeps Selection's runtime state free of fields it never reads, and keeps
the shared factory honest about what's actually shared.

## When to use the factory vs extras

When adding a new field, ask: "would Selection also care about this?"

- **Yes** → add it to `createQueryFilterState()`. Cross-consumer. Selection's instance will carry it whether or not the
  Selection wrapper reads it today.
- **No** → add it to `createSearchExtrasState()` in `lib/search/`. Search-only.
- **No, but Selection has its own variant** → keep both in their respective consumer's "extras" module. Don't try to
  share via the core when the semantics diverge.

The `lastAiLabel` field is the textbook "no" case. Search's snapshot pane needs a short LLM-produced title for the
breadcrumb; Selection has no snapshot pane and no breadcrumb to seed.

## `recordAiTranslation` is split (NG3)

Pre-M2, `recordAiTranslation({pattern, kind, label})` wrote four pieces in one function: `handTyped[mode]`,
`lastAiPattern`, `lastAiPatternKind`, `lastAiLabel`.

M2 splits it because three of the four writes are Search-only:

- **Core's `recordAiTranslation({pattern, kind})`** writes ONLY to `handTyped[mode]` (R3 B2: AI's output overwrites the
  matching mode's hand-typed buffer). Both consumers call this.
- **Extras' `recordAiPatternAndLabel({pattern, kind, label})`** writes ONLY to the Search-only fields. Search's wrapper
  calls this right after the core method; Selection's wrapper skips it.

The Search façade in `lib/search/search-state.svelte.ts` keeps the legacy `recordAiTranslation({pattern, kind, label})`
shape as a convenience that calls both methods in sequence. Existing call sites work unchanged.

## `deriveEnterAction` (D8)

Pure helper. Same eight-permutation table as before; just relocated so the upcoming `QueryDialog` primitive can import
it without pulling the factory.

| `lastDialogEvent`                         | `resultsCount > 0` | Result       |
| ----------------------------------------- | ------------------ | ------------ |
| any                                       | false              | `run-search` |
| `results-arrived` or `cursor-moved`       | true               | `go-to-file` |
| `opened`, `query-edited`, `filter-edited` | true               | `run-search` |

See `enter-action.test.ts`.

## What's NOT here (yet)

- `QueryDialog.svelte`, `QueryBar.svelte`, `ModeChips.svelte`, etc. — M3 moves the visible components.
- `filter-chip-state.ts`, `filter-popover-helpers.ts`, `recent-searches-utils.ts` — M3 relocates these too. M2 only
  moves the most-portable helpers (`enter-action`, `recent-chips-layout`).
- `recent-items-state.svelte.ts` (factory store for the recent-items footer) — M3.
- `AiPromptStrip.svelte`, `FilterChips.svelte`, `QueryResults.svelte` — M3.

See `docs/specs/selection-dialog-plan.md` § "Where this all goes" for the full target layout.
