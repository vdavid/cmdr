# Pane subsystem details

Pull-tier docs for `apps/desktop/src/lib/file-explorer/pane/`: architecture, flows, and decision rationale. Must-know
invariants and gotchas live in [CLAUDE.md](CLAUDE.md).

Per-pane orchestrator: cursor, scroll, focus, dual-pane coordination, tab state, selection state, type-to-jump, dialog
lifecycle, drag handling, volume tinting, and navigation primitives.

`DualPaneExplorer.svelte` is the root: it owns both panes, the unified key/command dispatch, the dialog manager, and the
MCP-exposed surface. `FilePane.svelte` is one pane: it owns its listing, cursor, selection, view mode, type-to-jump
buffer, rename flow, breadcrumb, and the alt-view rendering ({#if/elseif} between `MtpConnectionView`,
`NetworkMountView`, `SmbReconnectingView`, `SearchResultsView`, `ErrorPane`, `VolumeUnreachableBanner`, and the regular
list).

## File map

### Components

- **`DualPaneExplorer.svelte`**: Root: two panes + resizer + dialog manager + key/command dispatch + MCP wiring
- **`FilePane.svelte`**: One pane: listing, cursor, selection, view mode, breadcrumb, alt-view switching
- **`DialogManager.svelte`**: Renders every modal dialog (transfer, delete, rename, new-folder, alert, error)
- **`FunctionKeyBar.svelte`**: F1–F10 bar at the bottom of the window
- **`PaneResizer.svelte`**: Drag handle between the two panes
- **`ErrorPane.svelte`**: Friendly-error display for listing failures (see parent § "Error display")
- **`VolumeUnreachableBanner.svelte`**: Volume resolution timed out OR SMB give-up state; retry + open home / disconnect
- **`SmbReconnectingView.svelte`**: Spinner + progress bar while `smb-reconnect-manager` runs its backoff cycle
- **`SmbReauthView.svelte`**: Sign-in prompt when an SMB reconnect gave up on auth (`needs-auth`); wraps
  `NetworkLoginForm`
- **`MtpConnectionView.svelte`**: Placeholder pane for MTP connection states
- **`NetworkMountView.svelte`**: Network browser host/share list + login form
- **`SearchResultsView.svelte`**: Snapshot view for `volumeId === 'search-results'` (see parent § "Search-results")
- **`TypeToJumpIndicator.svelte`**: Bottom-right "Jump: …" chip

### Reactive state (`*.svelte.ts`)

- **`explorer-state.svelte.ts`**: Explorer store: `focusedPane`, `showHiddenFiles`, layout split, the two tab-mgr
  holders
- **`dialog-state.svelte.ts`**: Dialog props + handlers (transfer, delete, mkdir, alert, error); factory
- **`selection-state.svelte.ts`**: `SvelteSet<number>` of indices + range anchor/end + `applyIndices` helpers
- **`rename-flow.svelte.ts`**: Rename validation, conflict + extension dialogs, save / cancel
- **`type-to-jump-state.svelte.ts`**: Buffer + indicator + reset/hide timers + generation counter (race protection)
- **`type-to-jump-controller.svelte.ts`**: Wraps `type-to-jump-state` with the IPC fuzzy-match runner
  (generation-guarded)
  - the MCP last-matched-name mirror. FilePane keeps handleJumpKeystroke / isJumpActive / clearJumpState delegates
- **`git-browser-sync.svelte.ts`**: Breadcrumb repo-chip + git-status-column: the two setting mirrors, the lazy repo
  lookup/subscribe lifecycle, the path-change `$effect`, and `cleanup()` (drops the setting listeners too — the pre-
  extraction FilePane leaked them)
- **`smb-view-state.svelte.ts`**: SMB reconnect view deriveds (reconnecting / gave-up / needs-auth) + the reconnect-
  manager subscription `$effect` + the cancel/disconnect/connect-directly handlers. Takes `currentVolumeInfo` (shared
  with tint/eject) as a dep
- **`volume-space.svelte.ts`**: Live per-pane disk space: the reactive readout, the fetch (disk-image skip), the backend
  live-update listener, and watch/unwatch keyed by pane id. FilePane keeps a `refreshVolumeSpace` delegate
- **`volume-tint.svelte.ts`**: `color-mix(...)` or sRGB hex by volume kind; pure `volumeKindFor` classifier
- **`pane-mcp-sync.svelte.ts`**: Mirrors pane state into the MCP `PaneState` store; skips network/search panes
- **`persistence-subscriber.svelte.ts`**: The single nav-state persistence subscriber (A5): reactive `$effect`s →
  `app-status.json`
- **`listing-diff-sync.svelte.ts`**: File-watcher listeners + `reconcileCursorAndSelection` (pure, off-by-one core)
- **`drag-drop-controller.svelte.ts`**: Native drag band: drop-target state, drag handlers, auto-scroll loop, Tauri
  listeners
- **`tab-mcp-sync.svelte.ts`**: Debounced mirror of each pane's tab structure into the MCP backend store
  (`updatePaneTabs`) via a reactive `$effect`; sibling of `pane-mcp-sync` (which mirrors pane state, not the tab set)
- **`quick-look-follow.svelte.ts`**: Quick Look cursor-follow (debounced `quickLookSetPath`) + the error-state
  auto-close, two reactive `$effect`s + the debounce/generation state
- **`debug-emitters.svelte.ts`**: Dev-only reactive `$effect`s that mirror per-pane history + closed-tab stacks to the
  debug window (no-op outside DEV / in tests)

### Pure utilities (`*.ts`)

- **`types.ts`**: `FilePaneAPI`, `SwapState`, `ListViewAPI`, `*BrowserAPI`, `NetworkCursorEntry`
- **`pane-access.ts`**: `PaneAccess`: live-reference read API over pane nav + chrome state for factories
- **`focused-pane-reads.ts`**: Store-backed focused-pane reads (path / volume id / searchable folder) for externals
- **`clipboard-operations.ts`**: System-clipboard copy/cut/paste factory (MTP refusal, snapshot, cut-vs-copy)
- **`file-operation-commands.ts`**: Rename / new-folder / new-file / viewer / transfer / delete openers factory
- **`pane-commands.ts`**: MCP/palette read-only + delegating command bodies (selection, key-route, MTP val)
- **`sort-operations.ts`**: Sort orchestration factory: column-click cycle, order toggle, atomic MCP `setSort`,
  re-sort-both-panes hook (over the pure `sorting-handlers` helpers)
- **`swap-panes.ts`**: Full left/right pane swap (nav-state trade + listing adoption) + the `canSwapPanes` gate
- **`volume-selection.ts`**: MCP/palette volume selection by index / name, folding onto `navigate({ selectVolume })`
- **`edge-flow-handlers.ts`**: The five recovery nav edge-flows (cancel-loading, MTP-fatal, retry-unreachable,
  open-home, volume-unmount), each folding onto `navigate({ source: 'fallback' | 'cancel' })`
- **`pane-mirror.ts`**: "Copy path from → to pane" — mirror location + network state without shifting focus
- **`key-dispatch.ts`**: Container-level keyboard + focus routing (`handleKeyDown` / `handleKeyUp` / `handleFocusGuard`,
  the volume-chooser swallow, escape-during-loading, type-to-jump intercept)
- **`mcp-tab-action.ts`**: The MCP `tab` tool's per-pane dispatch (new/close/close_others/reopen/activate/set_pinned)
- **`type-to-jump-keys.ts`**: Pure `isTypeToJumpChar` / `isTypeToJumpResetKey` shared by both jump intercepts
- **`initialization.ts`**: Load persisted tabs + status + settings; resolve volumes; apply E2E overrides
- **`tab-operations.ts`**: Tab CRUD + context menu + persistence wired to `tabs/tab-state-manager`
- **`transfer-operations.ts`**: Build `TransferDialogPropsData` (and snapshot/dropped variants) from a focused pane
- **`transfer-entry.ts`**: Shared transfer entry seam: `checkTransferDestinationGuard` + `resolveSourceVolumeId`
- **`sorting-handlers.ts`**: `getNewSortOrder` (column click cycle), `toFrontendIndices` (`..` offset)
- **`index-events.ts`**: Throttled `index-dir-updated` handler with `/private/` symlink resolution
- **`listing-loader.ts`**: The streaming directory-load pipeline + the pane-local generation/listingId drop-foreign-
  listings token model (see § "The listing loader" below). `createListingLoader` owns `loadDirectory` /
  `handleListingComplete` / `resetLoadingState`, the six streaming listeners, the `pendingLoad` promise machinery,
  `navigateToFallback` / `navigateToParent` / `navigateToPath` / `handleCancelLoading` / `whenLoadSettles`, and the swap
  pair (`getSwapState` / `adoptListing`). FilePane keeps the lifecycle `$state` + thin FilePaneAPI delegates.
- **`listing-token.ts`**: `isEventForCurrentLoad(payloadListingId, captured, liveGeneration)` — the pure drop-foreign
  predicate every streaming listener's synchronous entry checks
- **`navigate.ts`**: `navigate(intent, deps)` transaction: the single coordinator-level pane-nav entry. `Location` is
  navigation's currency (`{ goTo }` self-routes by volume; `{ selectVolume }` always switches) — its module doc is the
  canonical home for the destination shapes and the four edge resolvers.
- **`has-parent.ts`**: `computeHasParent({ hasParentRow, currentPath, effectiveVolumeRoot })`
- **`first-selected-index.ts`**: `firstSelectedIndex(idxs, hasParent)` (post-select cursor-jump target, skips the `..`
  row)
- **`volume-capabilities.ts`**: `VolumeKind` + frozen per-kind `VolumeCapabilities` table + `volumeKindOf` /
  `capabilitiesFor`
- **`search-results-keys.ts`**: Pure key→action dispatch for the flat snapshot pane
- **`search-pane-keys.ts`**: The side-effect wiring over `search-results-keys` (view/edit-file, toggle, move + shift-
  extend); snapshot-pane semantics (`hasParent = false`)
- **`cursor-nav-keys.ts`**: Brief/Full list cursor movement (arrows, Page/Home/End, Shift-extend) over
  `handleNavigationShortcut`; `applyNavigation` also feeds `toggleSelectionAndMoveDownAtCursor`
- **`selection-dialog-keys.ts`**: Classify `+` / `-` keypresses → open Selection dialog (Total Commander parity)
- **`function-key-commands.ts`**: `fnKeyToCommand`: the F-key bar's 9 button → command-id map (typed; unit-tested)
- **`error-pane-utils.ts`**: Tiny helper for `ErrorPane`'s technical-details rendering
- **`pane-background-dblclick.ts`**: `isFileListBackgroundClick(target)`: true only for a double-click on the empty
  file-list background (inside a `[role="listbox"]`, not on a `.file-entry` row). Gates the double-click-to-parent
  gesture, scoped to list views (error / network / search panes have no listbox, so they never trigger it).
- **`integration-test-utils.ts`**: Shared test scaffolding for pane integration tests

#### Easy-navigation gestures (GitHub #33)

Two mouse conveniences, both routed through the normal pane navigation (so Back/Forward history and the error pipeline
come for free):

- **Clickable breadcrumb segments.** Each path piece in the breadcrumb is a button that navigates to that ancestor. The
  breadcrumb shows a DISPLAY path (volume prefix stripped, home collapsed to `~`); reconstructing the real target is the
  pure `navigation/breadcrumb-navigation.ts` (`enrichBreadcrumbSegments`), unit-tested. The current folder (last
  segment), the empty root marker, and search-results panes (whose "path" is a query label) are non-clickable.
- **Double-click the empty pane background → parent folder** (Directory Opus-style), gated by
  `behavior.doubleClickPaneNavigatesToParent` (default on). The `ondblclick` lives on the `.file-pane` root (which
  already carries `role="region"`, so no new a11y exposure); `isFileListBackgroundClick` filters to genuine list
  background. The first time it fires it raises a one-time INFO toast (`DoubleClickPaneHintToastContent`) and flips the
  hidden `behavior.doubleClickOnPaneNotificationSeen` so the hint shows once. "Never do this again" turns the gesture
  off from the toast.

### Tests

Colocated with the code they pin. Notable cross-cutting suites: `DualPaneExplorer.test.ts`,
`selection-consistency.test.ts` (selection survives diffs / cancel / source-item-done), `listing-diff-sync.test.ts`
(pure `reconcileCursorAndSelection` off-by-one coverage), `file-pane-keyboard.test.ts`, `volume-breadcrumb.test.ts`,
`volume-tint.svelte.test.ts` (+ `volume-tint.svelte.fallback.test.ts` for the old-WebKit branch), `*.a11y.test.ts` (axe
sweeps per alt-view component). The drag-drop controller suite is split in two: `drag-drop-controller.svelte.test.ts`
(handler contracts incl. the self-drag-identity scenarios) and `drag-drop-controller.listeners.svelte.test.ts` (Tauri
listener registration + the enter→over→drop cycle), sharing volume constants and builders from
`drag-drop-controller.test-fixtures.ts` (the `vi.mock` blocks stay duplicated per file — vitest hoists them per module).

The drag-drop controller owns native drag auto-scroll lifecycle because it sees every terminal drag path (`drop`,
`leave`, `cleanup`). `FilePane.autoScrollDuringDrag` forwards one animation-frame scroll request to the active list; the
list owns whether that means vertical Full-mode scroll or horizontal Brief-mode scroll.

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

**Active-jump key widening.** `isTypeToJumpChar` (letters/digits) STARTS a jump. Once one is active (`isJumpActive()` —
buffer non-empty, before the reset-timeout empties it), the intercept widens to `isPrintableJumpContinuation` (any
single printable key, Shift allowed), so `-`, Space, etc. extend the buffer instead of firing their own single-char
command (deselect, toggle-selection). After the reset timeout the buffer empties and a lone `-` is a command again. Both
the DOM intercept (`DualPaneExplorer.handleKeyDown`) and the Quick Look panel mirror (`pane-commands.ts`
`routePanelKey`) apply the same widening — landmine L9, keep them identical.

**Open / parent keys are FilePane-local, not registry-dispatched.** `handleOpenOrParentKey` (in `FilePane`, above the
view-mode split so every view inherits it) handles Enter/`⌘↓` → open and Backspace/`⌘↑` → parent. The `⌘`-variants are
ALSO bound in the registry (`nav.open` / `nav.parent`) for Settings display and palette/MCP, so the local handler
`stopPropagation`s them — without that, the document-level dispatcher runs the command a SECOND time (`⌘↑` →
grandparent, `⌘↓` → double-open). `⌘Backspace` is deliberately excluded from the parent branch so it falls through to
`file.delete` (`⌘⌫` = move to trash, alongside `F8`).

**Snapshot pane (`volumeId === 'search-results'`).** Two integration points that MUST stay coupled: `computeHasParent`
returns `false` (no `..` row, via the `hasParentRow` capability), and opening a real entry from the result rows leaves
the snapshot volume. `FilePane.handleNavigate` gates the latter on the `isSearchResultsView` capability (A6 — the
`caps.kind === 'search-results'` classifier, never a raw id compare), resolves the entry's `Location`
(`resolveLocationOrToast`, shared with the other nav edges), and bubbles it via the `onGoToLocation` callback →
`navigate({ to: { goTo } })`, whose switch arm changes volume (a different volume than `search-results`). An
unresolvable entry shows the shared friendly toast. Skipping the has-parent rule breaks selection (off-by-one); skipping
the resolve+switch poisons the pane with `volumeId === 'search-results'` + a real path. `onGoToLocation` (go to a
location) and `onVolumeChange` (deliberate volume-(re)select) are the two distinct intents — `Location` carries no
`volumePath`, so the location-only callback is the clean seam.

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
  (`=== '/'`, `=== root`) stay (L5).
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
  parse), `clipboard-operations.ts:76` (`pathScheme !== 'search-results'` — the snapshot-clip path resolver; reads the
  table but it's a scheme question), `DualPaneExplorer.svelte` (synthetic `smb://` path/name synthesis + the
  network-mirror / copy-path-between-panes identity branches), `rename-flow.svelte.ts:166` (skip the Unix-`access()`
  permission check on MTP virtual paths — a syscall-support mechanic, not a "may rename" capability).
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

- **`focusedPane`**: `setFocusedPane`
- **`showHiddenFiles`**: `setShowHiddenFiles`, `toggleHiddenFiles`
- **`leftPaneWidthPercent`**: `setLeftPaneWidthPercent`
- **`leftTabMgr`**: `setTabMgr('left', …)`
- **`rightTabMgr`**: `setTabMgr('right', …)`

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
mapping lives in a typed `fnKeyToCommand` map (F2/⇧F6 → `file.rename`, F3 → `file.view`, F4 → `file.edit`, F5 →
`file.copy`, F6 → `file.move`, ⇧F4 → `file.newFile`, F7 → `file.newFolder`, F8 → `file.delete`, ⇧F8 →
`file.deletePermanently`). The map is extracted to `function-key-commands.ts` so it's unit-testable
(`function-key-commands.test.ts` pins the 9 mappings); it's a typed constant (not inlined at the call site) so
`cmdr/no-raw-command-dispatch` stays satisfied.

**The F-bar chips read live effective shortcuts, not hardcoded F-keys.** Each visible button shows its command's
`getFirstShortcutReactive(id)` value, so rebinding `file.copy` to `⌘C` re-renders the F5 button's chip immediately — the
bar never lies about what the keys do. The `aria-label` interpolates the same dynamic combo ("Copy (F5)" → "Copy (⌘C)").
When a command has no binding the chip renders nothing (the button keeps its label and stays clickable; an empty `<kbd>`
would read as broken). The chips keep the bar's quiet local `<kbd>` styling rather than the boxed `ShortcutChip` pill —
a boxed pill repeated 8× fights the flat bar; truthfulness is the must, the chip look is the want. The Shift fork stays
**presentational and hardcoded** (which buttons appear on Shift never changes), but each shown button reads ITS
command's effective FIRST binding — so the Shift-revealed "Rename" button shows `file.rename`'s first binding (`F2`),
not `⇧F6`. Slightly odd next to its siblings, but truthful, which is the whole point. The four Shift placeholder slots
(F2/F3/F5/F7, no command) keep their static F-key labels. Layout survives an absurd custom binding: the buttons are
`flex: 1; min-width: 0` and the label truncates before the chip, so a long combo can't push the bar past the window.
Routing F-clicks through the bus means they now get the dispatch preamble (`log.info` + `record_breadcrumb` breadcrumb +
the `blockedByCapabilities` guard) like every other entry path — a deliberate telemetry gain, not a behavior change. The
buttons' visible `disabled` flags (`canRename` / `canMkfile` / `canMkdir` / `canSourceOps`) win first: a disabled button
can't be clicked, so the dispatch capability guard never fires for an F-click (the guard's blocked set — `file.rename` /
`file.newFile` / `file.newFolder` — matches exactly the buttons the flags disable on a snapshot pane).

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
`handleTransferError(error)` accept the typed `WriteOperationError` from the backend `write-error` event; the dialog
renders the copy on the FE from that typed error (`transfer-error-messages.ts`). The factory pattern keeps the giant
component testable: pass deps in, get back a struct of state + handlers.

**Live disk space.** `FilePane` registers each pane independently with the backend space poller (`watchVolumeSpace`
keyed by pane ID). Two panes on the same volume have independent registrations; one navigating away doesn't unwatch the
other. See parent § "Live disk space". **Disk images (`.dmg`) are excluded from the watch** (mount and volume-change
sites), and `onVolumeSpaceChanged` ignores them too: a disk image has no meaningful free space, so polling it would leak
its free/total figure into the bottom bar and `SelectionInfo`. Don't drop these guards when refactoring the
registration.

**MCP surface.** `FilePane` mirrors `{ buffer, indicatorVisible, indicatorStale, lastMatchedName }` into the synced
`PaneState.typeToJump` whenever the buffer or indicator is live, so MCP-driven E2E can assert without DOM poking. See
`src-tauri/src/mcp/DETAILS.md` § State stores.

**The `navigate()` transaction (`navigate.ts`).** Every coordinator-level pane navigation goes through one
`navigate(intent, deps)` entry. `DualPaneExplorer` builds the `NavigateDeps` (store getters/mutators + the FilePane
handle + the persistence trigger + the side-keyed token map) and wraps `navigate()` as its `navigate` export; the bus,
the MCP adapter, the four external write-callers, and the FilePane render-prop shims all call it. It sits ON TOP of the
FilePane listing primitives (`navigateToPath` / `navigateToParent`); listing mechanics stay pane-owned. The only callers
of `setPaneVolumeId` / `setPanePath` / `setPaneHistory` are `navigate()`'s internal `commit` plus the two orthogonal
network-host pushes (`handleNetworkHostChange`, `mirrorNetworkStateToPane`, which carry an SMB host onto the history
entry — they're not pane-destination changes).

- **`Location` is navigation's currency; resolution happens at the edge.** A bare path becomes a `Location`
  (`{ volumeId, path }`) at exactly four edges — ⌘G "Go to path", MCP `nav_to_path`, search-result activation (dialog
  "Go to file" + a search-results row), downloads reveal (⌘J) — each via `navigation/resolve-location.ts`, before
  `navigate()` is called. `navigate()` itself never resolves a volume; it receives a fully-formed destination. An
  unresolvable path is a friendly toast (shared `resolveLocationOrToast`) or a typed MCP `ok: false`, never a
  wrong-volume listing. The canonical description of the shapes + edges lives in `navigate.ts`'s module doc.
- **Intent arms.** `{ goTo }` self-routes: same volume as the pane → the in-place arm, a different volume → the switch
  arm. `{ selectVolume }` is the deliberate volume-(re)select intent and ALWAYS takes the switch arm (its callers —
  network-restore-on-cancel, retry, `selectVolumeByIndex` — pass the CURRENT volume id on purpose).
  `{ history: 'back' | 'forward' | 'parent' }` walks the stack (`parent` delegates to `FilePane.navigateToParent`);
  `{ snapshot: id }` opens `search-results://<id>` through the volume-switch machinery. The pinned-tab fork (L7) lives
  in ONE place per arm: `commitPathFromListing` for the in-place landing, `commitVolumeSwitch` for the switch.
- **Per-arm optimism (P4).** The switch arm commits volumeId + path + history SYNCHRONOUSLY (truly optimistic). The
  in-place arm does NOT commit on call — it drives the FilePane primitive, and the commit lands when the listing
  completes and `onPathChange` re-enters `commitPathFromListing`. Don't "upgrade" the in-place arm to an immediate
  commit (it'd change when the breadcrumb updates relative to the listing).
- **`settled` resolve point, per arm.** In-place arm: resolves on `listing-complete` (the FilePane promise). Switch arm:
  resolves immediately (the optimistic commit is synchronous; the listing loads afterward) — callers that move the
  cursor after (`navigate-and-select`, `revealSearchResultInPane`) bridge the gap via `moveCursor`'s internal
  `whenLoadSettles`. History / edge flows: match the primitive they drive.
- **`NavigateResult` (L12).** `{ status: 'started', settled }` or `{ status: 'refused', reason }`. The refusal `message`
  strings (on-network, MTP-mismatch, on-MTP-volume, pane-unavailable) are EXACT contract — the MCP adapter forwards them
  verbatim as the `mcp-response` error; `navigate.test.ts` + the handler suite pin them byte-for-byte.
- **Token model (the staleness mechanism).** A per-pane `txToken` (caller-owned `Map`) governs the same-token
  self-re-entry rule: a parent-nav / walk-up completion re-entering via `onPathChange` carries the SAME token and so
  commits (not dropped); only a fresh `navigate()` advances the token. A single GLOBAL `correctionGen` (the old
  `volumeChangeGeneration`, shared by both panes) gates the background `determineNavigationPath` correction. The
  drop-foreign-listings policy (next note) is what drops a genuinely stale listing.

**Don't add `cd`-style heuristics in `commitPathFromListing`.** Stale `onPathChange` from a slow listing is dropped by
the drop-foreign-listings policy in `navigate.ts::commitPathFromListing` (`smb://` prefix for `network`,
`search-results://` prefix for snapshots, `isPathOnVolume` for everything else). Adding a new virtual-volume namespace?
Extend the explicit prefix branch. See parent § "Gotchas".

**The listing loader (pane-local generation guard).** `listing-loader.ts::createListingLoader` owns the streaming
directory-load pipeline for one pane. Every `loadDirectory` captures its identity as `{ listingId, generation }` and
bumps a per-pane `loadGeneration` (its ONLY two bump sites are `loadDirectory` and `adoptListing`, both loader-private);
each of the six streaming listeners checks `isEventForCurrentLoad(payload.listingId, captured, loadGeneration)`
(`listing-token.ts`) at its SYNCHRONOUS entry. So once a newer load (or a pane swap's `adoptListing`) advances the
generation, the older load's still-registered listeners no-op — even before their `unlisten*` fires. This is the
pane-local drop-foreign-listings guard, DISTINCT from `navigate.ts`'s coordinator-level policy above (that one drops a
stale `onPathChange`; this one drops the stale listing's streaming events). Two async tails run UNGUARDED and MUST stay
that way (a faithful move, behavior-locked by `listing-loader.test.ts`): the `onListingError` `pathExistsChecked`
continuation and `handleListingComplete`'s post-`await findFileIndex` cursor write. Boundary: the pane's lifecycle
`$state` (listingId / loading / totalCount / error / openingFolder / … ) stays in `FilePane` — ~60 non-loader read sites
(selection, stats, menu, MCP sync, markup, five sub-factory dep getters) — and the loader reads/writes it through
injected accessors (the `type-to-jump-controller` idiom, not a state-owning `.svelte.ts` factory). `getSwapState` /
`adoptListing` share `loadGeneration`, so they live in the loader too. `cleanup()` (called from FilePane's `onDestroy`)
owns the full listing teardown (`cancelListing` + `listDirectoryEnd` + `evictPerPathIconsForDir` + the six `unlisten*`).

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

- **The focus guard must exempt dialog content.** `DualPaneExplorer.handleFocusGuard` refocuses the container on any
  non-input focusin inside the explorer, and the rename dialogs (`RenameConflictDialog`, `ExtensionChangeDialog`) mount
  INSIDE FilePane. Without the `[role="dialog"], [role="alertdialog"]` exemption, the guard yanks focus off the dialog
  overlay while `use:trapFocus` (see `lib/ui/DETAILS.md` § "Focus trapping") pulls it back — an endless focus ping-pong
  of microtasks that starves the event loop and freezes the webview. Pinned by the "rename to existing name is rejected
  on MTP" E2E. Focus containment inside a dialog is the trap's job; the guard only corrals pane chrome.
- **Parent offset.** When `hasParent`, frontend cursor index = backend index + 1. `toFrontendIndices` applies this; the
  type-to-jump match callback applies it manually. Forgetting it lands the cursor one row off on every match.
- **Selection's `SvelteSet` requires mutations, not reassignment.** `selectionState.selectedIndices.add(i)` works;
  `state.selectedIndices = new SvelteSet([i])` breaks reactivity. The header comment in `selection-state.svelte.ts` pins
  this.
- **Volume tint old-WebKit branch.** On Safari < 16.2 (macOS 12), `color-mix()` doesn't parse, so `volume-tint` reads
  live CSS vars via `getComputedStyle` and mixes in sRGB. A reactive `mediaTick` re-fires `$derived` callers when
  `prefers-color-scheme` / `prefers-contrast` flips; without it, dark-mode swaps wouldn't repaint the tint. The branch
  is picked once at module load via `hasColorMix` from `$lib/utils/webkit-compat.ts`.
- **`DualPaneExplorer.svelte` (~1450 lines) and `FilePane.svelte` (~2815) are flagged by `file-length`.** Don't add to
  them without extracting first. New cross-cutting state goes into a `*.svelte.ts` factory; new pure logic goes into a
  `*.ts` helper with a colocated test. `DualPaneExplorer` has been drained to mostly its `ExplorerAPI` delegate facade +
  factory wiring + markup: its command bodies and coordinator handlers live in the factories above (`sort-operations`,
  `edge-flow-handlers`, `pane-mirror`, `key-dispatch`, `mcp-tab-action`, `swap-panes`, `volume-selection`, …). The
  `dialog-state` / `rename-flow` / `type-to-jump-state` extractions are the pattern to follow.

  **Why not child components.** The seam that works here is **state-ownership vs command-logic**, not feature-carved
  child components. A `<DialogCoordinator>` child-component split was rejected as "a boundary without a real
  responsibility seam": dialogs read and write pane state heavily, and a child-component boundary severs that. Every
  closure/factory/module extraction instead landed and stuck (`dialog-state`, `tab-operations`, `initialization`,
  `index-events`, `listing-diff-sync`, `pane-mcp-sync`, and the explorer store). So when a "clean up the 3000-line
  component" pass tempts you, reach for a store/factory/helper, never a child component to shrink the line count.

## Archive browsing (kind-from-path)

Pressing Enter on a `.zip` steps inside it like a folder, read-only (mutation is a later milestone). The design keeps
the frontend simple: the tab keeps ONE `volumeId` — the parent drive — and `archive-<hash>` volume ids never enter FE
state, history, persistence, or MCP sync. Archive-ness is derived from the PATH; all I/O routing happens backend-side in
`VolumeManager::resolve(volume_id, path)`.

- **`pathInsideArchive(path)` + `capabilitiesForPane(volumeId, path)`** (`volume-capabilities.ts`) are the seam. The
  first is a pure, extension-only check mirroring the backend's `SUPPORTED_ARCHIVE_EXTENSIONS`; the second returns the
  read-only `archive` capability row when the path is inside an archive, else defers to `capabilitiesFor(volumeId)`. The
  pane's `caps` uses it (`capabilitiesForPane(volumeId, currentPath)`), so `hasBackendListing`/`hasParentRow`/`syncsToMcp`
  are true and the write flags (`canPasteInto`/`canCreateChild`/`canRenameInPlace`) are false.
- **Why `VolumeInfo.isReadOnly` is insufficient**: the archive pane's `volumeId` is the writable parent drive, which has
  no read-only `VolumeInfo`. The write guards (`file-operation-commands.ts` `readOnlyRefusal`, `transfer-entry.ts`
  `checkTransferDestinationGuard`'s `destPath` param, `clipboard-operations.ts` copy/cut/paste, `drag-drop-controller`'s
  drop dest) each resolve the archive kind from the PATH and refuse with archive-specific copy. The backend
  `ReadOnlyDevice` rejection is the safety net behind them.
- **Navigation is nearly free.** `handleNavigate` forks on `entry.isDirectory || entry.isArchive` (a zip stays
  `isDirectory:false`; `isArchive` is backend-computed, extension-only, crosses IPC on `FileEntry`), routing in-place
  (same parent-drive volume). `navigateToParent` needs no archive branch: `parentOf('/a/foo.zip')` is `/a` (the
  containing dir), so walking up bubbles out of the archive by plain path arithmetic. The ONE reconciliation:
  `effectiveVolumeRoot` (feeds `computeHasParent`) uses `volumePath` (the parent mount) inside an archive, NOT the
  `.zip` path the backend emits as the listing's `volume_root` — otherwise the archive root would read as a volume root
  and hide its `..` row.
- **Opt-outs that `hasBackendListing:true` doesn't cover**: `git-browser-sync` skips inside archives (`pathInsideArchive`
  — a repo can't live in a zip); `volume-space` queries the parent mount path inside an archive (an archive-inner path
  isn't NSURL-resolvable, and the archive borrows the parent's space).
- **Path bar** renders the transparent `…/foo.zip/inner` for free: `breadcrumbDisplayPath` strips the parent
  `volumePath` prefix and `enrichBreadcrumbSegments` rebuilds ancestor targets from it, both path-agnostic.
- **Persistence/restore** is archive-safe with no FE change: the tab stores `(parentDriveId, fullPath)`; on restore
  `initialization.ts::resolveVolumeId` calls `resolvePathVolume(path)`, which the backend resolves to the parent drive
  for an archive-inner path (backend test `resolve_location_inside_an_archive_returns_the_parent_drive`). A deleted zip
  falls into the existing unreachable-path handling.

Full backend routing, the LRU lifecycle, and the viewer temp-extract: `docs/specs/archive-browsing-m1b-derivation.md`
and `src-tauri/src/file_system/volume/backends/archive/DETAILS.md`.
