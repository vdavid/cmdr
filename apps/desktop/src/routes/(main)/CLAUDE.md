# Main route

The app orchestrator. Mounts the dual-pane explorer, owns top-level dialogs (command palette, search, selection,
onboarding, licensing), and routes commands + MCP events into the explorer via a typed API. Up:
[`../../../CLAUDE.md`](../../../CLAUDE.md) (desktop app), sibling: [`../viewer/CLAUDE.md`](../viewer/CLAUDE.md).

## Module map

- **`+layout.svelte`** / **`+page.svelte`**: main-window layout (gates children on `settingsReady`) and the app shell
  (mounts `DualPaneExplorer`, owns dialog visibility + the `explorerRef` handle, wires keydown / menu listeners).
- **`command-dispatch.ts`** + **`command-handlers/`**: the dispatch core (preamble, then a flat `commandHandlers`-record
  lookup) and the family-grouped handler modules. Context types in `command-dispatch-context.ts`. See
  [`command-handlers/CLAUDE.md`](command-handlers/CLAUDE.md).
- **`mcp-listeners.ts`**, **`explorer-api.ts`**, **`dispatch-dedup.ts`**: the MCP transport adapter, the `ExplorerAPI`
  contract, and the cross-source double-fire guard. Per-file detail in DETAILS.md.

## Must-knows

- **`ExplorerAPI` is the only handle.** `+page.svelte` passes a `getExplorer()` getter (not the `explorerRef`) into
  `command-dispatch.ts` and `mcp-listeners.ts`, so they read the current ref each call (no stale `undefined`; HMR can
  swap the instance). HMR can land any event with `explorerRef === undefined`; every dispatch site uses `explorerRef?.…`
  and listeners bail silently or reply `ok: false`.
- **Adding a user-facing action** needs the id in `COMMAND_IDS`, a `command-registry.ts` entry, and a handler in the
  right `command-handlers/` family. The `commandHandlers` record is keyed by `Exclude<CommandId, DispatchExemptId>`, so
  a missing handler is a COMPILE error; an intentionally handlerless command goes in `DISPATCH_EXEMPT_IDS`
  (`command-handlers/types.ts`) with a reason. See `$lib/commands/CLAUDE.md` and `command-handlers/CLAUDE.md`. Branch on
  the `CommandId`, never the label.
- **❌ Never add a handler for a per-keystroke `nav.*` id.** A keystroke is transport, not a command; routing an arrow
  through a registry lookup + log + breadcrumb IPC per keypress is a P2 perf regression. These ids are exempt by design.
- **Dialog state lives in `+page.svelte`, not in dispatch.** `command-dispatch.ts` only flips visibility via
  `ctx.dialogs.showXxx(...)` callbacks (write-only); it never reads dialog state back.
- **Text-region intercept (⌘C / ⌘A).** `handleCommandExecute` short-circuits via `handleTextRegionShortcut` BEFORE the
  `log.info` line for `edit.copy` / `selection.selectAll` when the selection sits inside an `.error-pane` or
  `[data-text-region]`; otherwise copying error text triggers file-scope copy. Opt new components in with
  `data-text-region`.
- **Capability guard.** `blockedByCapabilities` reads the focused pane's `VolumeCapabilities` and toasts + bails for
  destination-side ops the pane can't satisfy (`edit.paste` / `pasteAsMove` without `canPasteInto`, `file.newFolder` /
  `newFile` without `canCreateChild`, `file.rename` without `canRenameInPlace`). Use capabilities, not a
  `volumeId === 'search-results'` compare; the toast fires ONLY for the `search-results` kind. Pre-dispatch guard in the
  core. Rationale in DETAILS.md § Capability guard.
- **`mcp-listeners.ts` validate-parses each `mcp-*` payload** via small pure parsers (`parsePane`, `parseSortColumn`, …;
  a malformed value collapses to `undefined` and the listener skips). Dispatch ids are typed `CommandId` consts so a
  registry rename breaks compilation here. Two events stay adapter-local (off the bus): `mcp-nav-to-path` (awaits the
  typed `NavigateResult`) and the `mcp-response` round-trips. See DETAILS.md § MCP transport before touching it.
- **Don't pile new state into `+page.svelte`** (it's >900 lines, `file-length`-flagged): extract a
  `setupXxxListeners(ctx)` module. New commands get a handler in a `command-handlers/` family, NOT a branch in the small
  core. `handleTextRegionShortcut` and `blockedByCapabilities` stay in the core.
- **E2E and debug listeners stay off the bus (intentional, not unfinished).** `e2e-trigger-file-drop` (gated on
  `getAppMode() === 'e2e'`) and the `import.meta.env.DEV` `debug-*-error` listeners call `explorerRef.*` directly:
  test/dev hooks with no registry entry, palette, or shortcut. Don't "finish the migration." See DETAILS.md.

## Gotchas

- **Don't remove the `{#if settingsReady}` wrapper** in `+layout.svelte`, and don't move setting-reading work ahead of
  the flag. The page subtree mounts only after `initReactiveSettings()` + `initSettingsApplier()` resolve; file-explorer
  components read `getSetting()` synchronously at mount, and a pre-init read returns registry defaults (which can be
  pushed back to the backend as if chosen). See `settings-store.ts` § `getSetting`.
- **⌘A is a native menu accelerator**, intercepted before the webview, so `selection.selectAll` routes to
  `active.select()` for an `<input>` / `<textarea>` before delegating to
  `explorerRef.handleSelectionAction('selectAll')`. The keydown bail doesn't help; the menu fires first.
- **`edit.paste` inside a text input reads via the `readClipboardText` Rust IPC +
  `document.execCommand('insertText')`.** Don't switch to `navigator.clipboard.readText()`: it surfaces a WebKit "Paste"
  confirmation the user must click.
- **`view.showHidden` is local-first**: flip frontend state via `explorerRef.toggleHiddenFiles()` synchronously, then
  push the check state to the native menu fire-and-forget. Routing the toggle through Rust adds an IPC + event hop and
  flaked the hidden-file E2E under slow-lane load.

Full details: [DETAILS.md](DETAILS.md).
