# Search (frontend)

Whole-drive file search dialog. Searches the in-memory index by filename (glob/regex), size, date, and scope (folder
include/exclude) filters. Optional AI mode translates natural-language queries into structured filters.

Backend: `src-tauri/src/search/` (index, engine, query, AI pipeline), `src-tauri/src/commands/search.rs` (thin IPC
wrappers).

This dialog is the first consumer of the shared Query UI primitives in [`lib/query-ui/`](../query-ui/CLAUDE.md): unified
query bar, mode chips, AI prompt strip, filter chips strip, virtualized results table, recent-items footer + popover,
the `createQueryFilterState()` factory that owns cross-consumer fields, and the in-dialog keyboard contract.
Search-specific concerns (snapshot store, virtual volume, MCP open path, "Open in pane", index lifecycle, scope smart
fallback) stay here. Selection (see `lib/selection-dialog/`) is the second consumer; both wrap `QueryDialog` and share
the same primitives.

Dialog dimensions: `max-width: min(1080px, 80vw)`, `max-height: 80vh`. The dialog grows up to 1080 px wide but shrinks
to 80vw on smaller windows, and the results region absorbs whatever vertical room is left.

## Files

| File                               | Purpose                                                                                                                                                                                                                                                                |
| ---------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SearchDialog.svelte`              | Thin Search-specific wrapper: builds a `QueryDialogConfig` and mounts `lib/query-ui/QueryDialog.svelte`. Owns index lifecycle, AI translation filter writes, snapshot promotion, recent-search add/remove, and the system-dir exclude tooltip. Zero orchestration code |
| `SearchFooterActions.svelte`       | Right-edge footer buttons: "Show all in main window" (⌥⏎) and "Go to file" (⏎). Search-specific verbs                                                                                                                                                                  |
| `SearchResultsView.svelte`         | Pane view for `search-results://` snapshot panes (lives in `lib/file-explorer/pane/`, but conceptually a Search consumer)                                                                                                                                              |
| `recent-searches-state.svelte.ts`  | Thin instantiation of the `lib/query-ui/recent-items/recent-items-state` factory wired to `getRecentSearches`. Exposes the legacy named API the rest of Search expects                                                                                                 |
| `search-state.svelte.ts`           | Façade composing `lib/query-ui/query-filter-state` (core) + `search-extras-state` (Search-only). Exposes the legacy named API for `SearchDialog.svelte`                                                                                                                |
| `search-state.test.ts`             | Vitest tests against the Search façade                                                                                                                                                                                                                                 |
| `search-extras-state.svelte.ts`    | Factory `createSearchExtrasState()` for Search-only fields (`scope`, `excludeSystemDirs`, AI label/pattern/kind, index flags)                                                                                                                                          |
| `search-extras-state.test.ts`      | Pins the extras shape and the AI-write split contract                                                                                                                                                                                                                  |
| `build-search-query.ts`            | Pure helper layering `excludeSystemDirs` onto the core's `buildBaseSearchQuery()` for the `searchFiles` IPC payload                                                                                                                                                    |
| `searchable-folder.ts`             | Pure helper: walks pane history backward for the most recent real folder when the focused pane is on `search-results://`. Drives D12 "Use current folder" smart fallback                                                                                               |
| `searchable-folder.test.ts`        | Pins the walk-back rule                                                                                                                                                                                                                                                |
| `snapshot-store.svelte.ts`         | Frontend-only in-memory map of search-result snapshots, refcounted. Pure module state, no Svelte reactivity. Exports `resolveSnapshotPaths` for source-side ops on the snapshot pane                                                                                   |
| `snapshot-store.svelte.ts.test.ts` | Create/read/no-overwrite, refcount inc/dec/delete, last-attempt slot swaps, entries-cap truncation, debug stats, `resolveSnapshotPaths`                                                                                                                                |
| `snapshot-label.ts`                | Pure helper: `buildSnapshotLabel({ mode, query, aiPrompt? })` for breadcrumb + tab title                                                                                                                                                                               |
| `snapshot-label.test.ts`           | Filename/regex/AI label shapes, AI prompt priority, truncation cap, fallbacks                                                                                                                                                                                          |
| `capabilities.ts`                  | Thin shim: `searchResultsVolumeCapabilities()` returns the `search-results` row of the per-kind table (`lib/file-explorer/pane/volume-capabilities.ts`); also owns the `SEARCH_RESULTS_NOT_A_FOLDER_TOAST` shortcut toast string                                       |
| `capabilities.test.ts`             | Pins the shim against the table row, the purity contract, and the toast string                                                                                                                                                                                         |

Shared components, helpers, and tests live in [`lib/query-ui/`](../query-ui/CLAUDE.md) — Search and Selection both
import the unified components (`QueryBar`, `ModeChips`, `AiPromptStrip`, `FilterChips`, `FilterChip`,
`FilterChipPopover`, `PathPills`, `SearchRowMenu`, `QueryResults`, `EmptyState`, the `recent-items/` family, and the
`filter-chip-state` / `filter-popover-helpers` / `path-pills-layout` helpers).

## Search wrapper

`SearchDialog.svelte` no longer carries the dialog orchestration. The overlay, keyboard contract, IME guard, auto-apply
gates, `lastDialogEvent` writes, the `⏎` ownership swap, the title bar, the chip strip, the AI prompt strip, the results
table, the recent-items footer + popover, and the empty state all live in
[`lib/query-ui/QueryDialog.svelte`](../query-ui/QueryDialog.svelte). The Search wrapper builds a
[`QueryDialogConfig`](../query-ui/query-dialog-config.ts) for Search and mounts QueryDialog with it.

What the wrapper still owns (Search-specific glue):

- `prepareSearchIndex` on mount, `releaseSearchIndex` on destroy, the `search-index-ready` listener (and the
  auto-run-after-index-ready hook).
- `translateAi` callback: calls `translate_search_query` IPC and applies the AI's filter writes (`size`, `date`, scope,
  `caseSensitive`, `excludeSystemDirs`, Pattern chip + label). Returns `{ caveat, highlightedFields }` to QueryDialog.
- `runQuery` callback: calls `buildSearchQuery()` from the helper, layers the AI pattern when in AI mode, calls
  `parse_search_scope` (async) and merges, calls `searchFiles` IPC, returns `{ entries, totalCount }`.
- Primary action: "Show all in main window" (⌥⏎) — builds the `SearchSnapshot`, mints an id, pins via
  `setLastAttemptId`, persists the recent-search entry (the only call site that adds), hands the id to the host, closes
  the dialog.
- Secondary action: "Go to file" (⏎) — routes through `onNavigate` to close the dialog and navigate the active pane to
  the cursor row.
- Recent-search adapter + key (the only seam where Search-specific fields like `scope` / `excludeSystemDirs` leak into
  the chip's tooltip).
- `onClearState`: wires ⌘N to the `clearSearchState()` facade (core + extras).
- System-dir exclude tooltip loader (`getSystemDirExcludes` IPC).
- Live AI-provider subscription so the AI chip appears / disappears with the setting.

The wrapper has no line-count target; its size is what Search-specific glue costs. It does not own the overlay element,
the keyboard handler, the IME guard, the auto-apply debounce, the popover toggle, or any other orchestration concern.

The route (`+page.svelte`) mounts SearchDialog with its props: `onNavigate`, `onClose`, `searchableFolder`,
`onShowAllInMainWindow`.

## State shape

The user's typed text and the active mode are one model:

```ts
let query = $state('') // The text in the bar
let mode = $state<SearchMode>('filename') // 'ai' | 'filename' | 'regex'
let lastAiPrompt = $state<string | null>(null) // The natural-language prompt before AI overwrites `query`
let lastAiCaveat = $state<string | null>(null) // The AI translator's caveat (or null)
```

`buildSearchQuery()` reads `query` + `mode`: `mode === 'regex'` produces `patternType: 'regex'`, anything else produces
`patternType: 'glob'`. AI mode is only ever invoked via `executeAiSearch()`, which (1) captures the user's prompt into
`lastAiPrompt`, (2) calls `translateSearchQuery`, (3) overwrites `query` + `mode` with the AI's result so the user can
see and iterate on the translated pattern, and (4) sets `lastAiCaveat` from the result. The `AiPromptStrip` is visible
whenever `lastAiPrompt` is non-null; it clears on `⌘N` (via `clearSearchState`) and on any successful non-AI search
(`executeSearch(fromAiTranslation = false)`).

There is **no `aiPrompt` state and no `namePattern` state**. Read `query` instead. Anywhere `patternType` is needed,
derive from `mode` (`regex => regex`, everything else => glob).

### Where the state actually lives

The state is split into two factories so Search and Selection can each own an instance:

- **Cross-consumer core**: [`lib/query-ui/query-filter-state.svelte.ts`](../query-ui/query-filter-state.svelte.ts) —
  factory `createQueryFilterState()`. Owns `query`, `mode`, size + date filters, `caseSensitive`, `lastAiPrompt`,
  `lastAiCaveat`, per-mode `handTyped` buffers, `results`, `totalCount`, `cursorIndex`, `isSearching`,
  `lastDialogEvent`, `runOnMount`, `lastRunQuery`. See [`lib/query-ui/CLAUDE.md`](../query-ui/CLAUDE.md).
- **Search-only extras**: [`search-extras-state.svelte.ts`](search-extras-state.svelte.ts) — factory
  `createSearchExtrasState()`. Owns `scope`, `excludeSystemDirs`, `isIndexReady`, `indexEntryCount`, `isIndexAvailable`,
  `lastAiLabel`, `lastAiPattern`, `lastAiPatternKind`. Selection doesn't carry these (no whole-drive index, no
  Search-style scope row, no snapshot breadcrumb, no Pattern chip).
- **`buildSearchQuery()`** lives in [`build-search-query.ts`](build-search-query.ts) and layers `excludeSystemDirs` onto
  `core.buildBaseSearchQuery()`.
- **`recordAiTranslation` is split**: the core writes ONLY to `handTyped[mode]`; the extras' `recordAiPatternAndLabel`
  writes the Pattern chip + label slots. The Search façade calls both in sequence. See
  [`lib/query-ui/CLAUDE.md`](../query-ui/CLAUDE.md) § "`recordAiTranslation` is split".

`lib/search/search-state.svelte.ts` is a transparent façade re-exporting the legacy named functions that the Search
dialog imports. It also exports `searchQueryState` (the core instance) so prop-driven components like `FilterChips` can
be wired to Search's instance without going through the per-setter façade.

## Search-specific UI behavior

Search-only contracts (cross-consumer ones live in [`lib/query-ui/CLAUDE.md`](../query-ui/CLAUDE.md)):

- The Search façade's `recordAiTranslation` (composed call) overwrites the matching hand-typed buffer
  (`handTyped.filename` for a glob, `handTyped.regex` for a regex) so a fresh AI run clobbers the user's earlier
  hand-typed pattern in the same kind.
- `filter-chip-state.ts::deriveSizeChip` accepts a `FileSizeFormat` argument; the chip follows the popover's `kB`/`KB`
  mapping instead of printing the raw enum value.
- `path-pills-layout.ts::scheduleStableWidthMeasure` runs a follow-up re-measure on the next animation frame and again
  ~80ms later. Catches the CSS grid race where `el.clientWidth` reads stale before the parent track settles, which would
  otherwise cause "render full path, then collapse to ellipsis".
- `VolumeBreadcrumb.svelte` reports the static "Search results" name for the `search-results` volume;
  `FilePane.svelte::breadcrumbDisplayPath` renders the snapshot label (`*.svelte`, the AI title, etc.) as the path.
  Don't invert these (label in the volume slot, empty path).
- "Hide boring folders" (label, with that wording specifically — not "Hide system folders"). The tooltip lists ALL
  excluded directory names (one per line, mono font), no "+30 more" truncation.
- `+page.svelte::handleOpenSearchInPane` calls `explorerRef.refocus()` after opening the snapshot so the user can
  immediately navigate/select in the pane without an extra click.
- `pane/has-parent.ts` owns the `hasParent` derivation; `pane/has-parent.test.ts` pins that `selectAll` in a snapshot
  pane covers index 0, not 1.

## Scope row & shortcuts (Search-only)

Below the chips. Comma-separated folder paths with `!` prefix for exclusions. Parsed via `parseSearchScope()` IPC call
in `executeSearch()` (async, so not part of `buildSearchQuery()`). ⌥F sets scope to the focused pane's current
directory; ⌥D clears it. Info button `(i)` shows syntax help tooltip. Selection has no scope row (a selection runs
against a single in-memory folder), so `FilterChips.svelte` accepts a `scopeChipVisible` prop that Selection passes as
`false`; the underlying `⌥I` (open scope popover) and `⌥C` / `⌥V` (inside the scope popover) shortcuts are suppressed in
that case.

### Scope shortcuts inside the popover

`⌥F` is the Filename mode chip globally. The scope actions live as `⌥C` (Use current folder) and `⌥V` (All folders),
active ONLY while the Search-in popover is open. They're wired via a top-level `<svelte:window>` in `FilterChips.svelte`
that gates on `openChip === 'scope'`. Don't promote them back to global shortcuts — that collides with the mode chips.

### "Use current folder" smart fallback

When the focused pane's path starts with `search-results://`, naively reusing it as the scope seed produces an
unsearchable `search-results://sr-N` URL. `searchable-folder.ts` walks the pane's history backward for the most recent
non-snapshot path; if none is reachable, the dialog surfaces a `disabled: true` result with the canonical tooltip
("Current folder is search results, which isn't searchable. Open a real folder first.").

The plumbing:

- `getFocusedPaneSearchableFolder()` in `lib/file-explorer/pane/focused-pane-reads.ts` reads the focused pane's path +
  history from the explorer store and delegates to `resolveSearchableFolder`.
- `+page.svelte` calls it once per dialog mount and passes the result as the `searchableFolder` prop.
- `FilterChips.svelte` renders the "Use current folder" footer button disabled (with tooltip) when
  `searchableFolder.disabled === true`; otherwise it uses `searchableFolder.path`.
- `⌥C` inside the popover honors the same fallback so the keyboard shortcut never seeds a snapshot URL into the scope.

The pure helper (`resolveSearchableFolder`) is unit-tested in `searchable-folder.test.ts`.

## Data flow

```
User presses ⌘F
  -> +page.svelte sets showSearchDialog = true
  -> SearchDialog mounts, calls prepareSearchIndex() IPC
  -> Backend starts async index load (2-3s), emits "search-index-ready" when done
  -> User types in the bar -> 1s debounce -> searchFiles(query) IPC (filename/regex modes only)
  -> User presses Enter in AI mode -> translateSearchQuery -> populates filters -> searchFiles
  -> Results displayed, keyboard nav with ↑/↓, Enter navigates to file
  -> Dialog close -> releaseSearchIndex() IPC -> 5 min idle timer -> index dropped
```

The shared parts of this flow (debounce / IME guard / cursor model / Press-Enter hint / `runOnMount` / `lastDialogEvent`
/ `deriveEnterAction`) live in [`lib/query-ui/CLAUDE.md`](../query-ui/CLAUDE.md) — Search just sets up the lifecycle
around them.

## Search-specific patterns

**Index not available state**: When indexing is disabled or not started, `prepareSearchIndex()` errors. The dialog shows
a message ("Drive index not ready...") with scan progress if available. Inputs and filters are disabled.

**AI single-pass flow**: `executeAiSearch()` calls `translateSearchQuery()` once (LLM classifies intent into enums +
extracts keywords, Rust builds the query deterministically), then runs `executeSearch()`. No preflight, no refinement
pass. The previous two-pass system caused ~15% regressions; deterministic structure means there's nothing to refine.

**AI mode keeps the prompt in the bar; pattern lives in its own slot** (post-fixup, clarification 2): After AI
translates, the bar in AI mode STILL shows the user's natural-language prompt — the user can press Enter to
re-translate. The AI's produced pattern (glob or regex) is stored separately on `lastAiPattern` + `lastAiPatternKind`
and surfaced via the Pattern chip in the filter strip. Switching to filename or regex mode (⌘2 / ⌘3) is what hands the
pattern to the matching input; the other mode keeps whatever the user last typed by hand. Per-mode hand-typed buffers
live in `handTyped` inside the core state factory; `switchMode()` swaps `query` between them.

Lifecycle:

- `executeAiSearch(trimmed)` sets `lastAiPrompt = trimmed` BEFORE calling `translateSearchQuery`. The capture is
  unconditional: even if the IPC fails, the user still sees what they asked.
- After the translation succeeds, the façade's `recordAiTranslation({ pattern, kind, label })` populates the core's
  hand-typed buffer AND the extras' `lastAiPattern`, `lastAiPatternKind`, `lastAiLabel` (the LLM-produced short title
  used for the snapshot breadcrumb).
- `lastAiCaveat = translateResult.caveat ?? null`.
- `executeSearch(fromAiTranslation: boolean)` clears `lastAiPrompt` / `lastAiCaveat` when `fromAiTranslation` is false.
  In AI mode it also pulls `lastAiPattern` / `lastAiPatternKind` into the outgoing search query, so the bar's
  natural-language prompt isn't sent to the engine.
- `clearSearchState()` (called by `⌘N`) clears prompt + pattern + label + caveat + the per-mode hand- typed buffers.

**AI transparency strip lifecycle** (clarification 6): the strip stays visible until the user starts a new search OR
presses ⌘N. Switching modes (⌘1 / ⌘2 / ⌘3) does NOT hide it; the strip belongs to the most-recent AI run.

The disabled "Refine…" button on the strip is the placeholder for the chat-back UX.

**Auto mode fallback when AI gets disabled mid-session**: If the AI provider is switched off while the dialog is open
and the active mode is `ai`, the dialog quietly flips to `filename`. The user wouldn't be able to run a search
otherwise.

**MCP `open_search_dialog`**: External openers (the MCP tool) write to the same module-level `$state` and flip
`runOnMount` via `applySearchPrefill()`. The route's `mcp-listeners.ts` handles the `mcp-open-search-dialog` Tauri
event: it sanitizes the payload, defaults `mode` to `'ai'` when AI is enabled (else `'filename'`), calls
`applySearchPrefill`, then flips `showSearchDialog = true` on the route. The dialog's `$effect` consumer for
`runOnMount` fires for both cold-open and hot-prefill paths (one source of truth, two arrival modes), then dispatches to
`executeAiSearch` or `executeSearch` based on mode. The flag is cleared before the search call so the downstream state
writes can't re-trigger the effect. AI mode honors the explicit-trigger contract because the MCP caller's
`autoRun: true` counts as the explicit trigger.

**Footer right-edge actions** (post-fixup items 9–10): `SearchFooterActions.svelte` sits at the right of the dialog
footer, opposite the recent-searches strip. It renders two buttons whenever `results.length > 0`, each with its keyboard
shortcut surfaced inline as a tertiary-color hint:

- **"Go to file"** (⏎): closes the dialog and navigates the active pane to the cursor row's parent folder, focusing the
  file. Routes through the dialog's existing `onNavigate(path)` callback.
- **"Show all in main window"** (⌥A): the primary action. The handler in `SearchDialog.svelte::showAllInMainWindow`
  builds a `SearchSnapshot`, pins it via `setLastAttemptId`, adds the query to recent searches (the sole call site for
  that), hands the snapshot id to the host, and closes the dialog. The host routes the active pane to
  `search-results://<id>`. State is preserved across close + reopen, so `⌘F` reopens to the same results.

Both buttons are hidden (not just disabled) on empty/idle state. Empty + idle inputs disable both (index not ready).

## "Open in pane"

Click on the footer's "Open in pane" button promotes the current result set into a real pane view via the
`search-results://<id>` virtual volume. The handler in `SearchDialog.svelte::openInPane`:

1. Builds a `SearchSnapshot` from live state (`getResults()` / `getMode()` / `getQuery()` / filters / scope / flags).
2. Mints a fresh id via `nextSnapshotId()` and stores via `getOrCreate(id, snapshot)`.
3. Pins the snapshot via `setLastAttemptId(id)` so refcount stays ≥1 even before history pushes.
4. Calls `addRecentSearch(historyEntry)`. **This is the one and only call site that adds to recent searches** (per plan
   §3.5: auto-applies and Enter-runs don't pollute the history). For AI mode, the entry's `query` carries the original
   natural-language prompt (via `getLastAiPrompt()`), not the AI's translated pattern.
5. Calls `onOpenInPane?.(id)` to hand off to the host (`+page.svelte` → `DualPaneExplorer.openSearchSnapshotInPane`),
   which routes through `handleVolumeChange` so pinned- tab fork / focus / history-push all apply uniformly.
6. Closes the dialog. State is preserved (module-level `$state` survives unmount); ⌘F reopens to the same place.

The label shown in the pane breadcrumb (and the snapshot's `label` field) is built by
`snapshot-label.ts::buildSnapshotLabel`:

- AI mode: the LLM-produced label wins when present (a short human-friendly title; max ~40 chars). Falls back to the
  original prompt when the model omits the field.
- Filename mode: the pattern as-is (`*.pdf`).
- Regex mode: the pattern wrapped in slashes (`/pattern/`).

## Snapshot store

`snapshot-store.svelte.ts` holds `SearchSnapshot` records (query, mode, filters, scope, capped 10,000 entries,
totalCount, createdAt, friendly label) under monotonic `sr-N` ids, plus a per-record refcount. The store has no hard cap
on its own — **refcount is the only authority**. Refs come from two sources:

- **Pane history entries** whose `path` starts with `search-results://<id>` hold +1 per occurrence. The tab-state
  manager (`pushHistoryEntry` and the closed-tab lifecycle) drives inc/dec — `navigation-history.ts` itself stays pure
  (no snapshot-store import). Pushing past `MAX_HISTORY_PER_TAB = 100` evicts the oldest entry, and truncating forward
  on a new push after `back()` evicts the discarded tail. Both kinds of eviction surface via `push()`'s `droppedEntries`
  return field, and `pushHistoryEntry` releases the matching refs in one step.
- **The "last dialog attempt" slot** (`setLastAttemptId`) holds +1 for the most-recent dialog search regardless of
  whether any pane references it. Swaps decrement the old id and increment the new one atomically. The dialog calls this
  on each new search.

### Closed-tab lifecycle and refs

Tab close via `closeTabRecording` does NOT release refs; ownership transfers to the `ClosedTab` entry on the pane's
closed-tab stack (cap default 10). Reopen (`⌘⇧T`) just pops the entry back — no double- count. The refs only release
when the closed-tab stack evicts the entry (cap overflow in `pushClosed`, or manual `trimClosedStack`). Implemented via
the `transferSnapshotRefs(closedTab, 'transfer' | 'release')` helper in `tab-state-manager.svelte.ts`. The non-recording
`closeTab` / `closeOtherTabs` (used in tests and programmatic flows) release refs immediately, since nothing else holds
them.

**`{#key activeTabId}` recreation is safe**: history lives on `TabState`, not on the pane. The dual- pane explorer
destroys and recreates `FilePane` on tab switch (cold load), but `TabManager` survives, and the per-tab `history` field
is untouched. Snapshot refs therefore persist across pane recreation.

## Capability flags

`capabilities.ts::searchResultsVolumeCapabilities()` is a thin shim returning the `search-results` row of the per-kind
`VolumeCapabilities` table (`lib/file-explorer/pane/volume-capabilities.ts`):
`{ canPasteInto: false, canCreateChild: false, canRenameInPlace: false, canBeSource: true, … }`. Its one caller is
`SearchResultsView.svelte` (the row context menu's `restrict` flag reads `!caps.canRenameInPlace`). Every
capability-GUARD consumer reads the table via `capabilitiesFor` now (the A6 conversion is complete): the F-bar +
keyboard dispatch (destination-op guards), clipboard (snapshot-clip `pathScheme`, MTP refusal `kind === 'mtp'`),
transfer/delete (`!hasBackendListing` source routing + the `search-results`-kind-scoped dest block), `pane-commands`
(`isSnapshotPane` off `!hasBackendListing`), MCP sync (`!syncsToMcp`), and `has-parent` (`hasParentRow`). See
`lib/file-explorer/pane/CLAUDE.md` § "Volume capabilities" for the per-site breakdown. Consumers:

- **F-key bar** (`lib/file-explorer/pane/FunctionKeyBar.svelte` mounted in `routes/(main)/+page.svelte`): derives its
  `canMkdir` / `canMkfile` (= `caps.canCreateChild`), `canRename` (= `caps.canRenameInPlace`), and `canSourceOps` (=
  `caps.canBeSource`) off `capabilitiesFor(focusedVolumeId)`. On a `search-results` pane, F2 (Rename), F7 (New folder),
  and Shift+F4 (New file) render visibly disabled; F5 / F6 / F8 (Copy / Move / Delete) stay enabled because the snapshot
  row is source-OK.
- **Right-click context menu**: `showFileContextMenu` IPC takes a `restrictDestinationActions` flag. When `true`, the
  Rust menu builder omits Rename and New folder. Source-side items (Open, Copy, Move, Delete, Show in Finder, Copy
  filename, Copy path) stay. The flag is set when `!canRename && !canMkdir`.
- **Keyboard shortcut dispatch** (`routes/(main)/command-dispatch.ts::blockedByCapabilities`): catches `⌘V`, `⌘⌥V`,
  `F7`, Shift+F4, `F2` / `file.rename` when the focused pane's capabilities can't satisfy the destination op
  (`!canPasteInto` / `!canCreateChild` / `!canRenameInPlace`). Surfaces the friendly toast
  `"Search results aren't a folder. Paste into a real folder instead."` (canonical string
  `SEARCH_RESULTS_NOT_A_FOLDER_TOAST`) — for the `search-results` kind only; a `network` pane keeps its prior silence.

### Cross-snapshot delete sync

When the user deletes a row from a search-results pane, the delete dialog runs against the real file path (the snapshot
stores absolute paths). On `handleTransferComplete` for `op === 'delete' | 'trash' | 'move'`, `dialog-state.svelte.ts`
calls `removeEntryFromAllSnapshots(sourcePath)` once per deleted path. That helper:

1. Walks every stored snapshot and replaces its `entries` array with one that excludes the deleted path (preserves
   reference identity on the unchanged entries; only the array changes).
2. Bumps a module-level `mutationTick` `$state` whenever at least one snapshot was mutated.
3. Leaves `totalCount` alone — the existing `entries.length` vs `totalCount` mismatch is the truncation signal.

`SearchResultsView.svelte`'s snapshot lookup reads `getMutationTick()` inside its `$derived` so the view re-renders
after a delete. Without the tick, the `Map` mutation would be invisible to Svelte reactivity (snapshots aren't `$state`
themselves, by design — see the store's header).

### Source-side ops from the snapshot pane

With `isSourceOK: true`, Cmd+C / Cmd+X / F5 / F6 / drag-out run against the cursor + selection in the snapshot pane. The
snapshot pane shares `FilePane.selection` state with normal panes. Wire path:

- **Cmd+C / Cmd+X** route through `DualPaneExplorer.copyToClipboard` / `cutToClipboard`, which detect the snapshot pane
  via `getSnapshotClipboardPaths` and call `copy_paths_to_clipboard` / `cut_paths_to_clipboard` (paths-by-value sibling
  IPCs of the listing-id-keyed `copy_files_to_clipboard` family). The Rust commands reuse
  `clipboard::write_file_urls_to_clipboard` and `set_cut_state` / `clear_cut_state`, so the system clipboard contract
  (file URLs + newline-separated text) is identical.
- **F5 / F6** route through `openUnifiedTransferDialog`, which detects `volumeId === 'search-results'` and calls
  `transfer-operations::buildTransferPropsFromSnapshot` instead of the listing-id-driven builders. The snapshot's
  selected (or cursor) entries are resolved to paths via `snapshot-store::resolveSnapshotPaths`, fed into the same
  `TransferDialogPropsData` shape every transfer uses, and the existing `copy_files` / `move_files` IPCs run with
  `sources: Vec<String>`.
- **Drag-out** uses the `'paths'` drag context in `lib/file-explorer/drag/drag-drop.ts`: when `FullList` is rendered
  with `staticEntries` and the user drags a selection, the FE builds a paths array from `getEntryAt(idx)` and routes
  through `start_drag_paths`.
- **Post-move snapshot cleanup**: covered by the cross-snapshot delete-sync hook above. After F6 from the snapshot pane,
  the moved rows disappear from every snapshot that referenced them.

Destination-side write ops are still blocked: pasting INTO a search-results pane shows the canonical
`SEARCH_RESULTS_NOT_A_FOLDER_TOAST` (via the F-bar disablement, the menu item omission, and the dispatcher's
`blockedByCapabilities` guard). `openTransferDialog` also blocks F5/F6 when the OPPOSITE pane is a snapshot, so the
shortcut path can't accidentally route a copy/move INTO a snapshot.

## Search-specific decisions

**Decision**: Recent-search entries are added only on "Open in pane", not on Enter / auto-apply. **Why**: David's
explicit design call. The 1,000-entry budget stays signal-rich (results worth acting on) instead of polluted with every
keystroke-debounced auto-apply. Auto-apply fires on a 1 s debounce — adding every fire would turn the history into a
high-frequency log of false starts. The Rust IPC accepts any entry; the gate is the frontend's single `addRecentSearch`
call site in `SearchDialog.svelte::openInPane`.

**Decision**: "Open in pane" promotes to the `search-results` virtual volume, not a special FilePane mode. **Why**: We
already had the precedent: the `network` browser is a `volumeId` the FilePane special- cases, not a forked pane
component. Following that pattern lets us reuse the entire file-explorer toolkit (selection, keyboard nav, copy / move
source, history, Quick Look, drag-out) for free, and gives the user a real navigable pane with history-aware `⌘[` /
`⌘]`. A "special mode" branch would have leaked into every pane-aware module forever; the virtual-volume namespace
concentrates the special- casing into a small number of well-documented sites (FilePane gates,
`DualPaneExplorer.applyPathChange`, the breadcrumb label resolver).

**Decision**: Dialog, not a panel or sidebar. **Why**: Search is a focused, transient task. A command- palette-style
overlay matches this usage pattern and doesn't consume permanent screen real estate.

**Decision**: Structured filters always visible (not hidden behind "advanced"). **Why**: The filter row is compact (one
line) and makes the query model transparent. Users see exactly what's being searched.

## Search-specific gotchas

**Gotcha**: `prepareSearchIndex()` failure means index unavailable. **Why**: The backend returns an error when
`get_read_pool()` returns `None` (indexing disabled or not started). The dialog catches this and enters the disabled
state.

## References

- [AI search eval history](../../../../../docs/notes/ai-search-eval-history.md) -- Four rounds of prompt tuning for the
  AI natural language to structured query translation, with a 30-query test catalog and lessons learned.
- [`lib/query-ui/CLAUDE.md`](../query-ui/CLAUDE.md) -- Cross-consumer query UI primitives (the bar, mode chips, filter
  chips, results list, recent-items, the filter-state factory, the shared keyboard contract, gotchas, and decisions).

## Dependencies

- `$lib/tauri-commands` -- `prepareSearchIndex`, `searchFiles`, `releaseSearchIndex`, `translateSearchQuery`,
  `parseSearchScope`, `getRecentSearches`, `addRecentSearch`, `removeRecentSearch`, `clearRecentSearches`,
  `applyRecentSearchesMaxCount`, `showFileContextMenu`, `showInFinder`
- `$lib/shortcuts/key-capture` -- `isMacOS()` for the footer action's macOS/Linux label fork
- `$lib/indexing` -- `isScanning`, `getEntriesScanned` (scan progress for unavailable state)
- `$lib/settings` -- `getSetting('ai.provider')` (AI chip visibility, ⌘ shortcut numbering)
- Shared primitives from [`lib/query-ui/`](../query-ui/CLAUDE.md)
- CSS variables from `app.css`
