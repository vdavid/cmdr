# File explorer module

Dual-pane file explorer with keyboard-driven navigation, file selection, sorting, command palette, and adaptive layout.

## Selection (`selection/`)

### User interaction

- **Space**: toggle selection at cursor
- **Shift+click / Shift+arrow**: range selection with anchor (A) and end (B). If anchor already selected, range
  deselects.
- **Cmd+A / Cmd+Shift+A**: select all / deselect all
- **".." entry can't be selected**
- **Cleared on navigation**: selection is per-directory

### Implementation

- **State**: `SvelteSet<number>` (from `svelte/reactivity`) in `FilePane.svelte`. O(1) add/remove/has
- **Preserved on sort/filter**: `resort_listing` accepts `selectedIndices[]`, returns `newSelectedIndices[]`
- **Write operations receive indices**: backend resolves to paths from cached listing
- **Visual**: `--color-selection-fg` (yellow foreground)

### Operation lifecycle

- **Snapshot**: when an operation is confirmed, FilePane snapshots selected file names into `operationSelectedNames` (or
  `'all'` sentinel if all selected)
- **Diff-driven adjustment**: on each `directory-diff` during an operation, selection is re-resolved via
  `findFileIndices` batch IPC. A `diffGeneration` counter discards stale async results.
- **Cursor adjustment**: cursor index is also adjusted on structural diffs using the same `adjustSelectionIndices`
  mechanism (treating cursor as a single-element selection)
- **Source-item-done deselection**: `write-source-item-done` events individually deselect completed items (for copy and
  other ops that don't trigger diffs)
- **Clear on complete/error**: safety net clears all selection on the source pane
- **Cancel behavior**: selection reflects survivors. For `'all'` sentinel: calls `selectAll()` (move/delete/trash) or
  leaves untouched (copy)

### Gotchas

- **Parent offset**: when `hasParent`, frontend indices = backend indices + 1
- **Range shrinking**: moving cursor back toward anchor removes items no longer in range
- **Optimization flag**: `allSelected: true` avoids sending 500k indices over IPC
- **`allSelected` + cancel**: calls `selectAll()` for move/delete/trash (source listing changed), leaves untouched for
  copy (source listing unchanged)
- **Both panes same directory**: only source pane selection is adjusted; the other pane's selection may become stale
- **Snapshot timing**: must happen at confirm, not when progress dialog opens (same-FS moves are instant and may
  complete before dialog)
- **Snapshot covers clipboard paste**: `startTransferProgress` also snapshots, not just `handleTransferConfirm` /
  `handleDeleteConfirm`

## Navigation (`navigation/`)

### Back/forward navigation

- **Per-pane history**: independent stacks, session-only (not persisted)
- **Cross-volume**: works across local drives, network shares, MTP devices
- **Deleted folder handling**: walks up parent tree until existing dir found. Skipped entries remain in history.
- **Shortcuts**: Cmd+[ (back), Cmd+] (forward), Backspace (parent)

### Gotchas

- **History entries are paths**: stored as strings. No FileEntry metadata cached in history.
- **Volume switching**: changes volume context. History still tracks old volume's paths.

## Sorting

### Behavior

- **Directories first**: always
- **Natural sorting**: `file10.txt` after `file2.txt`
- **Extension grouping**: dotfiles → no-extension → by extension alphabetically
- **Per-tab sort**: each tab owns its `sortBy` + `sortOrder` (no global per-column memory)
- **Directory sort mode**: setting `listing.directorySortMode` controls how dirs sort among themselves:
  - `likeFiles` (default): dirs sort by the active column (uses `recursive_size` for Size). Dirs with unknown size sort
    last.
  - `alwaysByName`: dirs always sort by name, ignoring the active sort column.
- **Name ASC tiebreaker**: when primary sort values are equal, entries fall back to name ascending

### Implementation

- **Efficient re-sort**: `resort_listing` re-sorts cached listing without disk reads
- **Preserves cursor by filename**: frontend sends current filename, backend returns new index
- **Preserves selection by indices**: backend resolves filenames, re-finds after sort
- **Dir sort mode flows via IPC**: `directorySortMode` passed to `listDirectoryStart` and `resortListing`, stored in
  `CachedListing` so watcher re-sorts use the correct mode

### Gotchas

- **A and B cleared after sort**: range selection anchor/end reset (sorting is "new context")
- **Selected indices remapped**: backend returns `newSelectedIndices[]`, frontend updates `Set`
- **Dir size = `recursive_size`**: for sorting, dirs use `recursive_size` (from drive index), not `size` (always None)

## Command palette (`../command-palette/`)

### Features

- **Fuzzy search**: `@leeoniya/ufuzzy` (~3.5KB), typo-tolerant
- **Match highlighting**: matched characters underlined
- **Persisted query**: remembered within session
- **~60 commands**: all scopes (app, main window, file list, network, about)

### Gotchas

- **Low-level nav commands hidden**: `showInPalette: false` for arrow keys, Page Up/Down
- **Execution handler in +page.svelte**: `handleCommandExecute` delegates to `explorerRef`

## Git (`git/`)

Breadcrumb chip + status-column helpers + per-repo reactive store. Subscribe-driven, never polls.

- **`RepoChip.svelte`**: Pill rendered in the breadcrumb header. Six visual states: clean, ahead, behind, dirty,
  detached, unborn. Tooltip carries the long-form status sentence (used by screen readers via `aria-label`).
- **`git-store.svelte.ts`**: Per-repo reactive `RepoInfo` map with refcounted subscriptions. Two panes on the same repo
  share one watcher. Live updates flow via `git-state-changed` Tauri event.
- **`status-column.ts`**: Pure helpers (`glyphFor`, `labelFor`, `fetchStatusMap`) for the optional status column in Full
  mode. Each cell carries a single-glyph code with a long-form `aria-label` and tooltip.

`FilePane.svelte` wires the chip on every `currentPath` change: it does a one-shot `lookupRepoInfo`, then
`subscribeToRepo` on a new repo, and `unsubscribeFromRepo` on unmount or path-off-repo. The chip respects the
`fileExplorer.git.showRepoChip` setting. `FilePane` also forwards `gitRepoRoot` and `showGitColumn` to `FullList`, which
drives the optional status column from its own `fetchStatusMap` + `git-state-changed` subscription.

For the full module map, decisions, and gotchas, see `git/CLAUDE.md`.

## Network browser (`network/`)

- **NetworkBrowser.svelte**: Top-level network view; lists discovered servers
- **ShareBrowser.svelte**: Lists shares on a selected server
- **NetworkLoginForm.svelte**: Credential entry for authenticated SMB connections
- **network-store.svelte.ts**: Reactive state for discovered servers, selected server/share, and auth mode

## Operations (`operations/`)

- **apply-diff.ts**: applies file-watcher diffs (add/remove/modify events) to a cached listing in-place
- **adjust-selection-indices.ts**: pure function that maps selected indices from an old listing to their positions in a
  new listing, given removed and added indices. Also used for cursor index adjustment on structural diffs.

## TCC-restricted treatment

Sidebar entries (`VolumeBreadcrumb.svelte`) AND file-list rows (`views/FullList.svelte`, `views/BriefList.svelte`) flag
paths in the runtime "TCC-restricted" set with italic + opacity-0.6 styling, a Lucide `info` icon, and a tooltip
pointing the user at System Settings → Privacy & Security → Full Disk Access (or per-folder Files & Folders → Cmdr). The
file-list Size column shows `<no perms>` for these rows instead of the misleading `0` the indexer recorded after a
denied scan. `pickSizeDisplay(entry, isRestricted)` in `views/full-list-utils.ts` is the single source of truth for that
override; `measure-column-widths.ts::computeFullListColumnWidths` accepts an optional `isRestricted` fn so the Size
column tracks the `<no perms>` text width during pre-DOM measurement. The state lives in
`$lib/stores/restricted-paths-store.svelte` (`isRestricted(path)`); see `navigation/CLAUDE.md` § "Restricted-folder
indicator (TCC)" and `apps/desktop/src-tauri/src/restricted_paths/` for the backend.

## Rename (`rename/`)

Inline rename with validation, conflict resolution, and an extension change confirmation dialog.

- **InlineRenameEditor.svelte**: inline text editor for renaming files directly in the file list
- **RenameConflictDialog.svelte**: dialog shown when the new name conflicts with an existing entry
- **ExtensionChangeDialog.svelte**: confirmation dialog when the file extension is being changed
- **rename-activation.ts**: logic for triggering rename mode
- **rename-operations.ts**: rename execution and error handling
- **rename-state.svelte.ts**: reactive state for the rename editor

## Pane (`pane/`)

Core explorer UI components:

- **DualPaneExplorer.svelte**: root component; manages both panes, unified key/command handlers, MCP exports
- **FilePane.svelte**: single pane (navigation, listing, cursor, selection, view mode)
- **DialogManager.svelte**: renders all modal dialogs (transfer, delete, rename, new-folder, etc.)
- **FunctionKeyBar.svelte**: F1–F10 bar at the bottom of the window
- **MtpConnectionView.svelte** / **NetworkMountView.svelte**: placeholder panes for MTP/network mount states
- **PaneResizer.svelte**: drag handle between the two panes
- **ErrorPane.svelte**: unified error display for listing failures. See [Error display](#error-display) below.
- **VolumeUnreachableBanner.svelte**: shown when a tab's volume resolution timed out at startup (retry + open home), and
  also when the SMB reconnect manager has given up after exhausting its backoff cycle (retry + disconnect, `smbGaveUp`
  variant)
- **SmbReconnectingView.svelte**: shown while the per-volume SMB reconnect cycle is running (waiting/attempting).
  Spinner + progress bar for the current backoff window + dynamic body text. Three actions: Retry now / Cancel /
  Disconnect. Driven by `smb-reconnect-manager.svelte.ts` in `network/`.
- **selection-state.svelte.ts**: reactive selection set (indices) with range/toggle helpers
- **sorting-handlers.ts** / **transfer-operations.ts** / **tab-operations.ts**: pure logic extracted from
  DualPaneExplorer
- **initialization.ts**: startup logic (load persisted tabs + app status, resolve volumes, apply E2E overrides, create
  tab managers)
- **index-events.ts**: throttled index-dir-updated handler with macOS `/private/` symlink resolution
- **dialog-state.svelte.ts**: dialog state and handlers (transfer, delete/trash, new folder, alert, error) extracted
  from DualPaneExplorer via factory pattern. `TransferErrorPropsData` carries an optional `FriendlyError` (from the
  backend `write-error` event payload) alongside the typed `WriteOperationError`;
  `handleTransferError(error, friendly?)` accepts both and stores them so the rendered dialog (see
  `file-operations/CLAUDE.md`) can prefer the backend copy.
- **rename-flow.svelte.ts**: rename flow logic (validation, conflict/extension dialogs) extracted from FilePane
- **type-to-jump-state.svelte.ts** / **TypeToJumpIndicator.svelte**: type-to-jump factory + the "Jump: …" chip. See
  [Type-to-jump](#type-to-jump) below.
- **volume-tint.svelte.ts**: per-pane background tinting by volume kind (local / SMB / MTP). Reads
  `appearance.tint{Local,Smb,Mtp}` reactively, returns a `color-mix(in oklch, ...)` expression that `FilePane.svelte`
  applies as inline `background-color`. Mix share flows through `--pane-tint-{bg,fg}-pct` so dark mode and
  `prefers-contrast: more` can each dial it up without re-evaluating the helper. Live-tuned matrix: 10% (light), 15%
  (light AAA), 15% (dark), 25% (dark AAA) — dark needs more because there's less luminance headroom against `#1e1e1e`.
  Pure classifier `volumeKindFor` is unit-tested separately in `volume-tint.test.ts`.

### Type-to-jump

Per-pane in-directory navigation. The user types letters/digits in a focused pane → the cursor jumps to the
highest-scoring fuzzy match. A bottom-right "Jump: tes" chip shows the live buffer. Backend match runs in
`apps/desktop/src-tauri/src/file_system/listing/fuzzy_jump.rs` via the `find_first_fuzzy_match` IPC.

**State** (one factory instance per pane, in `FilePane.svelte`):

- `buffer`: chars typed since the last reset (lowercased).
- `indicatorVisible` / `indicatorStale`: asymmetric timeouts (see below).
- `generation`: monotonic counter bumped per keystroke. The async match callback discards responses where
  `generation !== state.generation` (same race-protection pattern as `adjust-selection-indices.ts`'s `diffGeneration`).

**Two timers**:

- **Buffer reset** (configurable, default 1000 ms via `fileExplorer.typeToJump.resetDelay`): empties `buffer` but keeps
  the indicator visible in a "stale" (italic + dim) state. The cue tells the user the next keystroke starts fresh.
- **Indicator hide** (hardcoded 5000 ms): removes the chip entirely.

**Reset triggers** (call `clearJumpState()`): ESC, arrows / Page / Home / End / Enter / Tab / Backspace (handled in
`DualPaneExplorer.svelte`'s key intercept), rename-mode entry, context-menu open, drag start, pane switch, tab switch,
directory change, re-sort, and listing replace.

**Parent offset gotcha**: the IPC returns a backend index (no `..`). The frontend prepends `..` when `hasParent`, so the
cursor index is `backendIndex + 1` when `hasParent` is true. Forget that and the cursor lands one row off on every
match.

**Streaming listings**: a single keystroke = exactly one match against the cache as it stands at that moment. We don't
auto-jump on subsequent `listing-progress` events, as that would move the cursor under the user without input, violating
top-5 principle 3 ("the user is always in control").

**MCP surface**: when the buffer or indicator is live, `FilePane` mirrors
`{ buffer, indicatorVisible, indicatorStale, lastMatchedName }` into the synced `PaneState.typeToJump`, so MCP-driven
E2E tests can assert the feature without poking at the DOM. See `src-tauri/src/mcp/CLAUDE.md` § State stores.

### Live disk space

The status bar and usage bar below each pane show live disk space. `FilePane` registers with the backend space poller
(`space_poller.rs`) via `watchVolumeSpace(paneId, volumeId, path)` on mount and volume change, and listens for
`volume-space-changed` events. The watcher key is the pane ID, so two panes on the same volume have independent
registrations (one pane navigating away doesn't affect the other). The backend deduplicates by volume_id, polls each
volume at its own cadence (`Volume::space_poll_interval()`: 2 s local, 5 s network/MTP), and emits only when the change
exceeds a configurable threshold (Settings > Advanced). The volume dropdown (`volume-space-manager.svelte.ts`) uses a
separate on-demand fetch and is unaffected.

## Error display

When a directory listing fails, the user sees a full-pane `ErrorPane` instead of the file list. This replaces the old
raw "I/O error: Operation timed out (os error 60)" text and the separate `PermissionDeniedPane` with a unified, warm,
and actionable error experience.

### How it works

1. `listing-error` Tauri event arrives with `{ message, friendly?: FriendlyError }`
2. `FilePane` checks: is this an MTP volume? → short-circuit to `MtpConnectionView` (MTP has its own UX)
3. Does the path still exist? → if gone, auto-navigate to nearest valid parent (not an error state)
4. Path exists but listing failed → render `ErrorPane` (if `friendly` is present) or raw error div (if not)

### `ErrorPane.svelte`

Receives a `FriendlyError` struct from Rust (all content is pre-baked on the backend, the frontend doesn't do any error
classification or OS-specific logic):

- **Title**: large text, always in accent color. Lucide icon (via `unplugin-icons`) signals severity: ⚠
  `~icons/lucide/triangle-alert` in warning color for transient, ⊘ `~icons/lucide/circle-alert` in error color for
  serious, no icon for needs-action
- **Folder path**: shown in secondary text so the user knows exactly which folder is affected
- **Explanation**: rendered as markdown via `snarkdown` (plain-language description of what happened)
- **Suggestion**: rendered as markdown (actionable steps, often provider-specific, for example, "Open **MacDroid** and
  check that your phone is connected")
- **"Try again" button**: shown only for `transient` category. Calls `navigateTo(currentPath)` to retry the listing.
  Tracks retry count and timestamps, displays them in the technical details ("Retry #2 · first try 45s ago · last try
  12s ago")
- **"Open System Settings" button**: shown for permission-denied errors on macOS (reuses `openPrivacySettings()`)
- **Collapsible "Technical details"**: shows the raw errno name and code for power users / bug reports
- **Markdown links**: anchors in the explanation/suggestion are routed through a click delegate.
  `x-apple.systempreferences:` URLs go through `openSystemSettingsUrl()` (a dedicated Rust IPC, since Tauri's opener
  plugin only allows http/https/mailto/tel by default). Other URLs go through `openExternalUrl()`. Anchors override the
  global `cursor: default` with `cursor: pointer`.
- **⌘C / ⌘A in this pane**: the dispatcher's text-region intercept (`command-dispatch.ts :: handleTextRegionShortcut`)
  treats `.error-pane` as a text zone. ⌘C copies the text selection; ⌘A selects the whole pane (including hidden
  `<details>` content). Neither logs `FE:user-action` nor fires file-scope behavior.

### For future agents

The error messages and provider suggestions live in **Rust** (`file_system/volume/friendly_error.rs`), not in this
Svelte component. The frontend is intentionally thin here: it renders what Rust sends. If you want to change the
wording, add a new error state, or add a new provider: edit the Rust file. See `file_system/volume/CLAUDE.md` §
"Friendly error system" for the writing rules and how-to guides.

The `ErrorPane` component should rarely need changes unless you're adding new UI elements (like illustrations, new
button types, or new sections). The content flexibility comes from markdown rendering, not component code.

### Debug preview

The debug window has an "Error pane preview" section that can trigger any error state on either pane. The flow is
cross-window: debug page calls `preview_friendly_error` (Tauri command, `#[cfg(debug_assertions)]` only) to get a real
`FriendlyError` from Rust, then emits `debug-inject-error` via `emitTo('main', ...)`. The main window's `+page.svelte`
listens for this event and calls `explorerRef.injectError(pane, friendly)`, which delegates to
`FilePane.injectError(friendly)` setting the `friendlyError` state directly. Reset works via `debug-reset-error` which
re-navigates the pane (clearing `friendlyError` in `loadDirectory`).

## Key decisions

**Decision**: Scoped CSS for file explorer list components (and throughout the app, as Tailwind was removed due to 15s
dev startup from JIT scanning). **Why**: File lists render 50k+ items. Scoped CSS produces smaller DOM (no repetitive
utility classes on each file entry), enabling faster rendering and lower memory.

**Decision**: Icon registry pattern (`iconId` refs in file entries, separate `get_icons()` call, frontend caches).
**Why**: 50k JPEG files would otherwise transmit 50k identical icon blobs (~100-200MB). Instead, file entries carry only
an `iconId` (like `"ext:jpg"`), and a separate IPC call fetches unique icons. Frontend caches icons in IndexedDB across
sessions.

**Decision**: Non-reactive `FileDataStore` (only visible range, ~50-100 items, enters Svelte reactivity). **Why**:
Loading 20k+ files into Svelte `$state` causes 9+ second freezes (Svelte tracks the full array internally even with
virtual scrolling). Storing data outside reactivity and slicing only visible items reduces reactivity cost from O(total)
to O(visible). See [benchmarks](../../../../../docs/notes/non-reactive-file-store.md).

## Gotchas

**Root-layout HMR can trigger a SvelteKit TDZ crash.** When an HMR update propagates through the root `+layout.svelte`
(for example, `app.css` changes), SvelteKit's client router can crash with
`ReferenceError: Cannot access 'component' before initialization`, a TDZ error from a route component module not yet
finished importing during the rebuild. This is a SvelteKit bug (sveltejs/kit#15287). `$lib/hmr-recovery.ts` catches the
crash and forces a clean page reload. The recovery listener is imported from `+layout.ts` (a stable module that survives
layout component re-evaluation). If sveltejs/kit#15287 gets fixed, the workaround can be removed.

## Views (`views/`)

`BriefList` and `FullList` virtual-scrolling components. Brief-mode column widths come from the
`get_brief_column_text_widths` IPC (backend computes the widest filename's text width per column from the listing's
cached entries + font metrics); the FE adds chrome and clamps. See `views/CLAUDE.md`.

## Tabs (`tabs/`)

Each pane has an independent tab bar. Tabs use `{#key}` for clean FilePane recreation on switch (cold load, no warm
cache). See `tabs/CLAUDE.md` for details.
