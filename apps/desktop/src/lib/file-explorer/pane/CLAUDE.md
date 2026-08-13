# Pane subsystem

Per-pane orchestrator: cursor, focus, tabs, selection, type-to-jump, dialogs, drag, tinting, navigation. Up:
`../CLAUDE.md`.

## Module map

- `DualPaneExplorer.svelte`: root, owns both panes, unified key/command dispatch, the dialog manager, the MCP surface.
- `FilePane.svelte`: one pane (lifecycle `$state`, the `FilePaneAPI` exports, the alt-view `{#if}` chain). Its
  controller and the rest live in siblings: `*.svelte.ts` state factories, `*.ts` pure helpers, listed in DETAILS.

## Must-knows

- **One pane is always focused and only `setFocusedPane` mutates it**; a switch clears type-to-jump and rename, and
  startup must call `updateFocusedPane` or Rust's left default misdirects Ask Cmdr and MCP.
- **Explorer-store fields are module-private with one mutator each** (`cmdr/no-explorer-state-writes`); `cursorIndex`,
  selection, and listing UI state stay LOCAL to `FilePane` (perf P3).
- **Guard logic branches on `VolumeCapabilities`, ❌ never volume-id strings** (invariant A6). `volume-capabilities.ts`
  is the single FE source of truth; per-VOLUME runtime flags (`isReadOnly`, …) stay on `VolumeInfo`. `capabilitiesFor` /
  `volumeKindOf` stay TOTAL (unknown ids fall to `local`); the tint classifier `volumeKindFor` is separate and ❌ never
  gets that default.
- **Archive panes are KIND-FROM-PATH: gate via `capabilitiesForPane(volumeId, path)`, never `VolumeInfo` alone** — a
  pane inside an archive keeps the parent DRIVE's `volumeId`. Zip is WRITABLE, tar/7z READ-ONLY.
- **The snapshot pane (`volumeId === 'search-results'`) couples two points**: `computeHasParent` returns `false`, AND
  opening a real entry must LEAVE the snapshot volume. Skip either and selection goes off-by-one, or `search-results`
  sticks on a real path.
- **BIRTH CONTEXT and an ADOPTED operation are separate slots in separate MODULES.** `adopted-operation.svelte.ts` and
  `archive-password-flow.svelte.ts` get a read-only `hasBirthContext()` and argument-free commands, ❌ never the props,
  a writer, or a getter. ❌ Never read the progress slot's occupancy off `showTransferProgressDialog`. DETAILS § "Birth
  context".
- **A dialog on screen refuses the commands that START a file operation, ❌ never the ones that STEER a running one.**
  Cancel, pause, rollback, queue, and answering a clash must keep working with the progress dialog up. Which dialogs
  block is declared per entry in `$lib/ui/dialog-registry.ts` (a new one won't compile without a verdict); the four
  refusal layers, and why greying the menu can't be the guard, are DETAILS § "The operation-start gate".
- **Every dialog renders inside ONE `<svelte:boundary>` in `DialogManager.svelte`**: `show*` flips before the dialog
  renders and suppresses pane keys, so a mid-render throw would wedge the keyboard with a blank screen.
- **Nav-state persistence fires from ONE subscriber** (`persistence-subscriber.svelte.ts`, A5): mutate the store and let
  it react; ❌ don't scatter `saveAppStatus` / `saveTabsForPaneSide` across nav paths.
- **The first-run layout's `markAlreadyLaidOut` branch is load-bearing, ❌ never "simplify" it away.** An install that
  already has pane state gets the `firstRunLayoutApplied` marker recorded and its panes left alone; without that,
  everyone upgrading into the rule (Full Disk Access granted, no marker) gets their real layout overwritten, and pane
  paths persist, so there's no way back. Same for the order: `~/Downloads` is probed only once Full Disk Access is
  confirmed, or the probe can raise a TCC dialog. DETAILS § "First-run pane layout".
- **`navigate(intent, deps)` is the single pane-nav entry**: `{ goTo }` self-routes by volume, `{ selectVolume }` always
  switches. Resolve bare paths to a `Location` at the edge. Refusal `message` strings are byte-pinned.
- **`DualPaneExplorer.svelte` and `FilePane.svelte` are `file-length`-flagged**: don't add to them, and ❌ don't carve
  child components either. Cross-cutting state → a `*.svelte.ts` factory, pure logic → a `*.ts` helper.

The file table, the key-dispatch focus guard's dialog exemption, the walk-up fallback's volume re-resolve, `getTabMgr`'s
live `$state` holder, the select-only cursor jump, the MTP clipboard gate, self-drag identity, the volume tint's
`hasColorMix` fallback, `ErrorPane`'s ways out, and the A6 residue inventory: `DETAILS.md`. Read it before any
non-trivial work here: editing, planning, reorganizing, or advising.
