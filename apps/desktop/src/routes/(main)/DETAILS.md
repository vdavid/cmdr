# Main route details

Depth and rationale for the app orchestrator. `CLAUDE.md` holds the must-knows; this file holds the full mechanism.

## File map

- **`+layout.svelte`**: main-window layout (updater, settings applier, AI state init, MCP shortcuts/settings bridges,
  toast container, crash + MTP + error-report dialogs).
- **`+page.svelte`**: app shell: mounts `DualPaneExplorer`, owns top-level dialog visibility (`$state`) and the
  `explorerRef` handle, wires keydown / context-menu / menu-event listeners, runs onboarding gating.
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

- **`mcp-nav-to-path`** bypasses the bus entirely. The adapter calls
  `explorerRef.navigate({ pane, to: { path }, source: 'mcp' })` and branches on the typed `NavigateResult`: a
  `'refused'` result forwards `result.reason.message` byte-identically as the `mcp-response` error; a `'started'` result
  awaits `result.settled` before replying `ok: true`. The bus dispatch is fire-and-forget and can't surface this
  round-trip.
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
