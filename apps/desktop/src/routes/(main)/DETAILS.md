# Main route details

Depth and rationale for the app orchestrator. `CLAUDE.md` holds the must-knows; this file holds the full mechanism.

## File map

- **`+layout.svelte`**: main-window layout (updater, settings applier, AI state init, MCP shortcuts/settings bridges,
  toast container, crash + MTP + error-report dialogs).
- **`+page.svelte`**: app shell: mounts `DualPaneExplorer`, owns top-level dialog visibility (`$state`) and the
  `explorerRef` handle, owns the keydown / context-menu handlers and onboarding / licensing gating, and orchestrates the
  extracted listener setup (`setupTauriEventListeners` calls into `listener-setup.ts`, then wires MCP + the event
  bridges).
- **`listener-setup.ts`**: the menu, MCP-dialog, and window-focus Tauri listener wiring, extracted out of the component
  to keep it focused on reactive `$state`. A plain `.ts` (no runes), so it can't hold `$state` directly: state crosses
  the boundary through `ListenerSetupContext` (getter functions for reads, setter callbacks for writes), which keeps the
  moved closures reading LIVE reactive values instead of a stale capture. Exports `setupMenuListeners`,
  `setupDialogListeners`, `setupWindowFocusListener`, and `makeListenTauri` (the cleanup-array-bound `listenTauri` the
  component also passes into `setupMcpListeners`). Every registered unlisten is pushed onto the component-owned
  `unlistenFns` array (folded `unlistenExecuteCommand` / `unlistenWindowFocus` into it too), so the `onDestroy` loop
  tears them all down — important for HMR, which otherwise stacks duplicate listeners on reload. The keydown handler,
  licensing init, and onboarding gating stay in the component because they read/write `$state` directly.
- **`command-dispatch.ts`**: `handleCommandExecute<K extends CommandId>(commandId, ctx, ...args)`, the dispatch core.
  Referenced from `$lib/commands` and many tests.
- **`command-dispatch-context.ts`**: `CommandDispatchContext` + `CommandDispatchDialogs`: the per-call context (the
  `getExplorer()` getter, dialog-visibility callbacks, `dispatch`). A leaf so handler modules and the core import it
  without a cycle; re-exported from `command-dispatch.ts`.
- **`command-handlers/`**: family-grouped handler modules (`app-dialog`, `view`, `pane`, `tab`, `nav`, `sort`, `file`,
  `clipboard`, `selection`, `misc`) plus `types.ts` (the `CommandHandler` / `CommandHandlerRecord` / `DispatchExemptId`
  seam) and `index.ts` (assembles the one `commandHandlers` record).
- **`dispatch-dedup.ts`**: cross-source double-fire guard. `markDispatchSource('keyboard' | 'menu')` tags a dispatch;
  the core drops the same command arriving from the OTHER source within 300 ms (the macOS menu-accelerator +
  webview-keydown double fire). Same-source repeats and untagged dispatches (palette, MCP) always pass. Unit-tested with
  injectable time.
- **`mouse-nav.ts`**: `navCommandForMouseButton(button)`, the pure map from a pointer's X1/X2 side buttons to `nav.back`
  / `nav.forward`. Unit-tested; see § Mouse back / forward buttons for the wiring.
- **`explorer-api.ts`**: `ExplorerAPI`, the contract `DualPaneExplorer` exposes upward; shared by `+page.svelte`,
  `command-dispatch.ts`, and `mcp-listeners.ts` so none import the component directly.
- **`mcp-listeners.ts`**: `setupMcpListeners(ctx)`, the transport adapter onto the command bus.

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
per family in `command-handlers/CLAUDE.md` § "The exempt families". The core silently no-ops these after the preamble.

## Capability guard

`blockedByCapabilities` reads `capabilitiesFor(getFocusedPaneVolumeId())`, the same source the F-bar `disabled` flags
and the context menu read (invariant A6). F-bar buttons and context menus disable visibly at the source; this guard
catches the shortcut-driven path that bypasses the UI. The toast (`SEARCH_RESULTS_NOT_A_FOLDER_TOAST`) fires only for
the `search-results` kind: a `network` pane has the same `false` destination caps, but those ops are unreachable through
its UI and the shortcut path falls through silently to the explorer no-op, so network keeps its prior silence. The
capability decides the block; the kind decides the toast.

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
  `explorerRef.navigate({ pane, to: { location }, source: 'mcp' })` and branches on the typed `NavigateResult`: a
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

A `mcp-key` GoBack/GoForward routes through the bus (`nav.back`/`nav.forward`), whose handlers call
`explorerRef.navigate({ pane, to: { history: 'back' | 'forward' }, source: 'user' })`, same shape as `nav.parent`
(`to: { history: 'parent' }`). Every other key stays a `sendKeyToFocusedPane` passthrough (invariant P2).

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

## Native-menu and input-focus interactions

These three CLAUDE.md gotchas share the same root: a native macOS menu accelerator fires before the webview keydown, so
the dispatch path can't rely on the keydown bail.

- **⌘A (`selection.selectAll`).** Intercepted as a menu accelerator before the webview. The handler routes to
  `active.select()` when a `<input>` / `<textarea>` is focused, otherwise delegates to
  `explorerRef.handleSelectionAction('selectAll')`. The keydown bail doesn't help; the menu fires first.
- **`edit.paste` into a text input.** Reads via the `readClipboardText` Rust IPC, then writes with
  `document.execCommand('insertText')`. `navigator.clipboard.readText()` would surface a WebKit "Paste" confirmation the
  user must click each time, so it's avoided.
- **`view.showHidden` is local-first.** Flips frontend state via `explorerRef.toggleHiddenFiles()` synchronously, then
  pushes the check state to the native menu fire-and-forget. Routing the toggle through Rust adds an IPC + event hop and
  flaked the hidden-file E2E under slow-lane load.

## Off-bus test and debug hooks

- **E2E drop hook.** `+page.svelte` registers an `e2e-trigger-file-drop` listener gated on `getAppMode() === 'e2e'` (set
  by `CMDR_E2E_MODE=1`, never true in prod). It forwards to `explorerRef.triggerFileDrop`, which delegates to the drag
  controller's `handleFileDrop`, the same seam the live `onDragDropEvent` 'drop' branch runs. Real OS drag can't be
  synthesized in Playwright, so the harness emits this event to exercise drop handling end to end (shared destination
  guard, source-volume resolution, transfer dialog). See `test/e2e-playwright/DETAILS.md` § "Transfer-dialog counters +
  programmatic drop entry".
- **Debug-error listeners.** The `debug-inject-error` / `debug-reset-error` / `debug-trigger-transfer-error` listeners
  (gated by `import.meta.env.DEV`) call `explorerRef.injectError` / `resetError` / `triggerTransferError` directly. They
  inject test state from the debug window's error-pane preview. Routing them through the bus would pollute the
  `CommandId` union with dev-only ids for zero gain. See `lib/file-explorer/DETAILS.md` § "Debug preview".
