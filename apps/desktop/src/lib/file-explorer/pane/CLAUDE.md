# Pane subsystem

Per-pane orchestrator: cursor, focus, tabs, selection, type-to-jump, dialogs, drag, tinting, navigation. Up:
`../CLAUDE.md`.

## Module map

- `DualPaneExplorer.svelte`: the root, owning both panes, key/command dispatch, the dialog manager, the MCP surface.
- `FilePane.svelte`: one pane (lifecycle `$state`, the `FilePaneAPI` exports, the alt-view `{#if}` chain); its
  controllers and helpers are siblings (`DETAILS.md` § File map).

## Must-knows

- **Only `setFocusedPane` mutates the focused pane**, and startup must call `updateFocusedPane` or Rust's left default
  misdirects Ask Cmdr and MCP.
- **Guard logic branches on `VolumeCapabilities`, ❌ never volume-id strings** and ❌ never a backend-sourced KIND: an
  un-upgraded SMB share is served by a local one.
- **The two ROUTED panes are KIND-FROM-PATH: gate via `capabilitiesForPane(volumeId, path)`**, since an archive or
  `.git`-portal pane keeps the parent DRIVE's `volumeId`.
- **Two archive path predicates, ❌ not swappable**: `pathCrossesArchiveBoundary` (at-or-inside) asks about a PANE path,
  `pathInsideArchive` (strictly inside) about a site acting ON one.
- **The snapshot pane (`volumeId === 'search-results'`) couples five points**, and skipping one silently breaks
  selection, the path, delete, the MCP mirror, or sort (`DETAILS.md` § Conventions).
- **BIRTH CONTEXT and an ADOPTED operation are separate slots in separate MODULES**: flow modules get a read-only
  `hasBirthContext()`, ❌ never a writer.
- **Every dialog renders inside ONE `<svelte:boundary>` in `DialogManager.svelte`**: `show*` flips first and suppresses
  pane keys, so a mid-render throw wedges the keyboard behind a blank screen.
- **Nav-state persistence fires from ONE subscriber** (`persistence-subscriber.svelte.ts`): mutate the store and let it
  react, ❌ never a scattered `saveAppStatus`.
- **Each first-run-layout guardrail looks like a tidy-up. ❌ Never "simplify" one away** (`DETAILS.md` § First-run pane
  layout).
- **`navigate(intent, deps)` is the single pane-nav entry**: `{ goTo }` self-routes by volume, `{ selectVolume }` always
  switches.
- **The `network` pane is the SERVERS HUB** (`NetworkMountView`): it owns its MCP push, so `pane-mcp-sync` skips it, and
  ❗ its NAME is spelled in four places (`../network/DETAILS.md` § Gotchas).
- **ONE typed state renders every remote wait** (`remote-connect-state.ts` + `RemoteConnectView`): `place-connect` gates
  on CONNECTION STATE, `device-connect` on `deviceReadiness`, both in FRONT of the kind chain. ❌ No second renderer, no
  inert affordance. ❗ `device-connect` is a phone's ONE dialer and HOLDS the listing (`holdsListing`): an undialed
  phone's listing can only refuse. An eject (row loses `capabilities`) re-dials.
- **`DualPaneExplorer.svelte` / `FilePane.svelte` are at their size cap**: cross-cutting state → a `*.svelte.ts`
  factory, pure logic → a `*.ts` helper, ❌ never a child component.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
