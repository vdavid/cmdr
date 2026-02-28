# Centralized command dispatch plan

## Intention

Keyboard shortcuts in Cmdr have two sources of truth, and they've already caused a real bug. The command registry
(`command-registry.ts`) holds ~60 commands with shortcut arrays, but those shortcuts are **display-only** -- they show
up in the command palette and tooltips, but don't actually dispatch keypresses. Real keyboard routing is scattered across
multiple components, each with its own hard-coded key matching:

- `handleFunctionKey()` in `DualPaneExplorer.svelte` -- F1-F8 via a `switch` statement
- `handleKeyDown()` in `DualPaneExplorer.svelte` -- Tab, Ctrl+Tab, Escape
- `FilePane.svelte` -- Backspace, Enter, F4, arrows, Space, Page Up/Down, Home/End
- `handleGlobalKeyDown()` in `+page.svelte` -- Cmd+Shift+P, Cmd+comma, Cmd+A suppression

The central dispatcher already exists: `handleCommandExecute()` in `+page.svelte` handles ~50 commands by ID. But it's
only called from the command palette UI and MCP events -- never from actual keypresses.

**The bug that proved this is broken:** When delete was added, `file.delete` got registered in the command registry with
shortcut `F8`, and a case was added to `handleCommandExecute`. The command palette worked. The FunctionKeyBar click
worked. But pressing F8 on the keyboard did nothing -- because nobody added it to `handleFunctionKey()`. Two sources of
truth, one got missed. The fix was trivial (add `case 'F8'` to the switch), but the architecture made the bug
inevitable.

The goal: make the registry the single source of truth for action shortcuts, so adding a new command means one registry
entry + one `handleCommandExecute` case, and the keyboard, command palette, and MCP all use the same path.

## Design decisions

### Two tiers: action commands vs navigation commands

Don't try to centralize everything. There are two fundamentally different kinds of keyboard commands:

**Tier 1 -- "Action" commands (~20):** Global actions that belong in the command palette. F-keys, Cmd+ combos, and
other operations that make sense regardless of exactly which element has focus. Examples: F5 Copy, F6 Move, F7 New
folder, F8 Delete, Cmd+Q Quit, Cmd+W Close tab, Cmd+comma Settings, Cmd+Shift+P Command palette.

**Tier 2 -- "Navigation" commands (~40):** Low-level, context-dependent keys. Arrow keys, Space, Enter, Backspace, Page
Up/Down, Home/End. These mean different things depending on context -- arrows navigate the file list in one context,
the volume chooser in another, and the command palette in a third. Centralizing these would require a full `when`-clause
system like VS Code's, where each keybinding has a boolean expression like `fileListFocused && !renameActive`. That's a
significant architecture investment with low payoff for Cmdr's current scope.

**Why this split?** Tier 1 commands are the ones where the "two sources of truth" bug actually hurts. They're also the
ones that benefit from user customization (rebinding F-keys, adding Cmd+ combos). Tier 2 commands are inherently local
-- their meaning depends on the component that has focus, and the component is the right place to handle them.

### Intercept-then-propagate: one top-level listener for Tier 1

The approach: a single `keydown` listener at the top level (in `+page.svelte`, where `handleGlobalKeyDown` already
lives) that intercepts keypresses **before** they reach individual components. The flow:

1. A keypress fires on `document`
2. The top-level listener formats it into a canonical shortcut string using `formatKeyCombo()` (already exists in
   `key-capture.ts`)
3. Look up the formatted string against the registry -- but only Tier 1 commands (identified by having
   `showInPalette: true`, or by a new flag if the categories don't align perfectly)
4. If a match is found, call `handleCommandExecute(commandId)` -- the exact same path the command palette and MCP use
5. Call `e.preventDefault()` and `e.stopPropagation()` so the event doesn't also trigger local handlers
6. If no match, let the event propagate normally to `DualPaneExplorer`, `FilePane`, and other components (Tier 2)

**Why `formatKeyCombo()` as the canonical format?** It's already the format used in the registry (`shortcuts` arrays)
and in the shortcuts store. No normalization layer needed -- the keypress format and the registry format are the same
string.

**Why intercept before propagation, not after?** If the top-level listener runs after component handlers (via event
bubbling), both the centralized and local handlers would fire for the same keypress. By intercepting first (capture
phase or `document`-level listener that runs before Svelte component handlers), we can `stopPropagation` cleanly when
a Tier 1 command matches. The current `handleGlobalKeyDown` already runs at the `document` level, so this extends
naturally.

### Using the shortcuts store, not just the registry

The top-level listener should look up shortcuts via `getEffectiveShortcuts()` from the shortcuts store, not directly
from the static `commands` array. This way, user customizations (from the Settings window or MCP) are automatically
respected. If a user rebinds F5 to Cmd+C, the centralized dispatch picks it up without any extra wiring.

The shortcuts store already merges custom shortcuts with registry defaults -- it returns the effective binding for any
command ID. The lookup just needs to go the other direction: given a key combo string, find which command (if any) it's
bound to. This is a reverse index: `shortcutString -> commandId`. Build it on startup and rebuild when shortcuts change
(the store already has a `onShortcutChange` listener mechanism).

### What about scope/context for Tier 1?

Most Tier 1 commands are truly global -- F-keys, Cmd+Q, Cmd+W, Cmd+T work the same regardless of what's focused. The
few that are context-sensitive (like F2 Rename only making sense when a file list is focused, not when the about window
is open) can use a guard in `handleCommandExecute` -- check whether the right component is active before acting.

This is pragmatic: `handleCommandExecute` already has access to `explorerRef` and the various modal states
(`showAboutWindow`, `showCommandPalette`, etc.). A guard like "skip if a modal is open" or "skip if the explorer isn't
mounted" covers the real cases without building a declarative scope system.

**Why not enforce `scope` from the registry?** The `scope` field on commands is a documentation string
(`'Main window/File list'`, `'App'`, etc.), not a runtime-checkable state. Making it enforceable would require mapping
each scope to a runtime condition (is the file list focused? is a dialog open? which pane is active?). That's the
`when`-clause system we're explicitly deferring. The imperative guards in `handleCommandExecute` are good enough for
~20 commands and don't require new infrastructure.

### What about dialogs suppressing shortcuts?

When a modal dialog is open (delete confirmation, transfer dialog, about window, etc.), Tier 1 shortcuts should
generally be suppressed -- pressing F8 while the delete confirmation is showing shouldn't open a second delete dialog.

The simplest approach: the top-level listener checks whether any modal dialog is active before doing registry lookup. If
a dialog is open, skip the centralized dispatch and let the event propagate to the dialog's own handler (Escape to
close, Enter to confirm, etc.). The dialog state is already tracked in `dialog-state.svelte.ts` and the various
`showAboutWindow` / `showCommandPalette` / etc. flags in `+page.svelte`.

This matches the current behavior: `handleFunctionKey()` in DualPaneExplorer doesn't run when a dialog has focus because
the dialog captures keyboard events first. The centralized listener should replicate this by checking dialog state
explicitly.

### Removing redundant local handlers

Once a command is handled centrally, the corresponding local handler becomes dead code. For example, after F5/F6/F7/F8
go through centralized dispatch, the `handleFunctionKey()` switch cases for those keys should be removed. The function
itself can stay (for any F-keys that aren't Tier 1), but the duplicate cases go away.

Same for `handleGlobalKeyDown()` -- the manual `isCommandPaletteShortcut()`, `isSettingsShortcut()`, and
`isDebugWindowShortcut()` checks can be replaced by registry entries for `app.commandPalette`, `app.settings`, and
`app.debugWindow`. The `shouldSuppressKey()` for Cmd+A stays as-is (it's a browser behavior suppression, not a command).

This is the key payoff: every command has exactly one place where its shortcut is defined (the registry) and one place
where its behavior is defined (`handleCommandExecute`). No more two-place updates.

### Reverse lookup performance

With ~60 commands and ~80 shortcut strings total, a linear scan on every keypress is fine. But a `Map<string, string>`
(shortcut string to command ID) is trivial to build and makes lookup O(1). Rebuild it on init and when shortcuts change.

One subtlety: multiple commands can share the same shortcut if they have different scopes (for example, `Enter` is used
by `nav.open`, `network.selectHost`, `share.selectShare`, `volume.select`, and `palette.execute`). But since only
Tier 1 commands (with `showInPalette: true`) participate in centralized dispatch, and those rarely share shortcuts, the
reverse map is almost always unambiguous. If there's ever a collision, the first match wins -- same as the current
behavior where the first handler to call `preventDefault` wins.

## Implementation plan

This is a refactor, not a new feature. The user-visible behavior should be identical before and after. Do it
incrementally, testing at each step.

### Step 1: Build the reverse shortcut lookup

Create a `shortcut-dispatch.ts` module (in `src/lib/shortcuts/` or `src/lib/commands/`) that:

- Imports the commands array and the shortcuts store
- Builds a `Map<string, string>` from shortcut strings to command IDs, considering only Tier 1 commands (those with
  `showInPalette: true`)
- Exposes a `lookupCommand(shortcutString: string): string | undefined` function
- Rebuilds the map when `onShortcutChange` fires
- Unit test: given a key combo string, returns the correct command ID; returns `undefined` for navigation keys

### Step 2: Wire centralized dispatch for F-keys

In `+page.svelte`, expand `handleGlobalKeyDown()`:

- Format the keypress with `formatKeyCombo(event)`
- Call `lookupCommand()` to check for a Tier 1 match
- If a dialog is open or the command palette is showing, skip (let local handlers deal with it)
- If a match is found, call `handleCommandExecute(commandId)`, `preventDefault()`, `stopPropagation()`
- If no match, fall through to the existing behavior (which eventually reaches DualPaneExplorer's handlers)

Then remove the F-key cases (F2, F3, F5, F6, F7, F8) from `handleFunctionKey()` in DualPaneExplorer. Keep the function
for F1 (volume chooser toggle, which has special two-pane logic) until it can also be centralized.

**Test**: Press F5/F6/F7/F8, verify they work exactly as before. Open the command palette, run the same commands,
verify identical behavior. Change an F-key binding in Settings, verify the new binding works.

### Step 3: Centralize Cmd+ combos

Move the remaining hard-coded Cmd+ handlers from `handleGlobalKeyDown()` into the registry lookup:

- Cmd+Shift+P (command palette) -- already registered as `app.commandPalette`
- Cmd+comma (settings) -- add as `app.settings` in the registry and `handleCommandExecute`
- Cmd+D (debug window, dev only) -- either add as a dev-only command or keep as special-cased

Remove `isCommandPaletteShortcut()`, `isSettingsShortcut()`, and their `if` branches from `handleGlobalKeyDown()`.

**Test**: Cmd+Shift+P opens the palette. Cmd+comma opens settings. Rebind in Settings, verify the new binding works.

### Step 4: Handle Tab and Ctrl+Tab centrally

`Tab` (pane switch) and `Ctrl+Tab` / `Ctrl+Shift+Tab` (tab cycling) are currently in DualPaneExplorer's
`handleKeyDown()`. These are good candidates for centralization -- they're already in the registry (`pane.switch`,
`tab.next`, `tab.prev`) and always do the same thing regardless of file list state.

Move them to the centralized listener. Remove from DualPaneExplorer's `handleKeyDown()`.

**Test**: Tab switches panes, Ctrl+Tab cycles tabs, Ctrl+Shift+Tab cycles backward.

### Step 5: Clean up and verify

- Remove any now-dead handler functions (empty `handleFunctionKey`, unused shortcut-checking helpers)
- Verify that Tier 2 commands (arrows, Space, Enter, Backspace, etc.) still work -- they should be unaffected since
  the centralized listener lets unmatched events propagate
- Run `./scripts/check.sh --svelte` for all checks
- Manual test pass: every F-key, every Cmd+ combo, the command palette, and MCP commands

### Future: user-customizable keybindings

Once centralized dispatch is in place, user-customizable keybindings become straightforward. The shortcuts store already
supports custom bindings. The Settings UI already has a keyboard shortcuts section. The missing piece was runtime
dispatch from those custom bindings -- and that's exactly what this plan adds. After this refactor, changing a binding in
the store automatically changes what keypress triggers it.

### Future: enforceable scope

If Cmdr grows enough to need it, the `scope` field on commands can be made enforceable by mapping each scope to a
runtime predicate (for example, `'Main window/File list'` maps to "file list pane is focused and no dialog is open").
The centralized listener would check the predicate before dispatching. This is a natural extension of the current
design -- the lookup function would take a `currentScope` parameter and filter by it. Not needed now, but the
architecture doesn't block it.

## Task list

### Step 1: Reverse shortcut lookup
- [x] Create `shortcut-dispatch.ts` with `lookupCommand()` and `Map<string, string>` reverse index
- [x] Filter to Tier 1 commands only (those with `showInPalette: true`)
- [x] Subscribe to `onShortcutChange` to rebuild the map on customization
- [x] Write unit tests for lookup (match, no match, custom shortcut override)

### Step 2: Centralize F-key dispatch
- [x] Expand `handleGlobalKeyDown()` in `+page.svelte` to format keypresses and look up Tier 1 commands
- [x] Add dialog-open guard (skip dispatch when modals are active)
- [x] Remove F2, F3, F5, F6, F7, F8 cases from `handleFunctionKey()` in DualPaneExplorer
- [x] Keep F1 (volume chooser) in DualPaneExplorer for now (two-pane coordination logic)
- [x] Manual test: all F-keys, command palette equivalents, FunctionKeyBar clicks

### Step 3: Centralize Cmd+ combos
- [x] Add `app.settings` to command registry (Cmd+comma)
- [x] Add settings case to `handleCommandExecute()`
- [x] Remove `isCommandPaletteShortcut()`, `isSettingsShortcut()` from `+page.svelte`
- [x] Keep `shouldSuppressKey()` for Cmd+A (browser suppression, not a command)
- [x] Manual test: Cmd+Shift+P, Cmd+comma, rebind in Settings and verify

### Step 4: Centralize Tab/Ctrl+Tab
- [x] Move Tab, Ctrl+Tab, Ctrl+Shift+Tab from DualPaneExplorer `handleKeyDown()` to centralized dispatch
- [x] Manual test: pane switching, tab cycling

### Step 5: Clean up and verify
- [x] Remove dead handler functions and unused helpers
- [x] Verify Tier 2 commands (arrows, Space, Enter, Backspace, etc.) still work
- [x] Run `./scripts/check.sh --svelte`
- [x] Add Linux E2E tests to test all important shortcuts (e.g. F5 should open the Copy window, ESC should close it)
      based on the @docs/tooling/e2e-testing-guide.md and run them. Reuse/extend any helpful existing E2E tests
      instead of duplicating them.
