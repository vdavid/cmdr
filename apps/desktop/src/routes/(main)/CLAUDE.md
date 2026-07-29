# Main route

The app orchestrator: mounts the dual-pane explorer, owns top-level dialogs, and routes commands + MCP events into it
via a typed API. Up: `apps/desktop/CLAUDE.md`, sibling: `../viewer/CLAUDE.md`.

## Module map

- **`+layout.svelte`** / **`+page.svelte`**: layout (gates children on `settingsReady`) and the app shell (mounts
  `DualPaneExplorer`; owns dialog visibility, the `explorerRef` handle, keydown, onboarding / licensing).
- **`listener-setup.ts`**: menu, MCP-dialog, and window-focus Tauri listeners (plain `.ts`, no runes); state crosses via
  a `ListenerSetupContext` of getters + setters.
- **`command-dispatch.ts`** + **`command-handlers/`**: the dispatch core (preamble, then a flat `commandHandlers`-record
  lookup) and the family-grouped handlers; context types in `command-dispatch-context.ts`. Also
  `command-handlers/CLAUDE.md`.
- **`mcp-listeners.ts`**, **`explorer-api.ts`**, **`dispatch-dedup.ts`**, **`global-keydown.ts`**,
  **`global-contextmenu.ts`**: MCP transport adapter, `ExplorerAPI` contract, cross-source double-fire guard, pure
  keydown and right-click decisions.

## Must-knows

- **`ExplorerAPI` is the only handle.** Pass the `getExplorer()` getter, never the bare `explorerRef`, so each call
  reads the live ref. HMR can swap or null it: `explorerRef?.…` everywhere; listeners bail or reply `ok: false`.
- **Adding a user-facing action** needs the id in `COMMAND_IDS`, a `command-registry.ts` entry, and a
  `command-handlers/` handler (missing = COMPILE error; handlerless ids go in `DISPATCH_EXEMPT_IDS`). Branch on the
  `CommandId`, never the label.
- **❌ Never add a handler for a per-keystroke `nav.*` id**: a registry lookup + log + breadcrumb IPC per keypress is a
  P2 perf regression; exempt by design.
- **Dialog state lives in `+page.svelte`, not in dispatch.** `command-dispatch.ts` only flips visibility via write-only
  `ctx.dialogs.showXxx(...)` callbacks, never reads it back.
- **Text-region intercept (⌘C / ⌘A).** `handleTextRegionShortcut` short-circuits `edit.copy` / `selection.selectAll`
  inside `.error-pane` or `[data-text-region]`, so copying error text doesn't copy files. Opt in with
  `data-text-region`.
- **Capability guard.** `blockedByCapabilities` (pre-dispatch) bails for destination-side ops the focused pane's
  `VolumeCapabilities` can't satisfy. Gate on capabilities, never a `volumeId === 'search-results'` compare. DETAILS §
  Capability guard.
- **`mcp-listeners.ts` validate-parses each `mcp-*` payload** and dispatches typed `CommandId` consts, so a registry
  rename breaks compilation here; `mcp-nav-to-path` and `mcp-response` round-trips stay off the bus (DETAILS § MCP
  transport).
- **New Tauri listener wiring goes in `listener-setup.ts`, not `+page.svelte`** (`file-length`-flagged): thread `$state`
  through `ListenerSetupContext` (getters/setters; shared `unlistenFns` for HMR cleanup). Runes-touching logic
  (onboarding, licensing) can't move.
- **E2E and debug listeners stay off the bus by design.** `e2e-trigger-file-drop` and the DEV `debug-*-error` listeners
  call `explorerRef.*` directly: gated hooks, no registry entry. Don't "finish the migration" (DETAILS § Off-bus hooks).

## Gotchas

- **Don't remove the `{#if settingsReady}` wrapper** in `+layout.svelte`, and don't read settings ahead of the flag: the
  subtree mounts only after `initReactiveSettings()` + `initSettingsApplier()` resolve, and a pre-init `getSetting()`
  returns registry defaults that can get hot-applied to the backend as if chosen (`settings-store.ts`).
- **Native-menu accelerators fire before the webview keydown** (mechanism in DETAILS § Native-menu and input-focus
  interactions):
  - **A focused text input owns the text-editing family** (`edit.cut` / `edit.copy` / `edit.paste` /
    `selection.selectAll`): ⌘A routes to `active.select()`, and with a modal open `global-keydown.ts` still dispatches
    these four, whose `preventDefault` stops WebKit ALSO pasting (⌘V landed twice per dialog). ❌ Don't widen the
    family, gate `execute-command` on modal state, or hardcode ⌘V.
  - **`edit.paste` into a text input**: ❌ keep the `readClipboardText` IPC (`navigator.clipboard.readText()` surfaces a
    WebKit "Paste" confirmation to click). The capability guard exempts it.
  - **`view.showHidden` is local-first**: ❌ don't route it through Rust; the extra hop flaked the E2E.
- **Right-click is Cmdr's except in text fields.** The document `contextmenu` listener is CAPTURE-phase: on an editable
  TARGET (not `activeElement`; the field may be unfocused) it stops propagation for WebKit's editing menu, else
  `preventDefault()`s so rows/tabs keep their own. DETAILS § Right-click ownership.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
