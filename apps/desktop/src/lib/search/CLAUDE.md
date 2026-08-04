# Search (frontend)

File search dialog: filename (glob/regex), size, date, scope, plus an AI mode translating natural language into
structured filters. First consumer of `../query-ui/CLAUDE.md`. Backend: `src-tauri/src/search/`.

## Module map

- `SearchDialog.svelte`: thin wrapper building a `QueryDialogConfig` for `lib/query-ui/QueryDialog.svelte`. Owns index
  lifecycle, coverage honesty, AI filter writes, snapshot promotion, recent-search add/remove. Zero orchestration.
- `search-state.svelte.ts` (façade over core `query-filter-state` + `search-extras-state`), plus `snapshot-store`,
  `searchable-folder`, `SearchResultsView.svelte` (in `lib/file-explorer/pane/`), and helpers. Footer buttons render
  from `QueryDialog`'s `config.*Action`.
- Two Search-only snippets bracket QueryDialog's results: `CoverageNote.svelte` + pure `coverage-note.ts` above (why an
  answer is short, plus the indexing offer), `ImageSearchResults.svelte` + `ocr-snippet.ts` below (the "text in images"
  OCR grid over `media_index`). `search-target-volume.ts` resolves the session's one volume, shared by both.

## Must-knows

- **No `aiPrompt` / `namePattern` state. Read `query`**, and derive `patternType` from `mode` (`regex` else `glob`).
  After an AI run `query` holds the translated pattern; the user's input is `getLastAiPrompt()`.
- **State split across two factories**: cross-consumer fields in core `createQueryFilterState()` (`lib/query-ui/`),
  Search-only ones (`scope`, `excludeSystemDirs`, index readiness, the coverage note, `lastAi*`) in
  `createSearchExtrasState()`. `recordAiTranslation` splits too: core writes `handTyped[mode]`, extras the Pattern chip
  + label.
- **Recent-search entries persist when the user ACTS on a result** ("Show all in main window", "Go to file"), not on
  every run. AI entries carry the prompt, not the translated pattern.
- **"Open in pane" promotes to the `search-results://` virtual volume**, not a special FilePane mode. Refcount is the
  ONLY lifetime authority (no cap), from pane-history refs + `setLastAttemptId`; `navigation-history.ts` stays pure.
- **Snapshot mutations are invisible to Svelte unless you bump `mutationTick`.** Snapshots aren't `$state` by design;
  `removeEntryFromAllSnapshots` bumps a module tick `SearchResultsView` reads in its `$derived`, or cross-snapshot
  delete sync won't re-render.
- **Closed-tab reopen must not double-count refs.** Tab close transfers snapshot-ref ownership to the `ClosedTab`
  (`transferSnapshotRefs`); refs release on its eviction (non-recording `closeTab` at once). `{#key activeTabId}` is
  safe (history lives on `TabState`).
- **An EMPTY scope box means the CURRENT FOLDER** (the volume root when the pane has none behind it), resolved per run
  in `runSearch()`. ❌ Never write that path into `scope` state, or every recent search bakes in a machine-specific
  path.
- **Two scope rungs, one volume max: `⌥C` current folder, `⌥V` this volume.** Popover-only (❌ not global: collides with
  the mode chips), off when `scopeChipVisible=false`. `⌥C` never seeds a `search-results://` URL —
  `searchable-folder.ts` walks pane history back to a real folder.
- **Destination write ops are blocked on `search-results` panes** (`SEARCH_RESULTS_NOT_A_FOLDER_TOAST`) at three sites:
  F-bar disablement, menu-item omission, `blockedByCapabilities`; `openTransferDialog` also blocks F5/F6 when the
  OPPOSITE pane is one. Source ops (Cmd+C/X, F5/F6, drag-out) run.
- **AI mode never auto-applies** (cost); only Enter / `⌘Enter` / the ⏎ button / example chips fire it. ❌ No
  per-consumer catch swallowing AI errors: QueryDialog surfaces them once.
- **One volume per search, and ONE prop names it**: `searchVolume` drives the readiness gate, the coverage voice, and
  the image grid; only index-BUILD progress stays `ROOT_VOLUME_ID`. The gate asks about the TARGET: ❌ never gate on
  root or let ⌘N clear readiness, or search goes silently inert on a machine with no root index. Every run rewrites the
  coverage note, including to `null`, so a caveat can't outlive its run. DETAILS §§ The readiness gate, The coverage
  note.
- **`ImageSearchResults` OWNS every `cmdr-media://` token it mints** (no viewer-session close): drop the prior set
  before minting the next, and all on unmount (`mediaIndexDropThumbnailTokens`), or the token map leaks. With
  `mediaIndex.enabled` OFF it fires no IPC; ON, it renders the `[`/`]` snippet via `parseOcrSnippet` + `<mark>`, ❌
  never `{@html}`.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here.
