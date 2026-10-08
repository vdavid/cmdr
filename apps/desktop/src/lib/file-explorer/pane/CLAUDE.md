# Pane subsystem

Per-pane orchestrator: cursor, focus, tabs, selection, type-to-jump, dialogs, drag, tinting, navigation. Up:
`../CLAUDE.md`.

## Module map

`DualPaneExplorer.svelte` (root: both panes, key/command dispatch, dialogs, the MCP surface), `FilePane.svelte` (one
pane), and `navigate.ts` (the `navigate()` transaction, the single pane-nav entry). All three are split for length;
their helper siblings are listed in `DETAILS.md` § File map.

## Must-knows

- **Row indices belong to `pane-row-state.ts`'s applied revision.** Sort/visibility/filter changes use its serialized
  gate; compare and F5 require ready rows. Transition batches stay separate. Protocol: `DETAILS.md` § Compare
  directories.
- **Only `setFocusedPane` mutates the focused pane**, and startup must call `updateFocusedPane`, or Rust's left default
  misdirects Ask Cmdr and MCP.
- **Guard on `capabilitiesForPane(volumeId, path)`, ❌ never a volume-id string or a backend-sourced KIND**: an
  un-upgraded SMB share is served by a local one, and the two ROUTED panes (archive, `.git` portal) keep the parent
  DRIVE's `volumeId`. Its archive predicates are ❌ not swappable: `pathCrossesArchiveBoundary` (at-or-inside) asks
  about a PANE path, `pathInsideArchive` (strictly inside) about a site acting ON one.
- **The snapshot pane (`volumeId === 'search-results'`) couples six points**; skip one and selection, the path, delete,
  the MCP mirror, sort, or the footer's counts silently break. Its path must ❌ never reach disk, and only the
  `{ snapshot }` arm may open one.
- **Every dialog renders inside ONE `<svelte:boundary>` in `DialogManager.svelte`**: `show*` flips first and suppresses
  pane keys, so a mid-render throw wedges the keyboard behind a blank screen.
- **Nav-state persistence fires from ONE subscriber** (`persistence-subscriber.svelte.ts`): mutate the store and let it
  react, ❌ never a scattered `saveAppStatus`.
- **The `network` pane is the SERVERS HUB** (`NetworkMountView`): it owns its MCP push, so `pane-mcp-sync` skips it, and
  ❗ its NAME is spelled in four places (`../network/DETAILS.md` § Gotchas).
- **ONE typed state renders every remote wait** (`remote-connect-state.ts` + `RemoteConnectView`), gating in FRONT of
  the kind chain. ❌ No second renderer, no inert affordance. ❗ `device-connect` is a phone's ONE dialer and HOLDS the
  listing.
- **`DualPaneExplorer.svelte` / `FilePane.svelte` are at their size cap**: cross-cutting state → a `*.svelte.ts`
  factory, pure logic → a `*.ts` helper, ❌ never a child component.
- **A sub-100 ms navigation keeps the last settled rows visible** (`listing-presentation.svelte.ts`), and the list
  caches replace them only after the new visible range lands. ❌ Don't clear rows at navigation start. A longer load
  lays `LoadingIcon` over the rows (`loadingOverlay`); ❌ never swap the list out for it, or the column header blinks.
- **Five behaviors each carry a guardrail that reads like a tidy-up, so read the `DETAILS.md` section before touching
  one**: birth context (a read-only `hasBirthContext()` for flow modules, ❌ never a writer), first-run pane layout,
  Escape during a load (return to what the pane last SHOWED, ❌ never a guess from history), select-same-kind (`⌥⇧=`,
  over the whole-listing snapshot), and the `⌃⏎` keyboard context menu (anchor only, ❌ never a scroll).

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
