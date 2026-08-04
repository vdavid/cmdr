# Search (frontend)

File search dialog: filename (glob/regex), size, date, and scope. Optional AI mode translates natural-language queries
into structured filters. First consumer of the shared Query UI primitives in `../query-ui/CLAUDE.md` (Selection is the
second). Backend: `src-tauri/src/search/`.

## Module map

- `SearchDialog.svelte`: thin wrapper that builds a `QueryDialogConfig` and mounts `lib/query-ui/QueryDialog.svelte`.
  Owns index lifecycle, AI filter writes, snapshot promotion, recent-search add/remove. Zero orchestration.
- `search-state.svelte.ts` (façade over core `query-filter-state` + `search-extras-state`), plus `snapshot-store`,
  `searchable-folder`, `SearchResultsView.svelte` (which lives in `lib/file-explorer/pane/`), and helpers. Footer
  buttons render from the shared `QueryDialog`'s `config.*Action`.
- `ImageSearchResults.svelte` + `ocr-snippet.ts`: the "text in images" OCR grid, rendered below filename results via
  QueryDialog's `config.resultsExtra` slot (Search-only). Backend: `media_index`; `active-media-volume.ts` resolves the
  target volume.

## Must-knows

- **No `aiPrompt` / `namePattern` state. Read `query`**, and derive `patternType` from `mode` (`regex` else `glob`).
  After an AI run `query` holds the translated pattern, not the user's input; use `getLastAiPrompt()`.
- **State split across two factories.** Cross-consumer fields in core `createQueryFilterState()` (`lib/query-ui/`);
  Search-only ones (`scope`, `excludeSystemDirs`, index flags, `lastAiLabel/Pattern/PatternKind`) in
  `createSearchExtrasState()`. `recordAiTranslation` is split (core writes `handTyped[mode]`, extras the Pattern chip +
  label); the façade calls both. Selection carries no extras.
- **Recent-search entries persist when the user ACTS on a result** ("Show all in main window", "Go to file"), not on
  every run. AI entries carry the original prompt, not the translated pattern.
- **"Open in pane" promotes to the `search-results://` virtual volume**, not a special FilePane mode. Refcount is the
  ONLY lifetime authority (no cap), from pane-history refs + `setLastAttemptId`; keep `navigation-history.ts` pure.
- **Snapshot mutations are invisible to Svelte unless you bump `mutationTick`.** Snapshots aren't `$state` by design;
  `removeEntryFromAllSnapshots` bumps a module tick `SearchResultsView` reads in its `$derived`, else cross-snapshot
  delete sync won't re-render.
- **Closed-tab reopen must not double-count refs.** Tab close transfers snapshot-ref ownership to the `ClosedTab`
  (`transferSnapshotRefs`); refs release on closed-tab eviction (non-recording `closeTab` releases at once). `{#key
  activeTabId}` pane recreation is safe (history lives on `TabState`).
- **An EMPTY scope box means the CURRENT FOLDER**, not everywhere (the volume root when the pane has none behind it);
  `runSearch()` resolves it per run. ❌ Never write that path into `scope` state: a defaulted scope stays unpersisted,
  else every recent search bakes in a machine-specific path nobody chose.
- **Two scope rungs, one volume max: `⌥C` current folder, `⌥V` this volume.** Popover-only (❌ not global, collides with
  the mode chips), off when `scopeChipVisible=false`. `⌥C` never seeds a `search-results://` URL: `searchable-folder.ts`
  walks pane history back to a real folder.
- **Destination write ops are blocked on `search-results` panes** (`SEARCH_RESULTS_NOT_A_FOLDER_TOAST`) at three sites:
  F-bar disablement, menu-item omission, `blockedByCapabilities`. `openTransferDialog` also blocks F5/F6 when the
  OPPOSITE pane is a snapshot. Source ops (Cmd+C/X, F5/F6, drag-out) run (`canBeSource: true`).
- **AI mode never auto-applies** (cost); only Enter / `⌘Enter` / the ⏎ button / example chips fire it. ❌ No
  per-consumer catch swallowing AI errors: QueryDialog surfaces them once for both.
- **One volume per search** (backend-enforced), but `SearchDialog` still keys lifecycle + scanning indicator on
  `ROOT_VOLUME_ID`, the arena it WAITS for. The image grid follows `imageSearchVolume` instead, the focused pane's
  volume (a NAS search finds NAS photos), whose id IS the media-index id. DETAILS § Which volumes.
- **`ImageSearchResults` OWNS every `cmdr-media://` thumbnail token it mints** (no viewer-session close): drop the prior
  set before minting the next, and all on unmount (`mediaIndexDropThumbnailTokens`), or the backend token map leaks.
  With `mediaIndex.enabled` OFF it renders nothing and fires no IPC; ON, it renders the `[`/`]` snippet via
  `parseOcrSnippet` + `<mark>`, never `{@html}`.

Architecture, flows, and decision detail: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
