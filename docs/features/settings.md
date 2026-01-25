# Settings

The Settings system provides a comprehensive configuration interface for Cmdr. It uses a registry-based architecture
where all settings are defined in a single source of truth, enabling both UI and programmatic (AI agent) access.

## Opening settings

Press **⌘,** (Command + comma) on macOS to open the Settings window. The window opens as a separate window from the
main Cmdr window.

## Window layout

The Settings window uses a two-pane layout:

- **Left sidebar** (220px): Search bar at top, followed by a tree navigation of settings sections
- **Right content area**: The settings controls for the selected section

## Available sections

### General

- **Appearance**: UI density, file icons, file size format, date/time format
- **File operations**: Confirmation dialogs, progress update interval
- **Updates**: Auto-update checking

### Network

- **SMB/Network shares**: Share cache duration, connection timeout settings

### Keyboard shortcuts

A dedicated UI for viewing and customizing keyboard shortcuts. Features:

- **Search**: Filter by action name or key combination
- **Filters**: Show all, modified only, or conflicting shortcuts
- **Edit shortcuts**: Click any shortcut to change it
- **Conflict detection**: Warns when a shortcut is already in use
- **Reset**: Reset individual shortcuts or all to defaults

### Themes

Theme mode selection (Light, Dark, System).

### Developer

- **MCP server**: Enable/disable the Model Context Protocol server and configure its port
- **Logging**: Enable verbose logging, open log file, copy diagnostic info

### Advanced

Technical settings for fine-tuning performance, including drag threshold, prefetch buffer sizes, virtualization
settings, and various timeouts.

## Architecture

### Settings registry

All settings are defined in `src/lib/settings/settings-registry.ts`. Each setting has:

- Unique ID (for example `appearance.uiDensity`)
- Section path (for example `['General', 'Appearance']`)
- Type and constraints
- Default value
- UI component hint

### Settings store

Persistence is handled by `src/lib/settings/settings-store.ts` using tauri-plugin-store:

- Settings are stored in `~/Library/Application Support/com.veszelovszki.cmdr/settings-v2.json`
- Changes are debounced (500ms) and saved atomically
- Schema versioning supports future migrations

### Keyboard shortcuts

Custom shortcuts are stored separately in `shortcuts.json` and managed by `src/lib/shortcuts/`:

- `types.ts`: KeyCombo, ShortcutConflict interfaces
- `scope-hierarchy.ts`: Defines which shortcuts are active in each context
- `key-capture.ts`: Formats keyboard events to display strings
- `shortcuts-store.ts`: Persistence for custom shortcuts
- `conflict-detector.ts`: Detects when shortcuts overlap

## Adding a new setting

1. Define the setting in `settings-registry.ts`:

```typescript
{
    id: 'mySection.mySetting',
    section: ['My Section'],
    label: 'My setting',
    description: 'What this setting does',
    type: 'boolean',
    default: true,
    component: 'switch',
}
```

2. Add UI in the appropriate section component under `src/lib/settings/sections/`

3. Wire up the setting in your feature code using `getSetting()` and `setSetting()`

## Adding a new command with shortcuts

1. Add the command to `src/lib/commands/command-registry.ts`:

```typescript
{
    id: 'myScope.myCommand',
    name: 'My command',
    scope: 'Main window',
    showInPalette: true,
    shortcuts: ['⌘M'],
}
```

2. Handle the command in `handleCommandExecute()` in `+page.svelte`

3. The command will automatically appear in the Keyboard shortcuts section

## Technical details

### Scope hierarchy

Shortcuts respect a scope hierarchy. When "File list" is active:

- Shortcuts in "File list" scope trigger
- Shortcuts in "Main window" scope also trigger (parent)
- Shortcuts in "App" scope also trigger (global)
- Shortcuts in "About window" scope do NOT trigger (different branch)

### Conflict detection

Two commands conflict if:

1. They have the same shortcut, AND
2. Their scopes overlap in the hierarchy

The UI shows conflicts with a warning icon and count badge. Users can resolve conflicts by reassigning or keeping
both shortcuts.

## Testing

- Unit tests: `pnpm vitest run src/lib/settings src/lib/shortcuts`
- Type checking: `pnpm svelte-check`
- E2E tests: `test/e2e-linux/settings.spec.ts`
