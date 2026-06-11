# Search (frontend)

Whole-drive file search dialog. Searches the in-memory index by filename (glob/regex), size, date, and scope (folder
include/exclude). Optional AI mode translates natural-language queries into structured filters. First consumer of the
shared Query UI primitives in [`lib/query-ui/`](../query-ui/CLAUDE.md); Selection (`lib/selection-dialog/`) is the
second. Backend: `src-tauri/src/search/` + `src-tauri/src/commands/search.rs`.

## Module map

- `SearchDialog.svelte`: thin wrapper that builds a `QueryDialogConfig` and mounts `lib/query-ui/QueryDialog.svelte`.
  Owns index lifecycle, AI translation filter writes, snapshot promotion, recent-search add/remove. Zero orchestration.
- `search-state.svelte.ts` (façade over core `query-filter-state` + `search-extras-state.svelte.ts`),
  `build-search-query.ts`, `searchable-folder.ts`, `snapshot-store.svelte.ts`, `snapshot-label.ts`,
  `SearchResultsView.svelte`, `recent-searches-state.svelte.ts`, `capabilities.ts`. (Footer buttons are rendered by the
  shared `QueryDialog` from `config.primaryAction` / `config.secondaryAction`; there's no Search-local footer
  component.)

## Must-knows

- **No `aiPrompt` / `namePattern` state. Read `query` instead.** Derive `patternType` from `mode` (`regex => regex`,
  else `glob`). After an AI run, `query` holds the translated pattern, NOT the user's natural-language input; use
  `getLastAiPrompt()` for that (preserved in `lastAiPrompt`).
- **State split across two factories.** Cross-consumer fields live in the core `createQueryFilterState()`
  (`lib/query-ui/`); Search-only fields (`scope`, `excludeSystemDirs`, index flags, `lastAiLabel`, `lastAiPattern`,
  `lastAiPatternKind`) live in `createSearchExtrasState()`. `recordAiTranslation` is split: the core writes
  `handTyped[mode]`, the extras write the Pattern chip + label. The façade calls both in sequence. Selection carries
  none of the extras.
- **Recent-search entries are added only on "Open in pane"** (`SearchDialog.svelte::openInPane`), the ONE call site.
  Enter / auto-apply runs don't pollute history. For AI mode the entry carries the original prompt, not the pattern.
- **"Open in pane" promotes to the `search-results://` virtual volume**, not a special FilePane mode. Snapshots live in
  `snapshot-store.svelte.ts`; refcount is the ONLY authority for lifetime (no hard cap), driven by pane-history refs +
  the `setLastAttemptId` slot. `navigation-history.ts` stays pure (no snapshot-store import).
- **Snapshot mutations are invisible to Svelte unless you bump `mutationTick`.** Snapshots aren't `$state` by design;
  `removeEntryFromAllSnapshots` bumps a module-level tick, and `SearchResultsView` reads `getMutationTick()` inside its
  `$derived`. Without the tick, cross-snapshot delete sync won't re-render.
- **Closed-tab reopen must not double-count refs.** Tab close transfers snapshot-ref ownership to the `ClosedTab` entry
  (via `transferSnapshotRefs`); refs release only when the closed-tab stack evicts. Non-recording `closeTab` releases
  immediately. `{#key activeTabId}` pane recreation is safe (history lives on `TabState`, not the pane).
- **Scope shortcuts `⌥C` / `⌥V` are popover-only; don't promote them to global** (they'd collide with the mode chips).
  Search-only; suppressed when `scopeChipVisible=false` (Selection).
- **"Use current folder" never seeds a `search-results://` URL into the scope.** `searchable-folder.ts` walks pane
  history back to the most recent real folder; if none, surfaces a disabled result with the canonical tooltip.
- **Destination write ops are blocked on `search-results` panes** (`SEARCH_RESULTS_NOT_A_FOLDER_TOAST`) via three sites:
  F-bar disablement, menu-item omission, and `blockedByCapabilities`. `openTransferDialog` also blocks F5/F6 when the
  OPPOSITE pane is a snapshot. Source-side ops (Cmd+C/X, F5/F6, drag-out) run because the row is `canBeSource: true`.
- **AI mode never auto-applies** (cost); only Enter / `⌘Enter` / the ⏎ button / chip clicks fire it. Don't add a
  per-consumer catch that swallows AI errors: QueryDialog surfaces them once for both consumers.

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
