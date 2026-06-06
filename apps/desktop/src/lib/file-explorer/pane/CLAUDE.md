# Pane subsystem

Per-pane orchestrator: cursor, scroll, focus, dual-pane coordination, tab state, selection state, type-to-jump, dialog
lifecycle, drag handling, volume tinting, and navigation primitives. Up: [`../CLAUDE.md`](../CLAUDE.md).

`DualPaneExplorer.svelte` is the root: it owns both panes, the unified key/command dispatch, the dialog manager, and the
MCP-exposed surface. `FilePane.svelte` is one pane: it owns its listing, cursor, selection, view mode, type-to-jump
buffer, rename flow, breadcrumb, and the alt-view rendering ({#if/elseif} between `MtpConnectionView`,
`NetworkMountView`, `SmbReconnectingView`, `SearchResultsView`, `ErrorPane`, `VolumeUnreachableBanner`, and the regular
list).

## File map

### Components

| File                             | Purpose                                                                                       |
| -------------------------------- | --------------------------------------------------------------------------------------------- |
| `DualPaneExplorer.svelte`        | Root: two panes + resizer + dialog manager + key/command dispatch + MCP wiring                |
| `FilePane.svelte`                | One pane: listing, cursor, selection, view mode, breadcrumb, alt-view switching               |
| `DialogManager.svelte`           | Renders every modal dialog (transfer, delete, rename, new-folder, alert, error)               |
| `FunctionKeyBar.svelte`          | F1–F10 bar at the bottom of the window                                                        |
| `PaneResizer.svelte`             | Drag handle between the two panes                                                             |
| `ErrorPane.svelte`               | Friendly-error display for listing failures (see parent § "Error display")                    |
| `VolumeUnreachableBanner.svelte` | Volume resolution timed out OR SMB give-up state; retry + open home / disconnect              |
| `SmbReconnectingView.svelte`     | Spinner + progress bar while `smb-reconnect-manager` runs its backoff cycle                   |
| `SmbReauthView.svelte`           | Sign-in prompt when an SMB reconnect gave up on auth (`needs-auth`); wraps `NetworkLoginForm` |
| `MtpConnectionView.svelte`       | Placeholder pane for MTP connection states                                                    |
| `NetworkMountView.svelte`        | Network browser host/share list + login form                                                  |
| `SearchResultsView.svelte`       | Snapshot view for `volumeId === 'search-results'` (see parent § "Search-results")             |
| `TypeToJumpIndicator.svelte`     | Bottom-right "Jump: …" chip                                                                   |

### Reactive state (`*.svelte.ts`)

| File                               | Purpose                                                                                   |
| ---------------------------------- | ----------------------------------------------------------------------------------------- |
| `explorer-state.svelte.ts`         | Explorer store: `focusedPane`, `showHiddenFiles`, layout split, the two tab-mgr holders   |
| `dialog-state.svelte.ts`           | Dialog props + handlers (transfer, delete, mkdir, alert, error); factory                  |
| `selection-state.svelte.ts`        | `SvelteSet<number>` of indices + range anchor/end + `applyIndices` helpers                |
| `rename-flow.svelte.ts`            | Rename validation, conflict + extension dialogs, save / cancel                            |
| `type-to-jump-state.svelte.ts`     | Buffer + indicator + reset/hide timers + generation counter (race protection)             |
| `volume-tint.svelte.ts`            | `color-mix(...)` or sRGB hex by volume kind; pure `volumeKindFor` classifier              |
| `pane-mcp-sync.svelte.ts`          | Mirrors pane state into the MCP `PaneState` store; skips network/search panes             |
| `persistence-subscriber.svelte.ts` | The single nav-state persistence subscriber (A5): reactive `$effect`s → `app-status.json` |
| `listing-diff-sync.svelte.ts`      | File-watcher listeners + `reconcileCursorAndSelection` (pure, off-by-one core)            |
| `drag-drop-controller.svelte.ts`   | Native drag band: drop-target state, drag handlers, 3 Tauri listeners, highlight effect   |

### Pure utilities (`*.ts`)

| File                          | Purpose                                                                                        |
| ----------------------------- | ---------------------------------------------------------------------------------------------- |
| `types.ts`                    | `FilePaneAPI`, `SwapState`, `ListViewAPI`, `*BrowserAPI`, `NetworkCursorEntry`                 |
| `pane-access.ts`              | `PaneAccess`: live-reference read API over pane nav + chrome state for factories               |
| `focused-pane-reads.ts`       | Store-backed focused-pane reads (path / volume id / searchable folder) for externals           |
| `clipboard-operations.ts`     | System-clipboard copy/cut/paste factory (MTP refusal, snapshot, cut-vs-copy)                   |
| `file-operation-commands.ts`  | Rename / new-folder / new-file / viewer / transfer / delete openers factory                    |
| `pane-commands.ts`            | MCP/palette read-only + delegating command bodies (selection, key-route, MTP val)              |
| `type-to-jump-keys.ts`        | Pure `isTypeToJumpChar` / `isTypeToJumpResetKey` shared by both jump intercepts                |
| `initialization.ts`           | Load persisted tabs + status + settings; resolve volumes; apply E2E overrides                  |
| `tab-operations.ts`           | Tab CRUD + context menu + persistence wired to `tabs/tab-state-manager`                        |
| `transfer-operations.ts`      | Build `TransferDialogPropsData` (and snapshot/dropped variants) from a focused pane            |
| `transfer-entry.ts`           | Shared transfer entry seam: `checkTransferDestinationGuard` + `resolveSourceVolumeId`          |
| `sorting-handlers.ts`         | `getNewSortOrder` (column click cycle), `toFrontendIndices` (`..` offset)                      |
| `index-events.ts`             | Throttled `index-dir-updated` handler with `/private/` symlink resolution                      |
| `navigate.ts`                 | `navigate(intent, deps)` transaction: the single coordinator-level pane-nav entry              |
| `snapshot-pane-navigation.ts` | `isCrossVolumeNavigation` — snapshot-volume → real-path triggers volume switch                 |
| `has-parent.ts`               | `computeHasParent({ isSearchResultsView, currentPath, effectiveVolumeRoot })`                  |
| `volume-capabilities.ts`      | `VolumeKind` + frozen per-kind `VolumeCapabilities` table + `volumeKindOf` / `capabilitiesFor` |
| `search-results-keys.ts`      | Pure key→action dispatch for the flat snapshot pane                                            |
| `selection-dialog-keys.ts`    | Classify `+` / `-` keypresses → open Selection dialog (Total Commander parity)                 |
| `error-pane-utils.ts`         | Tiny helper for `ErrorPane`'s technical-details rendering                                      |
| `integration-test-utils.ts`   | Shared test scaffolding for pane integration tests                                             |

### Tests

Colocated with the code they pin. Notable cross-cutting suites: `DualPaneExplorer.test.ts`,
`selection-consistency.test.ts` (selection survives diffs / cancel / source-item-done), `listing-diff-sync.test.ts`
(pure `reconcileCursorAndSelection` off-by-one coverage), `file-pane-keyboard.test.ts`, `volume-breadcrumb.test.ts`,
`volume-tint.svelte.test.ts` (+ `volume-tint.svelte.fallback.test.ts` for the old-WebKit branch), `*.a11y.test.ts` (axe
sweeps per alt-view component). The drag-drop controller suite is split in two: `drag-drop-controller.svelte.test.ts`
(handler contracts incl. the self-drag-identity scenarios) and `drag-drop-controller.listeners.svelte.test.ts` (Tauri
listener registration + the enter→over→drop cycle), sharing volume constants and builders from
`drag-drop-controller.test-fixtures.ts` (the `vi.mock` blocks stay duplicated per file — vitest hoists them per module).

## Conventions

**Focus contract.** Exactly one pane is focused (`focusedPane: 'left' | 'right'`). The flag lives in the explorer store
(`explorer-state.svelte.ts`), with `setFocusedPane` its single mutator; `DualPaneExplorer` reads it via a `$derived` and
calls the mutator on pane switch. Key dispatch in `DualPaneExplorer` resolves which pane handles a keystroke via this
state, then delegates to `FilePane.handleKeyDown`. Pane-switch (Tab) clears type-to-jump and rename mode on both panes
(see "Reset triggers" in parent § "Type-to-jump").

**Type-to-jump factory.** One `createTypeToJumpState` instance per pane, inside `FilePane`. Reset triggers (ESC, arrows,
Page/Home/End, Enter, Tab, Backspace, rename entry, context menu, drag start, pane switch, tab switch, dir change,
re-sort, listing replace) all call the factory's `clearJumpState()`. The generation counter discards stale async match
responses. Backend match runs in `apps/desktop/src-tauri/src/file_system/listing/fuzzy_jump.rs`.

**Snapshot pane (`volumeId === 'search-results'`).** Two integration points that MUST stay coupled: `computeHasParent`
returns `false` (no `..` row), and `isCrossVolumeNavigation` routes any navigation to a real path through the
volume-change machinery (`onVolumeChange` / `handleVolumeChange`). Skipping either breaks selection (off-by-one) or
poisons the pane with `volumeId === 'search-results'` + real path.

**Volume capabilities (`volume-capabilities.ts`).** The single FE source of truth for "what can a pane on a given volume
KIND do" (invariant A6 — guard logic branches on capabilities, never on volume-id strings). A closed `VolumeKind`
discriminated union (`local` / `smb` / `mtp` / `network` / `search-results`) keys a frozen, by-reference
`Record<VolumeKind, VolumeCapabilities>` table; each row carries structural capabilities (`hasBackendListing`,
`canPasteInto`, `canCreateChild`, `canRenameInPlace`, `canBeSource`, `supportsSystemClipboard`, `hasParentRow`,
`syncsToMcp`, `pathScheme`). It's NOT a `Record<string, boolean>` bag — `kind` is the discriminant.

- **Per-KIND vs per-VOLUME.** The table is structural per-kind capability. Per-volume runtime flags (`isReadOnly`,
  `supportsTrash`, `smbConnectionState`) are NOT in the table — they stay on `VolumeInfo` and layer on top (a specific
  USB stick is read-only; the `local` KIND is not). That's Q4's resolution: FE table keyed by kind, since `VolumeInfo`
  carries no capability surface today and the two virtual kinds have no `VolumeInfo` at all.
- **One classifier, not two.** `volumeKindOf` is the SUPERSET of `volume-tint.svelte.ts::volumeKindFor`: it checks the
  two virtual ids first, then DELEGATES to `volumeKindFor` for the real kinds, overriding its `'other'` fall-through
  (favorites + real-but-unclassified) to a `'local'` default so the kind → table lookup is TOTAL (no input can miss the
  table; `capabilitiesFor` never returns `undefined`). The tint classifier keeps its own body and output, so tint stays
  byte-stable — this module never feeds its `'local'` default back into tinting.
- **`capabilitiesFor(volumeId)`** is the store-reading convenience: callers holding only a `volumeId` (F-bar, dispatch)
  get the row without replicating the find-in-store dance. The virtual ids short-circuit before the store lookup; a
  stale/missing real id falls to the `local` default. The pure pair (`volumeKindOf`, `capabilitiesForKind`) stays
  store-free for the FilePane site (which already holds the `VolumeInfo`) and for tests.
- **To add virtual volume #3:** add a `VolumeKind` member, a table row, and a `volumeKindOf` branch — no codebase sweep.

Consumers read the table directly: `SearchResultsView.svelte` reads `capabilitiesForKind('search-results')` (it always
renders a search-results pane), and every capability-GUARD consumer reads the table via `capabilitiesFor`. There's no
Search-specific capabilities shim — `lib/search/capabilities.ts` keeps only the `SEARCH_RESULTS_NOT_A_FOLDER_TOAST` L10
string. The guards:

- **Dispatch** (`command-dispatch.ts::blockedByCapabilities`) + **F-bar** (`FunctionKeyBar.svelte`): the destination-op
  guards (paste / mkdir / mkfile / rename) off `canPasteInto` / `canCreateChild` / `canRenameInPlace`.
- **Clipboard** (`clipboard-operations.ts`): the snapshot-clip path gate off `pathScheme === 'search-results'`; the MTP
  copy/cut/paste refusals (the "Use F5/F6" toasts) off `caps.kind === 'mtp'` via `isMtpClipboardRefusal`. The MTP gate
  keys on the `kind` discriminant, NOT `!supportsSystemClipboard`, because `network` + `search-results` also lack a
  system clipboard, and an MTP-worded toast on a reachable network paste would be a new, mis-worded toast (PR3). On the
  live clipboard-time pane id set this is byte-equivalent to the old `startsWith('mtp-')` gate, pinned by the
  equivalence test in `clipboard-operations.test.ts`.
- **Transfer / delete** (`file-operation-commands.ts`): source routing (snapshot builder) off `!hasBackendListing`. The
  destination guards (search-results dest-paste block off `!canPasteInto` scoped to the `search-results` kind so the
  toast wording stays correct; the `isReadOnly` alert per-`VolumeInfo`, Q4) live in `transfer-entry.ts`'s
  `checkTransferDestinationGuard` so F5/F6, drag-and-drop, AND paste run the identical chain — see
  `file-operations/transfer/CLAUDE.md` § "One transfer entry seam". The `search-results://` URL parses stay (namespace
  mechanics).
- **`pane-commands.ts`**: `isSnapshotPane` (the Selection-dialog banner flag) off `!hasBackendListing`.
- **MCP sync** (`pane-mcp-sync.svelte.ts`): the network/search skip off `!syncsToMcp`. The deps interface carries a
  single `getSyncsToMcp()` accessor (FilePane supplies it from its derived caps); the two `getIs*View()` deps retired.
- **`has-parent.ts`**: `computeHasParent` folds ONLY the snapshot rule via `hasParentRow`; the two PATH comparisons
  (`=== '/'`, `=== root`) stay, and the rule stays coupled to `isCrossVolumeNavigation` (L5).
- **FilePane alt-view chain** (`FilePane.svelte`): the kind-structural view selection resolves through a `paneViewKind`
  derived discriminant (`'network' | 'search-results' | 'mtp-connect' | 'normal'`) off `caps.kind` (+ the MTP
  device-only connection sub-state, which the table doesn't carry — it's a runtime connection state, not a kind). The
  `{#if}` chain branches on `paneViewKind` for the three alt-views (NetworkMountView / SearchResultsView /
  MtpConnectionView) and the SelectionInfo footer (`paneViewKind === 'normal'`). The RUNTIME-state branches
  (`unreachable`, SMB reconnecting / gave-up / needs-auth sign-in, the inline SMB upgrade login, `loading` /
  `friendlyError` / `error`) stay per-feature and gate IN FRONT of the descriptor, byte-identical precedence (L10). This
  is a derived discriminant, NOT a new component (A8). The per-feature gates (git lookup, type-to-jump keystroke,
  dir-exists poll) read `!caps.hasBackendListing` for the "is there a real directory" half; the MTP-path-specific checks
  (`isMtpVolumeId(volumeId)` for git-skip, `isMtpView` for the dir-poll, `isMtpDeviceOnly` for the jump) STAY — MTP has
  a backend listing but git can't run on it, there's no on-disk path to `pathExists`-poll, and the not-yet-connected
  sub-state isn't a kind capability. `caps` is derived once per pane (`caps = $derived(capabilitiesFor(volumeId))`); the
  named `isNetworkView` / `isSearchResultsView` deriveds re-source off `caps.kind`.

**A6 residue inventory (the sweep is DONE — don't "finish" it).** Every capability GUARD now reads `VolumeCapabilities`;
the A6 conversion is complete. A grep for `=== 'search-results'` / `=== 'network'` / `startsWith('mtp-')` (and the `!==`
forms) across `apps/desktop/src/` still returns hits, but each is justified residue, NOT a guard left behind. Before
converting any of these, understand which category it's in — forcing a mechanics/display/classifier site through the
capability table is the "differently complicated" failure mode the refactor explicitly avoids:

- **Classifier internals (the inputs that FEED `volumeKindOf`).** `volume-capabilities.ts` (the two virtual-id checks),
  `volume-tint.svelte.ts::volumeKindFor` (`category === 'network' || fsType === 'smbfs'`), `volume-grouping.ts`
  (`category === 'network'` sidebar grouping), `mtp-path-utils.ts::isMtpVolumeId` (`startsWith('mtp-')`). These ARE the
  classifier — converting them would be circular.
- **Namespace / path mechanics (which string scheme, not what's allowed).** `navigate.ts` (the on-network / on-MTP
  refusal sources + the `smb://` / `search-results://` drop-foreign-listings prefix + `validateMtpNavigation` path
  parse), `snapshot-pane-navigation.ts` (the cross-volume trigger), `clipboard-operations.ts:76`
  (`pathScheme !== 'search-results'` — the snapshot-clip path resolver; reads the table but it's a scheme question),
  `DualPaneExplorer.svelte` (synthetic `smb://` path/name synthesis + the network-mirror / copy-path-between-panes
  identity branches), `rename-flow.svelte.ts:166` (skip the Unix-`access()` permission check on MTP virtual paths — a
  syscall-support mechanic, not a "may rename" capability).
- **Display / view selection.** `VolumeBreadcrumb.svelte` (the "Network" / "Search results" labels + the
  network-disabled gate), `FilePane.svelte` (`paneViewKind === 'network' | 'search-results'` in the `{#if}` chain — the
  kind-driven view choice, sourced off `caps.kind`; the `isNetworkView` / `isSearchResultsView` named deriveds; the MTP
  device-only sub-state + the `loadDirectory` skip for network/device-only panes), `MtpConnectionView.svelte`
  (device-only sub-state).
- **Persistence / init mechanics.** `app-status-store.ts` (skip filesystem path-resolution for the virtual `network`
  volume on persist), `initialization.ts` (trust the stored `network` id at startup, no `resolvePathVolume`).
- **Converted-caps scope (reads the table, kind-scopes a toast).** `command-dispatch.ts:114` +
  `file-operation-commands.ts:300` (`caps.kind === 'search-results'` decides the toast WORDING after the capability
  decides the block — the capability / kind split, not a string guard).
- **Tests + debug.** `navigate.test.ts` and the other `*.test.*` fixtures, `routes/debug/DebugHistoryPanel.svelte`.

**Command-body factories read through `PaneAccess`.** The MCP/palette command bodies live in factories
(`clipboard-operations`, `file-operation-commands`, `pane-commands`) that take a `PaneAccess` (live-reference read API)
plus the dialog state. The component keeps one-line `export function` delegates so the `ExplorerAPI` surface is
unchanged. Read-only / delegating bodies move; functions that WRITE component navigation state (`switchPane`,
`swapPanes`, `toggleHiddenFiles`, `setViewMode`, `navigate`, `setSort*`, `moveCursor`, `selectVolumeBy*`,
`copyPathBetweenPanes`, the `mirror*`/`restoreFocus` helpers) stay in the component — un-trapping that state is the
explorer-store phase, not this factoring. The `navigate(intent)` transaction itself lives in `navigate.ts` (the
component builds its `NavigateDeps` and wraps it as the `navigate` export). The MTP capability check lives in
`navigate.ts` (`validateMtpNavigation`, the synchronous refusal gate for the in-place path arm); its refusal strings are
byte-pinned by `navigate.test.ts`. `moveCursorByName*` moved into `pane-commands` even though it's called from
component-resident writers (`moveCursor`, `restoreCursorByFilename`); those callers reach back via `paneCommands.*`.

**Explorer store (`explorer-state.svelte.ts`).** Module store owning the dual-pane navigation + UI-chrome state that
`DualPaneExplorer` used to trap in component closures: `focusedPane`, `showHiddenFiles`, `leftPaneWidthPercent`, and the
two tab-manager holders. State is module-private (A1): `createExplorerState()` closes over `$state` locals and exposes
only getters + one named mutator per field. There's no exported writable surface — callers can't assign a field, only
call a mutator (A2; the `cmdr/no-explorer-state-writes` lint rule makes this a hard wall — assigning to any property of
the store object outside `explorer-state.svelte.ts` is a lint error). `createExplorerState()` is factory-first for
testability; the module-level `explorerState` singleton is what the component binds, with `_resetForTesting()` for tests
that touch it.

The **writers** (A2 — exactly one mutator per field, all inside the store module):

| Field                  | Mutator(s)                                |
| ---------------------- | ----------------------------------------- |
| `focusedPane`          | `setFocusedPane`                          |
| `showHiddenFiles`      | `setShowHiddenFiles`, `toggleHiddenFiles` |
| `leftPaneWidthPercent` | `setLeftPaneWidthPercent`                 |
| `leftTabMgr`           | `setTabMgr('left', …)`                    |
| `rightTabMgr`          | `setTabMgr('right', …)`                   |

**Enforced by lint (`cmdr/no-explorer-state-writes`).** Assigning to any property of the store object outside
`explorer-state.svelte.ts` is a lint error (`explorerState.x = …`, compound assignment, `++`, and monkey-patching a
mutator like `explorerState.setFocusedPane = …`). The rule tracks the imported `explorerState` singleton and any
`createExplorerState()` instance. It does NOT police direct `$state` field writes (A1 already makes those inexpressible
from outside — nothing writable is exported) or mutator re-exports (an alias is still a named-mutator call, not a new
writer; forbidding it would false-positive on the read wrappers in `focused-pane-reads.ts`). The rule lives in
`apps/desktop/eslint-plugins/no-explorer-state-writes.js` with a colocated RuleTester test; the store file and test
files are exempt. This is the durable A2 guardrail — discipline that isn't enforced decays once the component wall is
down.

**A1/A2-vs-tab-manager scope boundary.** The private-state + one-mutator rules govern the store's **own** fields only.
The tab managers are _values the store holds_, not store fields: they keep their existing setter-based API
(`createTabManager`) and are mutated via the free functions in `tabs/tab-state-manager.svelte` / `tab-operations`. The
store holds the holder reference and swaps it via `setTabMgr`; it never wraps tab-manager setters behind store intents.

**Live-reference getters.** `getTabMgr(pane)` returns the live `$state<TabManager>` holder, never a copy or a
`$state.snapshot` — a `$derived` reading `getActiveTab(getTabMgr(p))` keeps tracking both when the holder is swapped and
when the held manager mutates in place. Returning a snapshot would silently sever reactivity at the seam (the same rule
`pane-access.ts` documents). What the store does NOT own: `cursorIndex`, selection, and listing UI state stay local to
`FilePane` (perf invariant P3).

**`FunctionKeyBar` reads the store, not props.** The F-key bar is mounted in `+page.svelte` (a sibling of
`DualPaneExplorer`, not a child), yet it derives its capability flags from `explorerState` directly: one
`caps = $derived(capabilitiesFor(getActiveTab(getTabMgr(getFocusedPane())).volumeId))`, then `canMkdir` / `canMkfile` =
`caps.canCreateChild`, `canRename` = `caps.canRenameInPlace`, `canSourceOps` = `caps.canBeSource` (invariant A6 —
capabilities, not a `volumeId === 'search-results'` string compare; `capabilitiesFor` resolves `fsType`/`category` from
the volume store, so the bar passes only the volumeId). This is the A9 pattern — a store getter inside a `$derived` is
reactive across the component boundary, so there's no `onFocusedVolumeChange` callback or `+page.svelte` mirror `$state`
in the chain. Per-pane read only (P1): touch the focused pane's manager, never both. `canSourceOps` is no longer a prop
(it was a dead-true `+page.svelte={true}` placeholder); a focused `network` pane now disables the source buttons too
(`canBeSource: false`), which only makes the bar honest — those ops already no-op'd deep down on a network pane.

**`FunctionKeyBar` dispatches `file.*` onto the bus.** Each button click calls a single
`onCommand?: (id: CommandId) => void` prop, wired in `+page.svelte` to `handleCommandExecute`. The button-to-command
mapping lives in a typed `fnKeyToCommand` map inside the component (F2/⇧F6 → `file.rename`, F3 → `file.view`, F4 →
`file.edit`, F5 → `file.copy`, F6 → `file.move`, ⇧F4 → `file.newFile`, F7 → `file.newFolder`, F8 → `file.delete`, ⇧F8 →
`file.deletePermanently`). The keys are held in a typed map (not inlined at the call site) so
`cmdr/no-raw-command-dispatch` stays satisfied. Routing F-clicks through the bus means they now get the dispatch
preamble (`log.info` + `record_breadcrumb` breadcrumb + the `blockedByCapabilities` guard) like every other entry path —
a deliberate telemetry gain, not a behavior change. The buttons' visible `disabled` flags (`canRename` / `canMkfile` /
`canMkdir` / `canSourceOps`) win first: a disabled button can't be clicked, so the dispatch capability guard never fires
for an F-click (the guard's blocked set — `file.rename` / `file.newFile` / `file.newFolder` — matches exactly the
buttons the flags disable on a snapshot pane).

**Selection-dialog keys dispatch onto the bus.** The `+` / `-` keypresses are classified by `selection-dialog-keys.ts`
and reach the bus through a typed `onCommand?: (commandId: CommandId) => void` prop chain: `FilePane` (the classifier at
`FilePane.svelte` emits `'selection.selectFiles'` / `'selection.deselectFiles'`) → `DualPaneExplorer` (same typed prop)
→ `+page.svelte`, wired to `handleCommandExecute`. The prop is `CommandId`-typed end to end, so
`cmdr/no-raw-command-dispatch` stays satisfied and a registry rename breaks compilation along the chain. See
`$lib/file-explorer/CLAUDE.md` § Selection for the dialog itself.

**Focused-pane reads for externals (`focused-pane-reads.ts`).** Consumers outside `DualPaneExplorer` that need the
focused pane's directory path, active-tab volume id, or "searchable folder" read them from the explorer store via
`getFocusedPanePath()` / `getFocusedPaneVolumeId()` / `getFocusedPaneSearchableFolder()` instead of through
`explorerRef` getters. Each is a live, reactive READ over store-owned tab state (the same
`getActiveTab(getTabMgr(getFocusedPane()))` chain `FunctionKeyBar` uses), per-pane only (P1), no snapshot. Today's
callers: the Go-to-path dialog, the Search dialog's `searchableFolder` prop, and `command-dispatch` (the
search-results-pane guard, copy-current-dir-path, Quick Look's volume gate). Anything that WRITES navigation
(`navigateToPath` / `moveCursor`) still threads the `ExplorerAPI` handle and reads the focused pane through it — those
write surfaces retire in a later phase, so the downloads helpers (`go-to-latest.ts`) stay fully on `explorerRef` for now
(their `getFocusedPane()` read only feeds the navigation write).

**Cross-pane drag.** `DualPaneExplorer.getFileAndPathUnderCursor()` prefers `FilePane.getPathUnderCursor()` over
`${currentPath}/${filename}` so snapshot-pane drags carry real filesystem paths, not `search-results://sr-N/<name>`.

**Self-drag identity (drop builds from app state, not the pasteboard).** `drag-drop-controller.svelte.ts::handleDrop`
consumes the self-drag identity recorded at drag start (`drag/drag-drop.ts::recordSelfDragIdentity`) instead of
resolving the pasteboard-derived paths, but only when `getIsDraggingFromSelf()` is true AND the recorded
`sourceVolumeId` is a registered backend-real volume (`consumableSelfDragIdentity`). This is what fixes the MTP/SMB
self-drag: a virtual volume's relative listing path (`/photos/sunset.jpg`) round-trips through wry's drop event looking
like a local absolute path, so the resolver would mis-resolve it to local and the dialog would read 0 bytes. The
recorded identity carries the truth (source volume id + the paths the volume knows). External drops and search-results
drags (virtual id, real absolute paths) fall through to `resolveSourceVolumeId`. `FilePane` threads its `volumeId` as a
prop into `FullList` / `BriefList` so the drag-start sites can stamp the source volume onto the recorded identity. Full
architecture in [`../drag/CLAUDE.md`](../drag/CLAUDE.md) § "Self-drag identity".

**Dialog state lifecycle.** `dialog-state.svelte.ts` exposes one factory per `DualPaneExplorer`. Handlers like
`handleTransferError(error, friendly?)` accept the typed `WriteOperationError` plus the optional `FriendlyError` from
the backend `write-error` event so the rendered dialog can prefer the backend copy. The factory pattern keeps the giant
component testable: pass deps in, get back a struct of state + handlers.

**Live disk space.** `FilePane` registers each pane independently with the backend space poller (`watchVolumeSpace`
keyed by pane ID). Two panes on the same volume have independent registrations; one navigating away doesn't unwatch the
other. See parent § "Live disk space".

**MCP surface.** `FilePane` mirrors `{ buffer, indicatorVisible, indicatorStale, lastMatchedName }` into the synced
`PaneState.typeToJump` whenever the buffer or indicator is live, so MCP-driven E2E can assert without DOM poking. See
`src-tauri/src/mcp/CLAUDE.md` § State stores.

**The `navigate()` transaction (`navigate.ts`).** Every coordinator-level pane navigation goes through one
`navigate(intent, deps)` entry. `DualPaneExplorer` builds the `NavigateDeps` (store getters/mutators + the FilePane
handle + `resolveVolume` + the persistence trigger + the side-keyed token map) and wraps `navigate()` as its `navigate`
export; the bus, the MCP adapter, the four external write-callers, and the FilePane render-prop shims all call it. It
sits ON TOP of the FilePane listing primitives (`navigateToPath` / `navigateToParent`); listing mechanics stay
pane-owned. The only callers of `setPaneVolumeId` / `setPanePath` / `setPaneHistory` are `navigate()`'s internal
`commit` plus the two orthogonal network-host pushes (`handleNetworkHostChange`, `mirrorNetworkStateToPane`, which carry
an SMB host onto the history entry — they're not pane-destination changes).

- **Intent arms.** `{ volumeId?, path }` is a volume switch (volumeId set) or in-place path nav (omitted);
  `{ history: 'back' | 'forward' | 'parent' }` walks the stack (`parent` delegates to `FilePane.navigateToParent`);
  `{ snapshot: id }` opens `search-results://<id>` through the volume-switch machinery. The pinned-tab fork (L7) lives
  in ONE place per arm: `commitPathFromListing` for the in-place landing, `commitVolumeSwitch` for the switch.
- **Per-arm optimism (P4).** The volume switch commits volumeId + path + history SYNCHRONOUSLY (truly optimistic). The
  in-place path nav does NOT commit on call — it drives the FilePane primitive, and the commit lands when the listing
  completes and `onPathChange` re-enters `commitPathFromListing`. Don't "upgrade" the in-place arm to an immediate
  commit (it'd change when the breadcrumb updates relative to the listing).
- **`settled` resolve point, per arm.** In-place + real-volume switch: resolves on `listing-complete` (the FilePane
  promise). Cross-volume snapshot exit: resolves when the volume-switch commit is DONE, BEFORE the new listing loads
  (callers that move the cursor after — `navigate-and-select`, `handleSearchNavigate` — bridge the gap via
  `moveCursor`'s internal `whenLoadSettles`). Network / no-volume-resolved / state-restore branches: resolve
  immediately.
- **`NavigateResult` (L12).** `{ status: 'started', settled }` or `{ status: 'refused', reason }`. The refusal `message`
  strings (on-network, MTP-mismatch, on-MTP-volume, pane-unavailable) are EXACT contract — the MCP adapter forwards them
  verbatim as the `mcp-response` error; `navigate.test.ts` + the handler suite pin them byte-for-byte.
- **Token model (the staleness mechanism).** A per-pane `txToken` (caller-owned `Map`) gates the cross-volume resolve
  bail; a single GLOBAL `correctionGen` (the old `volumeChangeGeneration`, shared by both panes) gates the background
  `determineNavigationPath` correction. A same-token self-re-entry (parent-nav / walk-up completion via `onPathChange`)
  is NOT dropped — only a fresh `navigate()` advances the token. The drop-foreign-listings policy (next note) is what
  drops a genuinely stale listing.

**Don't add `cd`-style heuristics in `commitPathFromListing`.** Stale `onPathChange` from a slow listing is dropped by
the drop-foreign-listings policy in `navigate.ts::commitPathFromListing` (`smb://` prefix for `network`,
`search-results://` prefix for snapshots, `isPathOnVolume` for everything else). Adding a new virtual-volume namespace?
Extend the explicit prefix branch. See parent § "Gotchas".

**Nav-state persistence fires from ONE subscriber (A5).** `persistence-subscriber.svelte.ts` is the single module that
writes pane navigation state to `app-status.json`. `DualPaneExplorer` creates it synchronously during init (L3, the
`initListingDiffSync` pattern). Its two per-pane reactive `$effect`s watch the store's active-tab nav-state (path /
volumeId / viewMode / sortBy / sortOrder) and a third watches `focusedPane`; each diffs against the last-persisted
snapshot and calls the already-debounced `saveAppStatus` with only the changed fields, plus `saveTabsForPane` for the
pane whose nav-state moved. There are NO scattered `saveAppStatus` / `saveTabsForPaneSide` trigger sites in the nav /
sort / view-mode / focus / swap / mirror paths — they all mutate the store and the subscriber reacts (subscribe, don't
poll). Grep "where does pane nav-state persist?" → this one module.

Two values can't be derived from a store snapshot, so they come in as explicit hooks on the subscriber (still the same
single module — A5 is per concern, not per call shape):

- **Layout split** (`leftPaneWidthPercent`): persisted drag-END only via `persistLayout(percent)`, called from the
  resize-end / reset handlers. A reactive effect would persist on every drag FRAME (`handlePaneResize` sets the width
  per frame); the 200 ms debounce would still leak intermediate widths on a slow drag.
- **Last-used-path** (the `volumeId → path` map): a DELTA, not a snapshot — on a volume switch the OLD path of the OLD
  volume is recorded, a value the store no longer holds by the time an effect could read it. `navigate()` owns that
  delta (it has the old value before the swap) and forwards it through its `persist` callback →
  `persistLastUsedPath(record)`.

**The A5 per-surface split — what the subscriber does NOT own:**

- **Tab-set STRUCTURE** (open / close / reorder / pin / reopen) persists from `tab-operations.ts` (`saveTabsForPane`).
  That's tab CRUD — a separate surface. The subscriber owns active-tab NAV-state + focus; `tab-operations` owns tab
  structure. Both write `app-status.json` tab keys through `savePaneTabs`, but a nav change and a tab-bar action are
  distinct triggers. The same split applies to the MCP `tab` tool's CRUD branches in `handleMcpTabAction` (close /
  close_others / set_pinned), which keep their own `saveTabsForPaneSide`.
- **The MCP backend mirror** (`syncTabsToBackend` / `updatePaneTabs` / `updateFocusedPane`, L8): the Rust state store
  for MCP, a different target and debounce (100 ms), NOT disk persistence. Untouched.
- **`showHiddenFiles`**: a SETTING, persisted via the settings store (`saveSettings`), not `app-status`. Stays in
  `toggleHiddenFiles` / the settings-change listener.

**The five edge-flow handlers fold onto `navigate()`.** `handleCancelLoading`, `handleMtpFatalError`,
`handleRetryUnreachable`, `handleOpenHome`, and `handleVolumeUnmount` are thin shims: they do their flow-specific async
orchestration (resolve the default volume, clear `tab.unreachable`, `requestVolumeRefresh`, re-anchor DOM focus) and
route the actual state change through `navigate({ source: 'fallback' | 'cancel' })`. They carry NO direct
`saveAppStatus` / `saveTabsForPaneSide` calls — the store mutation `navigate()`'s commit makes drives the persistence
subscriber. Two behaviors the fold preserves byte-for-byte:

- **History-push asymmetry.** MTP-fatal / retry / open-home push a history entry (`source: 'fallback'`, default
  `pushHistory`); the volume-unmount redirect does NOT (`pushHistory: false` ⇒ `commit` history `'none'`), so ejecting a
  volume can't inject a spurious Back target. The unmount handler redirects EACH affected pane independently (left and
  right), not just the focused one.
- **Per-source focus.** The `'fallback'` / `'cancel'` flows re-anchor DOM focus on the container (the cancel walk-up /
  network-restore branches call `containerElement?.focus()` where today's code does) but do NOT shift the focused pane —
  unlike a `'user'` / `'mcp'` volume select, which makes the navigated pane focused. `shiftsFocus(source)` in
  `navigate.ts` is the single source of that rule. The `'fallback'` source is also `terminal`: a fixed recovery target,
  so no old-path pre-save and no background `determineNavigationPath` correction.

## Gotchas

- **Parent offset.** When `hasParent`, frontend cursor index = backend index + 1. `toFrontendIndices` applies this; the
  type-to-jump match callback applies it manually. Forgetting it lands the cursor one row off on every match.
- **Selection's `SvelteSet` requires mutations, not reassignment.** `selectionState.selectedIndices.add(i)` works;
  `state.selectedIndices = new SvelteSet([i])` breaks reactivity. The header comment in `selection-state.svelte.ts` pins
  this.
- **Volume tint old-WebKit branch.** On Safari < 16.2 (macOS 12), `color-mix()` doesn't parse, so `volume-tint` reads
  live CSS vars via `getComputedStyle` and mixes in sRGB. A reactive `mediaTick` re-fires `$derived` callers when
  `prefers-color-scheme` / `prefers-contrast` flips; without it, dark-mode swaps wouldn't repaint the tint. The branch
  is picked once at module load via `hasColorMix` from `$lib/utils/webkit-compat.ts`.
- **`DualPaneExplorer.svelte` and `FilePane.svelte` are ~3000 lines each and flagged by `file-length`.** Don't add to
  them without extracting first. New cross-cutting state goes into a `*.svelte.ts` factory; new pure logic goes into a
  `*.ts` helper with a colocated test. The `dialog-state` / `rename-flow` / `type-to-jump-state` extractions are the
  pattern to follow.
