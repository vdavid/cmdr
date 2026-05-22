# Search (frontend)

Whole-drive file search dialog. Searches the in-memory index by filename (glob/regex), size, date, and scope (folder
include/exclude) filters. Optional AI mode translates natural language queries into structured filters.

Backend: `src-tauri/src/search/` (index, engine, query, AI pipeline), `src-tauri/src/commands/search.rs` (thin IPC
wrappers).

Dialog width: 1080 px (was 900 px). Internal layout is fluid; no fixed inner widths. The bump leaves room for the filter
chip row and path-pill column landing in later milestones.

## Files

| File                                 | Purpose                                                                                                                                                                       |
| ------------------------------------ | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `SearchDialog.svelte`                | Orchestrator: overlay, mount/unmount, keyboard dispatch, search execution, state wiring                                                                                       |
| `SearchBar.svelte`                   | Unified query input: one `<input>` for AI / filename / regex, placeholder updates per mode                                                                                    |
| `SearchModeChips.svelte`             | Mode chip row below the bar: AI / Filename / Content (disabled) / Regex, arrow-key navigable                                                                                  |
| `AiTransparencyStrip.svelte`         | Strip below the chip row showing the original AI prompt, the caveat, and a disabled Refine button                                                                             |
| `SearchFilterChips.svelte`           | Filter chip strip (Size, Modified, Search in) plus Add filter dropdown. Each opens a popover                                                                                  |
| `FilterChip.svelte`                  | Single chip: default/configured states, `×` clear, Backspace clear, aria-expanded                                                                                             |
| `FilterChipPopover.svelte`           | Generic popover: frosted-glass, auto-flip, focus trap, Esc closes without disrupting dialog                                                                                   |
| `filter-chip-state.ts`               | Pure helpers: `deriveSizeChip`, `deriveDateChip`, `deriveScopeChip` (testable in isolation)                                                                                   |
| `SearchResults.svelte`               | Column headers + results list + all states (loading, empty, populated) + status bar                                                                                           |
| `EmptyState.svelte`                  | Pre-search "Try…" block: three example chips (AI prompts or filename patterns), index size, keyboard tip                                                                      |
| `RecentSearchesFooter.svelte`        | Chip strip at the bottom of the dialog, up to 6 most recent entries plus an "All searches…" trailing chip                                                                     |
| `RecentSearchesPopover.svelte`       | Fuzzy-searchable popover over the full recent-searches history (`⌘H` opens, ufuzzy under the hood)                                                                            |
| `SearchFooterActions.svelte`         | Right-edge footer buttons: "Open in pane" (STUB in M7) and "Open in Finder" / "Open in file manager"                                                                          |
| `PathPills.svelte`                   | Clickable path-pill strip rendered inside each result row's path column (replaces flat `parentPath`)                                                                          |
| `SearchRowMenu.svelte`               | Per-row `…` button: always visible on cursor row, hover-revealed on other rows; opens native context menu                                                                     |
| `recent-searches-state.svelte.ts`    | Module-level reactive store for the loaded recent-searches list; loads from backend once per session                                                                          |
| `recent-searches-utils.ts`           | Pure helpers: `modeBadge`, `modeName`, `formatAge`, `filterSummary`, `chipTooltip`                                                                                            |
| `search-state.svelte.ts`             | Module-level `$state` for query fields, results, index readiness, AI state                                                                                                    |
| `search-state.test.ts`               | Vitest tests for state helpers (`parseSizeToBytes`, `buildSearchQuery`, etc.)                                                                                                 |
| `filter-chip-state.test.ts`          | Default → configured → cleared rules for each filter chip's display summary                                                                                                   |
| `SearchBar.svelte.test.ts`           | Per-mode placeholder, value mirror, `onInput` callback                                                                                                                        |
| `SearchModeChips.svelte.test.ts`     | Chip set, active marker, click + keyboard activation, focus motion (skipping Content)                                                                                         |
| `SearchFilterChips.svelte.test.ts`   | Chip rendering, `×` and Backspace clear, popover open/close, Add filter list, scope behavior                                                                                  |
| `AiTransparencyStrip.svelte.test.ts` | Renders prompt, renders caveat when set, Refine button is disabled with Coming soon tooltip                                                                                   |
| `SearchDialog.svelte.test.ts`        | `⌘N` clears, close+reopen preserves, `⌘1`/`⌘2`/`⌘3` mode switch, `⌘Enter` triggers AI, AI strip lifecycle                                                                     |
| `SearchDialog.a11y.test.ts`          | Tier-3 axe-core audit across loading / index-ready / AI-on macro-states                                                                                                       |
| `SearchFilterChips.a11y.test.ts`     | Tier-3 axe-core audit across default, configured, disabled, and open-popover states                                                                                           |
| `AiTransparencyStrip.a11y.test.ts`   | Tier-3 axe-core audit for prompt-only and prompt-plus-caveat states                                                                                                           |
| `SearchResults.a11y.test.ts`         | Tier-3 axe-core audit across result states                                                                                                                                    |
| `PathPills.svelte.test.ts`           | Path-pill split semantics (`/` only), click → onPick wiring, stopPropagation contract                                                                                         |
| `PathPills.a11y.test.ts`             | Pins `tabindex="-1"` per pill (not in Tab order); axe-core audit                                                                                                              |
| `SearchRowMenu.svelte.test.ts`       | Button rendering, `is-cursor` marker, onOpen + stopPropagation on click                                                                                                       |
| `SearchRowMenu.a11y.test.ts`         | Tier-3 axe-core audit for cursor-row and non-cursor variants                                                                                                                  |
| `SearchFooterActions.svelte.test.ts` | Visibility per `resultCount`, macOS/Linux label fork, disabled state, click handlers                                                                                          |
| `SearchFooterActions.a11y.test.ts`   | Tier-3 axe-core audit for enabled and disabled states                                                                                                                         |
| `snapshot-store.svelte.ts`           | Frontend-only in-memory map of search-result snapshots, refcounted (M8a). Pure module state, no Svelte reactivity. Exports `resolveSnapshotPaths` for the M8d source-side ops |
| `snapshot-store.svelte.ts.test.ts`   | Create/read/no-overwrite, refcount inc/dec/delete, last-attempt slot swaps, entries-cap truncation, debug stats, `resolveSnapshotPaths` (M8d)                                 |
| `snapshot-label.ts`                  | Pure helper: `buildSnapshotLabel({ mode, query, aiPrompt? })` for breadcrumb + tab title (M8b)                                                                                |
| `snapshot-label.test.ts`             | Filename/regex/AI label shapes, AI prompt priority, truncation cap, fallbacks                                                                                                 |
| `capabilities.ts`                    | `searchResultsVolumeCapabilities()` returns the per-pane flag set (M8c) and the shortcut toast text                                                                           |
| `capabilities.test.ts`               | Pins the flag shape, the purity contract, and the toast string                                                                                                                |

## State shape (post-M4)

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
see and iterate on the translated pattern, and (4) sets `lastAiCaveat` from the result. The `AiTransparencyStrip` is
visible whenever `lastAiPrompt` is non-null; it clears on `⌘N` (via `clearSearchState`) and on any successful non-AI
search (`executeSearch(fromAiTranslation = false)`).

There is **no `aiPrompt` state and no `namePattern` state**. M2 deleted both. Anywhere the old code read `aiPrompt` or
`namePattern`, the new code reads `query`. Anywhere the old code branched on `patternType`, the new code branches on
`mode` (with the mapping `regex => regex`, everything else => glob).

## Keyboard shortcuts (in-dialog, hard-coded)

| Shortcut  | Action                                                            |
| --------- | ----------------------------------------------------------------- |
| `Enter`   | Run search in the active mode (AI in AI mode, manual otherwise)   |
| `⌘Enter`  | Run AI search regardless of active mode (only when AI is enabled) |
| `⌘N`      | Clear all dialog state ("new search")                             |
| `⌘H`      | Toggle the recent-searches popover (fuzzy over the full history)  |
| `⌘1`      | Switch to AI (AI on) or Filename (AI off)                         |
| `⌘2`      | Switch to Filename (AI on) or Regex (AI off)                      |
| `⌘3`      | Switch to Regex (AI on); no-op when AI is off                     |
| `⌘4`      | Reserved for Content when it ships; not wired now                 |
| `⌥F`      | Set scope to the focused pane's current directory                 |
| `⌥D`      | Clear the scope (search the whole drive)                          |
| `⌥←`      | Navigate the active pane to the cursor row's parent folder        |
| `⌥→`      | Navigate the active pane to the cursor row's path (descend back)  |
| `↑` / `↓` | Move the cursor through the results list                          |
| `←` / `→` | When focus is on a mode chip: move between chips (skip Content)   |
| `Tab`     | Trapped within the dialog; cycles through interactive elements    |
| `Escape`  | Close the dialog                                                  |

The Content chip is visible-disabled with a "Coming soon" tooltip. It has **no** shortcut. Wiring a shortcut to a
disabled control is hostile UX (either silent no-op or a popup on every press); reserving `⌘4` is the better contract.
When Content ships, it claims `⌘3` and Regex moves to `⌘4`.

**`⌥F` and `⌥D` work globally**, including when the scope popover is closed. They live on the dialog's
`handleModifierShortcuts` and don't depend on focus being inside the scope textarea. The scope popover's footer mirrors
the same two actions as "Use current folder" and "All folders" buttons so mouse users have first-class access. This is
the explicit contract from search-redesign-plan §3.2.

**Esc inside an open filter-chip popover closes only the popover.** The dialog's Escape handler runs in capture phase on
`window`, which would otherwise fire before the popover's bubble handler. The dialog checks
`dialogElement.querySelector('.filter-chip-popover')` and, when a popover is present, returns without closing the
dialog. The popover's own keydown handler (on the popover element) then runs on the bubble, closes itself, and calls
`stopPropagation` so nothing else fires. Without this guard, Escape inside a popover would close the whole dialog and
lose the user's place. Pinned in `SearchFilterChips.svelte.test.ts`.

## Data flow

```
User presses ⌘F
  -> +page.svelte sets showSearchDialog = true
  -> SearchDialog mounts, calls prepareSearchIndex() IPC
  -> Backend starts async index load (2-3s), emits "search-index-ready" when done
  -> User types in the bar -> 200ms debounce -> searchFiles(query) IPC (filename/regex modes only)
  -> User presses Enter in AI mode -> translateSearchQuery -> populates filters -> searchFiles
  -> Results displayed, keyboard nav with ↑/↓, Enter navigates to file
  -> Dialog close -> releaseSearchIndex() IPC -> 5 min idle timer -> index dropped
```

## Key patterns

**Command palette pattern**: Own fixed overlay + backdrop, not `ModalDialog`. Needs custom keyboard handling (arrow keys
for results, Tab between filters) that would fight `ModalDialog`'s focus management.

**Two-cursor hover model**: Same as command palette. `cursorIndex` (keyboard) and `hoveredIndex` (mouse) are
independent.

**Live search with debounce**: 1 s debounce on filename/regex modes only, gated by the `search.autoApply` setting
(default on, in `Settings > Behavior > Search`). AI mode never auto-applies regardless of the setting: AI calls cost
money and the user must explicitly opt in via Enter / `⌘Enter` / the `⏎` run button on the right of the bar.

The debounce constant lives in `search-state.svelte.ts` as `SEARCH_AUTO_APPLY_DEBOUNCE_MS = 1000`. All auto-apply
callsites read it from there so changing the value is one edit. The bump from 200 ms to 1 s in M6 matches Spotlight's
feel on a 10M-entry index: the user gets to finish a word before we react. Enter / ⌘Enter / the ⏎ button bypass the
debounce for immediate search.

**Auto-apply gates (M6)**: `scheduleSearch()` returns early in three cases:

1. `mode === 'ai'`: AI never auto-applies.
2. `search.autoApply === false`: the user runs every search explicitly.
3. IME composition is in progress: we don't fire mid-character on Chinese / Japanese / Korean input. On
   `compositionend`, the parent calls `scheduleSearch` again so the user gets one fire after the composed character
   lands.

The setting is mirrored into the dialog's local `autoApplyEnabled` state via
`onSpecificSettingChange('search.autoApply', ...)`. Live-applied: toggling in the settings window updates the dialog
immediately without reopening.

**`⏎` run button**: Always visible on the right end of the bar. Clicking it is equivalent to pressing Enter in the
input: AI mode runs `runAiFromQuery()`, every other mode runs `executeSearch()`. The button is the mouse-first path; the
keyboard-first path is Enter.

**"Press Enter to search" hint**: Appears in the right gutter of the bar in `--color-text-tertiary` when (a) the query
is non-empty and (b) it has changed since the last actually-issued search and (c) auto-apply won't pick it up
(`mode === 'ai'` OR `search.autoApply === false`). Tracked by `lastRunQuery`, set by `executeSearch()` after a
successful backend call. `⌘N` resets `lastRunQuery` to `null` along with the rest of state.

**Scope row**: Below the chips. Comma-separated folder paths with `!` prefix for exclusions. Parsed via
`parseSearchScope()` IPC call in `executeSearch()` (async, so not part of `buildSearchQuery()`). ⌥F sets scope to the
focused pane's current directory, ⌥D clears it. Info button `(i)` shows syntax help tooltip.

**Index not available state**: When indexing is disabled or not started, `prepareSearchIndex()` errors. The dialog shows
a message ("Drive index not ready...") with scan progress if available. Inputs and filters are disabled.

**AI single-pass flow**: `executeAiSearch()` calls `translateSearchQuery()` once (LLM classifies intent into enums +
extracts keywords, Rust builds the query deterministically), then runs `executeSearch()`. No preflight, no refinement
pass. The previous two-pass system caused ~15% regressions; deterministic structure means there's nothing to refine.

**AI overwrites the bar; the strip preserves the prompt**: After AI translates, the bar shows the AI's translated
pattern (filename / regex), and `mode` flips accordingly. The user sees what was searched and can keep iterating. The
original natural-language prompt and the AI's caveat are surfaced in the `AiTransparencyStrip` below the chip row. The
strip is the source of truth for "what did I ask the AI?" once the bar has been overwritten. Lifecycle:

- `executeAiSearch(trimmed)` sets `lastAiPrompt = trimmed` BEFORE calling `translateSearchQuery`. The capture is
  unconditional: even if the IPC fails, the user still sees what they asked.
- After the translation succeeds, `lastAiCaveat = translateResult.caveat ?? null`.
- `executeSearch(fromAiTranslation: boolean)` clears both fields when `fromAiTranslation` is false. `executeAiSearch`
  passes `true`, so the AI flow's tail (`executeSearch(true)`) leaves the strip intact.
- `clearSearchState()` (called by `⌘N`) clears both fields.

The disabled "Refine…" button on the strip is the placeholder for the chat-back UX. No keyboard shortcut is wired (same
contract as the Content mode chip: visible-disabled with an explanatory tooltip is fine; shortcut-but-no-op is hostile).

**Auto mode fallback when AI gets disabled mid-session**: If the AI provider is switched off while the dialog is open
and the active mode is `ai`, the dialog quietly flips to `filename`. The user wouldn't be able to run a search
otherwise.

**IME composition guard**: The dialog tracks `imeComposing` via `oncompositionstart` / `oncompositionend` on the search
bar input. While composing, `scheduleSearch()` is a no-op so we don't fire mid-character on Chinese / Japanese / Korean
input. On `compositionend` the dialog calls `scheduleSearch()` once so the user gets exactly one auto-apply fire after
the composed character lands. Non-negotiable for IME users: see search-redesign-plan §3.6.

**Deferred loading indicator**: The "Loading drive index..." message in the results area only appears when the user has
triggered a search while the index is still loading. On initial open, the results area is empty (no loading message)
since the user is still typing their query.

**State preservation across close + reopen**: The module-level `$state` in `search-state.svelte.ts` survives dialog
unmount. Closing the dialog (Escape or overlay click) does NOT wipe query, mode, filters, scope, results, or cursor.
Reopening the dialog lands the user back where they left off. The only reset path is `⌘N` ("new search") inside the
dialog, which calls `clearSearchState()` and refocuses the bar.

**`⌘N` shortcut**: Hard-coded in `SearchDialog.svelte`'s `handleModifierShortcuts`. Captured before the dialog's global
`stopPropagation` would let it reach the route-level `⌘N` (new tab) handler. The choice of `⌘N` matches the macOS "new
X" idiom (new tab, new document) for the same reason the user reads "fresh search" the same way.

**Path pills (M7, §3.8)**: Each result row's path column renders as a strip of clickable ancestor pills produced by
`PathPills.svelte`. Clicking a pill calls the dialog's existing `onNavigate(ancestorPath)` callback, which closes the
dialog and navigates the active pane to that ancestor — the same exit path "navigate to a file" already uses. Pills are
**not** in the keyboard Tab order (`tabindex="-1"`): tabbing through them would break the row's arrow-down keyboard flow
inside the virtualized list. The keyboard equivalents are `⌥←` (jump to the cursor row's parent) and `⌥→` (descend back
to the cursor row's path). Paths are split strictly on `/`; macOS and Linux only, no `\` handling. The pill's `onclick`
calls `e.stopPropagation()` so it doesn't double-fire the row's `onResultClick`. Svelte 5 delegates events at the
document root, so unit tests assert against the `stopPropagation` spy rather than racing a wrapper DOM listener.

**Per-row `…` menu (M7, §3.9)**: `SearchRowMenu.svelte` renders an ellipsis button on every row. The cursor row's button
is always visible (`.is-cursor` → `opacity: 1`); other rows' buttons render with `opacity: 0` and fade in on row hover
(CSS sibling selector in `SearchResults.svelte`). Both the button click and a right-click on the row call
`onRowMenu(entry)` on the parent, which routes to the existing native `showFileContextMenu` factory (the same one
`FilePane` uses). The native menu carries Open, Reveal in Finder (or Open in file manager on Linux), Copy path, Copy
name, plus the existing "Open with…" subtree — a superset of the spec's four core entries, all already keyboard-
accessible on macOS.

**Footer right-edge actions (M7, §3.9)**: `SearchFooterActions.svelte` sits at the right of the dialog footer, opposite
the recent-searches strip. It renders two buttons whenever `results.length > 0`:

- **"Open in Finder" (macOS)** / **"Open in file manager" (Linux)**: reveals the cursor row in the platform's file
  manager via the existing `showInFinder` IPC (`open -R` on macOS, `xdg-open` on the parent on Linux). The dialog stays
  open so the user can keep browsing results.
- **"Open in pane"**: the primary action. M7 ships this as a **STUB**: clicking closes the dialog and shows a "coming in
  M8" toast. M8 wires the real handoff (snapshot store + virtual-volume push). The stub keeps the affordance
  discoverable without overpromising.

Both buttons are hidden (not just disabled) on empty/idle state, because they have nothing to act on. Empty + idle
inputs disable both (index not ready). The platform branch uses `isMacOS()` from `$lib/shortcuts/key-capture`.

**Snapshot store (M8a, §3.7)**: `snapshot-store.svelte.ts` holds `SearchSnapshot` records (query, mode, filters, scope,
capped 10,000 entries, totalCount, createdAt, friendly label) under monotonic `sr-N` ids, plus a per-record refcount.
M8a only wires the store and the bookkeeping; M8b connects it to the pane view. The store has no hard cap on its own —
**refcount is the only authority**. Refs come from two sources:

- **Pane history entries** whose `path` starts with `search-results://<id>` hold +1 per occurrence. The tab-state
  manager (`pushHistoryEntry` and the closed-tab lifecycle) drives inc/dec — `navigation-history.ts` itself stays pure
  (no snapshot-store import). Pushing past `MAX_HISTORY_PER_TAB = 100` evicts the oldest entry, and truncating forward
  on a new push after `back()` evicts the discarded tail. Both kinds of eviction surface via `push()`'s `droppedEntries`
  return field, and `pushHistoryEntry` releases the matching refs in one step.
- **The "last dialog attempt" slot** (`setLastAttemptId`) holds +1 for the most-recent dialog search regardless of
  whether any pane references it. Swaps decrement the old id and increment the new one atomically. M8b wires the dialog
  to call this on each new search.

**Closed-tab lifecycle and refs**: tab close via `closeTabRecording` does NOT release refs; ownership transfers to the
`ClosedTab` entry on the pane's closed-tab stack (cap default 10). Reopen (`⌘⇧T`) just pops the entry back — no
double-count. The refs only release when the closed-tab stack evicts the entry (cap overflow in `pushClosed`, or manual
`trimClosedStack`). Implemented via the `transferSnapshotRefs(closedTab, 'transfer' | 'release')` helper in
`tab-state-manager.svelte.ts`. The non-recording `closeTab` / `closeOtherTabs` (used in tests and programmatic flows)
release refs immediately, since nothing else holds them.

**`{#key activeTabId}` recreation is safe**: history lives on `TabState`, not on the pane. The dual-pane explorer
destroys and recreates `FilePane` on tab switch (cold load), but `TabManager` survives, and the per-tab `history` field
is untouched. Snapshot refs therefore persist across pane recreation. Documented inline in `snapshot-store.svelte.ts`'s
header comment so the next agent doesn't need to re-verify.

**Capability flags (M8c, §3.7)**: `capabilities.ts::searchResultsVolumeCapabilities()` returns the per-pane flag set
`{ canPasteInto: false, canMkdir: false, canMkfile: false, canRename: false, isSourceOK: true }`. Consumers:

- **F-key bar** (`lib/file-explorer/pane/FunctionKeyBar.svelte` mounted in `routes/(main)/+page.svelte`): the bar takes
  `canMkdir` / `canMkfile` / `canRename` / `canSourceOps` / `canPasteInto` props. When the focused pane is on
  `volumeId === 'search-results'`, F2 (Rename), F7 (New folder), and Shift+F4 (New file) render visibly disabled. F5 /
  F6 / F8 (Copy / Move / Delete) stay enabled because the snapshot row is source-OK. The page reads the focused volume
  via the new `onFocusedVolumeChange` callback `DualPaneExplorer` fires whenever `focusedPane` or the active tab's
  `volumeId` on the focused side changes.
- **Right-click context menu** (`lib/file-explorer/pane/SearchResultsView.svelte` → `showFileContextMenu` →
  `src-tauri/src/menu/menu_structure.rs::build_context_menu`): the IPC now takes a `restrictDestinationActions` flag.
  When `true`, the Rust menu builder omits Rename and New folder. Source-side items (Open, Copy, Move, Delete, Show in
  Finder, Copy filename, Copy path) stay. Capabilities flow from `searchResultsVolumeCapabilities()` to the IPC; the
  flag is set when `!canRename && !canMkdir`.
- **Keyboard shortcut dispatch** (`routes/(main)/command-dispatch.ts::blockedBySearchResultsPane`): catches `⌘V`
  (`edit.paste`), `⌘⌥V` (`edit.pasteAsMove`), `F7` (`file.newFolder`), Shift+F4 (`file.newFile`), and `F2` /
  `file.rename` when the focused pane is `search-results`. Surfaces the friendly toast
  `"Search results aren't a folder. Paste into a real folder instead."` (the canonical string lives in `capabilities.ts`
  as `SEARCH_RESULTS_NOT_A_FOLDER_TOAST`). Toasts are the LAST RESORT here — the F-bar and the native context menu
  disable the same actions at the source, so the toast only fires when a shortcut bypasses the visible UI. (Per
  `docs/design-principles.md`: "disabled is better than 'you did the wrong thing' toasts.")

**Cross-snapshot delete sync (M8c, §3.7)**: When the user deletes a row from a search-results pane, the delete dialog
runs against the real file path (the snapshot stores absolute paths). On `handleTransferComplete` for
`op === 'delete' | 'trash' | 'move'`, `dialog-state.svelte.ts` calls `removeEntryFromAllSnapshots(sourcePath)` once per
deleted path. That helper:

1. Walks every stored snapshot and replaces its `entries` array with one that excludes the deleted path (preserves
   reference identity on the unchanged entries; only the array changes).
2. Bumps a module-level `mutationTick` `$state` whenever at least one snapshot was mutated.
3. Leaves `totalCount` alone — the existing `entries.length` vs `totalCount` mismatch is the truncation signal.

`SearchResultsView.svelte`'s snapshot lookup reads `getMutationTick()` inside its `$derived` so the view re-renders
after a delete. Without the tick, the `Map` mutation would be invisible to Svelte reactivity (snapshots aren't `$state`
themselves, by design — see the store's header).

The search-results pane's own `openDeleteDialog` path is plumbed in
`DualPaneExplorer.svelte::openDeleteFromSearchResults`: it reads the cursor row's entry from the snapshot, builds a
one-item `DeleteSourceItem`, and routes through the same `showDeleteConfirmation` dialog every other delete uses.
`supportsTrash = true` (the underlying file is on the local volume) and `sourceVolumeId = DEFAULT_VOLUME_ID`.

**Source-side ops from the snapshot pane (M8d, §3.7)**: With `isSourceOK: true`, Cmd+C / Cmd+X / F5 / F6 / drag-out now
run against the cursor + selection in the snapshot pane. The snapshot pane shares `FilePane.selection` state with normal
panes: Space toggles the cursor row, Insert toggles + advances, Cmd+click toggles, Shift+click ranges, Cmd+A /
Cmd+Shift+A work as elsewhere. `effectiveTotalCount` returns `snapshot.entries.length` for search-results panes so
range-select spans the full result set without bumping through `..`. Wire path:

- **Cmd+C / Cmd+X** route through `DualPaneExplorer.copyToClipboard` / `cutToClipboard`, which detect the snapshot pane
  via `getSnapshotClipboardPaths` and call `copy_paths_to_clipboard` / `cut_paths_to_clipboard` (paths-by-value sibling
  IPCs of the listing-id-keyed `copy_files_to_clipboard` family). The Rust commands reuse
  `clipboard::write_file_urls_to_clipboard` and `set_cut_state` / `clear_cut_state`, so the system clipboard contract
  (file URLs + newline-separated text) is identical.
- **F5 / F6** route through `openUnifiedTransferDialog`, which detects `volumeId === 'search-results'` and calls
  `transfer-operations::buildTransferPropsFromSnapshot` instead of the listing-id-driven builders. The snapshot's
  selected (or cursor) entries are resolved to paths via `snapshot-store::resolveSnapshotPaths`, fed into the same
  `TransferDialogPropsData` shape every transfer uses, and the existing `copy_files` / `move_files` IPCs run with
  `sources: Vec<String>` (no IPC contract change needed; those commands already take paths).
- **Drag-out** uses the new `'paths'` drag context in `lib/file-explorer/drag/drag-drop.ts`: when `FullList` is rendered
  with `staticEntries` and the user drags a selection, the FE builds a paths array from `getEntryAt(idx)` and routes
  through `start_drag_paths` (which accepts paths directly) instead of the listing-id-keyed `start_selection_drag`.
- **Post-move snapshot cleanup**: already covered by the existing M8c hook. `handleTransferComplete` calls
  `removeEntryFromAllSnapshots(sourcePath)` for every source path on `delete | trash | move`. So after F6 from the
  snapshot pane, the moved rows disappear from every snapshot that referenced them (the underlying file is gone, the row
  reflects that).

Destination-side write ops are still blocked: pasting INTO a search-results pane shows the canonical
`SEARCH_RESULTS_NOT_A_FOLDER_TOAST` (via the F-bar disablement, the menu item omission, and the dispatcher's
`blockedBySearchResultsPane` guard). `openTransferDialog` also blocks F5/F6 when the OPPOSITE pane is a snapshot, so the
shortcut path can't accidentally route a copy/move INTO a snapshot.

## Key decisions

**Decision**: Unified search bar plus mode chips instead of two separate input rows. **Why**: The AI prompt and the
filename pattern were two ways to ask the same question, sitting in two rows, each with its own input outlined. That
made them feel like competing features. Collapsing to one input with a chip-row discriminator below mirrors the modal
patterns of Spotlight and Raycast, halves the visual weight of the dialog's top, and lets the chip-row carry visible
keyboard hints (`⌘1`/`⌘2`/`⌘3`) without crowding the input.

**Decision**: No shortcut wired to the disabled Content chip. **Why**: A shortcut that silently no-ops on a disabled
control is worse than no shortcut. Reserving `⌘4` for Content when it ships keeps the user's mental model intact (the
numbers match the visible chip positions) and avoids "I pressed the key and nothing happened" moments.

**Decision**: Dialog, not a panel or sidebar. **Why**: Search is a focused, transient task. A command-palette-style
overlay matches this usage pattern and doesn't consume permanent screen real estate.

**Decision**: Structured filters always visible (not hidden behind "advanced"). **Why**: The filter row is compact (one
line) and makes the query model transparent. Users see exactly what's being searched. M3 will move them into chips with
popovers, but they stay always-on.

**Decision**: Recent-search entries are added only on "Open in pane", not on Enter / auto-apply. **Why**: David's
explicit design call (search-redesign-plan §3.5). The 1000-entry budget stays signal-rich (results worth acting on)
instead of polluted with every keystroke-debounced auto-apply. The IPC commands don't enforce this — the gate is the
frontend `addRecentSearch` call site (which M8 wires to the Open-in-pane handler). For M5 the IPC + footer + popover
ship; for local testing you can seed `{app_data_dir}/search-history.json` by hand.

**Decision**: AI mode example chips re-run on click. **Why**: AI mode's "explicit user trigger" rule (the user must
press Enter / ⌘Enter to spend an LLM call) counts a click as a trigger. The same applies to recent-search AI entries
(footer chip click + popover Enter both run). The user-friendliness call from the plan §3.4 is that anything they
deliberately picked from the dialog is the same kind of "yes, please" as pressing Enter.

**Decision**: `RecentSearchesPopover` reuses `FilterChipPopover` for positioning + focus trap + Esc-scoped close.
**Why**: The plan calls for a sub-overlay-of-an-overlay with the same auto-flip, focus-trap, and "Esc closes only the
popover" semantics as the filter chips. Reimplementing those would risk drift; reusing the primitive guarantees the
contract documented in the SearchDialog `CLAUDE.md` (Escape capture-phase guard) covers both popover kinds via the
single `.filter-chip-popover` DOM selector.

## Gotchas

**Gotcha**: `stopPropagation()` on every `keydown`. **Why**: Without this, keys propagate to the file explorer behind
the dialog and trigger quick-search or navigation.

**Gotcha**: `prepareSearchIndex()` failure means index unavailable. **Why**: The backend returns an error when
`get_read_pool()` returns `None` (indexing disabled or not started). The dialog catches this and enters the disabled
state.

**Gotcha**: Don't call `clearSearchState()` from `onDestroy`. **Why**: The dialog's lifecycle (mount on open, unmount on
close) doesn't match the user's mental model of "the search I was working on." Wiping state on unmount turned every
close + reopen into a lost-work moment. The only sanctioned reset path is `⌘N`. If you find yourself wanting to wipe
state from a lifecycle hook, you probably want a user-initiated action instead.

**Gotcha**: The AI's translation overwrites `query` and `mode`. **Why**: We want the bar to show what was searched, not
the natural-language prompt. The original prompt is preserved separately in `lastAiPrompt` (set by `executeAiSearch`
before the IPC call) so the `AiTransparencyStrip` can render it. Anyone building on top of this should not assume
`query` still contains the user's natural-language input after an AI run; use `getLastAiPrompt()` instead.

**Gotcha**: `nested-interactive` axe warning on the populated-results a11y test is skipped. **Why**: M7's row gains
interactive children (path-pill buttons + the `…` menu button) inside the `role="option"` row. Tab order is suppressed
via `tabindex="-1"` per spec (§3.8), but axe still flags the structural nesting. Cleanly fixing it means either dropping
the row's `role="option"` (and surfacing the cursor via a custom mechanism) or hoisting the buttons out of the row's
grid cell — both are out of M7 scope. The test stays `it.skip` with a TODO so the gap is visible to future work.

**"Open in pane" (M8b)**: Click on the footer's "Open in pane" button promotes the current result set into a real pane
view via the `search-results://<id>` virtual volume. The handler in `SearchDialog.svelte::openInPane`:

1. Builds a `SearchSnapshot` from live state (`getResults()` / `getMode()` / `getQuery()` / filters / scope / flags).
2. Mints a fresh id via `nextSnapshotId()` and stores via `getOrCreate(id, snapshot)`.
3. Pins the snapshot via `setLastAttemptId(id)` so refcount stays ≥1 even before history pushes.
4. Calls `addRecentSearch(historyEntry)`. **This is the one and only call site that adds to recent searches** (per plan
   §3.5: auto-applies and Enter-runs don't pollute the history). For AI mode, the entry's `query` carries the original
   natural-language prompt (via `getLastAiPrompt()`), not the AI's translated pattern.
5. Calls `onOpenInPane?.(id)` to hand off to the host (`+page.svelte` → `DualPaneExplorer.openSearchSnapshotInPane`),
   which routes through `handleVolumeChange` so pinned-tab fork / focus / history-push all apply uniformly.
6. Closes the dialog. State is preserved (module-level `$state` survives unmount); ⌘F reopens to the same place.

The label shown in the pane breadcrumb (and the snapshot's `label` field) is built by
`snapshot-label.ts::buildSnapshotLabel`:

- AI mode: the original prompt, truncated to ~40 chars with a `…` suffix.
- Filename mode: the pattern as-is (`*.pdf`).
- Regex mode: the pattern wrapped in slashes (`/pattern/`).

## References

- [AI search eval history](../../../../../docs/notes/ai-search-eval-history.md) -- Four rounds of prompt tuning for the
  AI natural language to structured query translation, with a 30-query test catalog and lessons learned.

## Dependencies

- `$lib/tauri-commands` -- `prepareSearchIndex`, `searchFiles`, `releaseSearchIndex`, `translateSearchQuery`,
  `parseSearchScope`, `getRecentSearches`, `addRecentSearch`, `removeRecentSearch`, `clearRecentSearches`,
  `applyRecentSearchesMaxCount`, `showFileContextMenu` (row context menu), `showInFinder` (footer Open in Finder / file
  manager)
- `$lib/ui/toast/toast-store.svelte` -- `addToast` for the M7 "Open in pane: coming in M8" stub
- `$lib/shortcuts/key-capture` -- `isMacOS()` for the footer action's macOS/Linux label fork
- `@leeoniya/ufuzzy` -- fuzzy filtering inside `RecentSearchesPopover`
- `$lib/indexing` -- `isScanning`, `getEntriesScanned` (scan progress for unavailable state)
- `$lib/settings` -- `getSetting('ai.provider')` (AI chip visibility, ⌘ shortcut numbering)
- `$lib/tooltip/tooltip` -- chip tooltips (Content chip's "Coming soon" copy)
- CSS variables from `app.css` (`--z-modal`, `--color-accent-subtle`, `--color-bg-secondary`, etc.)
