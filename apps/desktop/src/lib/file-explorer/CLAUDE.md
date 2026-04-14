# File explorer module

Dual-pane file explorer with keyboard-driven navigation, file selection, sorting, command palette, and adaptive layout.

## Selection (`selection/`)

### User interaction

- **Space** — toggle selection at cursor
- **Shift+click / Shift+arrow** — range selection with anchor (A) and end (B). If anchor already selected, range
  deselects.
- **Cmd+A / Cmd+Shift+A** — select all / deselect all
- **".." entry can't be selected**
- **Cleared on navigation** — selection is per-directory

### Implementation

- **State**: `SvelteSet<number>` (from `svelte/reactivity`) in `FilePane.svelte` — O(1) add/remove/has
- **Preserved on sort/filter** — `resort_listing` accepts `selectedIndices[]`, returns `newSelectedIndices[]`
- **Write operations receive indices** — backend resolves to paths from cached listing
- **Visual**: `--color-selection-fg` (yellow foreground)

### Operation lifecycle

- **Snapshot** — when an operation is confirmed, FilePane snapshots selected file names into `operationSelectedNames`
  (or `'all'` sentinel if all selected)
- **Diff-driven adjustment** — on each `directory-diff` during an operation, selection is re-resolved via
  `findFileIndices` batch IPC. A `diffGeneration` counter discards stale async results.
- **Cursor adjustment** — cursor index is also adjusted on structural diffs using the same `adjustSelectionIndices`
  mechanism (treating cursor as a single-element selection)
- **Source-item-done deselection** — `write-source-item-done` events individually deselect completed items (for copy and
  other ops that don't trigger diffs)
- **Clear on complete/error** — safety net clears all selection on the source pane
- **Cancel behavior** — selection reflects survivors. For `'all'` sentinel: calls `selectAll()` (move/delete/trash) or
  leaves untouched (copy)

### Gotchas

- **Parent offset** — when `hasParent`, frontend indices = backend indices + 1
- **Range shrinking** — moving cursor back toward anchor removes items no longer in range
- **Optimization flag** — `allSelected: true` avoids sending 500k indices over IPC
- **`allSelected` + cancel** — calls `selectAll()` for move/delete/trash (source listing changed), leaves untouched for
  copy (source listing unchanged)
- **Both panes same directory** — only source pane selection is adjusted; the other pane's selection may become stale
- **Snapshot timing** — must happen at confirm, not when progress dialog opens (same-FS moves are instant and may
  complete before dialog)
- **Snapshot covers clipboard paste** — `startTransferProgress` also snapshots, not just `handleTransferConfirm` /
  `handleDeleteConfirm`

## Navigation (`navigation/`)

### Back/forward navigation

- **Per-pane history** — independent stacks, session-only (not persisted)
- **Cross-volume** — works across local drives, network shares, MTP devices
- **Deleted folder handling** — walks up parent tree until existing dir found. Skipped entries remain in history.
- **Shortcuts**: Cmd+[ (back), Cmd+] (forward), Backspace (parent)

### Gotchas

- **History entries are paths** — stored as strings. No FileEntry metadata cached in history.
- **Volume switching** — changes volume context. History still tracks old volume's paths.

## Sorting

### Behavior

- **Directories first** — always
- **Natural sorting** — `file10.txt` after `file2.txt`
- **Extension grouping** — dotfiles → no-extension → by extension alphabetically
- **Per-tab sort** — each tab owns its `sortBy` + `sortOrder` (no global per-column memory)
- **Directory sort mode** — setting `listing.directorySortMode` controls how dirs sort among themselves:
  - `likeFiles` (default): dirs sort by the active column (uses `recursive_size` for Size). Dirs with unknown size sort
    last.
  - `alwaysByName`: dirs always sort by name, ignoring the active sort column.
- **Name ASC tiebreaker** — when primary sort values are equal, entries fall back to name ascending

### Implementation

- **Efficient re-sort** — `resort_listing` re-sorts cached listing without disk reads
- **Preserves cursor by filename** — frontend sends current filename, backend returns new index
- **Preserves selection by indices** — backend resolves filenames, re-finds after sort
- **Dir sort mode flows via IPC** — `directorySortMode` passed to `listDirectoryStart` and `resortListing`, stored in
  `CachedListing` so watcher re-sorts use the correct mode

### Gotchas

- **A and B cleared after sort** — range selection anchor/end reset (sorting is "new context")
- **Selected indices remapped** — backend returns `newSelectedIndices[]`, frontend updates `Set`
- **Dir size = `recursive_size`** — for sorting, dirs use `recursive_size` (from drive index), not `size` (always None)

## Command palette (`../command-palette/`)

### Features

- **Fuzzy search** — `@leeoniya/ufuzzy` (~3.5KB), typo-tolerant
- **Match highlighting** — matched characters underlined
- **Persisted query** — remembered within session
- **~60 commands** — all scopes (app, main window, file list, network, about)

### Gotchas

- **Low-level nav commands hidden** — `showInPalette: false` for arrow keys, Page Up/Down
- **Execution handler in +page.svelte** — `handleCommandExecute` delegates to `explorerRef`

## Network browser (`network/`)

- **NetworkBrowser.svelte** — Top-level network view; lists discovered servers
- **ShareBrowser.svelte** — Lists shares on a selected server
- **NetworkLoginForm.svelte** — Credential entry for authenticated SMB connections
- **network-store.svelte.ts** — Reactive state for discovered servers, selected server/share, and auth mode

## Operations (`operations/`)

- **apply-diff.ts** — Applies file-watcher diffs (add/remove/modify events) to a cached listing in-place
- **adjust-selection-indices.ts** — Pure function that maps selected indices from an old listing to their positions in a
  new listing, given removed and added indices. Also used for cursor index adjustment on structural diffs.

## Rename (`rename/`)

Inline rename with validation, conflict resolution, and an extension change confirmation dialog.

- **InlineRenameEditor.svelte** — Inline text editor for renaming files directly in the file list
- **RenameConflictDialog.svelte** — Dialog shown when the new name conflicts with an existing entry
- **ExtensionChangeDialog.svelte** — Confirmation dialog when the file extension is being changed
- **rename-activation.ts** — Logic for triggering rename mode
- **rename-operations.ts** — Rename execution and error handling
- **rename-state.svelte.ts** — Reactive state for the rename editor

## Pane (`pane/`)

Core explorer UI components:

- **DualPaneExplorer.svelte** — Root component; manages both panes, unified key/command handlers, MCP exports
- **FilePane.svelte** — Single pane: navigation, listing, cursor, selection, view mode
- **DialogManager.svelte** — Renders all modal dialogs (transfer, delete, rename, new-folder, etc.)
- **FunctionKeyBar.svelte** — F1–F10 bar at the bottom of the window
- **MtpConnectionView.svelte** / **NetworkMountView.svelte** — Placeholder panes for MTP/network mount states
- **PaneResizer.svelte** — Drag handle between the two panes
- **ErrorPane.svelte** — Unified error display for listing failures. See [Error display](#error-display) below.
- **VolumeUnreachableBanner.svelte** — Shown when a tab's volume resolution timed out at startup (retry + open home)
- **selection-state.svelte.ts** — Reactive selection set (indices) with range/toggle helpers
- **sorting-handlers.ts** / **transfer-operations.ts** / **tab-operations.ts** — Pure logic extracted from
  DualPaneExplorer
- **dialog-state.svelte.ts** — Dialog state and handlers (transfer, delete/trash, new folder, alert, error) extracted
  from DualPaneExplorer via factory pattern
- **rename-flow.svelte.ts** — Rename flow logic (validation, conflict/extension dialogs) extracted from FilePane

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

- **Title**: large text, always in accent color. UnoCSS/Lucide icon signals severity: ⚠ `i-lucide:triangle-alert` in
  warning color for transient, ⊘ `i-lucide:circle-alert` in error color for serious, no icon for needs-action
- **Folder path**: shown in secondary text so the user knows exactly which folder is affected
- **Explanation**: rendered as markdown via `snarkdown` — plain-language description of what happened
- **Suggestion**: rendered as markdown — actionable steps, often provider-specific (for example, "Open **MacDroid** and
  check that your phone is connected")
- **"Try again" button**: shown only for `transient` category. Calls `navigateTo(currentPath)` to retry the listing.
  Tracks retry count and timestamps, displays them in the technical details ("Retry #2 · first try 45s ago · last try
  12s ago")
- **"Open System Settings" button**: shown for permission-denied errors on macOS (reuses `openPrivacySettings()`)
- **Collapsible "Technical details"**: shows the raw errno name and code for power users / bug reports

### For future agents

The error messages and provider suggestions live in **Rust** (`file_system/volume/friendly_error.rs`), not in this
Svelte component. The frontend is intentionally thin here — it renders what Rust sends. If you want to change the
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

**Decision**: Scoped CSS for file explorer list components (and throughout the app — Tailwind was removed due to 15s dev
startup from JIT scanning). **Why**: File lists render 50k+ items. Scoped CSS produces smaller DOM (no repetitive
utility classes on each file entry), enabling faster rendering and lower memory.

**Decision**: Icon registry pattern — `iconId` refs in file entries, separate `get_icons()` call, frontend caches.
**Why**: 50k JPEG files would otherwise transmit 50k identical icon blobs (~100-200MB). Instead, file entries carry only
an `iconId` (like `"ext:jpg"`), and a separate IPC call fetches unique icons. Frontend caches icons in IndexedDB across
sessions.

**Decision**: Non-reactive `FileDataStore` — only visible range (~50-100 items) enters Svelte reactivity. **Why**:
Loading 20k+ files into Svelte `$state` causes 9+ second freezes (Svelte tracks the full array internally even with
virtual scrolling). Storing data outside reactivity and slicing only visible items reduces reactivity cost from O(total)
to O(visible). See [benchmarks](../../../../../docs/notes/non-reactive-file-store.md).

## Gotchas

**UnoCSS content list is manually tracked.** `uno.config.ts` lists the specific files that use UnoCSS classes
(`i-lucide:*` icons) so UnoCSS only watches those files during dev, not the entire `src/` tree. Without this, every file
change triggers 6-7 redundant HMR updates. When adding UnoCSS classes to a new file, add that file to the
`content.filesystem` array in `uno.config.ts`.

**UnoCSS triggers SvelteKit root-layout HMR crash.** `virtual:uno.css` regenerates on every Svelte file save. Because
it's imported in the root `+layout.svelte`, Vite treats it as a root-layout change, which forces SvelteKit to rebuild
the entire route tree. SvelteKit's client router (`client.js:373`, `get_navigation_result_from_branch`) crashes with
`ReferenceError: Cannot access 'component' before initialization` — a TDZ error where a route component module hasn't
finished importing during the rebuild. This is a SvelteKit bug (sveltejs/kit#15287, observed with SvelteKit 2.55.0 /
Svelte 5.54.1 / Vite 8.0.2). Workaround: `import.meta.hot.accept(() => { import.meta.hot!.invalidate() })` in the root
layout catches the update and triggers a clean full page reload instead of the broken HMR path. Side effect: edits to
the root layout or its deps (app.css, virtual:uno.css) cause a full reload instead of hot-swap. Leaf component edits are
unaffected — SvelteKit handles those fine. If sveltejs/kit#15287 gets fixed, the workaround can be removed.

## Tabs (`tabs/`)

Each pane has an independent tab bar. Tabs use `{#key}` for clean FilePane recreation on switch (cold load, no warm
cache). See `tabs/CLAUDE.md` for details.
