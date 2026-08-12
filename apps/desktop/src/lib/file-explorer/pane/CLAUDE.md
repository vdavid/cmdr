# Pane subsystem

Per-pane orchestrator: cursor, focus, tabs, selection, type-to-jump, dialogs, drag, tinting, navigation. Up:
`../CLAUDE.md`. File table and conventions: `DETAILS.md`.

## Module map

- `DualPaneExplorer.svelte`: root, owns both panes, unified key/command dispatch, the dialog manager, the MCP surface.
- `FilePane.svelte`: one pane (lifecycle `$state`, the `FilePaneAPI` exports, the alt-view `{#if}` chain). Its
  controller and everything else live in siblings: `*.svelte.ts` state factories and `*.ts` pure helpers, listed in
  DETAILS.

## Must-knows

- **One pane is always focused, and only `setFocusedPane` mutates it**; a pane switch clears type-to-jump and rename.
  Startup must call `updateFocusedPane`, or Rust's left default misdirects Ask Cmdr and MCP.
- **Explorer-store fields are module-private with exactly one mutator each** (`cmdr/no-explorer-state-writes`).
  `cursorIndex`, selection, and listing UI state stay LOCAL to `FilePane` (perf P3).
- **Guard logic branches on `VolumeCapabilities`, never on volume-id strings** (invariant A6). `volume-capabilities.ts`
  is the single FE source of truth; per-VOLUME runtime flags (`isReadOnly`, …) stay on `VolumeInfo`.
- **`capabilitiesFor` / `volumeKindOf` must stay TOTAL** (unknown real ids fall to the `local` default). Keep the tint
  classifier `volumeKindFor` separate, and never feed the `local` default into tinting.
- **Archive panes are KIND-FROM-PATH: gate via `capabilitiesForPane(volumeId, path)`, never `VolumeInfo` alone** — a
  pane inside an archive keeps the parent DRIVE's `volumeId`. Zip is WRITABLE, tar/7z READ-ONLY.
- **Keydown handlers match the WHOLE combo via `eventMatchesCommand`, never `e.key` + a modifier flag** (⌥⌘A is not
  ⌘A; `cmdr/no-raw-key-match`); the two class-of-key matchers are the exception.
- **The snapshot pane (`volumeId === 'search-results'`) couples two points**: `computeHasParent` returns `false`, AND
  opening a real entry must LEAVE the snapshot volume. Skip either and selection goes off-by-one, or `search-results`
  sticks on a real path.
- **Every dialog renders inside ONE `<svelte:boundary>` in `DialogManager.svelte`**: a `show*` flag is set before the
  dialog renders and suppresses pane keys, so a mid-render throw would wedge the keyboard with a blank screen.
- **The focus guard (`key-dispatch.ts`) must keep its `[role="dialog"], [role="alertdialog"]` exemption**: rename
  dialogs mount inside FilePane, and without it the guard and `use:trapFocus` ping-pong focus and freeze the webview
  (E2E-pinned).
- **Nav-state persistence fires from ONE subscriber** (`persistence-subscriber.svelte.ts`, A5): mutate the store and
  let it react, ❌ don't scatter `saveAppStatus` / `saveTabsForPaneSide` across nav paths.
- **`navigate(intent, deps)` is the single pane-nav entry**: `{ goTo }` self-routes by volume, `{ selectVolume }`
  always switches. Resolve bare paths to a `Location` at the edge, never feed one in. Refusal `message` strings are
  byte-pinned.
- **The walk-up fallback picks its volume from the TARGET** (`listing-loader.ts::navigateToFallback`): it can climb
  out of a vanished volume, and reusing the pane's dead id strands it on `Volume not found` for good.
- **`DualPaneExplorer.svelte` and `FilePane.svelte` are `file-length`-flagged**: don't add to them, and ❌ don't carve
  child components either. Cross-cutting state → a `*.svelte.ts` factory, pure logic → a `*.ts` helper.

DETAILS also owns the narrower contracts an edit here can trip over: `getTabMgr`'s live `$state` holder, the
select-only cursor jump, the `caps.kind === 'mtp'` clipboard gate, self-drag identity, the volume tint's `hasColorMix`
fallback, `ErrorPane`'s ways out, and the A6 residue inventory. Read `DETAILS.md` before any non-trivial work here:
editing, planning, reorganizing, or advising.
