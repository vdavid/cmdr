# Main route

The app orchestrator. Mounts the dual-pane explorer, owns top-level dialogs (command palette, search, selection,
onboarding, licensing), and routes commands + MCP events into the explorer via a typed API. Up:
[`../../../CLAUDE.md`](../../../CLAUDE.md) (desktop app), sibling: [`../viewer/CLAUDE.md`](../viewer/CLAUDE.md).

## File map

| File                  | Purpose                                                                                                                                                                                                                                                                                                     |
| --------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `+layout.svelte`      | Main-window layout: updater, settings applier, AI state init, MCP shortcuts/settings bridges, toast container, crash + MTP + error-report dialogs                                                                                                                                                           |
| `+page.svelte`        | App shell: mounts `DualPaneExplorer`, owns top-level dialog visibility ($state) and the `explorerRef` handle, wires keydown / context-menu / menu-event listeners, runs onboarding gating                                                                                                                   |
| `command-dispatch.ts` | `handleCommandExecute<K extends CommandId>(commandId, ctx, ...args)`: the single typed switch that turns `CommandId`s (palette, keyboard, menu, F-bar, MCP) into `ExplorerAPI` calls or dialog toggles. Arg-carrying ids take a typed payload. Load-bearing: referenced from `$lib/commands` and many tests |
| `explorer-api.ts`     | `ExplorerAPI` interface — the contract `DualPaneExplorer` exposes upward. Shared by `+page.svelte`, `command-dispatch.ts`, `mcp-listeners.ts` so none of them import the component directly                                                                                                                 |
| `mcp-listeners.ts`    | `setupMcpListeners(ctx)`: thin transport adapter — validate-parses each `mcp-*` Tauri payload into typed `CommandArgs` and `dispatch`es it through the bus. No business logic; the round-trip callers reply via `mcp-response`                                                                              |

## Conventions

**`ExplorerAPI` is the only handle.** `+page.svelte` holds `explorerRef: ExplorerAPI | undefined` and passes a
`getExplorer()` getter (not the ref) into both `command-dispatch.ts` and `mcp-listeners.ts`. The getter pattern lets
those modules read the current ref each call without capturing a stale `undefined` from before mount, and lets HMR swap
the explorer instance underneath.

**Adding a user-facing action.** Add the id to the `COMMAND_IDS` tuple in `$lib/commands/command-ids.ts`, register the
command in `command-registry.ts` (id, label, scope, palette visibility, default shortcut), and add a `case` in
`handleCommandExecute`. The registry is what the palette and shortcuts see; the dispatch is what runs. Skipping any side
gives a compile error or an invisible / inert command. See `$lib/commands/CLAUDE.md` § "Adding a command" for the full
checklist (arg shapes, the menu-id drift test, the palette-set pin). The AGENTS.md "no string-matching" rule applies —
branch on the `CommandId`, never on the label.

**Dialog state lives in `+page.svelte`, not in dispatch.** `command-dispatch.ts` only flips visibility via
`ctx.dialogs.showXxx(...)` callbacks. The page owns the `$state` flags + props for command palette, search dialog,
selection dialog, about, license-key, and onboarding re-entry. Dispatch never reads dialog state back — it's write-only
from there.

**Text-region intercept (⌘C / ⌘A).** `handleCommandExecute` short-circuits via `handleTextRegionShortcut` BEFORE the
`log.info(commandId)` line for `edit.copy` / `selection.selectAll` when the selection sits inside an `.error-pane` or
`[data-text-region]`. Without this, copying error text would log `FE:user-action edit.copy` and trigger file-scope copy.
Opt new components into the routing by adding `data-text-region`.

**Capability guard.** `blockedByCapabilities(commandId, explorer)` reads the focused pane's `VolumeCapabilities`
(`capabilitiesFor(getFocusedPaneVolumeId())`) and toasts + bails for destination-side ops the pane can't satisfy:
`edit.paste` / `edit.pasteAsMove` on `!canPasteInto`, `file.newFolder` / `file.newFile` on `!canCreateChild`,
`file.rename` on `!canRenameInPlace` (invariant A6 — capabilities, not a `volumeId === 'search-results'` string compare;
the same source the F-bar `disabled` flags and the context menu read). F-bar buttons and context menus disable visibly
at the source; this catches the shortcut-driven path that bypasses the UI. The toast
(`SEARCH_RESULTS_NOT_A_FOLDER_TOAST`) fires ONLY for the `search-results` kind: a `network` pane has the same `false`
destination caps, but those ops are unreachable through its UI and the shortcut path historically fell through silently
to the explorer no-op, so network keeps its prior silence (the capability decides the block; the kind decides the toast
— PR3).

**Per-command logging.** Each successful dispatch emits one `log.info(commandId)` (LogTape → fern → error-report
bundles) and one `record_breadcrumb` invoke (rolling manifest buffer). Both are best-effort; a failing breadcrumb must
not break the dispatch.

**`mcp-listeners.ts` is a transport adapter onto the command bus.** Every `mcp-*` event (except the two below)
validate-parses its raw payload into the command's typed `CommandArgs` — each discriminant string is whitelist-checked
by a small pure parser (`parsePane`, `parseSortColumn`, `parseTabAction`, …; unit-tested in `mcp-listeners.test.ts`), a
malformed value collapses to `undefined` and the listener skips the dispatch — then calls `ctx.dispatch(id, args)`. No
`as {...}` payload casts survive. The dispatch ids are typed `CommandId` consts (not literals) so
`cmdr/no-raw-command-dispatch` (A3) stays satisfied and a registry rename breaks compilation here. `ctx.dispatch` is
`+page.svelte`'s `handleCommandExecute` (bound with its context), so MCP events get the same preamble (per-command
`log.info` + `record_breadcrumb` + the search-results guard) as the keyboard / palette / menu paths — a deliberate,
uniform telemetry gain.

The per-pane MCP commands (`sort.set`, `selection.mcpSelect`, `cursor.moveTo`, `cursor.scrollTo`, `volume.selectByName`,
`tab.mcpAction`, `pane.refresh`, `dialog.confirm`, `nav.openUnderCursor`, and the optional-arg
`file.copy`/`file.move`/`file.delete`) exist because the focused-pane registry commands can't target a specific pane /
tab / option. They're all `showInPalette: false`. `view.setMode` is shared with the native-menu `view-mode-changed`
path; its `fromMenu` flag picks `setViewModeFromMenu` (menu, skip `pushViewMenuState`) vs `setViewMode` (MCP, push it).

**Two exceptions stay adapter-local (off the bus this phase).**

- **`mcp-nav-to-path`** bypasses the bus entirely. The adapter calls
  `explorerRef.navigate({ pane, to: { path }, source: 'mcp' })` directly and branches on the typed `NavigateResult`: a
  `'refused'` result forwards `result.reason.message` byte-identically as the `mcp-response` error (L12); a `'started'`
  result awaits `result.settled` (the listing completes) before replying `ok: true`. The bus dispatch is fire-and-forget
  and can't surface this round-trip, so it stays adapter-local.
- **`mcp-response` round-trips** (`mcp-open-under-cursor`, `mcp-move-cursor`): the bus dispatches the `void`-returning
  intent; the adapter owns the `requestId` correlation and the `emit('mcp-response', { requestId, ok, error? })` reply.
  It **awaits** the dispatch's promise so the ack fires only after the action settles (the backend has an ack timeout) —
  the dispatch case `await`s the underlying `openItemUnderCursor` / `moveCursor`, and an exception propagates to the
  adapter's `try/catch`, which replies `ok: false`. HMR can land these with no explorer; they reply `ok: false` rather
  than crashing.

A `mcp-key` GoBack/GoForward routes through the bus (`nav.back`/`nav.forward`); those dispatch cases call
`explorerRef.navigate({ pane, to: { history: 'back' | 'forward' }, source: 'user' })`, same as `nav.parent`
(`to: { history: 'parent' }`). Every other key stays a `sendKeyToFocusedPane` passthrough — a keystroke is transport,
not a command, so it never rides the bus (invariant P2).

**Debug-error listeners stay off the bus (intentional, not unfinished).** The three `debug-inject-error` /
`debug-reset-error` / `debug-trigger-transfer-error` listeners in `+page.svelte` (gated by `import.meta.env.DEV`) call
`explorerRef.injectError` / `resetError` / `triggerTransferError` directly. They inject test state from the debug
window's error-pane preview; they are NOT user commands (no registry entry, no palette, no shortcut), so routing them
through the bus would pollute the `CommandId` union with dev-only ids for zero gain. Leave them as direct calls — don't
"finish the migration." See `lib/file-explorer/CLAUDE.md` § "Debug preview" for the cross-window flow.

## Gotchas

- **`+page.svelte` is >900 lines and `command-dispatch.ts` is >670 lines, both flagged by `file-length`.** Don't pile
  new state into the page — extract another `setupXxxListeners(ctx)` module like `mcp-listeners.ts`. Don't pile new
  branches into the dispatcher's switch — group related ids and lift their bodies into small helpers (`showZoomToast`,
  `handleTextRegionShortcut`, `blockedByCapabilities` are the pattern).
- **⌘A is a native menu accelerator.** macOS intercepts it before the webview, so the `selection.selectAll` branch
  routes to `active.select()` when the focused element is an `<input>` / `<textarea>` BEFORE delegating to
  `explorerRef.handleSelectionAction('selectAll')`. The keydown bail in `+page.svelte` doesn't help here — the menu
  fires first.
- **HMR can land an event with `explorerRef === undefined`.** Every dispatch site uses `explorerRef?.…` and every MCP
  listener bails silently when `getExplorer()` returns `undefined` (backend's request timeout handles the missing
  reply). Adding a new MCP round-trip? Follow `mcp-nav-to-path`: emit `mcp-response` with `ok: false` and an error
  string instead of crashing.
- **`edit.paste` inside a text input bypasses the WebKit clipboard prompt.** It reads via the `readClipboardText` Rust
  IPC and `document.execCommand('insertText', false, text)`. Don't switch to `navigator.clipboard.readText()` — it
  surfaces a WebKit "Paste" confirmation button the user has to click.
- **`view.showHidden` is local-first.** Flip frontend state via `explorerRef.toggleHiddenFiles()` synchronously, then
  push the new check state to the native menu fire-and-forget. Routing the toggle through Rust (a `settings-changed`
  emit plus an FE listener) adds an IPC + event hop and flaked the `toggles hidden file visibility` E2E ~1/25 runs under
  slow-lane load.
