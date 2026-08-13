# Search (frontend)

File search dialog: filename (glob/regex), size, date, scope, plus AI mode. First consumer of `../query-ui/CLAUDE.md`;
backend in `src-tauri/src/search/`.

`SearchDialog.svelte` only wires a `QueryDialogConfig` for `lib/query-ui/QueryDialog.svelte`; the Search-only glue is
one module per job: `search-lifecycle.svelte.ts` (index prepare/release + the readiness gate), `search-runners.ts` (the
one-shot and live paths + the query builder they share), `ai-translate.ts` (the AI's filter writes),
`coverage-cta.svelte.ts` (what may be offered over a gap), `snapshot-promotion.ts` ("Open in pane" + the recent-search
writes), `search-run-tracking.ts` (the analytics clock). `search-state.svelte.ts` is the state façade over
`snapshot-store`, `searchable-folder`, `search-target-volume`, and `SearchResultsView.svelte`; `CoverageNote.svelte` and
`ImageSearchResults.svelte` bracket the results.

## Must-knows

- **No `aiPrompt` / `namePattern` state. Read `query`**, derive `patternType` from `mode`. After an AI run `query` holds
  the translated pattern; the user's input is `getLastAiPrompt()`.
- **State splits across two factories**: cross-consumer fields in `createQueryFilterState()`, Search-only ones in
  `createSearchExtrasState()`.
- **Recent-search entries persist when the user ACTS on a result**, not every run; an AI entry carries the prompt, not
  the translated pattern.
- **"Open in pane" promotes to the `search-results://` virtual volume**, not a FilePane mode. Refcount is the ONLY
  lifetime authority; tab close transfers ref ownership to the `ClosedTab`, so a reopen can't double-count.
- **"Open in pane" during a live walk KEEPS the walk** (`walk-handoff.svelte.ts`): the close must NAME it
  (`releaseSearchIndex(handedOffRunId())`) or it dies as the pane appears, silently. Reopening ADOPTS via
  `source.resume`, ❌ never re-runs.
- **A vanished file leaves every snapshot from ONE place**, `snapshot-purge.ts`, off the `write-source-item-done`
  stream's `sourceRemoved` flag (outcome, so a skip and a cancel are right for free). ❌ Never purge from a dialog or a
  pane: they hold intent, and a snapshot outlives both. DETAILS § "Cross-snapshot purge".
- **❌ Never write into a stored snapshot**: `store.set` a copy and bump `mutationTick`, or the `$derived` stays on one
  reference and freezes a handed-off pane.
- **An EMPTY scope box means the CURRENT FOLDER**, resolved per run in `buildRunQuery()`. ❌ Never write that path into
  `scope` state, or every recent search bakes in a machine-specific path. The two rungs (`⌥C`, `⌥V`) are popover-only,
  ❌ not global.
- **Destination write ops are blocked on `search-results` panes**, including F5/F6 when the OPPOSITE pane is one. Source
  ops run.
- **AI mode never auto-applies** (cost), and ❌ no per-consumer catch swallows AI errors: QueryDialog surfaces them.
- **Enter walks; auto-apply doesn't** (Decision 7). `streamingSource` takes every user-triggered run; the debounce takes
  `runQuery`, the ONLY path that can report a drive with no index, so the uncovered note lives there.
- **Unread ground arrives as TWO typed lists**: `permissionDenied` offers the FDA route (macOS, only when Cmdr lacks
  it), `declined` ❌ never offers a permission. `walk: completed` ≠ exhaustive — `abandonedGround` is the third way
  short, ❌ never folded into `interrupted`. `rankLiveResults` is ORDERING, ❌ never membership.
- **One volume per search, and ONE prop names it**: `searchVolume` drives the readiness gate, the coverage voice, and
  the image grid; only index-BUILD progress stays `ROOT_VOLUME_ID`. ❌ Never gate on root, or search goes inert on a
  machine with no root index.
- **A run reports to analytics when it ENDS, and its clock starts on the coverage callback's `null`**: a small folder's
  whole run can arrive before `searchFilesStreaming` resolves, so ❌ nothing downstream of that promise may count a run
  as started.
- **`ImageSearchResults` OWNS every `cmdr-media://` token it mints**: drop the prior set before minting the next, and
  all on unmount, or the token map leaks. It renders snippets via `parseOcrSnippet` + `<mark>`, ❌ never `{@html}`.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
