# Pane subsystem details

Pull-tier docs for `apps/desktop/src/lib/file-explorer/pane/`: architecture, flows, and decision rationale. Must-know
invariants and gotchas live in `CLAUDE.md`.

Per-pane orchestrator: cursor, scroll, focus, dual-pane coordination, tab state, selection state, type-to-jump, dialog
lifecycle, drag handling, volume tinting, and navigation primitives.

`DualPaneExplorer.svelte` is the root: it owns both panes, the unified key/command dispatch, the dialog manager, and the
MCP-exposed surface. `FilePane.svelte` is one pane: it owns its listing, cursor, selection, view mode, type-to-jump
buffer, rename flow, breadcrumb, and the alt-view rendering ({#if/elseif} between `MtpConnectionView`,
`NetworkMountView`, `RemoteConnectView`, `SearchResultsView`, `ErrorPane`, `VolumeUnreachableBanner`, and the regular
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
  fires IMMEDIATELY (it has to stay exact; it also follows `move`d rows by identity, see `../DETAILS.md` § Operation
  lifecycle), while the visible-listing refetch (soft-refresh tick, `totalCount`, stats, brief column widths) is
  coalesced by a leading + trailing `createThrottle` at `INDEX_LISTING_UPDATE_MIN_INTERVAL_MS` (250 ms, ≤4/sec). Under
  heavy churn the backend `diff_emitter` only collapses to ~50 ms (~20/sec), and each unthrottled refetch re-renders the
  range into fresh WebKit compositor surfaces (1+ GB GPU under a storm), so the throttle is the demand-side cap. The
  index-SIZE refresh path (`index-dir-updated` → `refreshIndexSizes`) is a separate source, already leading-throttled at
  2 s per pane in `index-events.ts`, which also resolves the well-known macOS `/private/` symlinks before matching
  paths.
- **`git-browser-sync.svelte.ts::cleanup()` has to drop the SETTING listeners too**, not just the repo subscription, or
  they leak per pane.
- **Two independent MCP mirrors, so a change to one doesn't cover the other**: `pane-mcp-sync.svelte.ts` mirrors pane
  state and deliberately skips network + search-results panes (`ServersHub` owns the MCP push for the network view and
  would get clobbered; a snapshot is local dialog state, not a directory agents query), while `tab-mcp-sync.svelte.ts`
  debounce-mirrors each pane's tab structure via `updatePaneTabs`.
- **The pane mirror fetches its visible range in ONE `getFileRange`**, capped at `MAX_MIRRORED_ROWS`. A row at a time
  was ~100 IPC round trips per sync, and the app stopped answering IPC on a big directory
  (`docs/notes/listing-row-fetch-quadratic-2026-08-22.md`). ⚠️ The gate is `syncsToMcp`, a pane-KIND capability, so this
  runs for every local pane whether or not MCP is enabled or an agent is attached — nothing here may assume an idle
  path.
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
- `snapshot-selection-sync.svelte.ts`: a search-results pane's cursor and selection remapped by PATH whenever its
  entries array is replaced. It exists because the selection is a set of INDICES and a snapshot pane has no listing to
  diff, so `listing-diff-sync` never runs for it: delete rows 2 and 3 of five and the selection still reads `{2, 3}`,
  now naming the fifth row and nothing, and the next F8 takes a file nobody picked. The rules match the diff path's — a
  surviving row keeps its selection at its new index, a vanished one leaves it, the cursor follows its own row or slides
  to whatever took its place. ❌ Not a widened `sourcePaneStillShowsBirthFolder` (below): that gate only sees deletes
  this pane started, and the array also shrinks when another window deletes the same file, when a move purges its
  sources, and when a result is trashed from a normal pane. It needs `FilePane`'s `searchSnapshot` to read the store's
  mutation tick, which also keeps `effectiveTotalCount` (Cmd+A, cursor clamping) honest after a purge.
- `path-sync.ts` / `hidden-files-resync.ts`: the prop-driven reload truth table, and the cursor follow after the
  hidden-files toggle.
- `entries-snapshot.ts`: the Selection dialog's entry list and the operation's selected-names snapshot. Both adapt a
  search snapshot's rows; the Selection list keeps the search engine's BASENAME in `name` (a mask like `*.txt` has to
  mean the filename), unlike `SearchResultsView`'s own adapter, which synthesizes the `~`-shortened full path for the
  Name column. `SearchResultEntry.parentPath` is home-relative too, so it is display text and never a path to join onto.
- `snapshot-source-volume.ts`: which real volume a search-results pane's rows live on, for the delete and transfer
  openers. ❌ Never assume `root` there — any volume with a persisted index is searchable, SMB and MTP included.
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
- **Right-click the function key bar → "Hide function key bar".** The bar builds its own one-item native menu
  (`show_function_key_bar_context_menu`), and the click comes back as the payload-less `FunctionKeyBarHideRequested`
  event, wired in `routes/(main)/listener-setup.ts` to `function-key-bar-hide.ts`: it turns off
  `appearance.showFunctionKeyBar` and raises an INFO toast whose inline link deep-links to that row
  (`['Appearance', 'Listing']` + `settingAnchorId`). Two details that look like tidy-ups and aren't: the toast runs 8 s
  rather than the 4 s default because it carries a link to read and click, and `button:disabled` in
  `FunctionKeyBar.svelte` sets `pointer-events: none` because the buttons tile the whole bar and WebKit dispatches no
  mouse events on a disabled control, so without it the menu would open on some pixels of the bar and not others. The
  bar's own handler deliberately doesn't `preventDefault()`; the document-level suppressor in `+page.svelte` already did
  (`routes/(main)/DETAILS.md` § Right-click ownership).

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

**Snapshot pane (`volumeId === 'search-results'`).** FIVE integration points that MUST stay coupled, and skipping one
gives an off-by-one selection, a stuck `search-results` path, a delete on rows nobody picked, an MCP delete refused by
stale pane state, or a folder re-sorted from a pane that isn't showing it:

1. `computeHasParent` returns `false` (no `..` row, via the `hasParentRow` capability).
2. Opening a real entry from the result rows leaves the snapshot volume (below).
3. `snapshot-selection-sync.svelte.ts` remaps the pane's index selection BY PATH whenever the snapshot changes — no
   listing diff does it, because the rows aren't a directory listing (§ File map).
4. The pane mirrors to MCP off the snapshot rather than off a backend listing (§ File map, `mcp-sync`).
5. Its column header sorts the SNAPSHOT in the store (`sort-operations.ts::snapshotPaneId` returns before any
   `setPaneSort` or `resortListing`), because every consumer resolves the index the user sees against
   `snapshot.entries[i]`. The header itself reads no capability (below).

`FilePane.handleNavigate` gates the second on the `isSearchResultsView` capability (the `caps.kind === 'search-results'`
classifier, never a raw id compare), resolves the entry's `Location` (`resolveLocationOrToast`, shared with the other
nav edges), and bubbles it via the `onGoToLocation` callback → `navigate({ to: { goTo } })`, whose switch arm changes
volume (a different volume than `search-results`). An unresolvable entry shows the shared friendly toast. Skip the
resolve+switch and the pane is poisoned with `volumeId === 'search-results'` + a real path. `onGoToLocation` (go to a
location) and `onVolumeChange` (deliberate volume-(re)select) are the two distinct intents — `Location` carries no
`volumePath`, so the location-only callback is the clean seam.

**Volume capabilities (`volume-capabilities.ts`).** Guard logic branches on a `VolumeCapabilities` record, ❌ never on a
volume-id string. The record has two halves, and which half answers is the whole design:

- **Rust answers "what can it do."** `Volume::capabilities()` publishes `backendCanWrite` + `canExport` per volume; they
  ride on `VolumeInfo.capabilities` and land on the record as `canWrite` / `canBeSource` via `withBackendCapabilities`.
  Canonical: `apps/desktop/src-tauri/src/file_system/volume/DETAILS.md` § "Trait capability model".
- **This module classifies "what is it."** `volumeKindOf` picks a closed `VolumeKind` (`local` / `smb` / `sftp` /
  `webdav` / `mtp` / `adb` / `network` / `search-results`), which keys a frozen, by-reference table of per-kind defaults
  carrying the per-namespace UI structure Rust has nothing to say about (`hasBackendListing`, `hasParentRow`,
  `syncsToMcp`) plus the fallback write/source answers. It's NOT a `Record<string, boolean>` bag — `kind` is the
  discriminant. The two ROUTED kinds (`archive`, `git-portal`) are in the same table but come from the PATH, resolved
  one layer up in `capabilitiesForPane`.

- **❌ Never source KIND from the backend.** An OS-mounted SMB share that hasn't been upgraded to a direct smb2 session
  is served by `LocalPosixVolume`, so a backend-published kind would say `local` for a share that's plainly SMB to the
  user, flipping its tint, its view, and the search-indexing wording. Kind is about the storage; capability is about the
  backend.
- **The table is a FALLBACK, not a duplicate.** It stands where Rust has no volume to ask: the two virtual kinds (no
  `VolumeInfo` at all), the two routed kinds (kind-from-path over the parent drive's volume, whose routed volume never
  enters FE state so it has no `VolumeInfo` either — and `ArchiveVolume` itself declares `backend_can_write: false`,
  because zip editing is the app's managed archive-edit rewrite), a favorite id, and the window before a discovered
  volume's backend registers. Where the backend HAS answered, its answer wins.
- **Per-KIND vs per-VOLUME.** The other per-volume runtime flags (`mountIsReadOnly`, `supportsTrash`, `connectionState`,
  `deviceReadiness`) stay on `VolumeInfo` and layer on top. `mountIsReadOnly` is a claim about the MOUNT and
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
- **`capabilitiesForPane(volumeId, path)`** is the layer every write guard uses: it resolves the two ROUTED kinds from
  the path first (archive by suffix, git portal by `isVirtualGitPath` gated on the live `showVirtualGitPortal` toggle),
  and otherwise defers to `capabilitiesFor`. ❌ Neither routed branch folds in the parent drive's published
  capabilities: those answer for the drive, and the pane is inside something ON it.
- **`rowIsOsVisible(volumeId, rowPath)`** answers ONE row's "is there a real file behind this", the gate behind the
  context menu's `Share` (see § "Sharing a row" below). ❌ It is NOT `capabilitiesForPane`: that one uses the WIDE
  archive check, so it would call a `.zip` FILE unshareable, and sharing a freshly-made archive is the point.
- **❗ Nothing switches exhaustively over `VolumeKind`.** Every consumer is a positive-list comparison, so a new member
  compiles clean everywhere and silently falls out of each list. The five to walk when you add one:
  `pane/clipboard-operations.ts` (the system-clipboard refusal — a missed kind puts an unusable scheme path on the OS
  clipboard), `volume-tint.svelte.ts::tintForKind` (falls through to `'none'`), `search/search-target-volume.ts` (a
  missed remote kind gets the LOCAL coverage voice), `open-terminal/terminal-target.ts::canOpenTerminalIn`, and
  `rowIsOsVisible` (a missed kind offers `Share` on a row with no file behind it).
- **To add virtual volume #3:** add a `VolumeKind` member, a table row, and a `volumeKindOf` branch, then walk those
  five.
- **To add a real backend:** override `is_writable` in Rust and there's nothing to do on this side.

Consumers read the record directly: `SearchResultsView.svelte` reads `capabilitiesForKind('search-results')` (it always
renders a search-results pane), and every capability-GUARD consumer reads it for a PANE via `capabilitiesForPane`.
There's no Search-specific capabilities shim — `lib/search/capabilities.ts` keeps only the
`SEARCH_RESULTS_NOT_A_FOLDER_TOAST` string. The guards:

- **Dispatch** (`command-dispatch.ts::blockedByCapabilities`) + **F-bar** (`FunctionKeyBar.svelte`): paste, mkdir,
  mkfile, and rename all off `!canWrite`. One flag, because it's one question — Rust answers it with one
  `backendCanWrite`, and splitting it here would be the hand-maintained duplicate all over again. Both read
  `capabilitiesForPane(volumeId, path)`: a routed pane's volume id is the writable parent drive, so the volume row would
  answer for the wrong thing. What differs between them is the ANSWER to a refusal, and deliberately: the F-bar disables
  the button, while dispatch blocks (with its toast) only on the `search-results` kind and leaves the routed kinds to
  `readOnlyRefusal`'s kind-worded alert, which is the last line for a shortcut bound outside the bar.
- **Clipboard** (`clipboard-operations.ts`): the snapshot-clip path gate off `kind === 'search-results'`; the routed
  copy-out refusals off `routedCopyOutHint`, which maps `kind === 'archive'` / `'git-portal'` to their own "use F5/F6"
  toast (a routed path isn't OS-resolvable, so the system clipboard can't carry it); the MTP copy/cut/paste refusals
  (the "Use F5/F6" toasts) off `caps.kind === 'mtp'` via `isMtpClipboardRefusal`. ❌ Don't generalize that MTP gate into
  a "no system clipboard" capability: `network` + `search-results` lack one too, and an MTP-worded toast on a reachable
  network paste would be a new, mis-worded toast. On the live clipboard-time pane id set it's byte-equivalent to the old
  `startsWith('mtp-')` gate, pinned by the equivalence test in `clipboard-operations.test.ts`.

  **The snapshot-clip branch gates TWICE, in this order** (`snapshotClipboardIsRefused`). First the PATH SCHEME:
  `$lib/path/canonical.ts::isPlainFilesystemPath` refuses any row that isn't a plain absolute filesystem path, from the
  path alone. Then the resolved row VOLUME (`snapshot-source-volume.ts::resolveSnapshotSourceVolume` →
  `isMtpClipboardRefusal`), because the pane's own volume id is the virtual `search-results` and a search covers any
  volume with a persisted index, MTP storages included. Either gate refuses the WHOLE set if any row offends; a partial
  copy under a toast claiming success is worse than a refusal.

  **Why the scheme gate leads:** the volume gate is only as good as the resolution, and a device unplugged while its
  snapshot pane stays open drops off the volume list, so `resolveSnapshotSourceVolume` answers the `root` fallback — a
  kind that copies — while the rows still read `mtp://…`. Such a path reaches `NSURL::fileURLWithPath`
  (`clipboard/pasteboard.rs`), which reads an unknown scheme as a RELATIVE path and hands back a file URL under the
  process working directory. The scheme gate holds with no volume registered at all, which is the case it exists for.

- **Transfer / delete** (`file-operation-commands.ts`): source routing (snapshot builder) off `!hasBackendListing`.
  `readOnlyRefusal` turns rename / mkdir / mkfile / delete away up front on a read-only routed pane, worded per kind
  (`fileExplorer.readOnly.archive*` for tar/7z, `fileExplorer.readOnly.gitPortal*` for a snapshot). The destination
  guards (search-results dest-paste block off `!canWrite` scoped to the `search-results` kind so the toast wording stays
  correct; the same two routed alerts; the `mountIsReadOnly` alert per-`VolumeInfo`) live in `transfer-entry.ts`'s
  `checkTransferDestinationGuard` so F5/F6, drag-and-drop, AND paste run the identical chain — see
  `file-operations/transfer/CLAUDE.md` § "One transfer entry seam". The `search-results://` URL parses stay (namespace
  mechanics).
- **`pane-commands.ts`**: `isSnapshotPane` (the Selection-dialog banner flag) off `!hasBackendListing`.
- **The column header** (`views/FullList.svelte` → `FullListHeader` → `SortableHeader`) reads no capability at all:
  every pane that renders a file list sorts one. What varies is `sortBy: SortColumn | null`, where `null` means the rows
  are in no column's order (the snapshot pane's ranked state): every header stays clickable, none is active, no caret
  draws, and no column claims the caret allowance in the measured tracks (`measure-column-widths::chromeFor`). Where a
  snapshot pane's click goes and why: `../../search/DETAILS.md` § "The snapshot pane's row order".
- **MCP sync** (`pane-mcp-sync.svelte.ts`): the network skip off `!syncsToMcp`. The deps interface carries a single
  `getSyncsToMcp()` accessor (FilePane supplies it from its derived caps). Only `network` is false, because `ServersHub`
  owns that pane's push. A search-results pane DOES mirror even with no backend listing: its rows come off the frontend
  snapshot through `snapshot-mcp-rows.ts` (basename in `name`, absolute path in `path`, no recursive fields), its
  `totalFiles` is the snapshot's own count, and `hasParentRow: false` tells the backend gate that one counted row is one
  real file. ❌ Don't turn that push back off: MCP's copy/move/delete pre-check reasons on this store, so a pane that
  pushes nothing leaves it describing whatever directory the pane came FROM, and an old cursor parked on that
  directory's `..` refused a delete over rows the user could see. Gate: `src-tauri/src/mcp/executor/DETAILS.md` §
  "Empty-operation fast-fail".
- **The mirrored rows carry the pane's OWN "is this size still moving" answer**, as `recursiveSizeUpdating`.
  `inFluxAnswerFor(volumeId)` composes it exactly as `FullList` composes its hourglass
  (`aggregating || under a walk || this dir's own pending writes`), resolved once per push and asked per row, so a row
  marked `[size-unsettled]` in `cmdr://state` is exactly a row wearing the hourglass on screen. ❌ Don't mirror the raw
  `recursiveSizePending` field instead: that's one of the three terms, and it calls a folder settled through the whole
  walk that's rewriting it, which is when its number is furthest from the truth (a folder read `≥422 GB` on its way down
  to 56 KB). Read outside a reactive context on purpose — a push is triggered, not subscribed, and the index storm that
  moves these numbers fires the `index-dir-updated` ticks that re-push. The rendering side:
  `src-tauri/src/mcp/resources/DETAILS.md` § "Directory sizes say how much they're worth".
- **`has-parent.ts`**: `computeHasParent` folds ONLY the snapshot rule via `hasParentRow`; the two PATH comparisons
  (`=== '/'`, `=== root`) stay.
- **FilePane alt-view chain** (`FilePane.svelte`): the kind-structural view selection resolves through a `paneViewKind`
  derived discriminant (`'network' | 'search-results' | 'mtp-connect' | 'normal'`) off `caps.kind` (+ the MTP
  device-only connection sub-state, which the table doesn't carry — it's a runtime connection state, not a kind). The
  `{#if}` chain branches on `paneViewKind` for the three alt-views (NetworkMountView / SearchResultsView /
  MtpConnectionView) and the SelectionInfo footer (`paneViewKind === 'normal'`). The RUNTIME-state branches
  (`unreachable`, the SAVED-place dial, the reconnect cycle's `RemoteConnectState`, the gave-up banner, `loading` /
  `friendlyError` / `error`) stay per-feature and gate IN FRONT of the descriptor, byte-identical precedence. This is a
  derived discriminant, NOT a new component. The per-feature gates (git lookup, type-to-jump keystroke, dir-exists poll)
  read `!caps.hasBackendListing` for the "is there a real directory" half; the MTP-path-specific checks
  (`isMtpVolumeId(volumeId)` for git-skip, `isMtpView` for the dir-poll, `isMtpDeviceOnly` for the jump) STAY — MTP has
  a backend listing but git can't run on it, there's no on-disk path to `pathExists`-poll, and the not-yet-connected
  sub-state isn't a kind capability. `caps` is derived once per pane
  (`caps = $derived(capabilitiesForPane(volumeId, currentPath))`); the named `isNetworkView` / `isSearchResultsView`
  deriveds re-source off `caps.kind`.

### A pane on a saved place

`place-connect.svelte.ts` is the pane's half of "opening a place that isn't live brings it to life in the pane, with a
cancel". Its `$effect` fires when the pane's `VolumeInfo` carries `connectionState === 'saved'` — a real volume id with
no session behind it, so every listing on it would refuse until something dials.

- **One dial per landing.** The effect re-runs on every `volumes-changed` refresh, so a `dialed` guard keyed on the
  volume id is what stops a dial per refresh against a server the user opened once.
- **The attempt id is held outside `$state`**: nothing renders it, and it exists so Cancel has something to aim at
  before the dial returns (`$lib/servers/CLAUDE.md`).
- **`connected` reloads the pane** rather than waiting for the row to flip to `direct` on the next broadcast, which is
  what makes the place feel like it opened rather than waited. `cancelled` clears the view and says nothing.
  `reconnecting` KEEPS the spinner: the backoff loop owns it, and `smb-view-state`'s own `RemoteConnectState` takes over
  once the row reaches `disconnected`.
- The listing still runs underneath and fails, setting `friendlyError`. That branch sits BEHIND this one in the chain,
  so it never shows, and the reload on connect clears it.

### A pane on a phone

`device-connect.svelte.ts` is the device twin of the section above: same seam, same typed `RemoteConnectState`, same
one-per-landing rule. Its gate is `deviceReadiness` rather than `connectionState`, because a phone waiting for its
"Allow USB debugging?" tap has no session to be in any state about (`$lib/adb/DETAILS.md` § "Two fields, two
questions").

- **A `waiting_for_authorization` row is NOT dialed.** The answer is known in advance (`unauthorized`), so the factory
  renders the waiting state and spends nothing. The next `volumes-changed` carrying `ready` re-runs the effect and the
  dial starts with nothing pressed.
- **The auto-proceed is a READ, not a listener.** `getCurrentVolumeInfo()` is the pane's own lookup into the volume
  store, and the store is what subscribes to `volumes-changed`. One subscription for the whole app, ❌ not one per pane.
- **The effect keys on `<volume>:<readiness>`**, so one landing is one dial AND a readiness change (the Allow tap) is a
  fresh decision. A plain volume-id guard would strand the pane in the waiting state forever.
- **❗ It HOLDS the pane's listing** (`holdsListing`). Without that, `list_directory` reaches
  `commands/volumes.rs::resolve_path_to_volume`, which dials the same phone AGAIN under the backend's own
  `adb-navigation:<serial>` attempt id — and Cancel, which aims at the id minted here, would call off the dial nobody
  was watching while the other quietly registered the volume. The hold is threaded through `path-sync.ts`'s
  `deviceIsConnecting` input (a `sync-path` arm, like device-only MTP's) and the mount-time load's own branch.
- **❗ While the hold is on, a `null` state renders NOTHING**, so every way a dial can end has to leave a non-`null`
  one: a `refused` with the reason and, where a second try could work, a Try again. That covers both cancels (the button
  on `connecting` and the one on `waiting_for_device`), the backend's own `cancelled` answer, a failure with no typed
  reason (which shows `adb.connect.transport`, because its own text is untranslated diagnostics that belong in the log),
  and a device row whose path names no serial. ❌ Releasing the hold instead is NOT the escape hatch: nothing re-runs
  the listing on that path, and if it did, `list_directory` would re-dial the very phone the user just called off. "A
  cancel says nothing" means not scolding someone for what they did, ❌ not leaving them in an empty pane.
- **`holdsListing` is a `$derived` off the volume id and the factory's own record, ❌ never off `state`.** The
  mount-time load runs before this factory's `$effect` has said anything, so a gate read from the view state would
  depend on `$effect` ordering — correct today, silently wrong after any reordering.
- **MTP is deliberately NOT folded in.** Its volume id CHANGES on connect (device-only → storage), which is a different
  pane transition with its own `path-sync.ts` arm, and it keeps `MtpConnectionView.svelte`.

**The volume-id string compares that REMAIN are not guards — don't "finish the sweep".** A grep for
`=== 'search-results'` / `=== 'network'` / `startsWith('mtp-')` (and the `!==` forms) across `apps/desktop/src/` returns
hits, and every one is a classifier input, a namespace mechanic, or a display choice. Forcing one of those through the
capability record is the "differently complicated" failure mode to avoid:

- **Classifier internals (the inputs that FEED `volumeKindOf`).** `volume-capabilities.ts` (the two virtual-id checks),
  `volume-tint.svelte.ts::volumeKindFor` (`fsType === 'sftp'` / `'webdav'` first, then
  `category === 'network' || fsType === 'smbfs'`), `volume-grouping.ts` (`category === 'network'` sidebar grouping),
  `mtp-path-utils.ts::isMtpVolumeId` (`startsWith('mtp-')`). These ARE the classifier — converting them would be
  circular.
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

**`refreshPane` is the one refresh, and it forces.** ⌘R (`pane.refresh`) and the MCP `refresh` tool both land in
`pane-commands.ts::refreshPane()`. It routes on the view: the network browser has no listing, so there it re-scans hosts
(`paneRef.refreshNetworkHosts()`); everywhere else it calls `refreshListing(listingId, true)` and then `refreshView()`.
The `true` matters — unforced, the backend answers a watcher-backed listing straight out of the cache, which is honest
on MTP and a lie on SMB (another machine's writes never reached the watcher). The post-write top-ups
(`transfer-pane-effects.ts`, the rename flows, `NewFolderDialog`) stay unforced on purpose: they fire after every
transfer, and a forced re-read of a 1k-entry MTP folder costs ~17 s. Rationale lives with `refresh_listing` in
`src-tauri/src/commands/file_system/listing.rs`.

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
`activeTab = $derived(getActiveTab(getTabMgr(getFocusedPane())))` and
`caps = $derived(capabilitiesForPane(activeTab.volumeId, activeTab.path))`, then `canMkdir` / `canMkfile` / `canRename`
= `caps.canWrite` and `canSourceOps` = `caps.canBeSource` (capabilities, not a `volumeId === 'search-results'` string
compare). ❌ The PANE's row, never `capabilitiesFor`'s volume row: both routed kinds are kind-from-path over a parent
drive that is itself writable, so asking the volume id alone put New folder and Rename up as enabled inside a `.git`
snapshot and inside a read-only tar, and the press then hit `readOnlyRefusal`'s alert. A zip keeps both, being the one
archive format the managed edit flow writes. A store getter inside a `$derived` is reactive across the component
boundary, so there's no `onFocusedVolumeChange` callback or `+page.svelte` mirror `$state` in the chain. Per-pane read
only (P1): touch the focused pane's manager, never both. `canSourceOps` is derived here rather than passed in as a prop,
so a focused `network` pane disables the source buttons too (`canBeSource: false`) instead of offering ops that no-op
deep down.

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
a boxed pill repeated 8× fights the flat bar; truthfulness is the must, the chip look is the want.

**Each row shows the binding that belongs to IT, and the Shift row's chips all carry Shift.** Both rows are seven fixed
slots, F2…F8, composed from one `actions` record; `defaultRow` maps each through `getFirstShortcutReactive`, `shiftRow`
through `getFirstShiftShortcutReactive` (first effective binding matching `comboHasShift`,
`$lib/shortcuts/key-capture`). `file.rename` carries two bindings, `F2` and `⇧F6`, so plain-first put a dead `F2` in the
Shift row's F6 slot — a key that does nothing while Shift is down, printed twice in one row and out of F-key order (a
user reported it). ❌ There's deliberately no fallback to the unshifted binding when a command has no shifted one: in
that row a chip is a claim about Shift+<key>, and no chip beats a wrong one. Which slots carry a command on Shift is
still fixed (⇧F4 New file, ⇧F6 Rename, ⇧F8 Permanently); the four empty ones derive `⇧F<n>` from their POSITION (slot 0
is F2), so the row always spells one ⇧F2…⇧F8 ladder. Their `aria-label` names the bare key, which is what the
`noShiftAction` message ("{fnKey} (no shift action)") is worded around, and spares a screen reader a modifier glyph.
Layout survives an absurd custom binding: the buttons are `flex: 1; min-width: 0` and the label truncates before the
chip, so a long combo can't push the bar past the window. Routing F-clicks through the bus means they now get the
dispatch preamble (`log.info` + `record_breadcrumb` breadcrumb + the `blockedByCapabilities` guard) like every other
entry path — a deliberate telemetry gain, not a behavior change. The buttons' visible `disabled` flags (`canRename` /
`canMkfile` / `canMkdir` / `canSourceOps`) win first: a disabled button can't be clicked, so the dispatch capability
guard never fires for an F-click (the guard's blocked set — `file.rename` / `file.newFile` / `file.newFolder` — matches
exactly the buttons the flags disable on a snapshot pane).

**Keydown handlers read their keys from the command registry.** `FilePane.handleKeyDown` runs before the document-level
dispatcher (it's a descendant, and dispatch is registered in the bubble phase), so a loose local match silently shadows
or doubles a real shortcut. Every handler here therefore resolves through `eventMatchesCommand` / `comboMatchesCommand`
(`$lib/shortcuts`) instead of testing `e.key` plus a modifier flag:

- `selection-keys.ts` — the pure `classifySelectionKey`, mapping a keypress to `selection.toggle` / `toggleAndDown` /
  `selectAll` / `deselectAll` / `invert`. Its arms all `stopPropagation()`, so each command runs exactly once (`⌘A` used
  to run twice, locally and centrally; invisible only because both did the same thing). `invert` defaults to `⇧8` plus a
  bare `*`. No layout types `⇧8` as `8` (`*` on US, `(` on Hungarian), so `eventMatchesCommand` also tries the physical
  `Digit<n>` key for a Shift+digit press and the binding stays layout-independent; the bare `*` is the numpad key, which
  that retry can't reach (`NumpadMultiply` is not a `Digit<n>` code). The menu item carries no accelerator for the same
  reason `+` / `-` don't: a bare `Shift+8` accelerator would eat `*` in every text field.
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

**"Open terminal here" reads the cursor through a third getter**, `getCursorRowForTerminal()`, which returns the row's
name, path, and `isDirectory`. It needs `..` as a real answer (like copy-path) AND the folder flag (unlike either
existing getter), because it chooses between the folder under the cursor and the pane's own. The rules themselves are
pure and live in `$lib/open-terminal/terminal-target.ts`; this is only the read.

**A command that ACTS on the cursor row calls `refreshCursorEntry()`, ❌ never the plain `getCursorEntry()`.** The
displayed entry is fed by an `$effect` firing one `get_file_at` per cursor move, so for a round trip after a move it
still holds the row the cursor just left. Displaying a stale row for a few milliseconds is fine; acting on one is not,
and the gap widens on a slow mount. Arrow-down then ⌥⌘T is an ordinary keyboard sequence, and it opened the previous
row's folder until `getCursorRowForTerminal()` was made async over the re-read. `getCursorEntry()` stays as it was for
the display and mirror readers, which want the cheap cached answer. The E2E spec
(`test/e2e-playwright/open-terminal-here.spec.ts`) is what catches a regression here: it drives a real cursor move, and
no unit test can reach the race.

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

`handleTransferConfirm` takes no scan flag: the progress dialog doesn't wait for a `TransferDialog` preview, because the
backend registers the operation at confirm and its own task waits for the preview it claimed
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
- **The journal read waits for `write-settled`, and reading it on the terminal event does not work.** Every single-item
  duplicate has an EMPTY journal page at complete time; the mechanism is `src-tauri/src/operation_log/DETAILS.md` § "Why
  a reader waits for `write-settled`". The tail waits through `whenOperationSettled(id)`
  (`$lib/file-operations/settled-operations.ts`), which answers immediately for the common case where the settle already
  landed. `duplicate-rename.test.ts` pins the ordering so a move back to the terminal event fails loudly instead of
  silently never opening an editor.
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
own `$state` for an operation this window is only watching: an id, a type, two paths, and `reverses` off the registry
row (set only when the adopted operation IS a reversal, and the one thing that lets the dialog title itself by what the
undo DOES rather than by the `move` / `delete` it runs as: `$lib/file-operations/DETAILS.md` § "The running reversal is
named from the SAME variant"). That factory is handed a read-only `hasBirthContext()` and nothing else, so an adoption
cannot overwrite a live birth context — not by convention, but for want of a binding to write it with. That is the one
hazard in this feature: `handleTransferError`'s archive branch takes the progress dialog down while keeping
`transferProgressProps` alive, and the password submit re-dispatches from it when the user types the password. A guard
that tested `showTransferProgressDialog` would find "no dialog shown" and let an adoption land on those props, and the
submit would then copy the ADOPTED operation's sources to the ADOPTED operation's destination — a wrong write, out of a
correct-looking guard. With two slots the question doesn't arise, and the occupancy test is "either slot full, or any
dialog open", which also covers the invisible case.

**Only a PERSON's submit re-dispatches.** The prompt has two answers that store a password, and they part company right
here. `handleSubmit` (someone typing into the dialog) re-dispatches the parked copy. `supplyStoredPassword` (the MCP
`unlock_archive` tool, routed in through `confirmOpenDialog('archive-password')`) ❌ **must never** call
`redispatchBirthOperation`: it settles the transfer and the agent runs `copy` / `move` again, so extraction reaches disk
through the same confirmation and token gate as every other write instead of falling out of an unlock. It also dodges
the stale-context hazard below, since a fresh `copy` reads current pane state rather than context captured before the
prompt. Browse mode has no such split: re-listing is a read, so both answers retry. Whole contract, including why the
parked operation is already settled: `src-tauri/src/mcp/DETAILS.md` § "Answering the archive password". Raising or
clearing the prompt also mirrors it to the backend (`notifyArchivePasswordPrompt` / `-Dismissed`), which is what lets
`cmdr://state` name the archive at all; ❌ the password is never part of that mirror.

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

A snapshot pane is outside this rule rather than an exception to it: its `search-results://<id>` can never equal an
operation's `sourceFolderPath`, so neither selection tail ever runs there. Its selection is kept honest by the entries
array instead, `snapshot-selection-sync.svelte.ts` above. Two consequences, both deliberate:

- **A PARTIAL delete leaves the survivors selected**, where a normal pane clears the selection outright. What the rows
  mean differs: `clearSourcePaneAfterTransfer` clears indices that no longer describe anything, while the remap has
  already dropped every row the operation took, so what is left is exactly the rows the user picked that are still there
  (a permission-denied one, say). Leaving them selected is a retry, and it can't act on a file nobody chose. ❌ Don't
  "fix" the divergence by threading the pane's own path into birth context: that trades a useful state for a matching
  one. Pinned in `snapshot-selection-sync.svelte.test.ts`.
- **No operation snapshot is recorded at all.** `entries-snapshot::fetchSelectedNames` returns early when the pane has
  no listing id, ahead of its `'all'` short-circuit, so there is nothing for the never-running
  `clearOperationSnapshot()` to leave behind.

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

`NetworkMountView` mirrors a mount that didn't go through the same way, as `PaneState.mountError` (`{ share, message }`)
— the "Couldn't mount share" pane and the login form an auth-class failure routes to alike. Without it a failed mount is
invisible from `cmdr://state`: the pane's `path` and `files` still describe the share list either view replaced, so a
reader sees a pane that simply didn't move. The clear is an explicit push of its own rather than something the next view
is trusted to do: `PlacesBrowser` only pushes once it has a share list, so a host that went quiet between the failure
and Back pushes nothing at all, and a `mountError` outliving its pane misleads a reader worse than the silence the
mirror replaced. `NetworkMountView.test.ts` holds that line.

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
  strings (on-network, smb-path-unsupported, MTP-mismatch, on-MTP-volume, pane-unavailable) are EXACT contract — the MCP
  adapter forwards them verbatim as the `mcp-response` error; `navigate.test.ts` + the handler suite pin them
  byte-for-byte.
- **An `smb://` path below the host-list sentinel is refused (`smb-path-unsupported`).** `resolve_location` maps EVERY
  `smb://` path onto the virtual `network` volume, whose state is a host plus a share list rather than a path, so
  `smb://` itself is the only navigable one. A longer path used to take the switch arm and report success from the host
  list, which is why `nav_to_path` advertised `smb://` support it never had. A mounted share is its own volume
  (`select_volume` by name); an unmounted one opens from the Network host list.
- **A switch TO the `network` volume resets the pane's open host** (`commitVolumeSwitch` calls
  `paneRef.setNetworkHost(options.networkHost ?? null)`, mirroring what `commitHistoryWalk` does for a history entry).
  The pane's own effect only clears the host when it LEAVES the network volume, so without this a re-select from inside
  a host (picker, breadcrumb, MCP `select_volume`) left the share list — or a mount-error pane — on screen, and the MCP
  tool timed out waiting for the volume name to fall back to plain `Network`.
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

**No pane hosts a credential form.** Every credential ask in the app is the one modal sign-in sheet
(`$lib/servers/sign-in-sheet-state.svelte.ts`), raised for SMB through `../network/smb-sign-in.ts`. A pane that could
render one made "which pane hosts it right now" a question the app had to answer for anything app-global (the OS-mount
fallback notice's retry button had no surface of its own), and Tab meant two different things inside a form that was
also a pane. ❌ Don't reintroduce an in-pane form: `smb-view-state.svelte.ts` owns waiting states only.

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
`OnboardingWizard` is the one exception (bespoke chrome, no `ModalDialog`), and it makes the same announcement by hand
from its own `onMount` / `onDestroy` pair. `anyDialogOpen()` reads that set first; the local `show*` flags beside it are
the same-tick guard between `show* = true` and the mount that registers it, ❌ not a second inventory.

**Ask Cmdr is not a dialog**, so it never reaches the set. It blocks the menu items only while the composer has FOCUS
(`explorerState.getRailFocused()`), ❌ never while it's merely visible: the rail is docked next to the panes most of the
time, and blocking on visibility would take Copy away from anyone who leaves it open.

**Two Rust-side traps worth keeping.** `set_menu_context` re-applies the blocked state LAST, because its own loop
enables every explorer item — without that, a focus round-trip through Settings re-offers Copy while the dialog is still
up. And `register_known_dialogs` clears the backend's open list, since a reloaded webview never fires the close half of
its pairs and one orphaned entry would refuse every MCP file operation until restart.

**A menu-bar rebuild resets the chrome layer too.** Changing the UI language throws every menu item away and builds new
ones (`src-tauri/src/menu/rebuild.rs`), which come up enabled. `DualPaneExplorer`'s `menu-bar-rebuilt` listener
re-pushes what only this window knows: the custom accelerators, the pin/unpin label, the "Reopen closed tab" flag, and
`activateWindowMenu('main')`, which is what re-applies the greying above. Checked states and the per-pane view modes
survive the rebuild on the Rust side, so they aren't in that list.

## Gotchas

- **Abandoning a listing goes through `listing-loader.ts::abandonListing`, never a bare `cancelListing`.**
  `cancelListing` only flips a cancel flag the backend checks atomically with its `LISTING_CACHE` insert
  (`file_system/listing/streaming.rs`), so it wins the race only while the listing hasn't landed yet; once it has, ONLY
  `listDirectoryEnd` removes the entry and stops its OS watcher (`file_system/listing/operations.rs`). Every path that
  drops a `listingId` — the error and cancelled handlers via `resetLoadingState`, the superseded-load branch, and
  `loadDirectory`'s own previous-listing cleanup — must call both, which is why they share one helper. Getting this
  wrong is invisible locally and costs a leaked cache entry plus an armed file watch per abandoned navigation until the
  6 h orphan reaper (`file_system/listing/DETAILS.md` § Backstop reaper). Measured before the fix: one Linux E2E run
  carried 11 concurrent orphans, each re-reading a deleted archive every debounce cycle for the whole run (verified on
  the Linux Docker lane, 2026-08-28).
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

- **TWO path predicates, and picking the wrong one is a real bug** (`volume-capabilities.ts`), mirroring the backend's
  own pair in `crates/cmdr-archive/src/boundary.rs`:
  - `pathCrossesArchiveBoundary(path)` — AT or inside an archive (any component carries a supported suffix). The
    ENTER-IT question, for sites reading a PANE path: capability rows, the git-repo lookup, disk space, the terminal
    target, the Enter policy's already-inside check.
  - `pathInsideArchive(path)` — strictly inside (a non-empty inner path). The OPERATE-ON question, for sites acting on a
    path: Quick Look and its cursor-follow, `isVolumeMove`, the rename permission pre-flight. The archive FILE itself is
    an ordinary file that must be previewed, copied, moved, and renamed like any other, which is the backend's rule too.

  **Why it matters, and why the frontend once got it wrong**: a single wide predicate refused Quick Look on a plain
  `.zip` and pulled its same-drive move off the local fast path. Harmless-looking until `.docx` became a browsable
  suffix, at which point the same predicate would have killed Quick Look on every Office document. A trailing slash
  still reads as the archive ROOT (`/a/foo.zip/` has an empty inner path), which the naive length check gets wrong.

  **Destination wide, source narrow.** ❗ The choice is per ARGUMENT, not per call site: one function can need both.
  `isVolumeMove` is the case — a DESTINATION names a container to write INTO (the enter-it question, so wide), while a
  SOURCE is a thing being operated ON (narrow). The backend encodes the identical asymmetry, which is what settles it:
  `create` uses `path_crosses_archive_boundary` because a new entry's parent can BE the `.zip`, while delete and the
  move source use `path_is_inside_archive` (`volume/manager/archive_routing.rs`). **Decision/Why**: asking narrow on
  BOTH sides looks tidy and silently breaks F6 into an open zip pane — the pane sits AT `/a/foo.zip`, which is exactly
  where Enter on a zip lands you, narrow answers `false`, both ids are the parent drive, the local `moveFiles` fast path
  runs, and the backend stats a regular file and refuses with "Destination must be a directory". Copy is unaffected (it
  always routes cross-volume), as are move-out and a move into a SUBfolder, so the gap is narrow enough to survive a
  casual test pass; `transfer-dispatch.test.ts` pins the archive-root destination and the mixed source/destination case
  precisely because of that.

- **`capabilitiesForPane(volumeId, path)`** returns the `archive` capability row when the path crosses an archive, else
  defers to `capabilitiesFor(volumeId)`. The pane's `caps` uses it (`capabilitiesForPane(volumeId, currentPath)`), so
  `hasBackendListing` / `hasParentRow` / `syncsToMcp` / `canWrite` are all true for a zip; a tar, 7z, or OOXML boundary
  gets the read-only variant (`canWrite: false`, `canBeSource: true` so extract-out still works). ⌘C/⌘X are refused
  separately and route to F5/F6, since archive-inner paths aren't OS-resolvable URLs. ❌ The archive branch never folds
  in the PARENT drive's published capabilities: they answer for the drive, and the pane is inside a file on it.
- **A DOCUMENT container (`.docx` / `.xlsx` / `.pptx` / `.jar` / `.apk`) browses but is never writable**, and two suffix
  tables express that here: `SUPPORTED_ARCHIVE_SUFFIXES` (browsable, mirrors the backend's `format_for_name` — a test
  parses the Rust table and asserts set equality) and `WRITABLE_ARCHIVE_SUFFIXES`, which is `['.zip']` and ❌ must stay
  that way. These lists are the VISIBLE half only: the backend refuses by TYPE, so an MCP or IPC caller that never sees
  a dialog is refused too, and read-only holds by construction rather than by hidden buttons. Why the split exists at
  all is the backend's decision to own: `crates/cmdr-archive/DETAILS.md` § "Why a document container is its own format".
- **Why `VolumeInfo.mountIsReadOnly` still matters**: the archive pane's `volumeId` is the parent drive. A writable zip
  runs the real managed archive-edit flow, but a zip that lives on a read-only `VolumeInfo` (a locked disk image) can't
  be rewritten in place — the write guards (`file-operation-commands.ts` `readOnlyRefusal`, `transfer-entry.ts`
  `checkTransferDestinationGuard`) still fall through to the per-volume `mountIsReadOnly` refusal for that case. The
  backend `ReadOnlyDevice` rejection is the safety net behind them.
- **Edits are managed ops, not instant.** A zip mutation is an O(archive) temp+rename rewrite, so mkdir/mkfile/rename
  inside a zip return an OPERATION handle, not a landed path, and copy/move into or out of a zip route through
  `copyBetweenVolumes`/`moveBetweenVolumes` (never the local `moveFiles` fast-path — `transfer/transfer-dispatch.ts`'s
  `isVolumeMove` asks the DESTINATION the wide question and each SOURCE the narrow one; see § "Destination wide, source
  narrow" below for why that asymmetry is load-bearing). The cursor lands on the new/renamed entry when the backing
  `.zip`'s live-watch refresh arrives (the durable `pendingCursorName` channel in `listing-diff-sync`, consumed on the
  refresh diff — no timer). `handleNewFileCreated` skips its open-in-editor for an archive target (the file is created
  async and an archive-inner path isn't editable in place). Deleting inside a zip is PERMANENT (no Trash inside an
  archive): `openDeleteDialog` forces `isPermanent`/`isArchive` and drops `supportsTrash`, and `DeleteDialog` shows the
  archive warning. The queue row for a zip edit is the `archive_edit` `WriteOperationType` (`file-archive` glyph,
  "Editing archive" label; no scan phase).
- **Navigation is nearly free.** `handleNavigate` forks on `entry.isDirectory || entry.isArchive` (a zip stays
  `isDirectory:false`; `isArchive` is backend-computed, extension-only, crosses IPC on `FileEntry`), routing in-place
  (same parent-drive volume) via `browseIntoEntry`. The Enter-behavior policy (below) runs FIRST and can divert to a
  popup or an external open before this browse arm. `navigateToParent` needs no archive branch: `parentOf('/a/foo.zip')`
  is `/a` (the containing dir), so walking up bubbles out of the archive by plain path arithmetic. The ONE
  reconciliation: `effectiveVolumeRoot` (feeds `computeHasParent`) uses `volumePath` (the parent mount) inside an
  archive, NOT the `.zip` path the backend emits as the listing's `volume_root` — otherwise the archive root would read
  as a volume root and hide its `..` row.
- **Opt-outs that `hasBackendListing:true` doesn't cover**: `git-browser-sync` skips inside archives
  (`pathCrossesArchiveBoundary` — a repo can't live in a zip); `volume-space` queries the parent mount path inside an
  archive (an archive-inner path isn't NSURL-resolvable, and the archive borrows the parent's space). Both read the PANE
  path, so both take the wide check.
- **Path bar** renders the transparent `…/foo.zip/inner` for free: `breadcrumbDisplayPath` strips the parent
  `volumePath` prefix and `enrichBreadcrumbSegments` rebuilds ancestor targets from it, both path-agnostic.
- **Persistence/restore** is archive-safe with no FE change: the tab stores `(parentDriveId, fullPath)`; on restore
  `initialization.ts::resolveVolumeId` calls `resolvePathVolume(path)`, which the backend resolves to the parent drive
  for an archive-inner path (backend test `resolve_location_inside_an_archive_returns_the_parent_drive`). A deleted zip
  falls into the existing unreachable-path handling.

Full backend routing, the LRU lifecycle, and the viewer temp-extract: `crates/cmdr-archive/DETAILS.md`.

## The virtual `.git` portal pane (kind-from-path)

The portal is the second routed kind, and it borrows the archive design wholesale: the tab keeps ONE `volumeId` (the
parent drive), the `GitPortalVolume`'s own id never enters FE state, history, persistence, or MCP sync, and portal-ness
is derived from the PATH while all I/O routing happens in `VolumeManager::resolve`.

- **`isVirtualGitPath(path)`** (`../git/path-detection.ts`) is the seam, and it's the SAME lexical test the backend
  routes on: a `.git/<category>/` segment for one of the six categories (`branches`, `tags`, `commits`, `stash`,
  `worktrees`, `submodules`). Keep the two in sync with `Cat` in `crates/cmdr-git/src/path.rs`.
- **The toggle is part of the predicate.** `capabilitiesForPane` reads `getShowVirtualGitPortal()`
  (`$lib/settings/reactive-settings.svelte`, mirroring `fileExplorer.git.showVirtualGitPortal`) before asking the path,
  because with the portal off `resolve` routes nothing and `.git/branches/` is whatever sits on disk. It's a reactive
  `$state` slot rather than a `getSetting` read: the pane's `caps` is a `$derived`, and a plain Map read would never
  recompute when the user flips the toggle with a portal pane open.
- **The row is read-only, not inert.** `canWrite: false` (every mutation method on `GitPortalVolume` keeps the trait's
  `NotSupported`), `canBeSource: true` (copying OUT is the headline feature and reads through the portal),
  `hasBackendListing` / `hasParentRow` / `syncsToMcp` all true. So F5/F6 out, the viewer, and MCP navigation work, while
  paste-in, F7, ⇧F4, F2, and delete are refused up front with the `fileExplorer.readOnly.gitPortal*` alert instead of
  reaching the backend's typed rejection.
- **Real files under `.git/` keep the parent volume's full row.** `.git/config`, `.git/HEAD`, and `.git/refs/heads/main`
  are not portal paths, so they stay editable, renamable, and deletable. That's a constraint the backend defends too; ❌
  don't widen the predicate to "any `.git` segment".
- **No own tint, no breadcrumb special case.** A portal pane lives on the parent drive and shows that drive's tint, the
  same way an archive pane does, so `git-portal` is a capability kind only. The path bar renders
  `…/repo/.git/branches/main/src` for free (`breadcrumbDisplayPath` strips the parent `volumePath` prefix), and
  `../navigation/path-segments.ts::splitPathSegments` already colors the `.git` segment and everything after it with
  `--color-git-portal-text`.
- **`effectiveVolumeRoot` needs no branch.** The portal volume's root is the repo's `.git`, which is always a strict
  ancestor of a portal path, so `..` shows and `navigateToParent` walks back out by plain path arithmetic.

Backend routing, the overlay that decorates the `.git/` root listing, and the repo cache: `crates/cmdr-git/DETAILS.md`
and `apps/desktop/src-tauri/src/file_system/git/DETAILS.md`.

## Enter-behavior policy (archives and bundles)

Pressing Enter on an archive or a macOS app bundle (`.app`/`.bundle`/`.framework`) is a three-way choice: browse inside,
open in the default app, or ask. The decision is a pure function; the UI is a small popup.

- **`archive-enter-policy.ts` is the pure resolver**:
  `resolveEnterPolicy(entry, behavior) -> 'browse' | 'open' | 'ask' | null`. `null` means the entry is an ordinary
  file/folder (the caller does its normal open/browse). Zip archives default Ask (matched off `entry.isArchive`, so
  tar/7z join automatically when the backend flags them); bundles default Ask (matched by directory extension);
  Office/app packages (`.docx`/`.xlsx`/`.pptx`/`.jar`/`.apk`) default Open. The actions come in as a per-format map;
  `enterBehaviorFromSettings(getSetting)` builds it by walking the format list, so the resolver stays a pure leaf and a
  new format is read for free.
- **A format IS its setting.** Each descriptor carries a `settingId` (`behavior.archiveEnter.zip` / `.ooxml` /
  `.bundle`), which is the whole answer to "can the user configure this format?" — there is no second `configurable`
  flag to disagree with it. `archive-enter-policy.test.ts` asserts parity both ways: every format names a registry entry
  whose default and options match it, and every `behavior.archiveEnter.*` entry belongs to a format. Adding `tar` means
  adding both halves, or the test is red.
- **❗ `ARCHIVE_ENTER_FORMATS` is ordered by MATCH SPECIFICITY, not by display order.** `classify` is first-match-wins
  and the zip matcher is the broadest predicate there is (`isArchive === true`), so `ooxml` — a strict subset, since an
  Office file is a zip — must precede it. Once the backend flags `.docx` as an archive, a zip-first order would swallow
  every Office document into the zip row and the Office documents setting would silently stop working. A test pins the
  order with a `.docx` carrying `isArchive: true`.
- **`handleNavigate` consults it before the browse arm**, but only when NOT `pathInsideArchive(entry.path)` (a file
  inside an archive keeps the viewer interim). `ask` → `enterMenu.openFor`; `open` → `openEntryExternally` (`openFile`,
  i.e. LaunchServices — a `.zip` opens in the OS archive tool, a `.app` launches); `browse` → falls through to
  `browseIntoEntry`. On the search-results snapshot pane the popup is skipped (opening any real entry switches volume
  first via `goToRealEntry`).
- **`enter-menu.svelte.ts` (`createEnterMenu`) holds the popup state**; `enter-menu.ts` builds the items and computes
  the cursor-row anchor. The menu (`lib/ui/Menu`) is **portaled to `document.body`** so the explorer's `onfocusin` focus
  guard (`key-dispatch.ts`, which only exempts `[role="dialog"]`) doesn't yank focus off the `role="menu"`; on close the
  controller calls `restoreFocus` (`onRequestFocus`), which re-focuses the explorer container so keyboard routing
  resumes. `Configure…` deep-links to `openSettingsWindow('enter-menu', ['Behavior', 'Archives'])`.
- **Settings** live in `settings/sections/ArchivesSection.svelte`: three registry-driven `SettingRow` +
  `SettingToggleGroup` rows over the three ids, so the section itself reads, writes, defaults, and validates nothing.
  Installs from before the split are carried over by settings migration 5 (`settings/settings-store.ts`), which unpacks
  the old `behavior.archiveEnterBehavior` JSON blob into the three keys and deletes it.

## The context menu's header line

The native file context menu opens with a line naming what it will act on (`photo.jpg · 2.1 MB`, or `3 items · 3.2 MB`
when the right-click landed inside the selection). Rust picks which of those shapes to draw, from the target count it
derives itself (`src-tauri/src/menu/DETAILS.md` § "The context menu's header line"); the frontend supplies every word
and digit inside, because only it can honour `appearance.fileSizeFormat`, `listing.sizeUnit`, a locale's grouping
separator, and a plural form. The rules and helpers live in `../selection/context-menu-target.ts`; what each caller here
can answer:

- `pane-pointer.ts::handleContextMenu`: the right-clicked row's own size when the click landed OUTSIDE the selection,
  and the pane's selection total when it landed inside — through the `getSelectedFilesTotalSize` dep, which `FilePane`
  fills from the same `ListingStats` the status bar reads. A pane can't hand its rows over (the listing lives outside
  reactivity by design), so the backend's totals stand in. `countText` comes off `paths.length`, the same array the
  backend re-counts.
- `SearchResultsView.svelte`: its snapshot holds its own rows, so `snapshotContextMenuRows` answers both cases. ❗ That
  function and `snapshotContextMenuPaths` are one rule in two shapes on purpose — a second walk could aim the header at
  a different set than the actions.

❌ No stand-in number when there's no honest size (a folder, a row with no size yet, a selection holding a folder): the
header shows the name or the count alone.

## Sharing a row

The native context menu's `Share` (macOS) is a submenu listing what macOS can send the right-clicked selection to. The
services and the submenu live in Rust (`src-tauri/src/file_system/DETAILS.md` § "The Share submenu"); what this
directory owns is whether the item MAY appear, pushed as `PaneContextMenuFacts.canShare` from
`pane-pointer.ts::handleContextMenu`. Rust adds one more condition of its own: macOS has to actually offer a service.
The same flag gates the context menu's `Services` submenu, which asks the same question for the same reason (a macOS
service takes file URLs too): `src-tauri/src/menu/DETAILS.md` § "Services in the right-click menu".

A share service takes file URLs, so the question is whether the right-clicked ROW has a real file behind it, and it
needs three inputs rather than one kind lookup (`rowIsOsVisible` in `volume-capabilities.ts`):

1. The VOLUME's kind, via the pure `paneRowsAreOsVisible`: `local`, `smb`, and `search-results` yes; `mtp`, `adb`,
   `network`, `archive`, `git-portal` no.
2. The ROW's path, for an archive's insides — `pathInsideArchive`, the NARROW check, so the `.zip` file itself stays
   shareable.
3. The ROW's path again, for the virtual `.git` portal, and only while `showVirtualGitPortal` is on.

**This is deliberately NOT `canOpenTerminalHere`, and the two part company in both directions.** The terminal item asks
about the pane's own FOLDER, so a search-results snapshot answers no while every row in it is a real file; an
archive-inner pane answers yes (the terminal opens the folder holding the `.zip`) while its rows have nothing behind
them. Folding them into one flag would be wrong for both panes.

Surfaces other than the two file panes leave `canShare` at its `false` default, so the Search dialog's row menu carries
no `Share` today. Not a considered no: it's the "a surface that can't answer says nothing" default the whole
`PaneContextMenuFacts` object takes.

## Feeding the macOS Services menu

`Cmdr > Services` acts on whatever is selected RIGHT NOW, at any moment, so the pane pushes it rather than answering a
question. AppKit asks synchronously on the main thread and can't await IPC; the mechanism, the send types, and the
pasteboard layout are Rust's (`src-tauri/src/services_menu/DETAILS.md`).

What this directory owns is the payload, built by the pure `services-selection.ts` and pushed by `FilePane.svelte`'s
`debouncedServicesSelection`:

- **Selected rows travel as listing INDICES**, not resolved paths, and the backend reads the paths out of the listing
  cache only when a service asks. A held ⇧↓ across a 500k-row folder would otherwise ship the whole path array on every
  keystroke. The search-results snapshot is the exception: its rows live here, so they travel as paths, resolved through
  a `pathAt` accessor so the cost is per SELECTED row rather than per row.
- **The payload is built inside the debounce, not at the trigger**, so the index-array copy happens on the timer.
  `syncPaneStateToMcp` reads its indices the same way and for the same reason.
- **The trigger is `createSelectionState`'s `onChanged`**, which fires once per real mutation. ❌ Not a `$effect` on
  `selectedIndices.size`: a gesture that swaps WHICH rows are selected without changing HOW MANY would push nothing, and
  a service would then act on the previous set. Everything else the payload reads (focus, cursor entry, listing id,
  `caps.kind`, hidden files, `hasParent`) rides a plain `$effect` beside the menu-context one.
- **The pane-level gate is `paneRowsAreOsVisible(capabilitiesForPane(...).kind)`**, and it's the WIDE archive check on
  purpose, unlike `rowIsOsVisible` above: the question here is about the pane's rows as a set, so a pane sitting INSIDE
  a `.zip` offers nothing while a pane merely CONTAINING one offers everything. A pane that fails the gate pushes both
  fields empty, leaving the Services menu exactly as it was before this feature.
- **Both panes run the effect; only the focused one pushes**, same as the menu context. Taking focus is itself a
  trigger, so the newly-focused pane overwrites the other's push.

This push is the MENU BAR's answer. The right-click menu's `Services` submenu overrides it with the right-clicked rows
for as long as that menu is up, out of the `paths` argument `handleContextMenu` already computes; nothing extra is
pushed for it (`src-tauri/src/services_menu/DETAILS.md` § "The right-click menu's Services submenu").

## Analytics emitted from this directory

Three of this directory's modules are analytics chokepoints, and they're chokepoints on purpose — a per-call-site event
drifts the moment a fourth trigger appears.

- `tab-operations.ts` emits the four `tab_*` events. It's the layer every trigger funnels through (the tab bar, the File
  menu, the keyboard, the palette, the MCP `tab` tool), and the pure `tabs/tab-state-manager.svelte.ts` beneath it is
  deliberately left alone: unit tests drive it directly, so emitting there would fire events from the test suite.
- `drag-drop-controller.svelte.ts::handleDrop` emits `drop_received` on EVERY arm, refusals included.
- `volume-selection.ts` emits `favorite_opened` from its `category === 'favorite'` branch, matching
  `VolumeBreadcrumb.handleVolumeSelect`. There's no lower chokepoint: both fold onto
  `navigate({ to: { selectVolume } })`, which by then holds the containing volume's id and can't tell a favorite from a
  drive.

Vocabulary and props: `src-tauri/src/analytics/DETAILS.md` § "Starter event set".

## A dialog outcome that navigates

`DialogStateDeps.getExplorer()` hands a dialog outcome the five-method `PaneRevealAPI`
(`../navigation/navigate-and-select.ts`): `getFocusedPane`, `setFocusedPane`, `getPaneLocation`, `navigate`,
`moveCursor`. That's everything the reveal primitives use and nothing else.

`DualPaneExplorer` builds it from its OWN exports, so no handle travels back in from the route: the component already
has the five functions as locals. `ExplorerAPI` satisfies the interface structurally, so the older call sites (⌘J, ⌘G,
search-result reveal) keep passing the whole explorer unchanged.

**Why narrow rather than pass `ExplorerAPI`.** The alternative was routing `explorerRef` back down as a prop from
`+page.svelte`, which means handing a component its own handle. Narrowing the dependency instead keeps the direction of
knowledge one-way, and a dialog gets no reach into tabs, volumes, quick-look, or the MCP surface it has no business
touching.

**Snapshot it at raise time.** The trash toast holds the handle for as long as it's on screen (the same shape as the
downloads toast). The object is a snapshot; the methods inside it read live state, so a pane that moves under the toast
still resolves correctly when the button is finally pressed. First consumer:
`$lib/file-operations/delete/go-to-trash.ts`.

## The connect views

`RemoteConnectView.svelte` is the pane while a remote place is on its way in or has stopped short, and it holds no state
of its own: the caller hands it a typed `remote-connect-state.ts` value and the callbacks that go with it. Every state
says what is happening in one sentence, and every state with a process behind it has a cancel. A spinner with no words
is the failure shape it replaces: a person can't tell a slow handshake from a wedged one, and has no way out of either.

Two producers:

- `place-connect.svelte.ts` owns the first-dial `connecting` and `refused`, for a pane landing on a `saved` place. It
  dials once per landing (a `dialed` guard), and its Cancel aims at the attempt id `connect-flow.ts` hands out before
  the dial.
- `smb-view-state.svelte.ts` maps the reconnect manager's status onto `connecting` (with a `cycle`), `signed_out`, and
  `host_key_changed`, in ONE exhaustive `switch`. ❗ Exhaustive, ❌ not a list of per-status booleans: a status without
  its own arm used to render a plain listing over a dead session.
- `device-connect.svelte.ts` owns `waiting_for_device` and a phone's `connecting` / `refused`. Its sentences come from
  `$lib/adb/adb-connect-errors.ts`, so the view stays generic and the words stay Android's.

**A backoff loop wears the `connecting` state, with a `cycle` payload.** It says how long the loop keeps going and which
attempt it is on, drains a bar toward the next attempt, and offers Try now beside Cancel and Disconnect. ❗ The
countdown and Try now are not decoration: without the bar a person can't tell a slow handshake from a wedged one, and
without Try now they sit out a delay for a server they can see is back. The payload is plain data plus callbacks, so the
view holds no reference to the reconnect manager and a second backend's loop renders through it unchanged. `waiting` is
`null` while an attempt is actually in flight — nothing to draw, nothing to skip — and the spinner carries the motion.

❗ **There is no `gave_up` state.** A loop that ran out of attempts renders `VolumeUnreachableBanner`'s `gaveUp`
variant, the app's one "couldn't reach this" surface, which already words the path, the retry, and the disconnect. Two
renderers for one state is worse than one in the file next door.

❗ **`waiting_for_device` carries no retry, deliberately.** Nothing is dialing, and the thing that would change the
state happens on the DEVICE. A "Try again" would re-ask a question already answered, and an "I tapped it" button would
be a lie about how the pane finds out (it reads the volume list). Cancel is the only control, because the wait is the
process and calling it off is the one thing a person can do from here.

❗ **A `refused` state's callbacks are all optional.** Some refusals have no move left: an unplugged phone is fixed by
the cable, an Android 6 phone by nothing. Those render the sentence with NO action row at all. ❌ Never supply a `retry`
that is guaranteed to fail again — an inert affordance is the one thing this view refuses. `openSettings` is for the
refusals only Settings can clear and never appears beside `retry`.

❗ **`signed_out` carries a `signIn` that may be `null`.** The reconnect manager stores what the backend said a sign-in
would ask for at the moment it flipped (`getSignInShape`), and a `nothing` shape means no secret a person could type
would bring the session back. The banner then says so instead of offering a button that cannot work.

❗ **A changed host key offers Disconnect, ❌ not "Trust it".** Nobody can answer for a fingerprint they haven't been
shown, and nothing on this side holds one: the backend keeps no pending prompt for a REGISTERED volume. Disconnecting
drops the dead session and leaves the place a `saved` row, so opening it dials afresh — and THAT dial's
`needs_host_key_approval` outcome is what the sheet's key step renders. The path works today; a backend command handing
back the pending prompt would make it one click instead of two.

❗ **A state lands only once something can act on it.** Adding one before its handler puts a button on screen that does
nothing, which is the one thing this view refuses to do (`../../servers/DETAILS.md` § "The sheet contract").

❗ **The `state` prop is destructured to a different local name.** A binding called `state` in a Svelte 5 component
makes every `$state(...)` in the file read as a store subscription instead of a rune, which the compiler reports as a
type error rather than a rename hint.
