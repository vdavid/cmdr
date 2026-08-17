# Main route details

Depth and rationale for the app orchestrator. `CLAUDE.md` holds the must-knows; this file holds the full mechanism.

## File map

Where a symbol lives and who calls it: `codegraph_search` / `codegraph_explore`. The area's shape: `CLAUDE.md` § Module
map, plus `command-handlers/CLAUDE.md` for the handler families. What each piece DOES is in the sections below (§
"Startup: paint-gated window show", § "Dispatch core", § "The exempt families", § "Capability guard", § "MCP transport",
§ "Cross-window: `foreground-operation`", § "Mouse back / forward buttons", § "Right-click ownership", § "Native-menu
and input-focus interactions", § "Off-bus test and debug hooks"). Only the layout facts that none of those carry live
here:

- **`listener-setup.ts` is a plain `.ts` with NO runes**, so it can't hold `$state`. State crosses the boundary through
  a `ListenerSetupContext` of getter functions (reads) and setter callbacks (writes), which is what keeps the moved
  closures reading LIVE reactive values instead of a stale capture. Every registered unlisten goes onto the
  component-owned `unlistenFns` array so the one `onDestroy` loop tears them all down: without that, HMR stacks
  duplicate listeners on every reload. The keydown handler, licensing init, and onboarding gating stay in `+page.svelte`
  precisely because they read and write `$state` directly. On the dispatch side the mirror rule is that only
  `handleTextRegionShortcut` and `blockedByCapabilities` belong in the core; everything else is a handler.
- **`global-keydown.ts` owns the keydown DECISION, `+page.svelte` owns the side effects.**
  `resolveGlobalKeyAction(event, isModalOpen)` is pure (`dispatch` / `openDebugWindow` / `suppress` / `ignore`), so
  every branch is unit-testable without mounting the shell; the component supplies `isModalDialogOpen()` (the only
  reactive input) and then runs `preventDefault`, `markDispatchSource('keyboard')`, and the dispatch. Keeping the
  decision out of the component is also what stops the `file-length`-flagged `+page.svelte` from growing per keyboard
  rule.
- **`global-contextmenu.ts` is the same split for the right-click**: `resolveGlobalContextMenuAction(event)` is pure
  (`native-text-menu` / `suppress`), `+page.svelte` runs `stopPropagation` or `preventDefault`. § Right-click ownership.
- **`startup-gates.ts` owns what a launch SHOWS.** § Startup gates.
- **`command-dispatch-context.ts` is deliberately a LEAF**, importing nothing from the core or the handlers, so handler
  modules and `command-dispatch.ts` can both import the context types without a cycle. It's re-exported from
  `command-dispatch.ts` for callers.
- **`dispatch-dedup.ts` guards a CROSS-source double fire only.** `markDispatchSource('keyboard' | 'menu')` tags a
  dispatch and the core drops the same command arriving from the OTHER source within 300 ms, which is the macOS
  menu-accelerator + webview-keydown double fire. Same-source repeats (a user pressing a key twice) and untagged
  dispatches (palette, MCP, mouse) always pass. Unit-tested with injectable time.
- **`+page.svelte` mounts Ask Cmdr's bulk-rename review beside the rail, but doesn't own it**: the rail owns its user
  decisions and the proposal's lifetime.

## Startup: showing the main window

The main window launches `visible: false`; `showMainOnMount()` shows it from `onMount`. This is the ONLY path that shows
the main window: the backend restores saved size and position during setup (`src-tauri/src/window_state/`, which runs
while the window is still hidden) but deliberately never shows it.

**Show first, check after.** There used to be a paint gate _before_ the show, awaiting `waitForNextPaint` (a double
`requestAnimationFrame`). That's gone: it cost a fixed second of startup and produced no usable signal. The check now
runs after the show, where the window is visible and rAF isn't throttled, so its answer means something. If no frame
lands within 1 s we re-show, because `makeKeyAndOrderFront:` re-invalidates the view.

The risk it guards is real but unreproduced: `show()` can land before the compositor presents a frame, and if nothing
invalidates the view afterward the window sits blank until the user resizes it (observed once on a cold prod launch
during a heavy full-root reindex). Showing from `onMount` narrows that window a lot compared to when the incident
happened, since the frontend has hydrated and built the DOM by then.

**Gotcha, and a retracted measurement.** This file used to state that rAF ticks while the window is hidden, citing
"first paint confirmed in ~25 ms" (2026-07-23). That measurement was taken while `tauri-plugin-window-state` was still
showing the window at window-ready, i.e. _before_ the frontend ran, so it measured a window that was already visible.
The gate was a no-op re-show and had never once run in the hidden configuration it was written for. Removing the plugin
is what surfaced this. A single later observation with a genuinely hidden window did hit the 1 s timeout, which points
at rAF being throttled while hidden, but that machine was at load average 79, so starvation isn't excluded. The
show-first shape makes the question moot for startup speed either way; don't reintroduce a pre-show gate on the strength
of either data point.

## Startup gates

`startup-gates.ts` holds the four decisions that determine what a launch actually shows, so each is exercisable without
mounting the shell (`startup-gates.test.ts`). They're the highest-stakes branches in the route: getting one wrong either
re-prompts someone who already answered the FDA question, or drops a first-run user into an explorer with no disk
access, and neither is visible from a passing type-check.

- **`resolveOnboardingMount(ctx)`**: reads `CMDR_FORCE_ONBOARDING`, settings, and a fresh FDA probe, then routes to the
  wizard or the explorer. The truth table it implements is canonical in `lib/onboarding/DETAILS.md` § "Mount +
  onboarding flag"; don't restate it here. A failing force probe degrades to "not forced" (`.catch(() => false)`) so a
  missing backend can't wedge the launch. Every branch ends with the shell revealed.
- **`maybeFireUpgradeNudge()`**: the one-time toast. Called only from the two branches that skip the wizard, which is
  why it needs no visibility check of its own. Copy and the E2E suppression: `lib/onboarding/DETAILS.md` § "Upgrade
  nudge".
- **`maybeRunWhatsNew(ctx, force)`**: the boot check plus the re-attempt after the wizard closes. It only gathers the
  gate inputs; the decision is `whats-new-trigger`'s.
- **`openOnboardingFromMenuOrPalette(ctx, source)`**: re-entry. Both `menu` and `palette` open at the first reachable
  step (`openWizard` enforces that per-source), so this only guards against re-opening an open wizard.

**The gates read settings SYNCHRONOUSLY, which puts weight on the `settingsReady` gate.** `onboarding.completed` and
`onboarding.fullDiskAccessChoice` are ordinary registry settings, so the gates just `getSetting` them: no store load, no
await. That's only safe because `+layout.svelte` mounts `+page.svelte` behind `{#if settingsReady}` and starts the
update checker after the same flag. A pre-init `getSetting` returns the REGISTRY DEFAULT, and here the defaults read as
"never onboarded, never asked about Full Disk Access" — so moving either call ahead of that flag would re-run the wizard
for everyone rather than fail loudly. The three writes (`onboarding.completed`, the FDA answer, the terms acceptance)
pair `setSetting` with an awaited `forceSave()` for the mirror-image reason: they record something that happened, and
the 500 ms save debounce would drop the record if the user quits right after answering.

**Why a context of getters.** `StartupGatesContext` passes setters for the `$state` these flip and GETTERS for what they
read. `maybeRunWhatsNew` runs at boot and again on wizard close, so a captured `showOnboarding` value would report the
boot-time answer on the second call and the popup would either double-show or never show. Same rule as
`ListenerSetupContext`.

**Wizard visibility moves in one place.** `setOnboardingVisible()` in `+page.svelte` writes `showOnboarding` AND
`setOnboardingShowing()` (the updater's mirror, which holds the "restart to apply" toast back while onboarding is up).
Every open and close goes through it, including `handleWizardComplete`; writing `showOnboarding` directly would let the
two drift and leak an update toast over the wizard.

## Dispatch core

`handleCommandExecute<K extends CommandId>(commandId, ctx, ...args)` runs the preamble (text-region intercept, then
`log.info`, then `record_breadcrumb`, then close palette, then capability guard), then looks the id up in the flat
`commandHandlers` record and awaits the handler. Arg-carrying ids take a typed payload.

Per-command logging: each successful dispatch emits one `log.info(commandId)` (LogTape, fern, error-report bundles) and
one `record_breadcrumb` invoke (rolling manifest buffer). Both are best-effort; a failing breadcrumb must not break the
dispatch. Because MCP events ride the same bus, they get the same telemetry, a deliberate uniform gain.

## The exempt families

Twenty ids are registered (for the rebinding UI) with NO dispatch handler: native-menu-owned, per-keystroke P2, and
component-scoped. The `DispatchExemptId` union in `command-handlers/types.ts` is the single maintained list, documented
per family in `command-handlers/DETAILS.md` § "The exempt families". The core silently no-ops these after the preamble.

## Capability guard

`blockedByCapabilities` reads `capabilitiesFor(getFocusedPaneVolumeId())`, the same source the F-bar `disabled` flags
and the context menu read. F-bar buttons and context menus disable visibly at the source; this guard catches the
shortcut-driven path that bypasses the UI. The toast (`SEARCH_RESULTS_NOT_A_FOLDER_TOAST`) fires only for the
`search-results` kind: a `network` pane has the same `false` destination caps, but those ops are unreachable through its
UI and the shortcut path falls through silently to the explorer no-op, so network keeps its prior silence. The
capability decides the block; the kind decides the toast.

One exemption: `edit.paste` with a text input focused (`isTextInputFocused`) skips the guard entirely, because that
dispatch inserts into the input and never touches the pane. Without it, a snapshot pane behind a dialog would block
pasting into the dialog's own field, and since the keydown resolver now `preventDefault`s that combo (§ Native-menu and
input-focus interactions), nothing else would insert. `edit.pasteAsMove` gets no exemption: its handler always drives
the pane.

## MCP transport

`mcp-listeners.ts` is a transport adapter onto the command bus. Every `mcp-*` event (except the two exceptions below)
validate-parses its raw payload into the command's typed `CommandArgs`, each discriminant string whitelist-checked by a
small pure parser (`parsePane`, `parseSortColumn`, `parseTabAction`, …; unit-tested in `mcp-listeners.test.ts`); a
malformed value collapses to `undefined` and the listener skips the dispatch. No `as {...}` payload casts survive.
`ctx.dispatch` is `+page.svelte`'s `handleCommandExecute` (bound with its context).

Per-pane MCP commands (`sort.set`, `selection.mcpSelect`, `cursor.moveTo`, `cursor.scrollTo`, `volume.selectByName`,
`tab.mcpAction`, `pane.refresh`, `dialog.confirm`, `nav.openUnderCursor`, and the optional-arg
`file.copy`/`file.move`/`file.delete`) exist because the focused-pane registry commands can't target a specific pane /
tab / option. They're all `showInPalette: false`. `view.setMode` is shared with the native-menu `view-mode-changed`
path; its `fromMenu` flag picks `setViewModeFromMenu` (skip `pushViewMenuState`) vs `setViewMode` (push it).

### Two exceptions stay adapter-local (off the bus)

- **`mcp-nav-to-path`** bypasses the bus entirely. The adapter resolves the bare path to a `Location` at the edge first
  (`resolveLocation` — the agent path can live on any volume), replying `ok: false` if it can't resolve, then calls
  `explorerRef.navigate({ pane, to: { goTo }, source: 'mcp' })` and branches on the typed `NavigateResult`: a
  `'refused'` result forwards `result.reason.message` byte-identically as the `mcp-response` error; a `'started'` result
  awaits `result.settled` before replying `ok: true`. Resolving at the edge also narrows the on-network refusal — a
  local target from a network pane now switches volumes instead of refusing; only an `smb://` target still refuses. The
  bus dispatch is fire-and-forget and can't surface this round-trip.
- **`mcp-response` round-trips** (`mcp-open-under-cursor`, `mcp-move-cursor`, `mcp-select`, `mcp-select-names`,
  `mcp-refresh`): the bus dispatches the `void`-returning intent; the adapter owns the `requestId` correlation and the
  `emit('mcp-response', { requestId, ok, error? })` reply. It awaits the dispatch's promise so the ack fires only after
  the action settles. The underlying handlers are `async`, and an exception (filename not found, index out of range,
  missing names, refresh timeout) propagates to the adapter's `try/catch`, which replies `ok: false` with the message,
  so the tool reports the real failure instead of a false-positive OK. HMR can land these with no explorer; they reply
  `ok: false` rather than crashing.

### Focus follows the navigated pane

The nav-family handlers that take a `pane` (`mcp-nav-to-path`, `mcp-scroll-to`, `mcp-select`, `mcp-select-names`) call
`explorerRef.setFocusedPane(pane)` so FE focus matches the backend `PaneStateStore`, whose focused pane the Rust
`nav_to_path` / `scroll_to` / `select` handlers set optimistically. Without it, FE focus and the backend store diverge
(`navigate()`'s in-place same-volume arm deliberately keeps focus put for keyboard nav), so `cmdr://state` reports one
pane focused while a follow-up focused-pane op (`mkdir` / `copy` / `move`) acts on the other — the wrong-pane
data-safety bug. `mcp-move-cursor` and `mcp-volume-select` don't need the explicit call: `moveCursor` focuses internally
and the volume-switch arm shifts focus itself. Don't remove these `setFocusedPane` calls. (`mkdir` / `mkfile`
additionally carry an optional `pane` so a create never races FE focus timing.)

A `mcp-key` GoBack/GoForward routes through the bus (`nav.back`/`nav.forward`), whose handlers call
`explorerRef.navigate({ pane, to: { history: 'back' | 'forward' }, source: 'user' })`, same shape as `nav.parent`
(`to: { history: 'parent' }`). Every other key stays a `sendKeyToFocusedPane` passthrough (invariant P2).

## Cross-window: `foreground-operation`

The operation-queue window's Show button asks THIS window to put one already-running operation into its progress dialog.
It's the only INBOUND channel here whose sender is another window rather than a menu, a key, or MCP. Which rows offer
the button, and why only the id travels: `$lib/file-operations/queue/DETAILS.md` § Show.

`setupDialogListeners` owns the receiving half (`onForegroundOperationRequested`):

- **This window comes forward whatever the verdict**, so `focusMainWindow()` runs before anything is decided. The dialog
  that might refuse is here, and so is the toast that says so; a refusal behind another window reads as the button doing
  nothing.
- **The id resolves against THIS window's snapshot**, `adoptedOperationFor(getMainWindowOperationRows(), id)`. A miss
  means the operation ended between the click and the delivery, which also took its queue row: log it and stop, since
  there's nothing left to show and nothing to say about it.
- **`ExplorerAPI.foregroundOperation` returns the verdict synchronously**, passing through to `dialog-state`'s
  single-occupancy progress slot (`$lib/file-explorer/pane/DETAILS.md` § "Birth context") and taking `busy` back to the
  toast. ❌ Don't route it through the command bus: a bus dispatch is fire-and-forget and would drop the verdict, and
  the queue window would have no way to learn its button did nothing.

## Mouse back / forward buttons

A pointer's dedicated X1/X2 side buttons drive the same `nav.back` / `nav.forward` bus commands as `⌘[` / `⌘]` (issue
#31), so history walks the same way regardless of input device. `+page.svelte` registers two document listeners that
both consult `navCommandForMouseButton` (`mouse-nav.ts`, mapping `button === 3 → nav.back`, `4 → nav.forward`):

- **`mouseup`** dispatches the command (gated by the same `isModalDialogOpen()` guard as the keyboard path, so the
  buttons stay inert while a dialog or overlay is up). The dispatch is left untagged for the cross-source dedup: a mouse
  button has no native-menu twin to double-fire, so it should always pass.
- **`mousedown`** only `preventDefault`s the side buttons (no dispatch). This is what cancels WKWebView's built-in page
  back / forward, which would otherwise pop the SvelteKit SPA history (e.g. unwinding a `/settings` visit) underneath
  us. The suppression can't move to `mouseup` — the webview commits its default nav on the press — so the two halves
  stay split across the two events. Suppression runs even while a modal is open (we never want the webview navigating
  itself); only the dispatch is gated.

## Right-click ownership

Cmdr owns right-click in the main window: file rows, tabs, the breadcrumb, volume rows, query results, and network rows
each build their own native macOS menu, so `+page.svelte` installs a document `contextmenu` listener that
`preventDefault()`s WKWebView's menu. Without it every one of those clicks would show two menus.

Text fields are the exception. WebKit's editing menu (Cut, Copy, Paste, Select All, plus the system Services and
spelling entries) is exactly what a text field should offer, and it acts on the field through WebKit's own editing
commands rather than through the command bus, so it can't double up with the `edit.*` handlers the way ⌘V did (§
Native-menu and input-focus interactions).

- **The predicate is the event TARGET, not focus.** `isTextInputTarget(event.target)` from
  `$lib/utils/text-input-focus`. A right-click can land on a field that isn't focused yet, so the focus-based
  `isTextInputFocused()` twin would deny the menu on that first click.
- **The listener runs in the CAPTURE phase.** On an editable target it `stopPropagation()`s, so an ancestor's handler
  can't open a Cmdr menu over the field: the inline rename editor sits inside a file row, and the volume switcher's
  favorite-rename field inside a volume row, and both rows have their own `oncontextmenu`. On everything else it
  `preventDefault()`s and lets the event keep bubbling, so those row handlers still open Cmdr's menu exactly as before.
- **Dev builds show more.** WKWebView adds its inspector entry to the menu when the webview has devtools enabled, which
  is Tauri's debug-build default; release builds don't enable them, so they show the editing items only. WKWebView gates
  that itself, so ❌ don't add a dev-only branch here.
- **Only the main window suppresses.** Settings, shortcuts, viewer, queue, and debug install no document-level
  suppressor, so WebKit's menu is already live there. The viewer's media surface has its own element-level
  `oncontextmenu` (`viewer-pointer-drag.svelte.ts`) because it opens an in-app menu at the pointer.

## Native-menu and input-focus interactions

These CLAUDE.md gotchas share the same root: a native macOS menu accelerator fires before the webview keydown, so the
dispatch path can't rely on the keydown bail.

- **⌘A (`selection.selectAll`).** Intercepted as a menu accelerator before the webview. The handler routes to
  `active.select()` when a `<input>` / `<textarea>` is focused, otherwise delegates to
  `explorerRef.handleSelectionAction('selectAll')`. The keydown bail doesn't help; the menu fires first.
- **The text-editing family while a modal is open.** `resolveGlobalKeyAction` (`global-keydown.ts`) normally resolves
  nothing when `isModalDialogOpen()`, so pane-scoped commands stay inert behind a dialog. `edit.cut` / `edit.copy` /
  `edit.paste` / `selection.selectAll` are the exception: with focus in a text input they still resolve, matched through
  the registry (`comboMatchesCommand`, so a rebind follows) rather than as literal combos.

  Two actors would otherwise insert on ⌘V: WebKit's native paste (the key event's default action on an editable element)
  and the Edit > Paste accelerator, which reaches the `edit.paste` handler through the `execute-command` menu listener.
  Outside a modal only one lands, because the resolver's dispatch means `+page.svelte` calls `preventDefault()` (killing
  the native one) and the menu twin is swallowed by the cross-source dedup. With a modal open there was no
  `preventDefault()`, so both ran and the clipboard text landed TWICE, in every dialog with a text field. The dedup
  can't catch it: only one command dispatch happens, and the other insertion is a browser default action it never sees.

  Two shapes that look like fixes and aren't: adding ⌘V to the suppress list makes paste depend on a native menu
  existing (it breaks in the dev browser, and on any platform whose menu lacks the item), and gating the
  `execute-command` listener on modal state breaks paste wherever AppKit consumes the key outright and the menu is the
  only path.

- **`edit.paste` into a text input.** Reads via the `readClipboardText` Rust IPC, then writes with
  `document.execCommand('insertText')`. `navigator.clipboard.readText()` would surface a WebKit "Paste" confirmation the
  user must click each time, so it's avoided.
- **`edit.paste` into a PANE (file-scope).** Routes `clipboard-handlers.ts` → `explorerRef.pasteFromClipboard`, which
  reads file URLs off the clipboard. When there are file URLs it runs the transfer path as before. When there are NONE,
  it falls back to "paste clipboard content as a file" (`pane/paste-clipboard-as-file.ts::pasteClipboardContentAsFile`),
  gated by the `fileOperations.pasteClipboardAsFile` setting: `doNothing` → today's "No files on the clipboard" warn
  toast, no command; else it calls the `paste_clipboard_as_file` command — a `null` result (nothing pasteable) shows the
  same warn toast, a created file shows the info toast (and, for `createFileAndRename`, starts an inline rename with the
  extension-change warning suppressed for that one auto-started rename). The auto-rename passes
  `startRename({ expectedName })`: the optimistic cursor move can resolve before the FE row array applies the new file's
  synthetic diff, so **the rename refuses to activate unless the entry under the cursor is exactly the created file** —
  it polls briefly while the diff lands, then gives up silently. This is a DATA-SAFETY guard: without it the editor
  could latch a DIFFERENT row and the user's next keystroke would rename the wrong file. (In a churning directory a
  watcher-triggered `loadDirectory` reread may `renameCancel()` the auto-rename before it activates — that's fine; what
  must be impossible is latching the WRONG entry.) `edit.pasteAsMove` behaves identically here (move semantics are
  meaningless for clipboard bytes). Backend flavor precedence + the write:
  `apps/desktop/src-tauri/src/clipboard/DETAILS.md` § Paste clipboard content as a file.
- **`view.showHidden` is local-first.** Flips the `listing.showHiddenFiles` setting, whose in-memory write is
  synchronous; the native menu's check state follows from `settings-applier.ts` and the save is debounced behind it.
  Routing the toggle through Rust adds an IPC + event hop and flaked the hidden-file E2E under slow-lane load.

## Off-bus test and debug hooks

- **E2E drop hook.** `+page.svelte` registers an `e2e-trigger-file-drop` listener gated on `isE2eRun()` (set by
  `CMDR_E2E_MODE=1`, never true in prod). It forwards to `explorerRef.triggerFileDrop`, which delegates to the drag
  controller's `handleFileDrop`, the same seam the live `onDragDropEvent` 'drop' branch runs. Real OS drag can't be
  synthesized in Playwright, so the harness emits this event to exercise drop handling end to end (shared destination
  guard, source-volume resolution, transfer dialog). See `test/e2e-playwright/DETAILS.md` § "Transfer-dialog counters +
  programmatic drop entry".
- **Debug-error listeners.** The `debug-inject-error` / `debug-reset-error` listeners (gated by `import.meta.env.DEV`)
  call `explorerRef.injectError` / `resetError` directly. They inject test state from the debug window's error-pane
  preview. Routing them through the bus would pollute the `CommandId` union with dev-only ids for zero gain. See
  `lib/file-explorer/DETAILS.md` § "Debug preview". The transfer-error dialog is NOT one of them: it's a gallery entry
  (below), which renders the real component from a typed `WriteOperationError` instead of a synthetic `io_error`.
- **Dialog gallery listener.** `debug-open-gallery-dialog` (same block, which is gated
  `import.meta.env.DEV || __CMDR_DIALOG_GALLERY__` so the capture and E2E builds carry it too) opens a soft dialog over
  the main window with fixture data, for design review from Debug > Soft dialogs. The handler seeds the gallery store
  and focuses the main window from this side (the Debug window's capability set is minimal and permission failures are
  silent). The harness itself mounts in `+layout.svelte`; `+page.svelte` only reads `isGalleryDialogOpen()` so global
  shortcuts don't fire behind a previewed dialog. A payload carrying `fixtures` (the disk-backed dialogs) first
  navigates the focused pane to that fixture directory and reads back its live listing id, because those dialogs need a
  pane-owned listing, not just a path. See `lib/dialog-gallery/DETAILS.md`.
- **`focusMainWindow()` needs `core:window:allow-set-focus` in `capabilities/default.json`**, and logs (never swallows)
  a rejection. Without the permission the call fails silently and the dialog opens behind the window that asked for it,
  which looks like a dialog bug. Both callers depend on it: the gallery listener and `onFocusConfirmation`.
