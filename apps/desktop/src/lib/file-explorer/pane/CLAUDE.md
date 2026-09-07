# Pane subsystem

Per-pane orchestrator: cursor, focus, tabs, selection, type-to-jump, dialogs, drag, tinting, navigation. Up:
`../CLAUDE.md`.

## Module map

- `DualPaneExplorer.svelte`: root, owns both panes, key/command dispatch, the dialog manager, the MCP surface.
- `FilePane.svelte`: one pane (lifecycle `$state`, the `FilePaneAPI` exports, the alt-view `{#if}` chain). Its
  controller and helpers are siblings, in DETAILS.

## Must-knows

- **One pane is always focused and only `setFocusedPane` mutates it**; a switch clears type-to-jump and rename, and
  startup must call `updateFocusedPane` or Rust's left default misdirects Ask Cmdr and MCP.
- **`cursorIndex`, selection, and listing UI state stay LOCAL to `FilePane`** (perf P3); explorer-store fields are
  module-private with one mutator each (`cmdr/no-explorer-state-writes`).
- **Guard logic branches on `VolumeCapabilities`, ❌ never volume-id strings** — Rust answers what a volume CAN DO
  (`canWrite` / `canBeSource`), `volume-capabilities.ts` what it IS. ❌ Never source KIND from the backend: an
  un-upgraded SMB share is served by a local one.
- **The two ROUTED panes are KIND-FROM-PATH: gate via `capabilitiesForPane(volumeId, path)`** — an archive or
  `.git`-portal pane keeps the parent DRIVE's `volumeId`. Zip is WRITABLE; tar/7z, OOXML docs, and portal snapshots
  READ-ONLY.
- **Two archive path predicates**: `pathCrossesArchiveBoundary` (at-or-inside) for a PANE path; `pathInsideArchive`
  (strictly inside) for a site acting ON one — a `.zip`/`.docx` file itself previews, moves, and renames normally. ❌
  Never add a document suffix to `WRITABLE_ARCHIVE_SUFFIXES`: a `.docx` is a zip the mutator would rewrite.
- **The snapshot pane (`volumeId === 'search-results'`) couples five points**; skip one and you get an off-by-one
  selection, a stuck `search-results` path, a delete on rows nobody picked, an MCP delete refused by stale pane state,
  or a folder re-sorted from a pane that isn't it (its header sorts the SNAPSHOT, ❌ never `setPaneSort`). DETAILS §
  Snapshot pane.
- **BIRTH CONTEXT and an ADOPTED operation are separate slots in separate MODULES.** The flow modules get a read-only
  `hasBirthContext()` and argument-free commands, ❌ never the props, a writer, a getter, or the progress slot's
  occupancy off `showTransferProgressDialog`. DETAILS § Birth context.
- **A dialog on screen refuses the commands that START a file operation, ❌ never the ones that STEER a running one.**
  `$lib/ui/dialog-registry.ts` declares the verdict per entry (a new dialog won't compile without one). DETAILS § The
  operation-start gate.
- **Every dialog renders inside ONE `<svelte:boundary>` in `DialogManager.svelte`**: `show*` flips before the dialog
  renders and suppresses pane keys, so a mid-render throw wedges the keyboard behind a blank screen.
- **Nav-state persistence fires from ONE subscriber** (`persistence-subscriber.svelte.ts`): mutate the store and let it
  react, ❌ don't scatter `saveAppStatus` / `saveTabsForPaneSide` across nav paths.
- **Three first-run-layout guardrails, each looking like a tidy-up. ❌ Never "simplify" one away.** DETAILS § First-run
  pane layout.
- **`navigate(intent, deps)` is the single pane-nav entry**: `{ goTo }` self-routes by volume, `{ selectVolume }` always
  switches, bare paths resolve to a `Location` at the edge.
- **The `network` pane is the SERVERS HUB** (`NetworkMountView`): it owns its MCP push, so `pane-mcp-sync` skips it.
  ❗ Its NAME is spelled in four places. `../network/DETAILS.md` § Gotchas.
- **A pane on a SAVED place dials it with a cancel, and ONE typed state drives every remote wait**
  (`place-connect.svelte.ts`, `remote-connect-state.ts`, `RemoteConnectView.svelte`). The dial's gate is the CONNECTION
  STATE, in FRONT of the kind chain. ❌ No second renderer, no inert button, no unused `RemoteConnectState` variant.
  DETAILS § The connect views, § A pane on a saved place.
- **`DualPaneExplorer.svelte` / `FilePane.svelte` are at their size cap**: cross-cutting state → a `*.svelte.ts`
  factory, pure logic → a `*.ts` helper, ❌ never a child component.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here.
