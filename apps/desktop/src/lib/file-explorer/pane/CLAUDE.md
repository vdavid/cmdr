# Pane subsystem

Per-pane orchestrator: cursor, scroll, focus, dual-pane coordination, tab state, selection state, type-to-jump, dialog
lifecycle, drag handling, volume tinting, and navigation primitives. Up: [`../CLAUDE.md`](../CLAUDE.md).

`DualPaneExplorer.svelte` is the root: it owns both panes, the unified key/command dispatch, the dialog manager, and the
MCP-exposed surface. `FilePane.svelte` is one pane: it owns its listing, cursor, selection, view mode, type-to-jump
buffer, rename flow, breadcrumb, and the alt-view rendering ({#if/elseif} between `MtpConnectionView`,
`NetworkMountView`, `SmbReconnectingView`, `SearchResultsView`, `ErrorPane`, `VolumeUnreachableBanner`, and the regular
list).

## File map

### Components

| File                             | Purpose                                                                           |
| -------------------------------- | --------------------------------------------------------------------------------- |
| `DualPaneExplorer.svelte`        | Root: two panes + resizer + dialog manager + key/command dispatch + MCP wiring    |
| `FilePane.svelte`                | One pane: listing, cursor, selection, view mode, breadcrumb, alt-view switching   |
| `DialogManager.svelte`           | Renders every modal dialog (transfer, delete, rename, new-folder, alert, error)   |
| `FunctionKeyBar.svelte`          | F1–F10 bar at the bottom of the window                                            |
| `PaneResizer.svelte`             | Drag handle between the two panes                                                 |
| `ErrorPane.svelte`               | Friendly-error display for listing failures (see parent § "Error display")        |
| `VolumeUnreachableBanner.svelte` | Volume resolution timed out OR SMB give-up state; retry + open home / disconnect  |
| `SmbReconnectingView.svelte`     | Spinner + progress bar while `smb-reconnect-manager` runs its backoff cycle       |
| `MtpConnectionView.svelte`       | Placeholder pane for MTP connection states                                        |
| `NetworkMountView.svelte`        | Network browser host/share list + login form                                      |
| `SearchResultsView.svelte`       | Snapshot view for `volumeId === 'search-results'` (see parent § "Search-results") |
| `TypeToJumpIndicator.svelte`     | Bottom-right "Jump: …" chip                                                       |

### Reactive state (`*.svelte.ts`)

| File                             | Purpose                                                                                 |
| -------------------------------- | --------------------------------------------------------------------------------------- |
| `dialog-state.svelte.ts`         | Dialog props + handlers (transfer, delete, mkdir, alert, error); factory                |
| `selection-state.svelte.ts`      | `SvelteSet<number>` of indices + range anchor/end + `applyIndices` helpers              |
| `rename-flow.svelte.ts`          | Rename validation, conflict + extension dialogs, save / cancel                          |
| `type-to-jump-state.svelte.ts`   | Buffer + indicator + reset/hide timers + generation counter (race protection)           |
| `volume-tint.svelte.ts`          | `color-mix(...)` or sRGB hex by volume kind; pure `volumeKindFor` classifier            |
| `pane-mcp-sync.svelte.ts`        | Mirrors pane state into the MCP `PaneState` store; skips network/search panes           |
| `listing-diff-sync.svelte.ts`    | File-watcher listeners + `reconcileCursorAndSelection` (pure, off-by-one core)          |
| `drag-drop-controller.svelte.ts` | Native drag band: drop-target state, drag handlers, 3 Tauri listeners, highlight effect |

### Pure utilities (`*.ts`)

| File                          | Purpose                                                                          |
| ----------------------------- | -------------------------------------------------------------------------------- |
| `types.ts`                    | `FilePaneAPI`, `SwapState`, `ListViewAPI`, `*BrowserAPI`, `NetworkCursorEntry`   |
| `pane-access.ts`              | `PaneAccess`: live-reference read API over pane nav + chrome state for factories |
| `clipboard-operations.ts`     | System-clipboard copy/cut/paste factory (MTP refusal, snapshot, cut-vs-copy)     |
| `file-operation-commands.ts`  | Rename / new-folder / new-file / viewer / transfer / delete openers factory      |
| `initialization.ts`           | Load persisted tabs + status + settings; resolve volumes; apply E2E overrides    |
| `tab-operations.ts`           | Tab CRUD + context menu + persistence wired to `tabs/tab-state-manager`          |
| `transfer-operations.ts`      | Build `TransferDialogPropsData` (and snapshot variant) from a focused pane       |
| `sorting-handlers.ts`         | `getNewSortOrder` (column click cycle), `toFrontendIndices` (`..` offset)        |
| `index-events.ts`             | Throttled `index-dir-updated` handler with `/private/` symlink resolution        |
| `snapshot-pane-navigation.ts` | `isCrossVolumeNavigation` — snapshot-volume → real-path triggers volume switch   |
| `has-parent.ts`               | `computeHasParent({ isSearchResultsView, currentPath, effectiveVolumeRoot })`    |
| `search-results-keys.ts`      | Pure key→action dispatch for the flat snapshot pane                              |
| `selection-dialog-keys.ts`    | Classify `+` / `-` keypresses → open Selection dialog (Total Commander parity)   |
| `error-pane-utils.ts`         | Tiny helper for `ErrorPane`'s technical-details rendering                        |
| `integration-test-utils.ts`   | Shared test scaffolding for pane integration tests                               |

### Tests

Colocated with the code they pin. Notable cross-cutting suites: `DualPaneExplorer.test.ts`,
`selection-consistency.test.ts` (selection survives diffs / cancel / source-item-done), `listing-diff-sync.test.ts`
(pure `reconcileCursorAndSelection` off-by-one coverage), `file-pane-keyboard.test.ts`, `volume-breadcrumb.test.ts`,
`volume-tint.svelte.test.ts` (+ `volume-tint.svelte.fallback.test.ts` for the old-WebKit branch), `*.a11y.test.ts` (axe
sweeps per alt-view component).

## Conventions

**Focus contract.** Exactly one pane is focused (`focusedPane: 'left' | 'right'`); `DualPaneExplorer` is the single
writer. Key dispatch in `DualPaneExplorer` resolves which pane handles a keystroke via this state, then delegates to
`FilePane.handleKeyDown`. Pane-switch (Tab) clears type-to-jump and rename mode on both panes (see "Reset triggers" in
parent § "Type-to-jump").

**Type-to-jump factory.** One `createTypeToJumpState` instance per pane, inside `FilePane`. Reset triggers (ESC, arrows,
Page/Home/End, Enter, Tab, Backspace, rename entry, context menu, drag start, pane switch, tab switch, dir change,
re-sort, listing replace) all call the factory's `clearJumpState()`. The generation counter discards stale async match
responses. Backend match runs in `apps/desktop/src-tauri/src/file_system/listing/fuzzy_jump.rs`.

**Snapshot pane (`volumeId === 'search-results'`).** Two integration points that MUST stay coupled: `computeHasParent`
returns `false` (no `..` row), and `isCrossVolumeNavigation` routes any navigation to a real path through the
volume-change machinery (`onVolumeChange` / `handleVolumeChange`). Skipping either breaks selection (off-by-one) or
poisons the pane with `volumeId === 'search-results'` + real path.

**Cross-pane drag.** `DualPaneExplorer.getFileAndPathUnderCursor()` prefers `FilePane.getPathUnderCursor()` over
`${currentPath}/${filename}` so snapshot-pane drags carry real filesystem paths, not `search-results://sr-N/<name>`.

**Dialog state lifecycle.** `dialog-state.svelte.ts` exposes one factory per `DualPaneExplorer`. Handlers like
`handleTransferError(error, friendly?)` accept the typed `WriteOperationError` plus the optional `FriendlyError` from
the backend `write-error` event so the rendered dialog can prefer the backend copy. The factory pattern keeps the giant
component testable: pass deps in, get back a struct of state + handlers.

**Live disk space.** `FilePane` registers each pane independently with the backend space poller (`watchVolumeSpace`
keyed by pane ID). Two panes on the same volume have independent registrations; one navigating away doesn't unwatch the
other. See parent § "Live disk space".

**MCP surface.** `FilePane` mirrors `{ buffer, indicatorVisible, indicatorStale, lastMatchedName }` into the synced
`PaneState.typeToJump` whenever the buffer or indicator is live, so MCP-driven E2E can assert without DOM poking. See
`src-tauri/src/mcp/CLAUDE.md` § State stores.

**Don't add `cd`-style heuristics in `applyPathChange`.** Stale `onPathChange` from a slow listing is dropped by the
volume guard in `DualPaneExplorer.applyPathChange` (`smb://` prefix for `network`, `search-results://` prefix for
snapshots, `isPathOnVolume` for everything else). Adding a new virtual-volume namespace? Extend the explicit prefix
branch. See parent § "Gotchas".

## Gotchas

- **Parent offset.** When `hasParent`, frontend cursor index = backend index + 1. `toFrontendIndices` applies this; the
  type-to-jump match callback applies it manually. Forgetting it lands the cursor one row off on every match.
- **Selection's `SvelteSet` requires mutations, not reassignment.** `selectionState.selectedIndices.add(i)` works;
  `state.selectedIndices = new SvelteSet([i])` breaks reactivity. The header comment in `selection-state.svelte.ts` pins
  this.
- **Volume tint old-WebKit branch.** On Safari < 16.2 (macOS 12), `color-mix()` doesn't parse, so `volume-tint` reads
  live CSS vars via `getComputedStyle` and mixes in sRGB. A reactive `mediaTick` re-fires `$derived` callers when
  `prefers-color-scheme` / `prefers-contrast` flips; without it, dark-mode swaps wouldn't repaint the tint. The branch
  is picked once at module load via `hasColorMix` from `$lib/utils/webkit-compat.ts`.
- **`DualPaneExplorer.svelte` and `FilePane.svelte` are ~3000 lines each and flagged by `file-length`.** Don't add to
  them without extracting first. New cross-cutting state goes into a `*.svelte.ts` factory; new pure logic goes into a
  `*.ts` helper with a colocated test. The `dialog-state` / `rename-flow` / `type-to-jump-state` extractions are the
  pattern to follow.
