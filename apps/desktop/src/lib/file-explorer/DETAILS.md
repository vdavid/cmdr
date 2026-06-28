# File explorer details

Pull-tier docs for `apps/desktop/src/lib/file-explorer/`: architecture, flows, and decision rationale. Must-know
invariants and gotchas live in [CLAUDE.md](CLAUDE.md).

The cross-cutting subsystem detail that belongs to a child directory (type-to-jump, live disk space, error display, the
`navigate()` transaction, volume capabilities) lives in [`pane/CLAUDE.md`](pane/CLAUDE.md) and
[`pane/DETAILS.md`](pane/DETAILS.md); this file owns selection, sorting, the command palette, operations, and the
cross-cutting decisions.

## Selection (`selection/`)

### User interaction

- **Space**: toggle selection at cursor (macOS only — must be unshifted; **Shift+Space** opens Quick Look instead, see
  [`quick-look/`](quick-look/) and `src-tauri/src/quick_look/CLAUDE.md`). Each plain-Space press also fires an
  educational toast for Finder converts (lives in [`quick-look/quick-look-hint.ts`](quick-look/quick-look-hint.ts)). The
  toast keeps reappearing as a gentle reminder until the user clicks "Don't show again" (or flips
  `fileExplorer.suppressQuickLookHint` in Settings > Advanced). While the toast is on screen, further Space presses just
  toggle selection — the hint module no-ops if the toast is already visible. The X on the toast frame closes the current
  instance without suppressing future ones. The toast's keys render as literal-mode `ShortcutChip`s: `Space` / `Enter`
  are fixed interaction keys, and the Quick Look key is snapshotted at toast creation
  (`getEffectiveShortcuts('file.quickLook')[0]`, default `⇧Space`) so a mid-display rebind doesn't rewrite the visible
  toast. The toast also carries a "Settings > Keyboard shortcuts" `LinkButton` that deep-links to the `file.quickLook`
  row, so the chips themselves stay non-clickable.
- **Insert**: toggle selection at cursor and move cursor down (Total Commander style). `..` isn't selectable, but the
  cursor still advances. At the last row the cursor stays put. No physical Insert key on Apple keyboards — users can
  remap via Karabiner-Elements, plug in a PC USB keyboard, or rebind in Settings → Shortcuts.
- **Shift+click**: mouse range selection with anchor (A) and end (B). If anchor was already selected, the range
  deselects.
- **Cmd+click**: toggles the clicked item's selection (Finder-style). Moves the cursor to the clicked item and clears
  the Shift+click anchor. `..` is a no-op. Shift wins when both modifiers are held.
- **Shift+arrow / Shift+Page / Shift+Home/End / Shift+Left/Right (Brief)**: keyboard toggle-and-fill. Toggles the item
  at the cursor's _old_ position, then sets (not toggles) every item the cursor jumps over to that toggled state. The
  landing item is included only when the jump **overflowed** (intended distance > actual distance because of a list
  boundary clamp). Home/End always overflow; arrows overflow when pressed at a boundary (no movement);
  PageUp/PageDown/Brief Left/Right overflow when clamped. Full-mode Shift+Left/Right behave like Shift+Home/End. The
  model is intentionally asymmetric: Shift+Down 3× then Shift+Up 3× does NOT restore the start state — each press
  independently toggles the cursor's item.
- **Cmd+A / Cmd+Shift+A**: select all / deselect all
- **`+` / `-`**: open the Selection dialog ("Select files…" / "Deselect files…", Total Commander parity). Bare keys, no
  modifier required. On US QWERTY, `Shift+=` IS the `event.key === '+'` event so `Shift` is intentionally NOT filtered.
  See [`$lib/selection-dialog/CLAUDE.md`](../selection-dialog/CLAUDE.md) for the dialog itself; the pane-side classifier
  lives in [`pane/selection-dialog-keys.ts`](pane/selection-dialog-keys.ts).
- **".." entry can't be selected**: keyboard fills from `..` default to "select" (so Shift+End from `..` selects).
- **Cleared on navigation**: selection is per-directory

### Select files / Deselect files dialog

A modal that lets the user select by a glob, regex, or natural-language prompt against the focused pane's listing.
Mounts via `+page.svelte`. Snapshot of entries + cursor is captured ONCE at open via
`explorerRef.getFocusedPaneEntries()`; mid-dialog focused-pane changes do NOT re-snapshot. The dialog hands matched
indices to `explorerRef.applyIndicesToFocusedPane(indices, mode)` on commit (`mode === 'add'` for select, `'remove'` for
deselect). See [`$lib/selection-dialog/CLAUDE.md`](../selection-dialog/CLAUDE.md) for the wiring, match semantics, AI
fallback contract, and the snapshot-pane note.

### Implementation

- **State**: `SvelteSet<number>` (from `svelte/reactivity`) in `FilePane.svelte`. O(1) add/remove/has
- **Preserved on sort/filter**: `resort_listing` accepts `selectedIndices[]`, returns `newSelectedIndices[]`
- **Write operations receive indices**: backend resolves to paths from cached listing
- **Visible-index ascending order**: `selection-state.svelte.ts::getSelectedIndices()` sorts ascending before returning,
  so write ops process selections top-to-bottom in pane sort order regardless of Cmd+click sequence. The `SvelteSet`
  itself remains insertion-ordered; only the read-out for callers is sorted.
- **`applyIndices(idxs, mode, hasParent)`**: bulk add/remove without disturbing the range anchor/end state. Skips `..`
  per `hasParent`, same rule as `selectAll`. Fires `onChanged?.()` exactly once per call. Exposed on `FilePane` as
  `applyIndices(idxs, mode)` and on `DualPaneExplorer` as `applyIndicesToFocusedPane(idxs, mode)`, which the Selection
  dialog (see [`$lib/selection-dialog/CLAUDE.md`](../selection-dialog/CLAUDE.md)) calls at commit time.
- **Visual**: three-tier `--color-selection-fg` cascade (red, Total-Commander-style):
  - `--color-selection-fg-primary` (strong red — `#cc0000` light, `#ff4040` dark) applies on the selection bg.
  - `--color-selection-fg-cursor` (`#b80808` / `#ff8c8c`) takes over when the row is also under the cursor
    (`.is-selected.is-under-cursor`), where the bg flips to the translucent cursor color.
  - `--color-selection-fg-fallback` (= `--color-text-primary`) takes over in the dark + tinted + cursor-active corner
    where no AA-clearing red exists; CSS rule keyed on `.file-pane[data-pane-tint]`.
  - `--color-selection-bg` paints a faint darker block under selected rows (light `#f2f2f2`, dark `#141414`); zebra
    stripes are auto-overridden by cascade order.
  - `--color-selection-border` draws a 1px `inset` `box-shadow` between consecutive selected rows so dense selections
    stay countable; suppressed on the cursor row's top.
  - Independent: every cursor row (focused or unfocused) gets a faint accent-colored `inset` outline via
    `--color-cursor-outline` so cursor stays distinguishable from the selection bg, regardless of selection state.
  - All combinations clear WCAG AA 4.5:1; verified by the row-state matrix in `scripts/check-a11y-contrast/`.

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
- **Range shrinking (mouse Shift+click only)**: moving cursor back toward anchor removes items no longer in range. The
  keyboard path is stateless (toggle-and-fill) and doesn't shrink.
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

Full history-stack contract and the volume-breadcrumb detail live in [`navigation/CLAUDE.md`](navigation/CLAUDE.md) and
[`navigation/DETAILS.md`](navigation/DETAILS.md).

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
- **~77 palette-visible commands**: all scopes (app, main window, file list, network, about)

### Gotchas

- **Low-level nav commands hidden**: `showInPalette: false` for arrow keys, Page Up/Down
- **Typed dispatch**: every entry path routes through `handleCommandExecute` (a `CommandId`-typed switch in
  `routes/(main)/command-dispatch.ts`), which turns the id into an `ExplorerAPI` call or a dialog toggle. See
  `$lib/commands/CLAUDE.md`.

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

## Search-results virtual volume (`pane/SearchResultsView.svelte`)

Second virtual-volume namespace alongside `network`. `volumeId === 'search-results'` and the pane path is
`search-results://<snapshot-id>` (opaque to filesystem APIs). The view selection runs off the pane's
`VolumeCapabilities`: `FilePane`'s `paneViewKind` derived (`caps.kind === 'search-results'`) picks `SearchResultsView`
in the `{#if/elseif}` chain, and the "is there a real directory" per-feature gates (git lookups, listing watcher,
dir-exists poll, MCP file sync) read `!caps.hasBackendListing` — the same gate that skips a `network` pane. See
[`pane/DETAILS.md`](pane/DETAILS.md) § "Volume capabilities" for the per-site breakdown (invariant A6 — capabilities,
not a `volumeId === 'search-results'` string compare).

`SearchResultsView` reads the snapshot from `$lib/search/snapshot-store.svelte` and feeds its entries into `FullList`
via `staticEntries`. No backend listing exists, no IPC traffic. Each adapted entry's `name` field is the friendly full
path (home folder shown as `~`); the col-name cell mid-truncates via `useShortenMiddle` and surfaces the full path on
hover. There's no separate Path column anymore. The view exports a small API (`setCursorIndex` / `findItemIndex` /
`openCursorItem` / `isMissing`) used by FilePane's keyboard handler; `findItemIndex` matches on the basename of `path`
so type-to-jump / MCP keep working with plain filenames.

Navigation:

- Enter / double-click on a row opens the real file (or navigates into the real folder), pushing a new history entry for
  the underlying path. ⌘[ returns to the snapshot view; the snapshot's still pinned by the history entry, so the view
  re-renders from memory with no re-query.

Leaving a snapshot pane for a real entry (the R4 cross-volume case): when the user activates a real folder from a
snapshot pane (or the search dialog's "Go to file" exit, or a search-results row), the navigation MUST switch to the
entry's real volume FIRST. The entry's volume is resolved to a `Location` at the edge (`resolveLocationOrToast`), then
routed through `navigate({ to: { goTo } })`, whose switch arm changes volume (a different volume than
`search-results`). `FilePane.handleNavigate` gates this on the `isSearchResultsView` capability and bubbles the
`Location` via the `onGoToLocation` callback; the search dialog and MCP `nav_to_path` resolve at their own edges.
Without the switch, the pane ends up with `volumeId === 'search-results'` while `path` points at a real filesystem
location, and `SearchResultsView` shows "Search results no longer available" (the snapshot-id extractor returns null
because the path doesn't start with `search-results://`). The `navigate()` destination shapes and the four edge
resolvers are documented canonically in `pane/navigate.ts`.

Navigation routing:

- `navigate()`'s `commitPathFromListing` drop-foreign-listings policy treats `'search-results'` like `'network'`: a
  landed path that doesn't start with `search-results://` is dropped (the two virtual namespaces are uniformly opaque to
  `isPathOnVolume`). See § "The stale-listing token + drop-foreign-listings policy" in Gotchas below.
- `openSearchSnapshotInPane(snapshotId, pane?)` is the public entry point the SearchDialog calls (via +page.svelte's
  `handleOpenSearchInPane`). It routes through `navigate({ to: { snapshot }, source: 'user' })` so pushed history
  entries flow through `pushHistoryEntry`, which increments the snapshot refcount via the snapshot-store integration.

Breadcrumb: `VolumeBreadcrumb` recognises `volumeId === 'search-results'` and reads the friendly label from
`getSnapshot(id).label` (with "Search" as fallback). The label itself is the snapshot's `label` field, which the search
dialog builds via `snapshot-label.ts::buildSnapshotLabel`: AI mode prefers the LLM-produced label (from
`TranslateResult.label`), filename mode shows the pattern (`*.pdf`), regex mode wraps it in slashes (`/pattern/`).
FilePane suppresses the trailing path segments entirely for search-results panes — the label IS the breadcrumb.

Source-side operations on the snapshot pane: selection works in the snapshot pane (Space, Insert, Shift+click range,
Cmd+click toggle, Cmd+A / Cmd+Shift+A). `effectiveTotalCount` returns the snapshot's entry count so range selection
spans the result set.

Keyboard contract: `FilePane.handleSearchResultsKeyDown` routes through the pure
`pane/search-results-keys.ts::computeSearchPaneKeyAction` helper, which translates each keypress into an action enum
(`move-cursor`, `open-cursor`, `toggle-selection-at-cursor`, `toggle-selection-and-advance`, `view-file`, `edit-file`,
`noop`). Splitting dispatch from side effects keeps the keyboard contract unit-testable without spinning up the whole
pane. Covered: PgUp / PgDn (visible-page step), Home / End, Shift+Up / Shift+Down (extends selection via the same
toggle-and-fill helper the regular pane uses), Space (toggle), Insert (toggle + advance), F3 (view), F4 (edit). Left /
Right return `noop` (no parent-folder semantics in a flat snapshot; the caller still calls `preventDefault` so the
regular full-pane handler can't jump to first / last row underneath). Cmd+A flows through the unified command dispatch
in `command-dispatch.ts`.

`hasParent` is `false` for `search-results` panes. The path comparison `currentPath !== effectiveVolumeRoot` would
otherwise be true (a `search-results://sr-N` URL never matches a real volume root), so
`selection.selectAll(hasParent=true, ...)` would skip index 0. Setting `hasParent = false` for snapshot panes keeps the
synthetic-`..` skip rule applied only to real panes. The derivation lives in `pane/has-parent.ts` as
`computeHasParent({ isSearchResultsView, currentPath, effectiveVolumeRoot })` and is pinned by
`pane/has-parent.test.ts`, which also covers the integration with `selection.selectAll` (snapshot pane → all 5 indices
selected; non-snapshot pane with hasParent → indices 1..4, skipping the `..` row at 0).

Search-results breadcrumb shape: the volume selector reads the static "Search results" label (set by
`VolumeBreadcrumb.svelte::currentVolume`) and `FilePane.svelte::breadcrumbDisplayPath` renders the snapshot's friendly
label (`*.svelte`, the AI title, ...) as the path. The `breadcrumbSegments` derived produces a single-segment list for
search-results panes so a label containing `/` (regex mode like `/foo\/bar/`) doesn't get broken into path-style
segments. Don't invert this (label in the volume slot, empty path).

Context-menu wiring on the snapshot pane:

- `DualPaneExplorer.getFileAndPathUnderCursor()` prefers the pane-reported `getPathUnderCursor()` over a
  `${currentPath}/${filename}` concatenation. Otherwise `file.showInFinder` / `file.copyPath` / `file.edit` on a
  snapshot pane would build a `search-results://sr-N/<name>` path that downstream IPCs can't act on.
- `SearchResultsView.svelte::onContextMenu` hands the Rust menu builder the path's basename, not the adapted entry's
  `name` (which is the friendly full path like `~/Library/.../test.md`). Otherwise the menu label reads
  `Copy ~/Library/.../test.md` instead of `Copy test.md`. The action itself is correct either way because
  `entryUnderCursor.name` on a snapshot pane mirrors the raw `SearchResultEntry.name` (a basename). Cmd+C / Cmd+X call
  the paths-by-value clipboard IPCs (`copy_paths_to_clipboard` / `cut_paths_to_clipboard`) instead of the
  listing-id-keyed family. F5 / F6 (the unified transfer dialog) detect `volumeId === 'search-results'` and call
  `transfer-operations::buildTransferPropsFromSnapshot` with paths resolved from `snapshot-store::resolveSnapshotPaths`;
  the existing `copy_files` / `move_files` IPCs already accept paths-by-value, so no IPC change was needed for the
  transfer path. Drag-out uses the `'paths'` drag context (see `drag/CLAUDE.md`) which routes through
  `start_drag_paths`. Post-move snapshot cleanup is the cleanup hook in `dialog-state::handleTransferComplete`.

For the dialog-side wiring see [`apps/desktop/src/lib/search/CLAUDE.md`](../search/CLAUDE.md).

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
`$lib/stores/restricted-paths-store.svelte` (`isRestricted(path)`); see `navigation/DETAILS.md` § "Restricted-folder
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

`DualPaneExplorer` + `FilePane` + dialog manager + per-pane state (selection, type-to-jump, rename flow, volume tint).
See [`pane/CLAUDE.md`](pane/CLAUDE.md) and [`pane/DETAILS.md`](pane/DETAILS.md) for the full file map, conventions, and
gotchas. The cross-reference sections below (type-to-jump, live disk space, error display) point into the same
subsystem.

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
E2E tests can assert the feature without poking at the DOM. See `src-tauri/src/mcp/DETAILS.md` § State stores.

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

- **Title**: large text, always in accent color. A glyph via `<Icon>` signals severity: ⚠ `triangle-alert` in warning
  color for transient, ⊘ `circle-alert` in error color for serious, no icon for needs-action
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

The error messages and provider suggestions live on the **frontend** (`$lib/errors/`); Rust ships only a typed,
word-free `ListingError` (reason + params + category + detected provider). `ErrorPane` resolves the copy via
`renderListingError` (`$lib/errors/listing-error.ts`), which picks the base message from the listing/git factory and
applies the provider-suggestion override. To change wording, add an error state, or add a provider: edit `$lib/errors/`
(and keep the frozen parity test green). See [`$lib/errors/CLAUDE.md`](../errors/CLAUDE.md) for the recipes, the writing
rules, and the markdown-escaping XSS boundary.

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

**The stale-listing token + drop-foreign-listings policy (the `navigate()` transaction contract).**
`FilePane.onPathChange` fires on `listing-complete` for whatever path the pane was loading. If the user (or a command
like "Copy path between panes") flips the pane to a different volume between `listing-start` and `listing-complete`, the
stale callback lands on a pane whose `volumeId` no longer matches the path it carries. Two mechanisms keep navigation
state uncorruptible, both inside `pane/navigate.ts`:

- **Drop-foreign-listings policy (the path landing).** `commitPathFromListing` drops a landed path that doesn't belong
  on the pane's current volume: `smb://`-prefixed for the virtual `network` volume, `search-results://`-prefixed for the
  snapshot volume, and `isPathOnVolume(path, volumePath)` for every other (real) volume. Without the drop, `pushPath` +
  `saveLastUsedPathForVolume` would write a foreign path under the new `volumeId`, corrupting in-memory tab state, the
  nav history, and persisted last-used-path state — for example, persisting `/Users/.../project` as the "last used path"
  for an SMB share, which would then attempt a doomed `Create` against `Users\...\project` and surface as
  `STATUS_OBJECT_PATH_NOT_FOUND`. The background `determineNavigationPath` correction applies the same `isPathOnVolume`
  filter when reading back `lastUsedPath`, so a foreign last-used-path can't re-trigger the bug.
- **The transaction token (the volume-switch + background-correction layer).** Each `navigate()` call mints a per-pane
  token; the cross-volume resolve bails and the background `determineNavigationPath` correction (a single GLOBAL
  generation shared by both panes) drops when superseded by a newer volume change. A same-token self-re-entry (the
  parent-nav / deleted-folder walk-up completion re-entering via `onPathChange`) carries the SAME token, so it commits
  rather than looking stale — the foreign-path policy, not the token, is what drops a genuinely stale listing.

If you introduce another virtual-volume namespace with its own non-filesystem prefix (something `isPathOnVolume` can't
match against), extend the explicit prefix branch in `commitPathFromListing`. See `pane/DETAILS.md` § "The `navigate()`
transaction" for the full intent-arm + persistence-split contract.

## Views (`views/`)

`BriefList` and `FullList` virtual-scrolling components. Brief-mode column widths come from the
`get_brief_column_text_widths` IPC (backend computes the widest filename's text width per column from the listing's
cached entries + font metrics); the FE adds chrome and clamps. See `views/CLAUDE.md`.

## Tabs (`tabs/`)

Each pane has an independent tab bar. Tabs use `{#key}` for clean FilePane recreation on switch (cold load, no warm
cache). See `tabs/CLAUDE.md` for details.
