# Keyboard shortcuts system

## Purpose

Customizable keyboard shortcuts for all Cmdr commands. Users can edit, add, remove, and reset shortcuts through the
Settings window. MCP tools can also modify shortcuts programmatically.

## Architecture

### Command registry (`../commands/command-registry.ts`)

Lives in the sibling `src/lib/commands/` directory (which has its own CLAUDE.md). Defines all commands with default
shortcuts:

```typescript
{
    id: 'nav.parent',
    name: 'Go to parent folder',
    scope: 'Main window/File list',
    shortcuts: ['Backspace', '⌘↑'],
    showInPalette: true
}
```

### Shortcuts store (`shortcuts-store.ts`)

- Persists to `shortcuts.json` in app data directory
- Delta-only storage: only customizations, not defaults
- Empty array means "all shortcuts removed"
- Missing command means "use defaults from registry"
- `initializeShortcuts` notifies `onShortcutChange` listeners for every loaded customization, so components that mounted
  before the async init finished (reactive shortcut reads, the dispatch map) catch up instead of showing registry
  defaults. The notification path also syncs menu accelerators (`updateMenuAccelerator` no-ops for commands without a
  menu item).

### Reactive reads (`reactive-shortcuts.svelte.ts`)

Two readers over one module-level `$state` version (bumped on every `onShortcutChange`):

- `getEffectiveShortcutsReactive(commandId)` returns the full effective list (the command palette shows up to three).
  Param is typed `CommandId`, not loose `string`. Returns a fresh array on every call (the underlying
  `getEffectiveShortcuts` copies the store's data), so consumers can't mutate the store — don't cache the reference.
- `getFirstShortcutReactive(commandId)` is `[0]` of that list (the one menus and inline `ShortcutChip`s show).

Both subscribe `$derived`/`$effect` consumers to shortcut changes, so a rebind updates the UI live. Use them for
long-lived UI that displays a shortcut (tooltips, hints, the palette, `ShortcutChip`). One-off reads at event time
(toasts, context menus) keep calling `getEffectiveShortcuts` directly — a toast deliberately snapshots the binding it
was created with.

### Deep link to a command's row (`../settings/settings-window.ts`)

A clickable `ShortcutChip` (and any other "customize this shortcut" affordance) deep-links to the command's row in
Settings > Keyboard shortcuts via `openShortcutCustomization(commandId)`. That helper calls
`openSettingsWindow(['Keyboard shortcuts'], shortcutAnchorId(commandId))`. The anchor-id convention
`shortcut-<commandId>` lives as the paired `shortcutAnchorId` / `commandIdFromShortcutAnchor` functions in
`settings-window.ts` so the writer (the helper) and the readers (the section's row id, the settings page's arrival
handler) can't drift. The settings side scrolls the row into its nested list and flashes it — see
[`../settings/sections/CLAUDE.md`](../settings/sections/CLAUDE.md) § "Deep-link arrival".

### Scope hierarchy (`scope-hierarchy.ts`)

Defines which shortcuts are active in each context:

- `App` scope: global, always active
- `Main window` → inherits `App`
- `File list` → inherits `Main window` → inherits `App`

When "File list" is focused, shortcuts from all three scopes can trigger.

Note: the command registry uses compound scopes like `'Main window/File list'`, `'Main window/Brief mode'`,
`'Main window/Full mode'`, `'Main window/Network'` that are distinct from the simple scope hierarchy defined in
`scope-hierarchy.ts`. These compound scopes represent specific UI contexts within the main window.

### Conflict detection (`conflict-detector.ts`)

Two commands conflict if:

1. They share the same key combo, AND
2. Their scopes overlap (via hierarchy)

Example: `⌘N` in "File list" and `⌘N` in "Main window" conflict because "File list" inherits from "Main window".

### Key capture (`key-capture.ts`)

Platform-specific formatting:

- macOS: `⌘⇧P` (symbols)
- Windows/Linux: `Ctrl+Shift+P` (names)

No normalization: shortcuts are stored exactly as displayed.

### MCP integration (`mcp-shortcuts-listener.ts`)

Main window listens for MCP events to modify shortcuts even when settings window is closed. This allows AI agents to
customize shortcuts on the fly.

### Centralized dispatch (`shortcut-dispatch.ts`)

Builds a reverse lookup `Map<shortcutString, commandId>` for Tier 1 commands (those with `showInPalette: true` plus
`app.commandPalette`). On every keypress, `handleGlobalKeyDown()` in `+page.svelte` calls `formatKeyCombo(e)` and
`lookupCommand()` to find a matching command, then routes through `handleCommandExecute()` -- the same path used by the
command palette and MCP events. Rebuilds automatically when custom shortcuts change via `onShortcutChange`.

Tier 2 commands (arrows, Space, Enter, Backspace, etc.) are not in the dispatch map. Unmatched keypresses propagate
normally to component-level handlers in DualPaneExplorer and FilePane.

## Key decisions

### Why platform-specific storage?

Cross-platform normalization is complex and error-prone. Storing shortcuts as display strings keeps the code simple.
Each platform captures and matches shortcuts in its native format.

### Why delta-only persistence?

Default shortcuts live in `command-registry.ts` and are baked into the app. Storing them in `shortcuts.json` would
duplicate data and make defaults harder to change. Only customizations are stored, keeping the file small and clear.

### Why scope hierarchy?

Allows the same key combo to have different meanings in different contexts without warnings. `⌘N` can be "New file" in
one window and "New folder" in another if their scopes don't overlap.

### Why 500ms confirmation delay?

Users often mis-press keys or change their mind mid-capture. The delay lets them press multiple combos rapidly and only
the final one (after 500ms of silence) is saved. Prevents accidental captures.

### ⌘J binds to "Go to latest download" (not Finder's "View Options")

**Decision.** The in-app `⌘J` shortcut triggers `downloads.goToLatest`, jumping the focused pane to the most recent file
in `~/Downloads`. We accept the deviation from Finder, which uses `⌘J` for the "View Options" inspector.

**Why.** Cmdr's view-mode toggles already live on dedicated single-key shortcuts (`⌘1` Full, `⌘2` Brief, plus the inline
view-mode toggle and the appearance controls under `⌘,`), so we're not displacing an existing Cmdr action — we're
choosing not to mirror Finder for this one binding. `⌘J` is short, easy to reach, and intuitive as "Jump." Chrome's
Downloads-tab shortcut is `⌘⇧J`, so there's no collision with the most common downloads workflow either. Finder migrants
pick up the new muscle memory within a few uses because the view-mode controls they expect are still keyboard-accessible
on dedicated keys. See `docs/specs/downloads-watcher-plan.md` § "In-app shortcut: ⌘J" for the full rationale.

### Why two tiers (action vs navigation commands)?

Tier 1 commands (~20 "action" commands like F-keys and Cmd+ combos) go through centralized dispatch. Tier 2 commands
(~40 navigation keys like arrows, Space, Enter, Backspace) stay in component-level handlers. Centralizing Tier 2 would
require a `when`-clause system (like VS Code's `fileListFocused && !renameActive`) because these keys mean different
things depending on context (file list vs volume chooser vs command palette). That's a significant architecture
investment with low payoff for Cmdr's current scope. Tier 1 commands are the ones where the "two sources of truth" bug
hurt (adding F8 to the registry but forgetting to add it to the keydown handler).

### Why separate MCP listener for main window?

The settings window has a full MCP bridge that syncs all state. The main window only needs to react to shortcut changes.
A lightweight listener keeps concerns separated and reduces overhead.

## Gotchas

### Modifier-key accelerators may fire twice (menu + JS)

For commands that have BOTH a native-menu accelerator (`menu/macos.rs` `Some("Shift+Space")` etc.) AND a registry
shortcut (`shortcuts: ['⇧Space']`), AppKit can leak the modifier keydown to the webview even after the menu accelerator
has fired. So `on_menu_event` emits `execute-command file.quickLook` AND `handleGlobalKeyDown` in `+page.svelte` also
sees the keydown and calls `handleCommandExecute('file.quickLook')`. **Both paths run, both reach the dispatcher.**

The race is not theoretical — observed empirically as `FE:user-action file.quickLook (×2, deduplicated)` log lines in
the Quick Look feature. Most other commands aren't toggles, so the double-fire is invisible (palette-open is idempotent,
etc.). For toggles, the dispatcher needs an arm-on-entry race-guard that swallows the second fire inside a short window
(~200 ms). See `file-explorer/quick-look/quick-look-state.svelte.ts` (`quickLookDispatchGuardJustFired` /
`armQuickLookDispatchGuard`) for the pattern.

If you add a new toggle command with both a menu accelerator and a registry shortcut, plan to add a similar guard.

### Scope hierarchy is hardcoded

`scopeHierarchy` in `scope-hierarchy.ts` is a static object. Adding a new scope requires updating the object manually.
There's no dynamic registration.

### Menu accelerator sync

When shortcuts change, `updateMenuAccelerator()` calls `invoke('update_menu_accelerator')` to update the native menu
label. The `menuCommands` array in `shortcuts-store.ts` lists all ~40 commands that have menu items. At startup,
`initializeShortcuts` pushes any persisted customizations into the menu via the `notifyListeners` path. On the Rust
side, `MenuState.items` is a `HashMap<String, MenuItemEntry>` that tracks regular `MenuItem`s by ID;
`update_menu_item_accelerator()` handles the remove/recreate/reinsert cycle. View mode CheckMenuItems still use the
separate `update_view_mode_accelerator()` path to preserve checked state.

### Conflict warnings are not errors

Users can keep conflicting shortcuts. The UI shows a warning and offers to resolve, but "Keep both" is a valid choice.
At runtime, the first match (most specific scope) wins.

### Modifier-only combos are rejected

Pressing just `⌘` or `Shift` doesn't capture anything. The `isModifierKey()` check prevents this. Users must combine a
modifier with a non-modifier key.

### Key capture is client-side only

The 500ms confirmation delay happens in the UI (`KeyboardShortcutsSection.svelte`). The store layer
(`shortcuts-store.ts`) has no delay. If you call `setShortcut()` directly, it saves immediately.

### Empty array vs missing key

In `shortcuts.json`:

- `"nav.parent": []` means "user removed all shortcuts, don't use defaults"
- Missing `nav.parent` means "use defaults from registry"

These are semantically different.

### Default shortcuts are immutable

`command-registry.ts` is compiled into the app. Changing defaults requires a new build. This is intentional: defaults
are part of the app's behavior, not user data.

### Scope overlap is transitive

If "File list" inherits "Main window" and "Main window" inherits "App", then "File list" also inherits "App". The
`getActiveScopes()` function returns all ancestors, not just the immediate parent.

### No chorded shortcuts

Shortcuts are single key combos. `Ctrl+K Ctrl+C` (press K, then C) is not supported. Only `Ctrl+K` or `Ctrl+C`
individually. This simplifies capture and matching logic.
