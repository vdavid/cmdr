# Pane subsystem

Per-pane orchestrator: cursor, focus, dual-pane coordination, tab state, selection, type-to-jump, dialogs, drag, volume
tinting, navigation. Up: [`../CLAUDE.md`](../CLAUDE.md). Full file table and conventions: [DETAILS.md](DETAILS.md).

## Module map

- `DualPaneExplorer.svelte`: root, owns both panes, unified key/command dispatch, the dialog manager, the MCP surface.
- `FilePane.svelte`: one pane, owns its listing, cursor, selection, view mode, type-to-jump, rename flow, breadcrumb,
  and the alt-view `{#if}` chain (MTP / network / SMB-reconnect / search-results / error / list).
- State factories (`*.svelte.ts`): `explorer-state`, `selection-state`, `rename-flow`, `type-to-jump-state` +
  `type-to-jump-controller`, `volume-tint`, `pane-mcp-sync`, `tab-mcp-sync`, `persistence-subscriber`,
  `listing-diff-sync`, `quick-look-follow`, `debug-emitters`, `drag-drop-controller`, `dialog-state`, `git-browser-sync`,
  `smb-view-state`, `volume-space`.
- Pure utilities (`*.ts`): `navigate`, `volume-capabilities`, `has-parent`, `pane-access`, `focused-pane-reads`,
  command/coordinator factories (full list in DETAILS), keyboard helpers (`search-pane-keys`, `cursor-nav-keys`,
  function-key, selection-dialog).

## Must-knows

- **Exactly one pane is focused.** `focusedPane: 'left' | 'right'` lives in the explorer store; `setFocusedPane` is its
  ONLY mutator. Pane-switch (Tab) clears type-to-jump and rename mode on both.
- **Explorer-store fields are module-private with exactly one mutator each.** Assigning any store property outside
  `explorer-state.svelte.ts` is a lint error (`cmdr/no-explorer-state-writes`). `cursorIndex`, selection, and listing UI
  state stay LOCAL to `FilePane` (perf P3).
- **`getTabMgr(pane)` returns the live `$state` holder** (`$state.snapshot` severs reactivity).
- **Guard logic branches on `VolumeCapabilities`, never on volume-id strings** (invariant A6). `volume-capabilities.ts`
  is the single FE source of truth (a closed `VolumeKind` union keys a frozen per-kind table); per-VOLUME runtime flags
  (`isReadOnly`, etc.) stay on `VolumeInfo`. The sweep is DONE; residual string hits are justified (DETAILS § "A6
  residue inventory").
- **`capabilitiesFor` / `volumeKindOf` must stay TOTAL** (never `undefined`): unknown real ids fall to the `local`
  default, the two virtual ids short-circuit. Keep the tint classifier `volumeKindFor` separate; never feed the `local`
  default into tinting.
- **`FilePane.applyIndices` jumps the cursor on SELECT only** (deselect leaves it put), via
  `firstSelectedIndex(idxs, hasParent)`, which skips the `..` row. Don't use raw `idxs[0]`; it can be `..`.
- **Snapshot pane (`volumeId === 'search-results'`) couples two points**: `computeHasParent` returns `false` (no `..`
  row) AND opening a real entry must LEAVE the snapshot volume (`FilePane.handleNavigate` → `onGoToLocation` → switch
  arm). Skip either → off-by-one selection, or `search-results` stuck on a real path. DETAILS § Conventions.
- **The MTP clipboard refusal gate keys on `caps.kind === 'mtp'`, not `!supportsSystemClipboard`** (network and
  search-results lack one too, so the MTP toast would misfire).
- **The focus guard (`key-dispatch.ts` `handleFocusGuard`) must keep its `[role="dialog"], [role="alertdialog"]`
  exemption.** Rename dialogs mount inside FilePane; without it the guard and `use:trapFocus` ping-pong focus endlessly
  and freeze the webview. Pinned by E2E.
- **Nav-state persistence fires from ONE subscriber** (`persistence-subscriber.svelte.ts`, A5). Don't scatter
  `saveAppStatus` / `saveTabsForPaneSide` across nav paths: mutate the store, the subscriber reacts (exceptions in
  DETAILS).
- **`navigate(intent, deps)` is the single coordinator-level pane-nav entry.** `{ goTo }` self-routes by volume (same →
  in-place, different → switch); `{ selectVolume }` always switches. Resolve bare paths to a `Location` at the edge,
  never feed one in. Refusal `message` strings are byte-pinned (the MCP adapter forwards them verbatim). DETAILS § "The
  navigate() transaction".
- **Self-drag drop builds from recorded app state, not the pasteboard** (`handleDrop` consumes
  `consumableSelfDragIdentity` for a self-drag from a backend-real volume). See
  [`../drag/CLAUDE.md`](../drag/CLAUDE.md).
- **`DualPaneExplorer.svelte` (~1450 lines, mostly `ExplorerAPI` facade + wiring + markup) and `FilePane.svelte` (~2815)
  are `file-length`-flagged.** Don't add to them or carve child components (DETAILS § "Why not child components");
  cross-cutting state → a `*.svelte.ts` factory, pure logic → a `*.ts` helper.
- **Volume tint has an old-WebKit (Safari < 16.2) sRGB fallback** gated by `hasColorMix`. Don't drop the reactive
  `mediaTick`, or dark-mode / contrast swaps won't repaint the tint.

Read [DETAILS.md](DETAILS.md) before any non-trivial work here: editing, planning, reorganizing, or advising.
