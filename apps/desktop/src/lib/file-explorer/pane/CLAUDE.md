# Pane subsystem

Per-pane orchestrator: cursor, focus, tabs, selection, type-to-jump, dialogs, drag, tinting, navigation. Up:
`../CLAUDE.md`. File table and conventions: `DETAILS.md`.

## Module map

- `DualPaneExplorer.svelte`: root, owns both panes, unified key/command dispatch, the dialog manager, the MCP surface.
- `FilePane.svelte`: one pane. Keeps its lifecycle `$state`, the `FilePaneAPI` exports, and the alt-view `{#if}` chain;
  its controller lives in siblings (DETAILS § "The FilePane controller modules").
- Siblings hold everything else: `*.svelte.ts` state factories and `*.ts` pure helpers (`navigate`, `listing-loader`,
  `volume-capabilities`, …). Full list in DETAILS.

## Must-knows

- **One pane is always focused.** Only `setFocusedPane` mutates it. Pane-switch clears type-to-jump and rename. Startup
  calls `updateFocusedPane`; otherwise Rust's left default misdirects Ask Cmdr/MCP.
- **Explorer-store fields are module-private with exactly one mutator each.** Assigning any store property outside
  `explorer-state.svelte.ts` is a lint error (`cmdr/no-explorer-state-writes`). `cursorIndex`, selection, and listing UI
  state stay LOCAL to `FilePane` (perf P3).
- **`getTabMgr(pane)` returns the live `$state` holder** (`$state.snapshot` severs reactivity).
- **Guard logic branches on `VolumeCapabilities`, never on volume-id strings** (invariant A6). `volume-capabilities.ts`
  is the single FE source of truth; per-VOLUME runtime flags (`isReadOnly`, etc.) stay on `VolumeInfo`. Residual hits:
  DETAILS § "A6 residue inventory".
- **`capabilitiesFor` / `volumeKindOf` must stay TOTAL** (never `undefined`; unknown real ids fall to the `local`
  default). Keep the tint classifier `volumeKindFor` separate; never feed the `local` default into tinting.
- **Archive panes are KIND-FROM-PATH: gate via `capabilitiesForPane(volumeId, path)`, never `VolumeInfo` alone** — a
  pane inside an archive keeps the parent DRIVE's `volumeId`; the PATH makes it the `archive` kind. Zip is WRITABLE;
  tar/7z READ-ONLY (copy-OUT only). DETAILS § "Archive browsing and editing".
- **Keydown handlers match the WHOLE combo via `eventMatchesCommand`, never `e.key` + a modifier flag** (⌥⌘A is not ⌘A;
  `cmdr/no-raw-key-match`); the two class-of-key matchers are the exception. DETAILS.
- **`FilePane.applyIndices` jumps the cursor on SELECT only** (deselect leaves it put), via `firstSelectedIndex` (skips
  the `..` row); raw `idxs[0]` can be `..`.
- **Snapshot pane (`volumeId === 'search-results'`) couples two points**: `computeHasParent` returns `false`, AND
  opening a real entry must LEAVE the snapshot volume. Skip either → off-by-one selection, or `search-results` stuck on
  a real path. DETAILS § Conventions.
- **The MTP clipboard refusal gate keys on `caps.kind === 'mtp'`, not `!supportsSystemClipboard`** (network and
  search-results lack one too, so the MTP toast would misfire).
- **Every dialog renders inside ONE `<svelte:boundary>` in `DialogManager.svelte`.** A `show*` flag is set before the
  dialog renders and suppresses pane keys, so a mid-render throw wedges the keyboard with a blank screen; `onerror` →
  `handleDialogRenderFailure`, then a deferred `reset()`. DETAILS § "A dialog that throws during render".
- **The focus guard (`key-dispatch.ts`) must keep its `[role="dialog"], [role="alertdialog"]` exemption.** Rename
  dialogs mount inside FilePane; without it the guard and `use:trapFocus` ping-pong focus and freeze the webview
  (E2E-pinned).
- **Nav-state persistence fires from ONE subscriber** (`persistence-subscriber.svelte.ts`, A5). Don't scatter
  `saveAppStatus` / `saveTabsForPaneSide` across nav paths: mutate the store, the subscriber reacts (DETAILS).
- **`navigate(intent, deps)` is the single pane-nav entry.** `{ goTo }` self-routes by volume; `{ selectVolume }` always
  switches. Resolve bare paths to a `Location` at the edge, never feed one in. Refusal `message` strings are
  byte-pinned. DETAILS § "The navigate() transaction".
- **The walk-up fallback picks its volume from the TARGET** (`listing-loader.ts::navigateToFallback`), via
  `resolvePathVolume`: the walk-up can climb out of a vanished volume, and reusing the pane's dead id strands it on
  `Volume not found` for good. DETAILS § "The walk-up fallback".
- **Self-drag drop builds from recorded app state, not the pasteboard** (`handleDrop` consumes
  `consumableSelfDragIdentity`). `../drag/CLAUDE.md`.
- **`DualPaneExplorer.svelte` and `FilePane.svelte` are `file-length`-flagged**: don't add to them or carve child
  components (DETAILS § "Why not child components"); cross-cutting state → a `*.svelte.ts` factory, pure logic → a
  `*.ts` helper.
- **Volume tint has an old-WebKit (Safari < 16.2) sRGB fallback** gated by `hasColorMix`. Keep the reactive `mediaTick`,
  or dark-mode / contrast swaps won't repaint the tint.
- **`ErrorPane`: ⌘D stays a CAPTURE-phase `document` listener** (it must outrank any user ⌘D binding), `Go back` needs
  `canGoBack`, and `Try again` keys on `retryHint` ALONE. DETAILS § "The error screen's ways out".

Read `DETAILS.md` before any non-trivial work here: editing, planning, reorganizing, or advising.
