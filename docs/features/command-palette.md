# Command palette

The command palette provides quick keyboard access to all application commands, similar to VS Code's command palette.

## Usage

1. Press **⌘⇧P** (Command + Shift + P) to open the command palette
2. Start typing to filter commands by name (fuzzy search)
3. Use **↑↓** arrow keys to navigate the list
4. Press **Enter** to execute the selected command
5. Press **Escape** to close without executing

## Features

- **Fuzzy search**: Type partial matches or even typos (e.g., "cop" matches "Copy path to clipboard")
- **Match highlighting**: Matched characters are underlined for clarity
- **Keyboard shortcuts**: Each command shows its keyboard shortcut if available
- **Persisted query**: The search query is remembered within the session

## Available commands

The palette includes commands from all scopes:

| Scope | Example commands |
|-------|------------------|
| App | Quit Cmdr, About Cmdr |
| Main window | Toggle hidden files, Switch view modes |
| File list | Copy path, Show in Finder, Go to parent |
| Network | Refresh hosts, Connect to share |
| About window | Open website, Close window |

## Technical implementation

### Dependencies

- **@leeoniya/ufuzzy**: Lightweight (~3.5KB), fast fuzzy search library with typo tolerance

### Components

- `src/lib/commands/types.ts` - Command and CommandMatch interfaces
- `src/lib/commands/command-registry.ts` - Complete list of all ~45 commands
- `src/lib/commands/fuzzy-search.ts` - Search wrapper using uFuzzy
- `src/lib/command-palette/CommandPalette.svelte` - Modal UI component

### Adding new commands

To add a new command:

1. Add the command definition to `command-registry.ts`:

    ```typescript
    const command = {
        id: 'scope.commandName',
        name: 'Human readable name',
        scope: 'Main window/File list',  // Hierarchical scope
        showInPalette: true,              // Set false for low-level nav
        shortcuts: ['⌘K'],                // Optional keyboard shortcuts
    }
    ```

2. Add the execution handler in `+page.svelte`'s `handleCommandExecute` function (or delegate to the appropriate component).

## Future enhancements

- [x] Add Tauri menu item to trigger palette (View → Command palette...)
- [x] Unit tests for fuzzy search
- [ ] Wire up remaining command handlers
- [ ] Command context (pass selected file info to commands)
- [ ] Recently used commands section
- [ ] Custom keyboard shortcut configuration
