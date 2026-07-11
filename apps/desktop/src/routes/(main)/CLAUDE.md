# Main route

The app orchestrator. Mounts the dual-pane explorer, owns top-level dialogs, and routes commands + MCP events into the
explorer via a typed API. Up: [`../../../CLAUDE.md`](../../../CLAUDE.md), sibling:
[`../viewer/CLAUDE.md`](../viewer/CLAUDE.md).

## Module map

- **`+layout.svelte`** / **`+page.svelte`**: main-window layout (gates children on `settingsReady`) and the app shell
  (mounts `DualPaneExplorer`, owns dialog visibility + the `explorerRef` handle + keydown + onboarding / licensing, and
  orchestrates `listener-setup.ts`).
- **`listener-setup.ts`**: menu, MCP-dialog, and window-focus Tauri listener setup (plain `.ts`, no runes); state
  crosses via a `ListenerSetupContext` of getters + setters. See DETAILS.md.
- **`command-dispatch.ts`** + **`command-handlers/`**: the dispatch core (preamble, then a flat `commandHandlers`-record
  lookup) and the family-grouped handlers; context types in `command-dispatch-context.ts`. See
  [`command-handlers/CLAUDE.md`](command-handlers/CLAUDE.md).
- **`mcp-listeners.ts`**, **`explorer-api.ts`**, **`dispatch-dedup.ts`**: MCP transport adapter, `ExplorerAPI` contract,
  cross-source double-fire guard. Per-file detail in DETAILS.md.

## Must-knows

- **`ExplorerAPI` is the only handle.** `+page.svelte` passes a `getExplorer()` getter (not the bare `explorerRef`) into
  `command-dispatch.ts` and `mcp-listeners.ts`, so each call reads the current ref. HMR can swap the instance or null
  it, so every site uses `explorerRef?.…`; listeners bail or reply `ok: false`.
- **Adding a user-facing action** needs the id in `COMMAND_IDS`, a `command-registry.ts` entry, and a
  `command-handlers/` handler (a missing one is a COMPILE error; handlerless ones go in `DISPATCH_EXEMPT_IDS`). Branch
  on the `CommandId`, never the label.
- **❌ Never add a handler for a per-keystroke `nav.*` id.** Per-keypress registry lookup + log + breadcrumb IPC is a P2
  perf regression; exempt by design.
- **Dialog state lives in `+page.svelte`, not in dispatch.** `command-dispatch.ts` only flips visibility via write-only
  `ctx.dialogs.showXxx(...)` callbacks; never reads it back.
- **Text-region intercept (⌘C / ⌘A).** `handleTextRegionShortcut` short-circuits `edit.copy` / `selection.selectAll`
  (before any logging) when the selection sits inside `.error-pane` or `[data-text-region]`, so copying error text
  doesn't trigger file-scope copy. Opt new components in with `data-text-region`.
- **Capability guard.** `blockedByCapabilities` (pre-dispatch, in the core) reads the focused pane's
  `VolumeCapabilities` and bails for destination-side ops the pane can't satisfy. Gate on capabilities, never a
  `volumeId === 'search-results'` compare. Detail: DETAILS.md § Capability guard.
- **`mcp-listeners.ts` validate-parses each `mcp-*` payload** and dispatches typed `CommandId` consts, so a registry
  rename breaks compilation here. `mcp-nav-to-path` and `mcp-response` round-trips stay off the bus; see DETAILS.md §
  MCP transport before touching it.
- **New Tauri listener wiring goes in `listener-setup.ts`, not `+page.svelte`** (which is `file-length`-flagged): thread
  `$state` through `ListenerSetupContext` (getters/setters; shared `unlistenFns` for HMR cleanup). Runes-touching logic
  (keydown, onboarding, licensing) can't move. New commands get a `command-handlers/` handler; only
  `handleTextRegionShortcut` and `blockedByCapabilities` belong in the core.
- **E2E and debug listeners stay off the bus (intentional, not unfinished).** `e2e-trigger-file-drop` and the
  `import.meta.env.DEV` `debug-*-error` listeners call `explorerRef.*` directly: gated test/dev hooks, no registry
  entry. Don't "finish the migration." See DETAILS.md § Off-bus test and debug hooks.

## Gotchas

- **Don't remove the `{#if settingsReady}` wrapper** in `+layout.svelte`, and don't move setting-reading work ahead of
  the flag. The subtree mounts only after `initReactiveSettings()` + `initSettingsApplier()` resolve; a pre-init
  `getSetting()` returns registry defaults that can get hot-applied to the backend as if chosen. See `settings-store.ts`
  § `getSetting`.
- **Native-menu accelerators fire before the webview keydown**, so these can't rely on the keydown bail and carry
  load-bearing constraints (mechanism in DETAILS.md § Native-menu and input-focus interactions):
  - **⌘A** routes to `active.select()` for a focused `<input>` / `<textarea>` before delegating to the explorer.
  - **`edit.paste` into a text input**: ❌ don't switch from `readClipboardText` IPC to
    `navigator.clipboard.readText()`, which surfaces a WebKit "Paste" confirmation the user must click.
  - **`view.showHidden` is local-first**: ❌ don't route the `explorerRef.toggleHiddenFiles()` toggle through Rust; the
    extra hop flaked the E2E.

Architecture, flows, and decisions: [DETAILS.md](DETAILS.md). Read it before any non-trivial work here: editing,
planning, reorganizing, or advising.
