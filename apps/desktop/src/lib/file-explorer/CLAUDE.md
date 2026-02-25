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

### Gotchas

- **Parent offset** — when `hasParent`, frontend indices = backend indices + 1
- **Range shrinking** — moving cursor back toward anchor removes items no longer in range
- **Optimization flag** — `allSelected: true` avoids sending 500k indices over IPC

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
    - `likeFiles` (default): dirs sort by the active column (uses `recursive_size` for Size). Dirs with unknown size
      sort last.
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
- **DialogManager.svelte** — Renders all modal dialogs (transfer, rename, new-folder, etc.)
- **FunctionKeyBar.svelte** — F1–F10 bar at the bottom of the window
- **MtpConnectionView.svelte** / **NetworkMountView.svelte** — Placeholder panes for MTP/network mount states
- **PaneResizer.svelte** — Drag handle between the two panes
- **PermissionDeniedPane.svelte** — Shown when a directory can't be read
- **selection-state.svelte.ts** — Reactive selection set (indices) with range/toggle helpers
- **sorting-handlers.ts** / **transfer-operations.ts** — Pure logic extracted from DualPaneExplorer

## Tabs (`tabs/`)

Each pane has an independent tab bar. Tabs use `{#key}` for clean FilePane recreation on switch (cold load, no warm
cache). See `tabs/CLAUDE.md` for details.
