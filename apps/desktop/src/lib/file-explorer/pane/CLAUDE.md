# Pane subsystem

Per-pane orchestrator: cursor, scroll, focus, dual-pane coordination, tab state, selection, type-to-jump, dialog
lifecycle, drag handling, volume tinting, and navigation primitives. Up: [`../CLAUDE.md`](../CLAUDE.md).

## Module map

- `DualPaneExplorer.svelte`: root, owns both panes, unified key/command dispatch, the dialog manager, the MCP surface.
- `FilePane.svelte`: one pane, owns its listing, cursor, selection, view mode, type-to-jump buffer, rename flow,
  breadcrumb, and the alt-view `{#if}` chain (MTP / network / SMB-reconnect / search-results / error / list).
- State factories (`*.svelte.ts`): `explorer-state` (the store), `selection-state`, `rename-flow`,
  `type-to-jump-state`, `volume-tint`, `pane-mcp-sync`, `persistence-subscriber`, `listing-diff-sync`,
  `drag-drop-controller`, `dialog-state`.
- Pure utilities (`*.ts`): `navigate`, `volume-capabilities`, `has-parent`, `snapshot-pane-navigation`, `pane-access`,
  `focused-pane-reads`, the command-body factories, `function-key-commands`, `selection-dialog-keys`, `transfer-entry`.

Full file table, conventions, and decision rationale: [DETAILS.md](DETAILS.md).

## Must-knows

- **Exactly one pane is focused.** `focusedPane: 'left' | 'right'` lives in the explorer store; `setFocusedPane` is its
  ONLY mutator. Pane-switch (Tab) clears type-to-jump and rename mode on both panes.
- **Explorer-store fields are module-private with exactly one mutator each** (`focusedPane`, `showHiddenFiles`,
  `leftPaneWidthPercent`, `leftTabMgr`, `rightTabMgr`). Assigning any property of the store object outside
  `explorer-state.svelte.ts` is a lint error (`cmdr/no-explorer-state-writes`). `cursorIndex`, selection, and listing UI
  state stay LOCAL to `FilePane` (perf invariant P3), never in the store.
- **`getTabMgr(pane)` returns the live `$state` holder, never a snapshot.** Returning a `$state.snapshot` severs
  reactivity at the seam (same rule `pane-access.ts` documents).
- **Guard logic branches on `VolumeCapabilities`, never on volume-id strings** (invariant A6). `volume-capabilities.ts`
  is the single FE source of truth: a closed `VolumeKind` union keys a frozen per-kind table. Per-VOLUME runtime flags
  (`isReadOnly`, `supportsTrash`, `smbConnectionState`) stay on `VolumeInfo`, NOT in the table. The A6 sweep is DONE;
  remaining `=== 'search-results'` / `startsWith('mtp-')` hits are justified residue (classifier internals, path/scheme
  mechanics, display selection, persistence). Read DETAILS § "A6 residue inventory" before converting any.
- **`capabilitiesFor` / `volumeKindOf` must stay TOTAL** (never return `undefined`): unknown real ids fall to the
  `local` default; the two virtual ids short-circuit first. The tint classifier `volumeKindFor` keeps its own body and
  output so tint stays byte-stable; never feed the `local` default back into tinting.
- **Snapshot pane (`volumeId === 'search-results'`) couples two integration points**: `computeHasParent` returns
  `false` (no `..` row) AND `isCrossVolumeNavigation` routes any real-path nav through the volume-change machinery.
  Skipping either breaks selection (off-by-one) or poisons the pane with a `search-results` volumeId + a real path.
- **The MTP clipboard refusal gate keys on `caps.kind === 'mtp'`, not `!supportsSystemClipboard`** (network +
  search-results also lack a system clipboard; an MTP-worded toast on a network paste would be wrong).
- **Functions that WRITE component navigation state stay in the component** (`switchPane`, `swapPanes`,
  `toggleHiddenFiles`, `setViewMode`, `navigate`, `setSort*`, `moveCursor`, `selectVolumeBy*`, `copyPathBetweenPanes`,
  the `mirror*`/`restoreFocus` helpers). Only read-only / delegating bodies move into the `PaneAccess`-reading factories.
- **The focus guard must exempt dialog content.** `DualPaneExplorer.handleFocusGuard` must keep its
  `[role="dialog"], [role="alertdialog"]` exemption: rename dialogs mount inside FilePane, and without it the guard and
  `use:trapFocus` ping-pong focus in an endless microtask loop that freezes the webview. Pinned by the "rename to
  existing name on MTP" E2E.
- **Nav-state persistence fires from ONE subscriber** (`persistence-subscriber.svelte.ts`, invariant A5). Don't add
  scattered `saveAppStatus` / `saveTabsForPaneSide` calls in nav / sort / view-mode / focus / swap / mirror paths:
  mutate the store and the subscriber reacts. Layout-split (drag-end only) and last-used-path (a delta `navigate()`
  owns) come in as explicit hooks. Tab STRUCTURE (open/close/reorder/pin) persists separately from `tab-operations.ts`.
- **Don't add `cd`-style heuristics in `commitPathFromListing`.** A stale `onPathChange` is dropped by the
  drop-foreign-listings policy (prefix match for virtual volumes, `isPathOnVolume` otherwise). New virtual namespace →
  extend the explicit prefix branch. See `../CLAUDE.md` and DETAILS § "The `navigate()` transaction".
- **`navigate(intent, deps)` is the single coordinator-level pane-nav entry**, sitting on top of FilePane's listing
  primitives. Its `NavigateResult` refusal `message` strings are an EXACT contract (the MCP adapter forwards them
  verbatim; tests pin them byte-for-byte). Don't reword without updating the pinned tests, and don't "upgrade" the
  in-place arm to commit immediately (it changes when the breadcrumb updates relative to the listing).
- **Self-drag drop builds from recorded app state, not the pasteboard** (`handleDrop` consumes
  `consumableSelfDragIdentity` only when self-drag is active AND `sourceVolumeId` is a registered backend-real volume).
  See [`../drag/CLAUDE.md`](../drag/CLAUDE.md).
- **`DualPaneExplorer.svelte` and `FilePane.svelte` are ~3000 lines each** and flagged by `file-length`. Don't add to
  them: new cross-cutting state → a `*.svelte.ts` factory; new pure logic → a `*.ts` helper with a colocated test.
- **Volume tint has an old-WebKit (Safari < 16.2) sRGB fallback** gated by `hasColorMix`; a reactive `mediaTick`
  repaints it on `prefers-color-scheme` / `prefers-contrast` flips (without it, dark-mode swaps wouldn't repaint).

Architecture, flows, and decision detail: [DETAILS.md](DETAILS.md). Read it in whole before structural changes here.
