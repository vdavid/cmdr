# Pane subsystem

Per-pane orchestrator: cursor, focus, tabs, selection, type-to-jump, dialogs, drag, tinting, navigation. Up:
`../CLAUDE.md`. File table and conventions: `DETAILS.md`.

## Module map

- `DualPaneExplorer.svelte`: root, owns both panes, unified key/command dispatch, the dialog manager, the MCP surface.
- `FilePane.svelte`: one pane, owns its listing, cursor, selection, view mode, type-to-jump, rename flow, breadcrumb,
  and the alt-view `{#if}` chain.
- State factories (`*.svelte.ts`): `explorer-state`, `selection-state`, `rename-flow`, `persistence-subscriber`,
  `drag-drop-controller`, `volume-tint`, and friends (full list in DETAILS).
- Pure utilities (`*.ts`): `navigate`, `listing-loader`, `volume-capabilities`, `has-parent`, `pane-access`,
  `focused-pane-reads`, command/coordinator factories, and keyboard helpers (full list in DETAILS).

## Must-knows

- **One pane is always focused.** Only `setFocusedPane` mutates it. Pane-switch clears type-to-jump and rename. Startup
  calls `updateFocusedPane`; otherwise Rust's left default misdirects Ask Cmdr/MCP.
- **Explorer-store fields are module-private with exactly one mutator each.** Assigning any store property outside
  `explorer-state.svelte.ts` is a lint error (`cmdr/no-explorer-state-writes`). `cursorIndex`, selection, and listing UI
  state stay LOCAL to `FilePane` (perf P3).
- **`getTabMgr(pane)` returns the live `$state` holder** (`$state.snapshot` severs reactivity).
- **Guard logic branches on `VolumeCapabilities`, never on volume-id strings** (invariant A6). `volume-capabilities.ts`
  is the single FE source of truth (a `VolumeKind` union keys a frozen per-kind table); per-VOLUME runtime flags
  (`isReadOnly`, etc.) stay on `VolumeInfo`. Residual string hits are justified (DETAILS § "A6 residue inventory").
- **`capabilitiesFor` / `volumeKindOf` must stay TOTAL** (never `undefined`; unknown real ids fall to the `local`
  default). Keep the tint classifier `volumeKindFor` separate; never feed the `local` default into tinting.
- **Archive panes are KIND-FROM-PATH: gate via `capabilitiesForPane(volumeId, path)`, never `VolumeInfo` alone.** A pane
  inside an archive keeps the parent DRIVE's `volumeId`; the PATH makes it the `archive` kind. Zip is WRITABLE
  (mutations are managed ops — op handle, not a path; delete is permanent); tar/7z are READ-ONLY, so
  `capabilitiesForPane` returns the write-flags-off `archive` row (`isWritableArchiveName`) and
  mkdir/mkfile/rename/paste gate off (copy-OUT still works). DETAILS § "Archive browsing and editing".
- **`FilePane.applyIndices` jumps the cursor on SELECT only** (deselect leaves it put), via `firstSelectedIndex` (skips
  the `..` row); raw `idxs[0]` can be `..`.
- **Snapshot pane (`volumeId === 'search-results'`) couples two points**: `computeHasParent` returns `false` (no `..`
  row) AND opening a real entry must LEAVE the snapshot volume (`FilePane.handleNavigate` → `onGoToLocation` → switch
  arm). Skip either → off-by-one selection, or `search-results` stuck on a real path. DETAILS § Conventions.
- **The MTP clipboard refusal gate keys on `caps.kind === 'mtp'`, not `!supportsSystemClipboard`** (network and
  search-results lack one too, so the MTP toast would misfire).
- **The focus guard (`key-dispatch.ts`) must keep its `[role="dialog"], [role="alertdialog"]` exemption.** Rename
  dialogs mount inside FilePane; without it the guard and `use:trapFocus` ping-pong focus and freeze the webview. Pinned
  by E2E.
- **Nav-state persistence fires from ONE subscriber** (`persistence-subscriber.svelte.ts`, A5). Don't scatter
  `saveAppStatus` / `saveTabsForPaneSide` across nav paths: mutate the store, the subscriber reacts (exceptions in
  DETAILS).
- **`navigate(intent, deps)` is the single coordinator-level pane-nav entry.** `{ goTo }` self-routes by volume;
  `{ selectVolume }` always switches. Resolve bare paths to a `Location` at the edge, never feed one in. Refusal
  `message` strings are byte-pinned. DETAILS § "The navigate() transaction".
- **Self-drag drop builds from recorded app state, not the pasteboard** (`handleDrop` consumes
  `consumableSelfDragIdentity`). See `../drag/CLAUDE.md`.
- **`DualPaneExplorer.svelte` (~1450 lines) and `FilePane.svelte` (~2815) are `file-length`-flagged**: don't add to them
  or carve child components (DETAILS § "Why not child components"); cross-cutting state → a `*.svelte.ts` factory, pure
  logic → a `*.ts` helper.
- **Volume tint has an old-WebKit (Safari < 16.2) sRGB fallback** gated by `hasColorMix`. Don't drop the reactive
  `mediaTick`, or dark-mode / contrast swaps won't repaint the tint.

Read `DETAILS.md` before any non-trivial work here: editing, planning, reorganizing, or advising.
