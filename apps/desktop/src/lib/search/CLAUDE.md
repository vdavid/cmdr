# Search (frontend)

File search dialog: filename (glob/regex), size, date, scope, AI mode. First consumer of `../query-ui/CLAUDE.md`;
backend in `src-tauri/src/search/`.

`SearchDialog.svelte` only wires a `QueryDialogConfig` for `lib/query-ui/QueryDialog.svelte`; Search-only glue, one
module per job: `search-lifecycle.svelte.ts` (index prepare/release + readiness gate), `search-runners.ts` (one-shot and
live paths + their query builder), `ai-translate.ts` (the AI's filter writes), `coverage-cta.svelte.ts` (what may be
offered over a gap), `index-load-hint.svelte.ts` (the wait's voice), `snapshot-promotion.ts` ("Open in pane" +
recent-search writes), `snapshot-fill.ts` (a pane's full row set), `search-run-tracking.ts` (the analytics clock),
`snapshot-store.svelte.ts` + `snapshot-sort.svelte.ts` (snapshots and their row order), `search-hit-menu.ts` (a hit's
menu facts). `search-state.svelte.ts` is the façade over these, `searchable-folder`, `search-target-volume`, and
`SearchResultsView.svelte`.

## Must-knows

- **No `aiPrompt` / `namePattern` state: read `query`** and derive `patternType` from `mode`. After an AI run `query`
  holds the translated pattern; the user's input is `getLastAiPrompt()`. AI mode never auto-applies (cost), and its
  errors reach QueryDialog.
- **State splits across two factories**: cross-consumer fields in `createQueryFilterState()`, Search-only ones in
  `createSearchExtrasState()`.
- **Recent-search entries persist only when the user ACTS on a result**; an AI entry carries the prompt.
- **"Open in pane" promotes to the `search-results://` virtual volume.** Refcount is the ONLY lifetime authority (a tab
  close transfers ref ownership to the `ClosedTab`). Destination write ops are blocked there; source ops run, and ❌
  never re-derive their rows: `resolveSnapshotEntries` decides.
- **The DIALOG lists 30 rows; a PANE gets every hit**, via `snapshot-fill.ts`. ❌ Never promote `getResults()`, and ❌
  never let a top-up shrink a pane.
- **"Open in pane" during a live walk KEEPS the walk** (`walk-handoff.svelte.ts`), so the close must NAME it
  (`releaseSearchIndex(handedOffRunId())`) or the walk dies silently as the pane appears. Its ENDING fills the pane. A
  reopen ADOPTS through `source.resume`, ❌ never re-runs.
- **A vanished file leaves every snapshot from ONE place**, `snapshot-purge.ts`, off `write-source-item-done`'s
  `sourceRemoved` flag, ❌ never its `outcome`. ❌ Never purge from a dialog or pane: a snapshot outlives both.
- **A snapshot pane's header sorts the SNAPSHOT**, ❌ never the tab and ❌ never view-side: replacing `entries` carries
  every index-resolving op along. DETAILS § "The snapshot pane's row order".
- **❌ Never write into a stored snapshot**: `store.set` a copy and bump `mutationTick`, or the `$derived` holds one
  reference and freezes a handed-off pane.
- **An EMPTY scope box means the CURRENT FOLDER**, resolved per run in `buildRunQuery()`. ❌ Never write that path into
  `scope` state, or every recent search bakes in a machine-specific one.
- **Enter walks; auto-apply doesn't** (Decision 7). `streamingSource` takes every user-triggered run; the debounce takes
  `runQuery`, the only path reporting a drive with no index, so the uncovered note lives there.
- **Unread ground arrives as TWO typed lists**: `permissionDenied` offers the FDA route (macOS, only when Cmdr lacks
  it), `declined` ❌ never does. `walk: completed` isn't exhaustive (`abandonedGround` is a third way to come up short);
  `rankLiveResults` is ORDERING, not membership.
- **The index-load hint waits 500 ms, then yields**: its only input is `pendingIndexVolumeId` (so a drive with no index
  never opens it), and `hasSearched` hands the wait to `QueryResults`.
- **One volume per search, and ONE prop names it**: `searchVolume` drives the readiness gate, the coverage voice, and
  the image grid; only index-BUILD progress stays `ROOT_VOLUME_ID`. ❌ Never gate on root.
- **A run reports to analytics when it ENDS**, on a clock started by the coverage callback's `null`.
- **`ImageSearchResults` OWNS every `cmdr-media://` token it mints**: drop the prior set before minting the next, and
  all on unmount, or the map leaks. It renders snippets via `parseOcrSnippet` + `<mark>`, ❌ never `{@html}`.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
