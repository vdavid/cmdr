# Search (frontend)

File search dialog: filename (glob/regex), size, date, scope, plus AI mode. First consumer of `../query-ui/CLAUDE.md`;
backend in `src-tauri/src/search/`.

## Module map

- `SearchDialog.svelte` builds a `QueryDialogConfig` for `lib/query-ui/QueryDialog.svelte` and owns the Search-only
  glue: index lifecycle, coverage honesty, the live transport (`live-search-source.ts`, `live-ranking.ts`), AI filter
  writes, snapshot promotion, recent searches, analytics (`search-analytics.ts`). Zero orchestration.
- `search-state.svelte.ts` is the state façade, over `snapshot-store`, `searchable-folder`, `search-target-volume`, and
  `SearchResultsView.svelte` (in `lib/file-explorer/pane/`).
- Two Search-only snippets bracket the results: `CoverageNote.svelte` (+ `coverage-note.ts`, `coverage-actions.ts`)
  above, `ImageSearchResults.svelte` below.

## Must-knows

- **No `aiPrompt` / `namePattern` state. Read `query`**; derive `patternType` from `mode`. After an AI run `query` holds
  the translated pattern, and the user's input is `getLastAiPrompt()`.
- **State splits across two factories**: cross-consumer fields in `createQueryFilterState()`, Search-only ones (`scope`,
  `excludeSystemDirs`, readiness, the coverage note, `lastAi*`) in `createSearchExtrasState()`.
- **Recent-search entries persist when the user ACTS on a result**, not every run; an AI entry carries the prompt, not
  the translated pattern.
- **"Open in pane" promotes to the `search-results://` virtual volume**, not a FilePane mode. Refcount is the ONLY
  lifetime authority, from pane-history refs + `setLastAttemptId`. Tab close transfers ref ownership to the `ClosedTab`,
  so a reopen can't double-count.
- **"Open in pane" during a live walk KEEPS the walk** (`walk-handoff.svelte.ts`): the close must NAME it
  (`releaseSearchIndex(handedOffRunId())`) or it dies as the pane appears, silently. Its toast is prop-free (a replaced
  toast keeps old props); reopening ADOPTS via `source.resume`, ❌ never re-runs.
- **A snapshot mutation needs BOTH the `mutationTick` bump and a REPLACED stored object**: snapshots aren't `$state` by
  design, so the tick is what wakes `SearchResultsView` — but a `$derived` that recomputes to the same reference tells
  the deriveds below it nothing, so a walk appending rows in place left the pane frozen on the rows it opened with. ❌
  Don't write into a stored snapshot; `store.set` a copy.
- **An EMPTY scope box means the CURRENT FOLDER** (the volume root when the pane has none behind it), resolved per run
  in `buildRunQuery()`. ❌ Never write that path into `scope` state, or every recent search bakes in a machine-specific
  path. The two rungs (`⌥C` folder, `⌥V` volume) are popover-only, ❌ not global: they collide with the mode chips.
- **Destination write ops are blocked on `search-results` panes**, including F5/F6 when the OPPOSITE pane is one. Source
  ops run.
- **AI mode never auto-applies** (cost). ❌ No per-consumer catch swallowing AI errors: QueryDialog surfaces them.
- **Enter walks; auto-apply doesn't** (Decision 7). `streamingSource` takes every user-triggered run; the debounce takes
  `runQuery`, the ONLY path left that can report a drive with no index, so the uncovered note + offer live there. Ground
  nothing will read arrives as TWO typed lists — `permissionDenied` offers the FDA route (macOS, and only when Cmdr
  lacks it), `declined` ❌ never offers a permission. `walk: completed` ≠ exhaustive: `abandonedGround` is the third way
  short, ❌ never folded into `interrupted`. `rankLiveResults` is ORDERING, ❌ never membership.
- **One volume per search, and ONE prop names it**: `searchVolume` drives the readiness gate, the coverage voice and the
  image grid. Only index-BUILD progress stays `ROOT_VOLUME_ID`. The gate asks about the TARGET: ❌ never gate on root,
  or search goes inert on a machine with no root index. Every run rewrites the coverage note, to `null` included.
- **A run reports to analytics when it ENDS, and its clock starts on the coverage callback's `null`**: a small folder's
  whole run can arrive before `searchFilesStreaming` resolves, so ❌ nothing downstream of that promise may count a run
  as started.
- **`ImageSearchResults` OWNS every `cmdr-media://` token it mints**: drop the prior set before minting the next, and
  all on unmount, or the token map leaks. With `mediaIndex.enabled` OFF it fires no IPC; ON, it renders the `[`/`]`
  snippet via `parseOcrSnippet` + `<mark>`, ❌ never `{@html}`.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here.
