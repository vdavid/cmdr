# Main route

The app orchestrator: mounts the dual-pane explorer, owns top-level dialogs, and routes commands + MCP events into it
via a typed API. Up: `apps/desktop/CLAUDE.md`, sibling: `../viewer/CLAUDE.md`.

## Module map

- `+layout.svelte` / `+page.svelte`: layout (gates children on `settingsReady`, mounts the quit prompt) and the app
  shell (mounts `DualPaneExplorer`; owns dialog visibility, the `explorerRef` handle, keydown, licensing).
- `command-dispatch.ts` + `command-handlers/` are the dispatch core and its family-grouped handlers; `listener-setup.ts`
  holds the menu / MCP-dialog / window-focus listeners; `window-services.ts` starts and stops every subscription the
  window holds for its lifetime.
- Supporting pure modules: `startup-gates.ts`, `mcp-listeners.ts`, `mcp-nav-landing.ts`, `explorer-api.ts`,
  `dispatch-dedup.ts`, `dialog-command-gate.ts`, `global-keydown.ts`, `global-contextmenu.ts`.

## Must-knows

- **`ExplorerAPI` is the only handle.** Pass the `getExplorer()` getter, ❌ never the bare `explorerRef`, so each call
  reads the live ref. HMR can swap or null it: `explorerRef?.…` everywhere; listeners bail or reply `ok: false`.
- **Adding a user-facing action** needs the id in `COMMAND_IDS`, a `command-registry.ts` entry, and a
  `command-handlers/` handler (missing = COMPILE error; handlerless ids go in `DISPATCH_EXEMPT_IDS`). Branch on the
  `CommandId`, ❌ never the label.
- **❌ Never add a handler for a per-keystroke `nav.*` id**: a registry lookup + log + breadcrumb IPC per keypress is a
  P2 perf regression. Exempt by design.
- **`$state` lives in `+page.svelte`; logic leaves through a context of setters and GETTERS.** Dialogs flip via
  write-only `ctx.dialogs.showXxx(...)`, new listeners go in `listener-setup.ts`, startup decisions in
  `startup-gates.ts`. ❌ Never capture a `$state` value; `isOnboardingVisible()` reads live.
- **The old-macOS notice is `topmost` AND rendered after `<OnboardingWizard>`**: that order is the only reason it clears
  the wizard's overlay. ❌ Don't move it or drop the prop. DETAILS § Startup gates.
- **One dispatcher per road** (`dispatchers.*`), ❌ never shared. DETAILS § The dialog gate.
- **`dialogsOnScreen()` reads the `open-dialogs` INVENTORY, ❌ never a list of `show*` booleans**: a hand list misses
  dialogs, and each miss lets a bare key (Tab, Space, F5) fire behind one. DETAILS § What `dialogsOnScreen()` is made
  of.
- **Text-region intercept (⌘C / ⌘A)**: `handleTextRegionShortcut` short-circuits `edit.copy` / `selection.selectAll`
  inside `.error-pane` or `[data-text-region]`, so copying error text doesn't copy files.
- **Gate on capabilities, ❌ never a `volumeId` compare**: `blockedByCapabilities` bails pre-dispatch for
  destination-side ops the focused pane can't satisfy.
- **`mcp-listeners.ts` validate-parses each `mcp-*` payload** and dispatches typed `CommandId` consts, so a registry
  rename breaks compilation here. Its `nav_to_path` ack reports the pane's LANDING, ❌ never `settled` alone
  (`mcp-nav-landing.ts`).
- **E2E and debug listeners stay off the bus by design** (`e2e-trigger-file-drop`, the DEV `debug-*-error` ones call
  `explorerRef.*` directly). Don't "finish the migration". DETAILS § Off-bus hooks.
- **`foreground-operation` is the one inbound channel from another WINDOW** (the queue's Show button). ❌ Never route it
  through the bus: a bus dispatch is fire-and-forget and would drop the verdict. DETAILS § Cross-window.

## Gotchas

- **❌ Never cancel work from a window-teardown hook**: the quit gate owns stopping operations, and its
  `initQuitPrompt()` must stay SYNCHRONOUS at the top of `onMount`. `$lib/quit/CLAUDE.md`.
- **Don't remove the `{#if settingsReady}` wrapper** in `+layout.svelte`, and don't read settings ahead of the flag: a
  pre-init `getSetting()` returns registry defaults that can get hot-applied to the backend as if chosen.
- **Native-menu accelerators fire before the webview keydown**, so a focused text input owns `edit.cut` / `edit.copy` /
  `edit.paste` / `selection.selectAll`. ❌ Don't refuse `execute-command` wholesale or hardcode ⌘V. DETAILS §
  Native-menu and input-focus interactions.
- **Right-click is Cmdr's except in text fields**, and the predicate is the event TARGET, ❌ never focus. DETAILS §
  Right-click ownership.

Architecture, flows, and decisions: `DETAILS.md`. Read it before any non-trivial work here: editing, planning,
reorganizing, or advising.
