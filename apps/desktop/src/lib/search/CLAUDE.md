# Search (frontend)

Whole-drive file search dialog: filename (glob/regex), size, date, and scope. Optional AI mode translates
natural-language queries into structured filters. First consumer of the shared Query UI primitives in
[`lib/query-ui/`](../query-ui/CLAUDE.md) (Selection is the second). Backend: `src-tauri/src/search/`.

## Module map

- `SearchDialog.svelte`: thin wrapper that builds a `QueryDialogConfig` and mounts `lib/query-ui/QueryDialog.svelte`.
  Owns index lifecycle, AI filter writes, snapshot promotion, recent-search add/remove. Zero orchestration.
- `search-state.svelte.ts` (façade over core `query-filter-state` + `search-extras-state`), plus `snapshot-store`,
  `searchable-folder`, `SearchResultsView.svelte`, and helpers (see DETAILS § Files). Footer buttons render from the
  shared `QueryDialog`'s `config.*Action`.
- `ImageSearchResults.svelte` + `ocr-snippet.ts`: the "text in images" OCR grid, rendered below filename results via
  QueryDialog's `config.resultsExtra` slot (Search-only). Backend: `media_index`; `active-media-volume.ts` resolves the
  target volume.

## Must-knows

- **No `aiPrompt` / `namePattern` state. Read `query` instead**, and derive `patternType` from `mode` (`regex` else
  `glob`). After an AI run `query` holds the translated pattern, not the user's input; use `getLastAiPrompt()`.
- **State split across two factories.** Cross-consumer fields live in core `createQueryFilterState()` (`lib/query-ui/`);
  Search-only fields (`scope`, `excludeSystemDirs`, index flags, `lastAiLabel/Pattern/PatternKind`) in
  `createSearchExtrasState()`. `recordAiTranslation` is split (core writes `handTyped[mode]`, extras write the Pattern
  chip + label); the façade calls both. Selection carries no extras.
- **Recent-search entries persist when the user ACTS on a result, not on every run.** "Show all in main window" and "Go
  to file" call `persistRecentSearch()`; plain Enter / auto-apply don't. AI entries carry the original prompt, not the
  translated pattern.
- **"Open in pane" promotes to the `search-results://` virtual volume**, not a special FilePane mode. Refcount is the
  ONLY lifetime authority (no hard cap), from pane-history refs + `setLastAttemptId`; keep `navigation-history.ts` pure.
- **Snapshot mutations are invisible to Svelte unless you bump `mutationTick`.** Snapshots aren't `$state` by design;
  `removeEntryFromAllSnapshots` bumps a module tick that `SearchResultsView` reads in its `$derived`, else
  cross-snapshot delete sync won't re-render.
- **Closed-tab reopen must not double-count refs.** Tab close transfers snapshot-ref ownership to the `ClosedTab` entry
  (`transferSnapshotRefs`); refs release only on closed-tab eviction (non-recording `closeTab` releases immediately).
  `{#key activeTabId}` pane recreation is safe (history lives on `TabState`).
- **Scope shortcuts `⌥C` / `⌥V` are popover-only; don't promote to global** (collides with the mode chips). Search-only;
  suppressed when `scopeChipVisible=false` (Selection).
- **"Use current folder" never seeds a `search-results://` URL into scope.** `searchable-folder.ts` walks pane history
  back to the most recent real folder; if none, surfaces a disabled result with the canonical tooltip.
- **Destination write ops are blocked on `search-results` panes** (`SEARCH_RESULTS_NOT_A_FOLDER_TOAST`) via three sites:
  F-bar disablement, menu-item omission, `blockedByCapabilities`. `openTransferDialog` also blocks F5/F6 when the
  OPPOSITE pane is a snapshot. Source ops (Cmd+C/X, F5/F6, drag-out) run (`canBeSource: true`).
- **AI mode never auto-applies** (cost); only Enter / `⌘Enter` / the ⏎ button / chip clicks fire it. Don't add a
  per-consumer catch that swallows AI errors: QueryDialog surfaces them once for both.
- **Two volume scopes: filename search is root-only; the image grid follows the active pane.** Filename search reads the
  LOCAL index, so `SearchDialog` keys its lifecycle + scanning indicator on `ROOT_VOLUME_ID` (don't network-scope it).
  The image grid targets the focused pane's volume via `imageSearchVolume` (a NAS search finds NAS photos). The pane's
  volume id IS the media-index id (`root` / `smb-…`). DETAILS § Which volume.
- **`ImageSearchResults` OWNS every `cmdr-media://` thumbnail token it mints** (no viewer-session close): drop the prior
  set before minting the next, and all on unmount (`mediaIndexDropThumbnailTokens`), or the backend token map leaks.
  With `mediaIndex.enabled` OFF the section renders nothing, fires no IPC; ON, it voices coverage and renders the
  `[`/`]` snippet via `parseOcrSnippet` + `<mark>`, never `{@html}`.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
