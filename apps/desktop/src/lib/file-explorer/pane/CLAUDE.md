# Pane subsystem

Per-pane orchestrator: cursor, focus, dual-pane coordination, tab state, selection, type-to-jump, dialogs, drag, volume
tinting, and navigation primitives. Up: [`../CLAUDE.md`](../CLAUDE.md). Full file table and conventions:
[DETAILS.md](DETAILS.md).

## Module map

- `DualPaneExplorer.svelte`: root, owns both panes, unified key/command dispatch, the dialog manager, the MCP surface.
- `FilePane.svelte`: one pane, owns its listing, cursor, selection, view mode, type-to-jump, rename flow, breadcrumb,
  and the alt-view `{#if}` chain (MTP / network / SMB-reconnect / search-results / error / list).
- State factories (`*.svelte.ts`): `explorer-state` (the store), `selection-state`, `rename-flow`, `type-to-jump-state`,
  `volume-tint`, `pane-mcp-sync`, `persistence-subscriber`, `listing-diff-sync`, `drag-drop-controller`, `dialog-state`.
- Pure utilities (`*.ts`): `navigate`, `volume-capabilities`, `has-parent`, `snapshot-pane-navigation`, `pane-access`,
  `focused-pane-reads`, the command-body factories, and the function-key / selection-dialog key helpers.

## Must-knows

- **Exactly one pane is focused.** `focusedPane: 'left' | 'right'` lives in the explorer store; `setFocusedPane` is its
  ONLY mutator. Pane-switch (Tab) clears type-to-jump and rename mode on both.
- **Explorer-store fields are module-private with exactly one mutator each.** Assigning any store property outside
  `explorer-state.svelte.ts` is a lint error (`cmdr/no-explorer-state-writes`). `cursorIndex`, selection, and listing UI
  state stay LOCAL to `FilePane` (perf P3).
- **`getTabMgr(pane)` returns the live `$state` holder, never a snapshot** (`$state.snapshot` severs reactivity).
- **Guard logic branches on `VolumeCapabilities`, never on volume-id strings** (invariant A6). `volume-capabilities.ts`
  is the single FE source of truth: a closed `VolumeKind` union keys a frozen per-kind table; per-VOLUME runtime flags
  (`isReadOnly`, `supportsTrash`, etc.) stay on `VolumeInfo`, NOT the table. The sweep is DONE; remaining string hits
  are justified residue (DETAILS § "A6 residue inventory").
- **`capabilitiesFor` / `volumeKindOf` must stay TOTAL** (never `undefined`): unknown real ids fall to the `local`
  default, the two virtual ids short-circuit first. Keep the tint classifier `volumeKindFor` separate for byte-stable
  tint; never feed the `local` default into tinting.
- **`FilePane.applyIndices` jumps the cursor on SELECT only** (deselect leaves it put), via
  `firstSelectedIndex(idxs, hasParent)`, which skips the `..` row. Don't use raw `idxs[0]`; it can be `..`.
- **Snapshot pane (`volumeId === 'search-results'`) couples two points**: `computeHasParent` returns `false` (no `..`
  row) AND `isCrossVolumeNavigation` routes any real-path nav through the volume-change machinery. Skip either and
  selection goes off-by-one or the pane keeps `search-results` on a real path.
- **The MTP clipboard refusal gate keys on `caps.kind === 'mtp'`, not `!supportsSystemClipboard`** (network and
  search-results also lack one, so the MTP-worded toast would be wrong).
- **The focus guard (`DualPaneExplorer.handleFocusGuard`) must keep its `[role="dialog"], [role="alertdialog"]`
  exemption.** Rename dialogs mount inside FilePane; without it the guard and `use:trapFocus` ping-pong focus in an
  endless microtask loop that freezes the webview. Pinned by E2E.
- **Nav-state persistence fires from ONE subscriber** (`persistence-subscriber.svelte.ts`, invariant A5). Don't scatter
  `saveAppStatus` / `saveTabsForPaneSide` calls across nav paths: mutate the store and the subscriber reacts (exceptions
  in DETAILS).
- **`navigate(intent, deps)` is the single coordinator-level pane-nav entry.** Its `NavigateResult` refusal `message`
  strings are an EXACT contract (pinned byte-for-byte; the MCP adapter forwards them verbatim): don't reword without
  updating the tests, and don't make the in-place arm commit immediately. Also don't add `cd`-style heuristics in
  `commitPathFromListing` (the drop-foreign-listings policy handles stale `onPathChange`; new virtual namespace → extend
  the prefix branch). DETAILS § "The navigate() transaction".
- **Self-drag drop builds from recorded app state, not the pasteboard** (`handleDrop` consumes
  `consumableSelfDragIdentity` only when self-drag is active AND `sourceVolumeId` is a registered backend-real volume).
  See [`../drag/CLAUDE.md`](../drag/CLAUDE.md).
- **`DualPaneExplorer.svelte` and `FilePane.svelte` are ~3000 lines each** (`file-length`-flagged). Don't add to them:
  cross-cutting state → a `*.svelte.ts` factory; pure logic → a `*.ts` helper + colocated test.
- **Volume tint has an old-WebKit (Safari < 16.2) sRGB fallback** gated by `hasColorMix`. Don't drop the reactive
  `mediaTick`, or dark-mode / contrast swaps won't repaint the tint.

Read [DETAILS.md](DETAILS.md) before any non-trivial work here: editing, planning, reorganizing, or advising.
