# Main route

The app orchestrator. Mounts the dual-pane explorer, owns top-level dialogs, and routes commands + MCP events into the
explorer via a typed API. Up: `apps/desktop/CLAUDE.md`, sibling: `../viewer/CLAUDE.md`.

## Module map

- **`+layout.svelte`** / **`+page.svelte`**: main-window layout (gates children on `settingsReady`) and the app shell
  (mounts `DualPaneExplorer`, owns dialog visibility + the `explorerRef` handle + keydown + onboarding / licensing).
- **`listener-setup.ts`**: menu, MCP-dialog, and window-focus Tauri listeners (plain `.ts`, no runes); state crosses via
  a `ListenerSetupContext` of getters + setters.
- **`command-dispatch.ts`** + **`command-handlers/`**: the dispatch core (preamble, then a flat `commandHandlers`-record
  lookup) and the family-grouped handlers; context types in `command-dispatch-context.ts`. See
  `command-handlers/CLAUDE.md`.
- **`mcp-listeners.ts`**, **`explorer-api.ts`**, **`dispatch-dedup.ts`**, **`global-keydown.ts`**: MCP transport
  adapter, `ExplorerAPI` contract, cross-source double-fire guard, pure keydown decision.

## Must-knows

- **`ExplorerAPI` is the only handle.** `+page.svelte` passes a `getExplorer()` getter (never the bare `explorerRef`)
  into `command-dispatch.ts` and `mcp-listeners.ts`, so each call reads the live ref. HMR can swap or null it: use
  `explorerRef?.…` everywhere; listeners bail or reply `ok: false`.
- **Adding a user-facing action** needs the id in `COMMAND_IDS`, a `command-registry.ts` entry, and a
  `command-handlers/` handler (missing = COMPILE error; handlerless ids go in `DISPATCH_EXEMPT_IDS`). Branch on the
  `CommandId`, never the label.
- **❌ Never add a handler for a per-keystroke `nav.*` id.** A registry lookup + log + breadcrumb IPC per keypress is a
  P2 perf regression; exempt by design.
- **Dialog state lives in `+page.svelte`, not in dispatch.** `command-dispatch.ts` only flips visibility via write-only
  `ctx.dialogs.showXxx(...)` callbacks; never reads it back.
- **Text-region intercept (⌘C / ⌘A).** `handleTextRegionShortcut` short-circuits `edit.copy` / `selection.selectAll`
  before any logging when the selection sits in `.error-pane` or `[data-text-region]`, so copying error text doesn't
  copy files. Opt in with `data-text-region`.
- **Capability guard.** `blockedByCapabilities` (pre-dispatch) bails for destination-side ops the focused pane's
  `VolumeCapabilities` can't satisfy. Gate on capabilities, never a `volumeId === 'search-results'` compare. Detail:
  DETAILS.md § Capability guard.
- **`mcp-listeners.ts` validate-parses each `mcp-*` payload** and dispatches typed `CommandId` consts, so a registry
  rename breaks compilation here. `mcp-nav-to-path` and `mcp-response` round-trips stay off the bus; read DETAILS.md §
  MCP transport first.
- **New Tauri listener wiring goes in `listener-setup.ts`, not `+page.svelte`** (`file-length`-flagged): thread `$state`
  through `ListenerSetupContext` (getters/setters; shared `unlistenFns` for HMR cleanup). Runes-touching logic
  (onboarding, licensing) can't move. Only `handleTextRegionShortcut` and `blockedByCapabilities` belong in the core.
- **E2E and debug listeners stay off the bus by design.** `e2e-trigger-file-drop` and the DEV `debug-*-error` listeners
  call `explorerRef.*` directly: gated hooks, no registry entry. Don't "finish the migration." See DETAILS.md § Off-bus
  test and debug hooks.

## Gotchas

- **Don't remove the `{#if settingsReady}` wrapper** in `+layout.svelte`, and don't read settings ahead of the flag. The
  subtree mounts only after `initReactiveSettings()` + `initSettingsApplier()` resolve; a pre-init `getSetting()`
  returns registry defaults that can get hot-applied to the backend as if chosen. See `settings-store.ts` §
  `getSetting`.
- **Native-menu accelerators fire before the webview keydown** (mechanism in DETAILS.md § Native-menu and input-focus
  interactions):
  - **A focused text input owns the text-editing family** (`edit.cut` / `edit.copy` / `edit.paste` /
    `selection.selectAll`): ⌘A routes to `active.select()`, and with a modal open `global-keydown.ts` still dispatches
    these four, whose `preventDefault` stops WebKit ALSO pasting (⌘V landed twice in every dialog). ❌ Don't widen the
    family, gate the `execute-command` listener on modal state, or hardcode ⌘V.
  - **`edit.paste` into a text input**: ❌ don't switch from `readClipboardText` IPC to
    `navigator.clipboard.readText()`, which surfaces a WebKit "Paste" confirmation the user must click. The capability
    guard exempts this case.
  - **`view.showHidden` is local-first**: ❌ don't route the `explorerRef.toggleHiddenFiles()` toggle through Rust; the
    extra hop flaked the E2E.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
