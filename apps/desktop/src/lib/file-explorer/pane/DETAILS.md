# Pane subsystem details

Pull-tier docs for `apps/desktop/src/lib/file-explorer/pane/`: architecture, flows, and decision rationale. Must-know
invariants and gotchas live in `CLAUDE.md`.

Per-pane orchestrator: cursor, scroll, focus, dual-pane coordination, tab state, selection state, type-to-jump, dialog
lifecycle, drag handling, volume tinting, and navigation primitives.

`DualPaneExplorer.svelte` is the root: it owns both panes, the unified key/command dispatch, the dialog manager, and the
MCP-exposed surface. `FilePane.svelte` is one pane: it owns its listing, cursor, selection, view mode, type-to-jump
buffer, rename flow, breadcrumb, and the alt-view rendering ({#if/elseif} between `MtpConnectionView`,
`NetworkMountView`, `SmbReconnectingView`, `SearchResultsView`, `ErrorPane`, `VolumeUnreachableBanner`, and the regular
list).

## File map

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md` § Module
map. What the mechanisms DO is in § Conventions and § Gotchas below: the focus contract, type-to-jump, the snapshot
pane, volume capabilities, `PaneAccess` and the command-body factories, the explorer store, `FunctionKeyBar`, self-drag
identity, dialog lifecycle, live disk space, the `navigate()` transaction and `Location`, the listing loader's token
model, nav-state persistence, the five edge flows, and the `..` parent offset. Only the layout facts that none of those
carry live here:

- **`FilePane.svelte`'s `.content` div is the single wrapper every view kind mounts inside** (Full, Brief, Network,
  search results, error, and SMB panes) and owns the pane's one background layer (`--color-pane-bg`). It does NOT own
  the side gutter: each list view owns that, because the column header has to keep spanning edge to edge while the rows
  inset. See `views/DETAILS.md`.
- **`listing-diff-sync.svelte.ts` runs the `directory-diff` handler at two rates.** Cursor/selection reconciliation
  fires IMMEDIATELY (it has to stay exact), while the visible-listing refetch (soft-refresh tick, `totalCount`, stats,
  brief column widths) is coalesced by a leading + trailing `createThrottle` at `INDEX_LISTING_UPDATE_MIN_INTERVAL_MS`
  (250 ms, ≤4/sec). Under heavy churn the backend `diff_emitter` only collapses to ~50 ms (~20/sec), and each
  unthrottled refetch re-renders the range into fresh WebKit compositor surfaces (1+ GB GPU under a storm), so the
  throttle is the demand-side cap. The index-SIZE refresh path (`index-dir-updated` → `refreshIndexSizes`) is a separate
  source, already leading-throttled at 2 s per pane in `index-events.ts`, which also resolves the well-known macOS
  `/private/` symlinks before matching paths.
- **`git-browser-sync.svelte.ts::cleanup()` has to drop the SETTING listeners too**, not just the repo subscription, or
  they leak per pane.
- **Two independent MCP mirrors, so a change to one doesn't cover the other**: `pane-mcp-sync.svelte.ts` mirrors pane
  state and deliberately skips network + search-results panes (`NetworkBrowser` owns the MCP push for the network view
  and would get clobbered; a snapshot is local dialog state, not a directory agents query), while
  `tab-mcp-sync.svelte.ts` debounce-mirrors each pane's tab structure via `updatePaneTabs`.
- **`debug-emitters.svelte.ts` is dev-only**: its `$effect`s no-op outside DEV and in tests.
- **`pane-background-dblclick.ts` is scoped to list views by construction.** `isFileListBackgroundClick` requires a
  `[role="listbox"]` ancestor and a non-`.file-entry` target, so error / network / search panes (no listbox) can never
  fire the double-click-to-parent gesture.

### The FilePane controller modules

`FilePane.svelte` keeps only what needs the component: the lifecycle `$state` slots many concerns read (`listingId` /
`loading` / `totalCount` / `error` / `cursorIndex` / …), the `FilePaneAPI` exports (Svelte instance exports can't live
anywhere else), the DOM/component refs, the factory wiring, and the template. Everything else is a sibling with its own
suite:

- `row-overlays.svelte.ts`: the cloud-sync, image-index file, and folder-coverage badge feeds (maps, fetchers, live
  setting gates, idle poll, enrich-driven refresh).
  - **The 3 s sync poll skips a folder holding no cloud files.** `unknown` is what a plain local file reports and it
    cannot become a live cloud status without the file moving, which re-lists and re-fetches anyway. Without the skip
    every pane re-asked the provider about every visible row every three seconds forever: measured at two batches of 267
    and 377 paths every 3 s on an idle prod session, each path costing two `stat`s plus a synchronous XPC round trip
    into `fileproviderd`. One cloud file keeps the whole folder polled, since its neighbours ride the same batch.
  - **Both image-index fetches are COALESCED (`createCoalesced`), one in flight per pane, newest request wins.** They're
    driven by things that arrive in storms: every visible-range render, every listing swap, every enrichment tick. The
    400 ms enrich debounce bounds how often work STARTS and nothing else, so once a call outlasted that window the calls
    simply stacked — a burst of watcher-driven refreshes during a large transfer reached hundreds of concurrent backend
    queries, took the whole blocking pool, and froze the panes and the volume picker until restart. `cleanup()` cancels,
    or a queued fetch fires for a destroyed pane. The sync fetch stays UNcoalesced on purpose: the backend already
    batches it and joins concurrent requests for overlapping paths, where it has more information than a pane does.
- `selection-info-feed.svelte.ts`: the entry under the cursor and the listing stats, with their debounce/throttle and
  the search-results snapshot mirror. `parent-entry.ts` builds the synthetic `..` row it and `entries-snapshot.ts`
  share.
- `pane-key-router.ts` / `pane-pointer.ts`: keyboard routing and mouse handling for a focused pane.
- `entry-activation.ts`: what opening an entry does (redirect, archive Enter policy, browse, viewer, OS default app).
- `breadcrumb-bar.ts`: the displayed path plus the segment-click, context-menu, and volume-switch handlers.
- `deleted-dir-poll.ts` / `mtp-disconnect-watch.svelte.ts`: the two "what I'm showing is gone" recoveries.
- `path-sync.ts` / `hidden-files-resync.ts`: the prop-driven reload truth table, and the cursor follow after the
  hidden-files toggle.
- `entries-snapshot.ts`: the Selection dialog's entry list and the operation's selected-names snapshot.
- `network-host-state.svelte.ts`: the open Network host and its queued auto-mount share.
- `rename-flow.svelte.ts`: the whole inline-rename flow (activation, save, the dialogs, the arrow-key chain). It lives
  here because it hangs off the pane, but everything it does is documented next to the rest of rename in
  `../rename/DETAILS.md`, whose `CLAUDE.md` you won't get autoloaded while editing this directory.

**Where a factory is CREATED matters when it owns `$effect`s.** Svelte runs effects in creation order, so a factory
whose effects interact with the component's own (`selection-info-feed`, which feeds the MCP push and the menu-context
effect) is created at the spot its effects used to occupy, not with the other factories at the top. Ones whose effects
touch only their own state (`row-overlays`, `network-host-state`) sit wherever reads them best.

**Deps are deferred closures**, so a factory can be created before the state it reads is declared. That's what lets the
listing loader (created near the top) reach `caps`, `hasParent`, and the feeds declared hundreds of lines below.

### Easy-navigation gestures (GitHub #33)

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

### The error screen's ways out (`ErrorPane.svelte`)

Every listing failure renders through the one `ErrorPane`, so its action row is where "the user is stuck" gets solved
for all ~60 error reasons at once. Only two reason-groups carry a CTA of their own (`Try again` for `retryHint`,
`Open System Settings` for `actionKind === 'open_privacy_settings'` on macOS); the rest have none, which is why the row
always renders at least one way out. All four buttons share ONE `.cta` row, above the Technical details disclosure.

- **`Try again` keys on `retryHint` ALONE.** That flag is the backend's "retrying might help" signal and is set across
  categories on purpose: `friendly_error/errno.rs`'s `serious` helper carries it (`diskReadProblem`,
  `unexpectedSystemResponse`, `deviceProblem`), as do `couldntReadUnknown` and `io_serious`, and the NeedsAction
  `emptyRootICloud` sets it alongside `OpenPrivacySettings` so the user can re-list after granting access. ❌ Don't
  re-add a `category === 'transient'` condition: it silently swallowed those six buttons, including the one
  `empty_root.rs`'s own doc comment promises.

- **`Go to home folder` always renders.** It calls the same `onOpenHome` prop the unreachable banner uses, so both land
  through `edgeFlow.handleOpenHome` (default volume + `~`, clearing `tab.unreachable`).
- **`Go back` renders only when `canGoBack`.** `DualPaneExplorer` derives it from the active tab's history
  (`canGoBack(getActiveTab(tabMgr).history)`). The gate is load-bearing, not cosmetic: history is per-tab and is NOT
  persisted across sessions, and `createHistory` seeds a single entry, so on a first-paint error (restored path fails to
  list, or a freshly opened tab) `nav.back` is a silent no-op (`navigate.ts` returns `SETTLED_NOOP` — no toast, no
  feedback). A button that visibly does nothing is worse than no button.
- **Both labels carry a live `ShortcutChip` in `commandId` mode**, so a rebind of `nav.back` / `nav.goHome` shows up
  immediately. `clickable={false}` because the chip sits inside a `<Button>` (a nested click target would
  double-activate).

**⌘D opens Technical details, and deliberately outranks any user binding on ⌘D.** `ErrorPane` registers a CAPTURE-phase
`document` keydown listener while mounted, which runs ahead of both the explorer container's `onkeydown` and the
document-level command dispatcher. `errorPane.toggleTechnicalDetails` is therefore a `fixedKey` command (registry +
`FIXED_KEY_COMMAND_IDS` + `DispatchExemptId`): rebinding it would be a no-op illusion, and releasing the key would
falsify the "Technical details ⌘D" hint the screen itself advertises. The listener is gated on `isFocused` so two
simultaneous error panes don't both toggle. Its `Main window/Error screen` scope is a SIBLING of `Main window/File list`
(an error screen renders instead of the file list), so the shadowing isn't reported as a conflict in Settings.

### Tests

Colocated with the code they pin (`codegraph_files` lists them; every alt-view component carries an `*.a11y.test.ts` axe
sweep). Three splits the layout doesn't explain for itself:

- **The drag-drop controller suite is split in two on purpose**: `drag-drop-controller.svelte.test.ts` (handler
  contracts, including the self-drag-identity scenarios) and `drag-drop-controller.listeners.svelte.test.ts` (Tauri
  listener registration + the enter→over→drop cycle), sharing volume constants and builders from
  `drag-drop-controller.test-fixtures.ts`. The `vi.mock` blocks stay DUPLICATED per file: vitest hoists them per module,
  so they can't move into the shared fixtures.
- **`volume-tint.svelte.fallback.test.ts` sits beside `volume-tint.svelte.test.ts`** because the two force opposite
  `hasColorMix` branches: the main file pins it `true` to assert the `color-mix(...)` string, the fallback file forces
  the JS sRGB-mix branch and asserts hex (stubbing `getComputedStyle`, since jsdom doesn't resolve CSS custom
  properties).
- **`integration-test-utils.ts` and `drag-drop-controller.test-fixtures.ts` are scaffolding, not suites** — they carry
  no tests of their own.

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
`routePanelKey`) apply the same widening — landmine L9, keep them identical. These two class-of-key matchers are the one
exception to the whole-combo rule (`cmdr/no-raw-key-match`, parent `src/CLAUDE.md`): they classify a key, they don't
test a combo.

**Open / parent keys are FilePane-local, not registry-dispatched.** `handleOpenOrParentKey` (in `FilePane`, above the
view-mode split so every view inherits it) handles Enter/`⌘↓` → open and Backspace/`⌘↑` → parent. The `⌘`-variants are
ALSO bound in the registry (`nav.open` / `nav.parent`) for Settings display and palette/MCP, so the local handler
`stopPropagation`s them — without that, the document-level dispatcher runs the command a SECOND time (`⌘↑` →
grandparent, `⌘↓` → double-open). `⌘Backspace` is deliberately excluded from the parent branch so it falls through to
`file.delete` (`⌘⌫` = move to trash, alongside `F8`).

**Selection moves the cursor on SELECT only.** `FilePane.applyIndices` reveals the first freshly selected row on an
`add` and leaves the cursor put on a `remove`: there's nothing new to reveal on a deselect, and yanking the cursor onto
a just-deselected row reads as wrong. The target comes from `firstSelectedIndex` (`first-selected-index.ts`), never
`idxs[0]`, because `selection.applyIndices` skips index 0 under `hasParent` (it never selects `..`) and the cursor must
land on the same first row it actually selected. Both sides apply the identical skip, so an `idxs` still carrying a
leading `0` can't park the cursor on the synthetic `..` row.

**Snapshot pane (`volumeId === 'search-results'`).** Two integration points that MUST stay coupled: `computeHasParent`
returns `false` (no `..` row, via the `hasParentRow` capability), and opening a real entry from the result rows leaves
the snapshot volume. `FilePane.handleNavigate` gates the latter on the `isSearchResultsView` capability (the
`caps.kind === 'search-results'` classifier, never a raw id compare), resolves the entry's `Location`
(`resolveLocationOrToast`, shared with the other nav edges), and bubbles it via the `onGoToLocation` callback →
`navigate({ to: { goTo } })`, whose switch arm changes volume (a different volume than `search-results`). An
unresolvable entry shows the shared friendly toast. Skipping the has-parent rule breaks selection (off-by-one); skipping
the resolve+switch poisons the pane with `volumeId === 'search-results'` + a real path. `onGoToLocation` (go to a
location) and `onVolumeChange` (deliberate volume-(re)select) are the two distinct intents — `Location` carries no
`volumePath`, so the location-only callback is the clean seam.

**Volume capabilities (`volume-capabilities.ts`).** Guard logic branches on a `VolumeCapabilities` record, ❌ never on a
volume-id string. The record has two halves, and which half answers is the whole design:

- **Rust answers "what can it do."** `Volume::capabilities()` publishes `backendCanWrite` + `canExport` per volume; they
  ride on `VolumeInfo.capabilities` and land on the record as `canWrite` / `canBeSource` via `withBackendCapabilities`.
  Canonical: `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Trait capability model".
- **This module classifies "what is it."** `volumeKindOf` picks a closed `VolumeKind` (`local` / `smb` / `mtp` /
  `network` / `search-results` / `archive`), which keys a frozen, by-reference table of per-kind defaults carrying the
  per-namespace UI structure Rust has nothing to say about (`hasBackendListing`, `hasParentRow`, `syncsToMcp`) plus the
  fallback write/source answers. It's NOT a `Record<string, boolean>` bag — `kind` is the discriminant.

- **❌ Never source KIND from the backend.** An OS-mounted SMB share that hasn't been upgraded to a direct smb2 session
  is served by `LocalPosixVolume`, so a backend-published kind would say `local` for a share that's plainly SMB to the
  user, flipping its tint, its view, and the search-indexing wording. Kind is about the storage; capability is about the
  backend.
- **The table is a FALLBACK, not a duplicate.** It stands where Rust has no volume to ask: the two virtual kinds (no
  `VolumeInfo` at all), `archive` (kind-from-path over the parent drive's volume — and `ArchiveVolume` itself declares
  `backend_can_write: false`, because zip editing is the app's managed archive-edit rewrite), a favorite id, and the
  window before a discovered volume's backend registers. Where the backend HAS answered, its answer wins.
- **Per-KIND vs per-VOLUME.** The other per-volume runtime flags (`mountIsReadOnly`, `supportsTrash`,
  `smbConnectionState`) stay on `VolumeInfo` and layer on top. `mountIsReadOnly` is a claim about the MOUNT and
  `capabilities.backendCanWrite` a claim about the BACKEND, so they're separate on purpose; both combinations occur (a
  writable backend on a read-only mount, a read-only backend on a writable disk), which is why the names say which is
  which. Only the transfer-destination guard reads `mountIsReadOnly` today.
- **One classifier, not two.** `volumeKindOf` is the SUPERSET of `volume-tint.svelte.ts::volumeKindFor`: it checks the
  two virtual ids first, then DELEGATES to `volumeKindFor` for the real kinds, overriding its `'other'` fall-through
  (favorites + real-but-unclassified) to a `'local'` default so the kind → table lookup is TOTAL (no input can miss the
  table; `capabilitiesFor` never returns `undefined`). The tint classifier keeps its own body and output, so tint stays
  byte-stable — this module never feeds its `'local'` default back into tinting.
- **`capabilitiesFor(volumeId)`** is the store-reading entry point: it resolves the `VolumeInfo` once, classifies from
  it, and folds in whatever the backend published. It returns the frozen row by reference when backend and default
  already agree (every ordinary volume), so the hot path stays allocation-free.
- **To add virtual volume #3:** add a `VolumeKind` member, a table row, and a `volumeKindOf` branch — no codebase sweep.
- **To add a real backend:** override `is_writable` in Rust and there's nothing to do on this side.

Consumers read the record directly: `SearchResultsView.svelte` reads `capabilitiesForKind('search-results')` (it always
renders a search-results pane), and every capability-GUARD consumer reads it via `capabilitiesFor`. There's no
Search-specific capabilities shim — `lib/search/capabilities.ts` keeps only the `SEARCH_RESULTS_NOT_A_FOLDER_TOAST`
string. The guards:

- **Dispatch** (`command-dispatch.ts::blockedByCapabilities`) + **F-bar** (`FunctionKeyBar.svelte`): paste, mkdir,
  mkfile, and rename all off `!canWrite`. One flag, because it's one question — Rust answers it with one
  `backendCanWrite`, and splitting it here would be the hand-maintained duplicate all over again.
- **Clipboard** (`clipboard-operations.ts`): the snapshot-clip path gate off `kind === 'search-results'`; the MTP
  copy/cut/paste refusals (the "Use F5/F6" toasts) off `caps.kind === 'mtp'` via `isMtpClipboardRefusal`. ❌ Don't
  generalize that MTP gate into a "no system clipboard" capability: `network` + `search-results` lack one too, and an
  MTP-worded toast on a reachable network paste would be a new, mis-worded toast. On the live clipboard-time pane id set
  it's byte-equivalent to the old `startsWith('mtp-')` gate, pinned by the equivalence test in
  `clipboard-operations.test.ts`.
- **Transfer / delete** (`file-operation-commands.ts`): source routing (snapshot builder) off `!hasBackendListing`. The
  destination guards (search-results dest-paste block off `!canWrite` scoped to the `search-results` kind so the toast
  wording stays correct; the `mountIsReadOnly` alert per-`VolumeInfo`) live in `transfer-entry.ts`'s
  `checkTransferDestinationGuard` so F5/F6, drag-and-drop, AND paste run the identical chain — see
  `file-operations/transfer/CLAUDE.md` § "One transfer entry seam". The `search-results://` URL parses stay (namespace
  mechanics).
- **`pane-commands.ts`**: `isSnapshotPane` (the Selection-dialog banner flag) off `!hasBackendListing`.
- **MCP sync** (`pane-mcp-sync.svelte.ts`): the network/search skip off `!syncsToMcp`. The deps interface carries a
  single `getSyncsToMcp()` accessor (FilePane supplies it from its derived caps).
- **`has-parent.ts`**: `computeHasParent` folds ONLY the snapshot rule via `hasParentRow`; the two PATH comparisons
  (`=== '/'`, `=== root`) stay.
- **FilePane alt-view chain** (`FilePane.svelte`): the kind-structural view selection resolves through a `paneViewKind`
  derived discriminant (`'network' | 'search-results' | 'mtp-connect' | 'normal'`) off `caps.kind` (+ the MTP
  device-only connection sub-state, which the table doesn't carry — it's a runtime connection state, not a kind). The
  `{#if}` chain branches on `paneViewKind` for the three alt-views (NetworkMountView / SearchResultsView /
  MtpConnectionView) and the SelectionInfo footer (`paneViewKind === 'normal'`). The RUNTIME-state branches
  (`unreachable`, SMB reconnecting / gave-up / needs-auth sign-in, the inline SMB upgrade login, `loading` /
  `friendlyError` / `error`) stay per-feature and gate IN FRONT of the descriptor, byte-identical precedence. This is a
  derived discriminant, NOT a new component. The per-feature gates (git lookup, type-to-jump keystroke, dir-exists poll)
  read `!caps.hasBackendListing` for the "is there a real directory" half; the MTP-path-specific checks
  (`isMtpVolumeId(volumeId)` for git-skip, `isMtpView` for the dir-poll, `isMtpDeviceOnly` for the jump) STAY — MTP has
  a backend listing but git can't run on it, there's no on-disk path to `pathExists`-poll, and the not-yet-connected
  sub-state isn't a kind capability. `caps` is derived once per pane
  (`caps = $derived(capabilitiesForPane(volumeId, currentPath))`); the named `isNetworkView` / `isSearchResultsView`
  deriveds re-source off `caps.kind`.

**The volume-id string compares that REMAIN are not guards — don't "finish the sweep".** A grep for
`=== 'search-results'` / `=== 'network'` / `startsWith('mtp-')` (and the `!==` forms) across `apps/desktop/src/` returns
hits, and every one is a classifier input, a namespace mechanic, or a display choice. Forcing one of those through the
capability record is the "differently complicated" failure mode to avoid:

- **Classifier internals (the inputs that FEED `volumeKindOf`).** `volume-capabilities.ts` (the two virtual-id checks),
  `volume-tint.svelte.ts::volumeKindFor` (`category === 'network' || fsType === 'smbfs'`), `volume-grouping.ts`
  (`category === 'network'` sidebar grouping), `mtp-path-utils.ts::isMtpVolumeId` (`startsWith('mtp-')`). These ARE the
  classifier — converting them would be circular.
- **Namespace / path mechanics (which string scheme, not what's allowed).** `navigate.ts` (the on-network / on-MTP
  refusal sources + the `smb://` / `search-results://` drop-foreign-listings prefix + `validateMtpNavigation` path
  parse), `DualPaneExplorer.svelte` (synthetic `smb://` path/name synthesis + the network-mirror /
  copy-path-between-panes identity branches), `rename-flow.svelte.ts` (skip the Unix-`access()` permission check on MTP
  virtual paths — a syscall-support mechanic, not a "may rename" capability).
- **Display / view selection.** `VolumeBreadcrumb.svelte` (the "Network" / "Search results" labels + the
  network-disabled gate), `FilePane.svelte` (`paneViewKind` in the `{#if}` chain, sourced off `caps.kind`; the
  `isNetworkView` / `isSearchResultsView` named deriveds; the MTP device-only sub-state + the `loadDirectory` skip for
  network/device-only panes), `MtpConnectionView.svelte` (device-only sub-state).
- **Persistence / init mechanics.** `app-status-store.ts` (skip filesystem path-resolution for the virtual `network`
  volume on persist), `initialization.ts` (trust the stored `network` id at startup, no `resolvePathVolume`).
- **Kind-scoped toast wording (reads the record, then picks words).** `command-dispatch.ts` +
  `file-operation-commands.ts` (`caps.kind === 'search-results'` decides the WORDING after the capability decides the
  block).
- **Tests + debug.** `navigate.test.ts` and the other `*.test.*` fixtures, `routes/debug/DebugHistoryPanel.svelte`.

**Command-body factories read through `PaneAccess`.** The MCP/palette command bodies live in factories
(`clipboard-operations`, `file-operation-commands`, `pane-commands`) that take a `PaneAccess` (live-reference read API)
plus the dialog state. The component keeps one-line `export function` delegates so the `ExplorerAPI` surface is
unchanged. Read-only / delegating bodies move; functions that WRITE component navigation state (`switchPane`,
`swapPanes`, `setViewMode`, `navigate`, `setSort*`, `moveCursor`, `selectVolumeBy*`, `copyPathBetweenPanes`, the
`mirror*`/`restoreFocus` helpers) stay in the component — un-trapping that state is the explorer-store phase, not this
factoring. The `navigate(intent)` transaction itself lives in `navigate.ts` (the component builds its `NavigateDeps` and
wraps it as the `navigate` export). The MTP capability check lives in `navigate.ts` (`validateMtpNavigation`, the
synchronous refusal gate for the in-place path arm); its refusal strings are byte-pinned by `navigate.test.ts`.
`moveCursorByName*` moved into `pane-commands` even though it's called from component-resident writers (`moveCursor`,
`restoreCursorByFilename`); those callers reach back via `paneCommands.*`.

**Explorer store (`explorer-state.svelte.ts`).** Module store owning the dual-pane navigation + UI-chrome state that
`DualPaneExplorer` used to trap in component closures: `focusedPane`, `leftPaneWidthPercent`, `railFocused`, and the two
tab-manager holders. State is module-private (A1): `createExplorerState()` closes over `$state` locals and exposes only
getters + one named mutator per field. There's no exported writable surface — callers can't assign a field, only call a
mutator (A2; the `cmdr/no-explorer-state-writes` lint rule makes this a hard wall — assigning to any property of the
store object outside `explorer-state.svelte.ts` is a lint error). `createExplorerState()` is factory-first for
testability; the module-level `explorerState` singleton is what the component binds, with `_resetForTesting()` for tests
that touch it.

The **writers** (A2 — exactly one mutator per field, all inside the store module):

- **`focusedPane`**: `setFocusedPane`
- **`leftPaneWidthPercent`**: `setLeftPaneWidthPercent`
- **`leftTabMgr`**: `setTabMgr('left', …)`
- **`rightTabMgr`**: `setTabMgr('right', …)`
- **`railFocused`**: `setRailFocused` (the Ask Cmdr rail's parallel focus flag — a third focus region alongside the
  `'left'|'right'` `focusedPane`, deliberately not folded into that union)

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
(`createTabManager`) and are mutated via the free functions in `tabs/tab-state-manager.svelte.ts` / `tab-operations`.
The store holds the holder reference and swaps it via `setTabMgr`; it never wraps tab-manager setters behind store
intents.

**Live-reference getters.** `getTabMgr(pane)` returns the live `$state<TabManager>` holder, never a copy or a
`$state.snapshot` — a `$derived` reading `getActiveTab(getTabMgr(p))` keeps tracking both when the holder is swapped and
when the held manager mutates in place. Returning a snapshot would silently sever reactivity at the seam (the same rule
`pane-access.ts` documents). What the store does NOT own: `cursorIndex`, selection, and listing UI state stay local to
`FilePane` (perf invariant P3).

**`FunctionKeyBar` reads the store, not props.** The F-key bar is mounted in `+page.svelte` (a sibling of
`DualPaneExplorer`, not a child), yet it derives its capability flags from `explorerState` directly: one
`caps = $derived(capabilitiesFor(getActiveTab(getTabMgr(getFocusedPane())).volumeId))`, then `canMkdir` / `canMkfile` /
`canRename` = `caps.canWrite` and `canSourceOps` = `caps.canBeSource` (capabilities, not a
`volumeId === 'search-results'` string compare; `capabilitiesFor` resolves the `VolumeInfo` from the volume store, so
the bar passes only the volumeId). A store getter inside a `$derived` is reactive across the component boundary, so
there's no `onFocusedVolumeChange` callback or `+page.svelte` mirror `$state` in the chain. Per-pane read only (P1):
touch the focused pane's manager, never both. `canSourceOps` is no longer a prop (it was a dead-true
`+page.svelte={true}` placeholder); a focused `network` pane now disables the source buttons too (`canBeSource: false`),
which only makes the bar honest — those ops already no-op'd deep down on a network pane.

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

**Keydown handlers read their keys from the command registry.** `FilePane.handleKeyDown` runs before the document-level
dispatcher (it's a descendant, and dispatch is registered in the bubble phase), so a loose local match silently shadows
or doubles a real shortcut. Every handler here therefore resolves through `eventMatchesCommand` / `comboMatchesCommand`
(`$lib/shortcuts`) instead of testing `e.key` plus a modifier flag:

- `selection-keys.ts` — the pure `classifySelectionKey`, mapping a keypress to `selection.toggle` / `toggleAndDown` /
  `selectAll` / `deselectAll`. Its arms all `stopPropagation()`, so each command runs exactly once (`⌘A` used to run
  twice, locally and centrally; invisible only because both did the same thing).
- `handleOpenOrParentKey` — `nav.open` (`Enter` / `⌘↓`) and `nav.parent` (`Backspace` / `⌘↑`). The `⌘Backspace`
  carve-out is now structural rather than a hand-written `!e.metaKey`: it's `file.delete`'s combo, not `nav.parent`'s,
  so it falls through to the dispatcher and deletes.
- `cursor-nav-keys.ts` — one `isCursorKey` gate in front of the per-view math, covering all ten cursor commands (the
  fixed six plus Home/End/PageUp/PageDown), with `allowShift` for the extend-selection gesture. It replaced a partial
  `⌘←`/`⌘→` bail, so no modifier superset moves the cursor any more.

`type-to-jump-keys.ts` and `selection-dialog-keys.ts` deliberately stay hand-rolled: they match a key CLASS (any
printable character; the physical Minus key) rather than a combo. Both already reject ⌘/⌃/⌥, which is the property that
matters. Full contract and the why: `$lib/shortcuts/DETAILS.md` § "Local handlers resolve through the registry too".

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

**Copy-path is the one arm that accepts `..`.** `pane-commands.ts::getPathToCopyUnderCursor()` returns the cursor
entry's path, or the pane's OWN directory when the cursor sits on the synthetic `..` row: copying a path from `..` in
`~/Downloads` yields `/Users/…/Downloads`, matching what you'd get by navigating up and pointing at that folder. It's a
separate function rather than a widening of `getFileAndPathUnderCursor()` because every other under-cursor arm (open,
rename, Get Info, Quick Look, tags, the cloud pair) must keep reading `..` as "no entry". Null when no row is under the
cursor at all, so the handler no-ops instead of copying a path nobody pointed at. Reading the pane path here is safe
from the `search-results://` hazard above: the `..` row only exists where `hasParent` holds, which a snapshot pane never
does.

**Self-drag identity (drop builds from app state, not the pasteboard).** `drag-drop-controller.svelte.ts::handleDrop`
consumes the self-drag identity recorded at drag start (`drag/drag-drop.ts::recordSelfDragIdentity`) instead of
resolving the pasteboard-derived paths, but only when `getIsDraggingFromSelf()` is true AND the recorded
`sourceVolumeId` is a registered backend-real volume (`consumableSelfDragIdentity`). This is what fixes the MTP
self-drag: a volume-relative listing path (`/photos/sunset.jpg`) round-trips through wry's drop event looking like a
local absolute path, so the resolver would mis-resolve it to local and the dialog would read 0 bytes. (Direct SMB isn't
in that class — its listing paths are absolute `/Volumes/…` mount paths — but it rides the same branch.) The recorded
identity carries the truth (source volume id + the paths the volume knows). External drops and search-results drags
(virtual id, real absolute paths) fall through to `resolveSourceVolumeId`. `FilePane` threads its `volumeId` as a prop
into `FullList` / `BriefList` so the drag-start sites can stamp the source volume onto the recorded identity. Full
architecture in `../drag/DETAILS.md` § "Self-drag identity".

**Dialog state lifecycle.** `dialog-state.svelte.ts` exposes one factory per `DualPaneExplorer`. Handlers like
`handleTransferError(error)` accept the typed `WriteOperationError` from the backend `write-error` event; the dialog
renders the copy on the FE from that typed error (`transfer-error-messages.ts`). The factory pattern keeps the giant
component testable: pass deps in, get back a struct of state + handlers.

That factory is a composition root over four siblings, and the split carries the safety argument of § "Birth context"
below rather than merely spreading lines:

- `dialog-props.ts`: the prop shape of every dialog plus `DialogStateDeps`. Types only, so the runtime modules can name
  each other's data without importing each other's behavior.
- `transfer-pane-effects.ts`: everything a settled transfer does to the panes, all of it reading birth context. The one
  module that can touch a pane, and the one an adopted view is built without.
- `adopted-operation.svelte.ts`: the progress dialog's adopted arm, owning that slot and its four outcomes.
- `archive-password-flow.svelte.ts`: the password prompt and its `transfer` / `browse` modes.
- `transfer-op-label.ts`: the log-line label for an operation type, shared by the two families.

`dialog-state.svelte.ts` keeps birth context, the confirmation / alert / error dialogs, and the cross-cutting queries
(`anyDialogOpen`, `isConfirmationDialogOpen`, `dismissAllAfterRenderFailure`, the MCP `confirmOpenDialog`).

`handleTransferConfirm` takes no scan flag: the progress dialog no longer waits for a `TransferDialog` preview, because
the backend registers the operation at confirm and its own task waits for the preview it claimed
(`apps/desktop/src-tauri/src/file_system/write_operations/DETAILS.md` § "The scan-wait"). What the handler MUST keep
threading is `previewId`, and the archive-password retry MUST keep clearing it: that retry is a new operation, a preview
accepts exactly one claimant, so a carried-over id would silently downgrade to a full re-walk.

### The Duplicate command

`duplicate-command.ts::duplicateInPlace` is the whole body of `file.duplicate` (⌘D, the palette, the right-click menu,
and the File menu). It copies the focused pane's selection, or the item under its cursor when nothing is selected, into
the folder that pane is already showing, and the backend gives each copy a free ` (N)` name.

- **It builds on the F5 builders**, handing `buildTransferPropsFromSelection` / `FromCursor` a `TransferContext` whose
  `sourcePath` and `destPath` are the same folder. That one substitution is the entire difference from F5.
- **It skips the confirmation dialog** and calls `dialogs.startTransferProgress` directly, the way paste does: there is
  no destination to pick and no conflict to answer, since a self-collision resolves before the conflict machinery is
  consulted (`src-tauri/src/file_system/write_operations/transfer/DETAILS.md` § "Self-collision (duplicating in
  place)").
- **It sets `direction` and `sourcePaneSide` to the FOCUSED pane**, not to the props the builders return: those name the
  other pane, which is right for a transfer across the panes and wrong for one that lands where it came from. The
  settled tail then refreshes the pane the copy appeared in and clears the selection it consumed.
- **`duplicateFollowUp: 'nothing'`** — no rename editor. `$lib/file-operations/transfer/DETAILS.md` § "Only paste and F5
  end a duplicate in the rename editor".
- **The shared destination guard runs against the pane's own folder**, so a read-only volume gets the same alert F5
  gives it. A search-results pane gets the same "not a folder" refusal, and the native context menu omits Duplicate
  there outright (`menu/menu_structure.rs`, `restrict_destination_actions`): a duplicate of snapshot rows would have to
  land in each row's own real folder, which one transfer can't express.

**Decision: no F-key bar button.** The bar's ten slots are full, and Duplicate is an F-key idiom in neither Finder nor
Total Commander. It's reachable by ⌘D, the palette, the right-click menu, and the File menu.

### Naming a duplicate

A transfer that duplicated ONE item in the folder it already lived in can end by opening the inline rename editor on the
copy. `handleTransferComplete` runs that tail last and unawaited, through `duplicate-rename.ts`, and only there: a
cancel and a failure reach their own handlers, so "a duplicate that didn't complete gets no editor" needs no check.

Three things hold it together:

- **The trigger decides, and has to say so.** `duplicateFollowUp` is a required field on the dispatch config, because
  every gesture that duplicates dispatches the same operation. Who answers what, and why paste and F5 differ from drag
  and the Duplicate command: `$lib/file-operations/transfer/DETAILS.md` § "Only paste and F5 end a duplicate in the
  rename editor".
- **The operation id is read while the progress dialog still owns it.** The dialog releases the foreground slot as it
  unmounts, a few lines further down `handleTransferComplete`, and the journal read needs that id. Same handover shape
  as `handleTransferError`'s failure id.
- **The journal read waits for `write-settled`, and reading it on the terminal event does not work.** The journal
  batches item rows and flushes the tail inside its finalize barrier, which runs after the handler emitted
  `write-complete`, so every single-item duplicate has an EMPTY journal page at complete time. The tail waits through
  `whenOperationSettled(id)` (`$lib/file-operations/settled-operations.ts`), which answers immediately for the common
  case where the settle already landed. `duplicate-rename.test.ts` pins the ordering so a move back to the terminal
  event fails loudly instead of silently never opening an editor.
- **The new name is read out of the operation journal, never recomputed.** No terminal event carries it:
  `WriteCompleteEvent` and `WriteSettledEvent` are counts and ids, and `WriteProgressEvent.currentFile` is a mid-flight
  filename with no promise of being the last. `getOperationLogDetail(id, 1, 0)` gives the resolved `destPath`, and the
  top-level name is its first segment below the destination — which is also what makes a duplicated FOLDER work, since
  its rows are the leaves inside it. An absent, skipped, or unreadable row means no editor, never an error and never a
  retry loop.

Activation itself is the `paste-clipboard-as-file` pattern: `moveCursorToNewFolder` arms the pending cursor name, then
`startRename({ suppressExtensionWarning: true, expectedName })` refuses to open on anything but the new item.
`rename/DETAILS.md` § "Programmatic activation".

### Birth context

The progress dialog can show an operation this window never started (Show, on a queue row:
`$lib/file-operations/queue/DETAILS.md` § Show). That splits what used to be one thing into three: **what the operation
did** belongs to its session, **what a pane should do about it** belongs to the view and is bound to the moment the
operation was born, and **what the dialog draws** is chrome either can supply.

**Two slots, and they live in separate MODULES on purpose.** `dialog-state.svelte.ts` owns `transferProgressProps`,
birth context: the paths, the pane side, the per-type counts, the dispatch input. `adopted-operation.svelte.ts` owns its
own `$state` for an operation this window is only watching: an id, a type, and two paths off the registry row. That
factory is handed a read-only `hasBirthContext()` and nothing else, so an adoption cannot overwrite a live birth context
— not by convention, but for want of a binding to write it with. That is the one hazard in this feature:
`handleTransferError`'s archive branch takes the progress dialog down while keeping `transferProgressProps` alive, and
the password submit re-dispatches from it when the user types the password. A guard that tested
`showTransferProgressDialog` would find "no dialog shown" and let an adoption land on those props, and the submit would
then copy the ADOPTED operation's sources to the ADOPTED operation's destination — a wrong write, out of a
correct-looking guard. With two slots the question doesn't arise, and the occupancy test is "either slot full, or any
dialog open", which also covers the invisible case.

**The archive-password flow can't aim the re-dispatch either.** `archive-password-flow.svelte.ts` owns the prompt and
both its modes but holds no reference to birth context: it asks `hasBirthContext()` and then calls
`redispatchBirthOperation()` or `settleBirthOperation()`, neither of which takes an argument. So "the retry re-runs the
operation the user unlocked" is a property of the wiring, not of the flow behaving itself. What the re-dispatch MUST
keep doing is clearing `previewId` (a preview accepts exactly one claimant, so a carried-over id silently downgrades the
retry to a full re-walk); that lives in `dialog-state.svelte.ts`'s `redispatchBirthOperation`.

**A refusal is the honest answer to an occupied slot**, and it is a toast in the main window rather than silence: the
listener focuses this window whatever the verdict, because a refusal behind the queue window reads as a dead button.
**Birth is the one thing that wins over adoption**: the three dispatch paths hand an adopted operation back to the queue
before showing their own, since the started dialog renders from the other slot. `DialogManager` renders the two arms as
ONE `{#if}` / `{:else if}` chain, which is what makes "two progress dialogs stacked over a user's transfer" unreachable
rather than a convention held here; keep it a chain. It resolves to the ADOPTED arm, though, so the handoff is still
what lets a newly started dialog appear at all. Pinned by `DialogManager.svelte.test.ts`.

**An adopted view's outcome handlers touch no pane.** `handleAdoptedComplete` / `-Cancelled` / `-Error` / `-Queue` are
separate callbacks in a separate module, not a flag on the started ones. Every pane effect a settled transfer runs lives
in `transfer-pane-effects.ts` — refresh, selection clear, snapshot drop, post-cancel re-select, all of them reading
birth context — and `createAdoptedOperation` is built WITHOUT it, so the pane work is not reachable from an adopted
outcome. It would have nothing to reach it with either: no `sourcePaneSide` to pick a pane, no `fileCount` /
`folderCount` to name what moved. The completion toast still reports the counts (those are facts about the OPERATION,
from the completion event) and falls back to the file count instead of the per-type split. A failure still opens the
error dialog with the same handover, because the reason is worth reading wherever the operation started.

**The axis is FRESH versus STALE context, not adopted versus started.** `handleArchivePasswordSubmit` starts a NEW
operation from context captured before the prompt went up and re-snapshots the source pane's selection against wherever
that pane is NOW; a plain transfer whose source pane navigated away mid-copy is the same shape. So
`clearSourcePaneAfterTransfer` and `adjustSelectionAfterCancel` ask `sourcePaneStillShowsBirthFolder()` first — the
pane's current folder against the one the operation was born in. Refreshing a listing is harmless whatever the answer
and still happens; changing a selection the user made somewhere else is not.

**❌ No dialog handler purges a search snapshot, in either family.** A dialog holds what the operation was ASKED to do,
and the purge needs what it DID; a snapshot also outlives every pane and dialog, in every window. So it is a
window-level subscription to the per-path `write-source-item-done` stream, `$lib/search/snapshot-purge.ts`, and an
adopted view's snapshots stay correct without the view knowing anything. Why that stream and not `directory-diff` or a
fatter completion event: `$lib/search/DETAILS.md` § "Cross-snapshot purge". Both families are pinned against reaching
for the store in `dialog-state.foreground.svelte.test.ts`.

**A dialog that throws during render must never wedge input.** Every dialog renders inside one `<svelte:boundary>` in
`DialogManager.svelte`. Opening a dialog sets its `show*` flag BEFORE anything renders, and `isConfirmationDialogOpen()`
suppresses the pane's keyboard while that flag is true, so a dialog that throws mid-render leaves the user with no keys
and nothing on screen to escape from. (Lived case: a doubly-mounted NAS put two volumes carrying one id into the
transfer dialog's destination `{#each}`; Svelte threw `each_key_duplicate` during the flush and F6 killed the keyboard.)
The boundary's `onerror` calls `dialogs.handleDialogRenderFailure(error)`, which logs through the app's error path,
toasts the user, and clears EVERY `show*` flag (`dismissAllAfterRenderFailure`, wider than `closeConfirmationDialog`)
before refocusing the pane. It then re-arms the boundary with `reset()` on a `setTimeout(0)`: without a reset the
boundary stays failed and no dialog opens again for the rest of the session, and the deferral lets the dismissal flush
first so the retry renders nothing rather than the same throw. A cap of three failures inside a five-second window stops
a re-render loop while still letting a later, unrelated failure recover. `setTimeout(0)`, ❌ never
`requestAnimationFrame`: macOS throttles it in unfocused windows (`docs/testing.md`). The `failed` snippet is
deliberately empty: by the time it renders there is no dialog left to show. Pinned by `DialogManager.svelte.test.ts`
(the boundary catches, through the real component) and `dialog-state.render-failure.svelte.test.ts` (the keyboard is
un-suppressed and every dialog is cleared).

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

**The walk-up fallback re-resolves the target's OWNING volume (`listing-loader.ts::navigateToFallback`).** All four
"what I'm showing is gone" recoveries funnel here — the `onListingError` deleted-path branch, `deleted-dir-poll.ts`, the
`onDirectoryDeleted` handler in `listing-diff-sync.svelte.ts`, and the two SMB cancel/disconnect handlers in
`smb-view-state.svelte.ts` — each after a `resolveValidPath` walk-up. That walk-up can climb OUT of the pane's volume,
so the target's owner decides where we land, via `resolvePathVolume(target)`; a differing owner routes through
`onVolumeChange` instead of a same-volume `loadDirectory`. `~` and `/` short-circuit to the root volume before the
resolve (they're the chain's last-resort rungs, and `~` is expanded backend-side so it isn't resolvable as written); an
unresolvable owner (dead mount, statfs timeout) lands in place, since the pane's own volume is then the honest guess.

_Decision / why:_ assuming the pane's volume still owns the fallback target strands the pane. An SMB share unmounts, its
volume id is unregistered, the walk-up climbs from `/Volumes/<share>/sub` out to `/Volumes` (owned by the ROOT volume),
and the listing goes out under the share's dead id → `Path not found: Volume not found`. It's PERMANENT, not transient:
the landed path exists, so the poll's miss counter resets and nothing retries. Note the walk-up gets there only because
`getVolumePath()` reports `/` for an unregistered volume (`DualPaneExplorer`'s `volumes.find(…)?.path ?? '/'`), which
also disarms the "volume root is gone, skip" guard in `deleted-dir-poll.ts`. That masking is deliberate cover, not a
second bug to fix: nothing else moves a pane off a vanished volume, so the poll is the only rescue, and walking up to a
live ancestor on the right volume is the outcome we want. Tighten the guard and the pane sits on a dead share
indefinitely instead.

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
- **Dotfile visibility**: the `listing.showHiddenFiles` SETTING, not pane state and not `app-status`. Both panes read
  the one reactive value (`getShowHiddenFiles()` from `$lib/settings/reactive-settings.svelte`), the settings store
  persists it, and `settings-applier.ts` mirrors it onto the View menu's CheckMenuItem.

### First-run pane layout

A brand-new install that already has Full Disk Access opens the left pane on `~` and the right pane on `~/Downloads`.
Every other launch restores whatever was persisted. The rule lives in `first-run-layout.ts` and fires from
`loadPersistedState` (`initialization.ts`), right after the persisted tabs load and BEFORE the `CMDR_E2E_START_PATH`
override, so a fixture path still wins. It edits the loaded `PersistedPaneTabs` in place, ahead of volume resolution, so
`~/Downloads` gets its volume resolved like any other path.

`decideFirstRunLayout(ctx)` is a pure function over four booleans and returns one of three outcomes. Its order of checks
is the whole design:

1. `isAutomatedRun` (from `isE2eRun()`) ⇒ `leaveAlone`. Nothing is written either, so a run leaves no trace.
2. `layoutAlreadyApplied` (the `firstRunLayoutApplied` key in `app-status.json`) ⇒ `leaveAlone`. Once, ever.
3. `hasPersistedPaneState` ⇒ `markAlreadyLaidOut`: record the marker, touch no panes.
4. `!hasFullDiskAccess` ⇒ `leaveAlone`. A never-answered prompt reads the same as a refusal.
5. Otherwise ⇒ `openHomeAndDownloads`.

The resolver returns an outcome whose `kind` is the decision and writes nothing itself; `loadPersistedState` performs
every write (see "What gets written, and in what order" below).

_Decision / why step 3 exists (the stakes, stated once; everything else points here):_ every user upgrading into the
build that introduced this rule has Full Disk Access and no marker, so steps 1, 2, and 4 all pass them straight through
to the layout. **An applied layout is persisted immediately, exactly as a navigation would be, so a layout applied over
somebody's real one silently BECOMES their layout, with nothing to undo it.** That is the one unrecoverable failure in
this feature, and step 3 is what prevents it by backfilling the marker for those installs instead.
`hasPersistedPaneState()` (`$lib/app-status-store`) asks whether any of the four pane KEYS is present (`leftTabs` /
`rightTabs` plus the pre-tabs scalars `leftPath` / `rightPath`), never what the tabs contain: a user who left an empty
tab list still has a layout of their own. Both sides are checked because nav-state persists per pane
(`persistence-subscriber.svelte.ts` runs one effect per side, and the first post-init run only seeds), so someone who
has only ever moved their right pane carries no left keys at all. An unreadable store answers `true` for the same
reason.

_Decision / why the marker rather than `onboarding.completed`:_ a fresh install on a Mac that already granted Cmdr Full
Disk Access gets `onboarding.completed` flipped to `true` during the same boot, by
`routes/(main)/startup-gates.ts::resolveOnboardingMount`. That is exactly the launch the layout is FOR, so gating on
that flag would switch the feature off in its main case.

Two guardrails that are easy to undo by accident:

- **`~/Downloads` is probed only after Full Disk Access is confirmed.** It sits behind a per-folder TCC gate
  (`crates/cmdr-fs/src/tcc_paths.rs`), so even stat'ing it without the permission can raise a system dialog the user has
  no context for. The pure decision function has no `~/Downloads` input at all, which makes the ordering structural
  rather than a comment. If the folder is missing, the right pane falls back to `~` and the marker is still recorded.
- **`isE2eRun()`, never `getAppMode() === 'e2e'`**, and it is only trustworthy because `DualPaneExplorer` renders behind
  `showApp`, which `routes/(main)/+page.svelte` sets after `await initAppMode()`. A capture build answers synchronously
  from its build define, a plain E2E run needs that resolved cache. Mount the explorer any earlier and the gate silently
  reads `dev`. See `$lib/app-mode.ts`.

**What gets written, and in what order.** `loadPersistedState` does it all in one block, after volume resolution (so the
stored `volumeId`s are the resolved ones) and before the `CMDR_E2E_START_PATH` override (so fixture paths can never
reach the store). On `openHomeAndDownloads`: both panes' tabs via `savePaneTabs`, then
`saveAppStatusNow({ leftPath, rightPath, firstRunLayoutApplied })`. On `markAlreadyLaidOut`: the marker alone.

- **The layout has to be persisted here, by this code.** The nav-state subscriber can't do it: it seeds its baseline
  from post-init state WITHOUT saving (`persistence-subscriber.svelte.ts`, "Seed the baseline on the first post-init
  run"), and an applied layout IS that state, so nothing would ever be written. It would then survive one session and
  vanish, with the marker guaranteeing the user never saw `~/Downloads` again.
- **Tabs before the marker.** A quit in between costs nothing: no marker means the rule simply runs again next launch.
  The reverse order loses the layout permanently.
- **`saveAppStatusNow`, not the debounced `saveAppStatus`, and awaited.** Startup is followed by plenty of things that
  can quit the app. Every persisted state's timing, and the `doSaveAppStatus` enumeration trap that a new `AppStatus`
  field has to dodge, live in `docs/architecture-patterns.md` § Persistence.

**The resolver is lazy, and that laziness has a precondition.** This sits between the app launching and the panes
appearing, so `resolveFirstRunLayout` seeds the two probed facts as placeholders and `settle` resolves one only when the
rule would decide differently either way. A returning user's launch does no I/O at all; an upgrading user's costs one
store read and no permission probe. The laziness lives in `settle`, ❌ never as a hand-written short-circuit ahead of
`decideFirstRunLayout`: that would repeat the guard order in a second place and drift the day the rule changes.

A skipped probe is only sound relative to the context as it stands, which still holds placeholders for facts settled
later. Two properties of the rule make it safe, and a change to `decideFirstRunLayout` must preserve both: each probed
fact is read by exactly ONE guard, and any skip is caused by an earlier guard that returns unconditionally. Break either
and a placeholder steers the answer: a guard reading `hasPersistedPaneState && hasFullDiskAccess` together would skip
the pane-state probe, leave it `false`, and lay out over a returning user's real layout. The pure function stays correct
throughout, so the 16-row matrix test can't see it. The
`matches the fully-probed decision for every combination of facts` test in `first-run-layout.test.ts` is what does; ❌
don't delete it when editing the rule.

Not covered by the automation gate: `scripts/marketing-shots.ts` leaves `CMDR_E2E_MODE` unset on purpose (it needs a
prod-looking title bar and the key-window shadow). Its data dir is persistent, so it carries pane state and takes the
backfill branch; only a wiped shots data dir would see the layout, on panes that would otherwise be `~` and `~` anyway.

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

### The operation-start gate

Starting a file operation while a dialog is up is refused, out loud. This is the canonical description; everything else
points here.

**The scope is commands that START an operation, ❌ never the ones that steer a running one.** Cancel, pause, resume,
rollback, queue, and answering a name clash all keep working while the progress dialog and the conflict prompt are up,
because that is exactly when a user needs them. Search's own "Show all in main window" is navigation, so it keeps
working too. Getting this boundary wrong would disable the queue controls, which is worse than the defect the gate
closes; don't let a later tidy-up widen it.

**Four layers, because four different actors can ask.** They are not redundant: three of them are reachable without
passing through any of the others.

- **The start itself** (`dialog-state.svelte.ts::startBirthOperation`) refuses when birth context is already alive, and
  returns an `OperationStartVerdict`. This one has to stand alone whatever the others do: the native menu is OS-side and
  MCP is a separate actor, and neither is gated on this window's modal state. Without it, a second start overwrote the
  running operation's props, so the mounted dialog re-rendered against an operation it had never dispatched and the user
  got nothing and heard nothing. An ADOPTED operation owns no slot (birth still wins over adoption); a password prompt
  DOES, and gets named as `archive-password` rather than `transfer-progress`, since telling an agent to close a dialog
  that isn't on screen strands it.
- **The command entry points** (`operation-start-gate.ts`, called from `file-operation-commands.ts`,
  `clipboard-operations.ts`, and `drag-drop-controller.svelte.ts`) refuse while any blocking dialog is open, so a
  confirmation never stacks over what the user is reading. In practice this catches the native menu, whose items stay
  clickable whatever is on screen, and the DROP: dragging files onto a pane starts an operation like any other actor
  does, and nothing about the mouse earns it an exemption. The drop's gate sits in `handleFileDrop`, after the drag
  lifecycle has been torn down and before any stat or volume-resolution work, so the refusal costs one toast and no
  syscalls. `triggerFileDrop` (the E2E entry) goes through the same function, so the gate is on that path too.
- **MCP** is refused in Rust before dispatching (`mcp/executor/mod.rs::refuse_while_dialog_blocks`), which is what turns
  a ten-second round-trip timeout into an immediate answer. The blocking dialog's id rides in the JSON-RPC error's
  `data.blockingDialog`, a TYPED field: an agent acts on it to decide what to close, so it's a contract, and the
  `no-error-string-match` rule applies to a sentence an agent parses just as it does to one our own code would. The
  conversational sentence stays alongside it for the human reading the transcript, and stops at "close it first": ❌
  don't restate the id or name the closing tool in prose. That's a second copy of what the typed field carries, and the
  two drift the moment either side is edited.
- **The native menu items grey out** (`routes/(main)/menu-operation-gate.svelte.ts` →
  `commands/menu.rs::set_file_operations_blocked`). ⚠️ CHROME ONLY. A disabled item's accelerator still fires, so this
  can never be the guard; it only stops the app offering what it would then turn down.

**Which dialogs block is DECLARED, not listed here.** Every `SOFT_DIALOG_REGISTRY` entry carries a required `whileOpen`
verdict (`$lib/ui/dialog-registry.ts`), so a new dialog fails to compile until its author answers the question. The
default is `BLOCKS_OPERATIONS`; `allowsOperations(reason)` is the opt-out and its reason is mandatory. Today exactly
three opt out, all for one reason: they're hosted outside the main window (`delete-ai-model` in Settings, the two viewer
copy sheets), so the main window has no modal up and no decision to lose. Every main-window dialog blocks, `about` and
`acknowledgements` included, because the window shows one modal at a time. **Search counts as a dialog** and blocks the
menu operations, which is a deliberate product call rather than a side effect.

**What's on screen comes from `$lib/ui/open-dialogs.svelte`**, which `ModalDialog` maintains from the same mount/destroy
pair that already tells the Rust `SoftDialogTracker`. Exhaustive by construction: rendering a soft dialog means
rendering a `ModalDialog` with a `SoftDialogId`. ❌ Don't replace it with a hand-written open/close pair — one missed
close would block every file operation for the rest of the session, which is why the pairing is left to Svelte.
`anyDialogOpen()` reads that set first; the local `show*` flags beside it are the same-tick guard between `show* = true`
and the mount that registers it, ❌ not a second inventory.

**Ask Cmdr is not a dialog**, so it never reaches the set. It blocks the menu items only while the composer has FOCUS
(`explorerState.getRailFocused()`), ❌ never while it's merely visible: the rail is docked next to the panes most of the
time, and blocking on visibility would take Copy away from anyone who leaves it open.

**Two Rust-side traps worth keeping.** `set_menu_context` re-applies the blocked state LAST, because its own loop
enables every explorer item — without that, a focus round-trip through Settings re-offers Copy while the dialog is still
up. And `register_known_dialogs` clears the backend's open list, since a reloaded webview never fires the close half of
its pairs and one orphaned entry would refuse every MCP file operation until restart.

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

## Archive browsing and editing (kind-from-path)

Pressing Enter on a `.zip` steps inside it like a folder, and a zip is WRITABLE (create/rename/delete inside, paste in,
move out). The design keeps the frontend simple: the tab keeps ONE `volumeId` — the parent drive — and `archive-<hash>`
volume ids never enter FE state, history, persistence, or MCP sync. Archive-ness is derived from the PATH; all I/O
routing happens backend-side in `VolumeManager::resolve(volume_id, path)`.

- **`pathInsideArchive(path)` + `capabilitiesForPane(volumeId, path)`** (`volume-capabilities.ts`) are the seam. The
  first is a pure, extension-only check mirroring the backend's `archive_format::format_for_name`; the second returns
  the `archive` capability row when the path is inside an archive, else defers to `capabilitiesFor(volumeId)`. The
  pane's `caps` uses it (`capabilitiesForPane(volumeId, currentPath)`), so `hasBackendListing` / `hasParentRow` /
  `syncsToMcp` / `canWrite` are all true for a zip; a tar or 7z boundary gets the read-only variant (`canWrite: false`,
  `canBeSource: true` so extract-out still works). ⌘C/⌘X are refused separately and route to F5/F6, since archive-inner
  paths aren't OS-resolvable URLs. ❌ The archive branch never folds in the PARENT drive's published capabilities: they
  answer for the drive, and the pane is inside a file on it.
- **Why `VolumeInfo.mountIsReadOnly` still matters**: the archive pane's `volumeId` is the parent drive. A writable zip
  runs the real managed archive-edit flow, but a zip that lives on a read-only `VolumeInfo` (a locked disk image) can't
  be rewritten in place — the write guards (`file-operation-commands.ts` `readOnlyRefusal`, `transfer-entry.ts`
  `checkTransferDestinationGuard`) still fall through to the per-volume `mountIsReadOnly` refusal for that case. The
  backend `ReadOnlyDevice` rejection is the safety net behind them.
- **Edits are managed ops, not instant.** A zip mutation is an O(archive) temp+rename rewrite, so mkdir/mkfile/rename
  inside a zip return an OPERATION handle, not a landed path, and copy/move into or out of a zip route through
  `copyBetweenVolumes`/`moveBetweenVolumes` (never the local `moveFiles` fast-path — `transfer-progress-state`'s
  `isVolumeMove` OR-s in `pathInsideArchive(sourcePaths | destinationPath)` so a same-drive archive move still crosses).
  The cursor lands on the new/renamed entry when the backing `.zip`'s live-watch refresh arrives (the durable
  `pendingCursorName` channel in `listing-diff-sync`, consumed on the refresh diff — no timer). `handleNewFileCreated`
  skips its open-in-editor for an archive target (the file is created async and an archive-inner path isn't editable in
  place). Deleting inside a zip is PERMANENT (no Trash inside an archive): `openDeleteDialog` forces
  `isPermanent`/`isArchive` and drops `supportsTrash`, and `DeleteDialog` shows the archive warning. The queue row for a
  zip edit is the `archive_edit` `WriteOperationType` (`file-archive` glyph, "Editing archive" label; no scan phase).
- **Navigation is nearly free.** `handleNavigate` forks on `entry.isDirectory || entry.isArchive` (a zip stays
  `isDirectory:false`; `isArchive` is backend-computed, extension-only, crosses IPC on `FileEntry`), routing in-place
  (same parent-drive volume) via `browseIntoEntry`. The Enter-behavior policy (below) runs FIRST and can divert to a
  popup or an external open before this browse arm. `navigateToParent` needs no archive branch: `parentOf('/a/foo.zip')`
  is `/a` (the containing dir), so walking up bubbles out of the archive by plain path arithmetic. The ONE
  reconciliation: `effectiveVolumeRoot` (feeds `computeHasParent`) uses `volumePath` (the parent mount) inside an
  archive, NOT the `.zip` path the backend emits as the listing's `volume_root` — otherwise the archive root would read
  as a volume root and hide its `..` row.
- **Opt-outs that `hasBackendListing:true` doesn't cover**: `git-browser-sync` skips inside archives
  (`pathInsideArchive` — a repo can't live in a zip); `volume-space` queries the parent mount path inside an archive (an
  archive-inner path isn't NSURL-resolvable, and the archive borrows the parent's space).
- **Path bar** renders the transparent `…/foo.zip/inner` for free: `breadcrumbDisplayPath` strips the parent
  `volumePath` prefix and `enrichBreadcrumbSegments` rebuilds ancestor targets from it, both path-agnostic.
- **Persistence/restore** is archive-safe with no FE change: the tab stores `(parentDriveId, fullPath)`; on restore
  `initialization.ts::resolveVolumeId` calls `resolvePathVolume(path)`, which the backend resolves to the parent drive
  for an archive-inner path (backend test `resolve_location_inside_an_archive_returns_the_parent_drive`). A deleted zip
  falls into the existing unreachable-path handling.

Full backend routing, the LRU lifecycle, and the viewer temp-extract: `crates/cmdr-archive/DETAILS.md`.

## Enter-behavior policy (archives and bundles)

Pressing Enter on an archive or a macOS app bundle (`.app`/`.bundle`/`.framework`) is a three-way choice: browse inside,
open in the default app, or ask. The decision is a pure function; the UI is a small popup.

- **`archive-enter-policy.ts` is the pure resolver**:
  `resolveEnterPolicy(entry, overrides) -> 'browse' | 'open' | 'ask' | null`. `null` means the entry is an ordinary
  file/folder (the caller does its normal open/browse). Zip archives default Ask (matched off `entry.isArchive`, so
  tar/7z join automatically when the backend flags them); bundles default Ask (matched by directory extension);
  Office/app packages (`.docx`/`.xlsx`/`.pptx`/`.jar`/`.apk`) default Open and aren't user-configurable yet (browse-into
  isn't supported for them). Per-format overrides come from the `behavior.archiveEnterBehavior` setting (a pinned-shape
  JSON object, parsed by `parseEnterBehaviorOverrides`).
- **`handleNavigate` consults it before the browse arm**, but only when NOT `pathInsideArchive(entry.path)` (a file
  inside an archive keeps the viewer interim). `ask` → `enterMenu.openFor`; `open` → `openEntryExternally` (`openFile`,
  i.e. LaunchServices — a `.zip` opens in the OS archive tool, a `.app` launches); `browse` → falls through to
  `browseIntoEntry`. On the search-results snapshot pane the popup is skipped (opening any real entry switches volume
  first via `goToRealEntry`).
- **`enter-menu.svelte.ts` (`createEnterMenu`) holds the popup state**; `enter-menu.ts` builds the items and computes
  the cursor-row anchor. The menu (`lib/ui/Menu`) is **portaled to `document.body`** so the explorer's `onfocusin` focus
  guard (`key-dispatch.ts`, which only exempts `[role="dialog"]`) doesn't yank focus off the `role="menu"`; on close the
  controller calls `restoreFocus` (`onRequestFocus`), which re-focuses the explorer container so keyboard routing
  resumes. `Configure…` deep-links to `openSettingsWindow(['Behavior', 'Archives'])`.
- **Settings** live in `settings/sections/ArchivesSection.svelte` (a custom section: two `ToggleGroup` cards over the
  one JSON setting, so the format list extends without a registry entry per format).
