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

No normalization—shortcuts are stored exactly as displayed.

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

### Why separate MCP listener for main window?

The settings window has a full MCP bridge that syncs all state. The main window only needs to react to shortcut changes.
A lightweight listener keeps concerns separated and reduces overhead.

## Gotchas

### Scope hierarchy is hardcoded

`scopeHierarchy` in `scope-hierarchy.ts` is a static object. Adding a new scope requires updating the object manually.
There's no dynamic registration.

### Menu accelerator sync

When shortcuts change, `updateMenuAccelerator()` calls `invoke('update_menu_accelerator')` to update the native menu
label. The `menuCommands` array in `shortcuts-store.ts` lists all ~30 commands that have menu items. At startup,
`syncMenuAccelerators()` pushes any persisted customizations into the menu. On the Rust side, `MenuState.items` is a
`HashMap<String, MenuItemEntry>` that tracks regular `MenuItem`s by ID; `update_menu_item_accelerator()` handles the
remove/recreate/reinsert cycle. View mode CheckMenuItems still use the separate `update_view_mode_accelerator()` path to
preserve checked state.

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

`command-registry.ts` is compiled into the app. Changing defaults requires a new build. This is intentional—defaults are
part of the app's behavior, not user data.

### Scope overlap is transitive

If "File list" inherits "Main window" and "Main window" inherits "App", then "File list" also inherits "App". The
`getActiveScopes()` function returns all ancestors, not just the immediate parent.

### No chorded shortcuts

Shortcuts are single key combos. `Ctrl+K Ctrl+C` (press K, then C) is not supported. Only `Ctrl+K` or `Ctrl+C`
individually. This simplifies capture and matching logic.
