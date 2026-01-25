# Keyboard shortcut customization specification

This document specifies the keyboard shortcut customization feature for Cmdr. This extends the existing keyboard
shortcuts section defined in [settings.md §8](./settings.md#8-keyboard-shortcuts).

## Table of contents

1. [Overview](#1-overview)
2. [Scope hierarchy](#2-scope-hierarchy)
3. [Data model](#3-data-model)
4. [Key capture and formatting](#4-key-capture-and-formatting)
5. [Conflict detection](#5-conflict-detection)
6. [Persistence](#6-persistence)
7. [UI behavior](#7-ui-behavior)
8. [Integration with keyboard handling](#8-integration-with-keyboard-handling)

---

## 1. Overview

### 1.1 Goals

- Let users customize keyboard shortcuts for any command
- Multiple shortcuts per command (like VS Code)
- Detect and resolve conflicts between shortcuts
- Platform-specific storage (no cross-platform translation)
- Immediate feedback during key capture

### 1.2 Non-goals

- Import/export of shortcuts (future enhancement)
- Chorded shortcuts like `Ctrl+K Ctrl+C` (single combo only)
- Per-profile shortcuts

---

## 2. Scope hierarchy

### 2.1 Scope definition

Each command has a single `scope` property for display grouping. The scope hierarchy determines which shortcuts are
active in a given context.

```typescript
type CommandScope =
    | 'App'                    // Global, works everywhere
    | 'Main window'            // Main window context
    | 'File list'              // File list focused
    | 'Command palette'        // Command palette open
    | 'Navigation'             // Navigation context
    | 'Selection'              // Selection operations
    | 'Edit'                   // Edit operations
    | 'View'                   // View operations
    | 'Help'                   // Help operations
    | 'About window'           // About window context
    | 'Settings window'        // Settings window context
```

### 2.2 Active scopes

When a given scope is active, shortcuts from that scope and its ancestors are available. The hierarchy is explicit:

```typescript
const scopeHierarchy: Record<CommandScope, CommandScope[]> = {
    'App':              ['App'],
    'Main window':      ['Main window', 'App'],
    'File list':        ['File list', 'Main window', 'App'],
    'Command palette':  ['Command palette', 'Main window', 'App'],
    'Navigation':       ['Navigation', 'Main window', 'App'],
    'Selection':        ['Selection', 'Main window', 'App'],
    'Edit':             ['Edit', 'Main window', 'App'],
    'View':             ['View', 'Main window', 'App'],
    'Help':             ['Help', 'Main window', 'App'],
    'About window':     ['About window', 'App'],
    'Settings window':  ['Settings window', 'App'],
}

function getActiveScopes(current: CommandScope): CommandScope[] {
    return scopeHierarchy[current] ?? [current, 'App']
}
```

### 2.3 Scope behavior

When determining if a shortcut should trigger:

```typescript
function shouldTrigger(command: Command, currentScope: CommandScope): boolean {
    const activeScopes = getActiveScopes(currentScope)
    return activeScopes.includes(command.scope)
}
```

**Example**: When `File list` is active:
- `⌘Q` (App scope) triggers — App is in File list's hierarchy
- `⌘N` (File list scope) triggers — exact match
- `⌘W` (About window scope) does NOT trigger — different branch

---

## 3. Data model

### 3.1 Command definition

```typescript
interface Command {
    id: string                  // Unique ID, for example 'file.copy'
    name: string                // Display name, for example "Copy"
    scope: CommandScope         // Single scope for grouping
    showInPalette: boolean      // Show in command palette
    shortcuts: string[]         // Default shortcuts (platform-specific)
    description?: string        // Optional description
}
```

### 3.2 Custom shortcuts storage

Custom shortcuts are stored separately from defaults:

```typescript
interface CustomShortcuts {
    // Only stores customizations, not defaults
    // Key: command ID, Value: array of shortcuts (empty array = all removed)
    [commandId: string]: string[]
}
```

### 3.3 Effective shortcuts

```typescript
function getEffectiveShortcuts(commandId: string): string[] {
    const customShortcuts = getCustomShortcuts()
    if (commandId in customShortcuts) {
        return customShortcuts[commandId]
    }
    const command = getCommand(commandId)
    return command?.shortcuts ?? []
}

function isShortcutModified(commandId: string): boolean {
    const customShortcuts = getCustomShortcuts()
    return commandId in customShortcuts
}
```

---

## 4. Key capture and formatting

### 4.1 Platform-specific storage

Shortcuts are stored as platform-specific display strings. No normalization or translation.

**macOS**: `⌘⇧P`, `⌥⌫`, `⌃Tab`
**Windows/Linux**: `Ctrl+Shift+P`, `Alt+Backspace`, `Ctrl+Tab`

### 4.2 Key symbols (macOS)

| Modifier | Symbol |
|----------|--------|
| Command  | ⌘      |
| Control  | ⌃      |
| Option   | ⌥      |
| Shift    | ⇧      |

### 4.3 Key capture implementation

```typescript
interface KeyCombo {
    meta: boolean
    ctrl: boolean
    alt: boolean
    shift: boolean
    key: string  // Normalized key name
}

function formatKeyCombo(event: KeyboardEvent): string {
    const parts: string[] = []

    // macOS uses symbols, Windows/Linux uses names
    if (isMacOS()) {
        if (event.metaKey) parts.push('⌘')
        if (event.ctrlKey) parts.push('⌃')
        if (event.altKey) parts.push('⌥')
        if (event.shiftKey) parts.push('⇧')
    } else {
        if (event.ctrlKey) parts.push('Ctrl')
        if (event.altKey) parts.push('Alt')
        if (event.shiftKey) parts.push('Shift')
        if (event.metaKey) parts.push('Win')
    }

    const key = normalizeKeyName(event.key)
    parts.push(key)

    return isMacOS() ? parts.join('') : parts.join('+')
}

function normalizeKeyName(key: string): string {
    // Single characters are uppercased
    if (key.length === 1) return key.toUpperCase()

    // Special key mappings
    const keyMap: Record<string, string> = {
        'Backspace': isMacOS() ? '⌫' : 'Backspace',
        'Delete': isMacOS() ? '⌦' : 'Delete',
        'Enter': isMacOS() ? '↩' : 'Enter',
        'Escape': isMacOS() ? '⎋' : 'Esc',
        'Tab': 'Tab',
        'ArrowUp': '↑',
        'ArrowDown': '↓',
        'ArrowLeft': '←',
        'ArrowRight': '→',
        ' ': 'Space',
    }

    return keyMap[key] ?? key
}
```

### 4.4 Matching shortcuts

To check if a keyboard event matches a stored shortcut:

```typescript
function matchesShortcut(event: KeyboardEvent, shortcut: string): boolean {
    return formatKeyCombo(event) === shortcut
}
```

---

## 5. Conflict detection

### 5.1 Conflict definition

Two commands conflict if:
1. They have the same shortcut, AND
2. Their scopes overlap in the hierarchy

### 5.2 Scope overlap check

```typescript
function scopesOverlap(scopeA: CommandScope, scopeB: CommandScope): boolean {
    const activeA = getActiveScopes(scopeA)
    const activeB = getActiveScopes(scopeB)
    // They overlap if either contains the other
    return activeA.includes(scopeB) || activeB.includes(scopeA)
}
```

### 5.3 Finding conflicts

```typescript
interface ShortcutConflict {
    shortcut: string
    commands: Command[]
}

function findConflictsForShortcut(shortcut: string, scope: CommandScope): Command[] {
    const allCommands = getAllCommands()
    return allCommands.filter(cmd => {
        const cmdShortcuts = getEffectiveShortcuts(cmd.id)
        return cmdShortcuts.includes(shortcut) && scopesOverlap(cmd.scope, scope)
    })
}

function getAllConflicts(): ShortcutConflict[] {
    const conflicts: ShortcutConflict[] = []
    const shortcutMap = new Map<string, Command[]>()

    for (const cmd of getAllCommands()) {
        for (const shortcut of getEffectiveShortcuts(cmd.id)) {
            const existing = shortcutMap.get(shortcut) ?? []
            // Check for scope overlap with any existing command
            const overlapping = existing.filter(e => scopesOverlap(e.scope, cmd.scope))
            if (overlapping.length > 0) {
                // Add to conflicts
                const conflict = conflicts.find(c => c.shortcut === shortcut)
                if (conflict) {
                    if (!conflict.commands.includes(cmd)) {
                        conflict.commands.push(cmd)
                    }
                } else {
                    conflicts.push({ shortcut, commands: [...overlapping, cmd] })
                }
            }
            existing.push(cmd)
            shortcutMap.set(shortcut, existing)
        }
    }

    return conflicts
}
```

---

## 6. Persistence

### 6.1 Storage file

Custom shortcuts are stored in a separate file from main settings:

`~/Library/Application Support/com.veszelovszki.cmdr/shortcuts.json`

### 6.2 File format

```json
{
    "_schemaVersion": 1,
    "shortcuts": {
        "file.copy": ["⌘C", "⌃C"],
        "file.paste": ["⌘V"],
        "nav.parent": []
    }
}
```

- Only modified commands are stored
- Empty array means all shortcuts removed
- Missing command means use defaults

### 6.3 Save behavior

- Debounced 500ms after last change
- Atomic write (temp file + rename)
- On error: log warning, retry once

### 6.4 Migration

Schema version supports future migrations. Currently version 1.

---

## 7. UI behavior

### 7.1 Edit flow

1. User clicks a shortcut pill
2. Pill changes to "Press keys..." state (highlighted)
3. User presses key combination
4. Combo is captured and displayed in pill
5. 500ms delay for confirmation (user can keep pressing to change)
6. After 500ms of no input:
   - Check for conflicts
   - If conflict: show inline warning
   - If no conflict: save immediately

### 7.2 Conflict resolution UI

When a conflict is detected:

```
┌────────────────────────────────────────────────────────────────┐
│ ⚠️ ⌘N is already bound to "New file" in File list scope       │
│                                                                │
│ [Remove from other]  [Keep both]  [Cancel]                     │
└────────────────────────────────────────────────────────────────┘
```

- **Remove from other**: Removes shortcut from conflicting command, assigns to current
- **Keep both**: Allows the conflict (user's choice)
- **Cancel**: Reverts to previous shortcut

### 7.3 Removing a shortcut

- Click pill to select
- Press Backspace or Delete
- Shortcut is removed (with 500ms delay like editing)

### 7.4 Adding a shortcut

- Click [+] button next to existing shortcuts
- New empty pill appears in edit mode
- Same capture flow as editing

### 7.5 Reset to defaults

**Single command**: Right-click context menu → "Reset to default"
**All commands**: "Reset all to defaults" button

Both show confirmation dialog.

### 7.6 Visual indicators

- **Blue dot**: Shortcut has been modified from default
- **Orange warning icon**: Shortcut has conflicts
- **Filter chips**:
  - "All": Show all commands
  - "Modified": Only commands with custom shortcuts
  - "Conflicts": Only commands with conflicting shortcuts (shows count badge)

---

## 8. Integration with keyboard handling

### 8.1 Current implementation

Keyboard handling is in `+page.svelte` `handleKeyDown`:

```typescript
handleKeyDown = (e: KeyboardEvent) => {
    if (e.metaKey && e.key === ',') {
        void openSettingsWindow()
        return
    }
    // ... more hardcoded checks
}
```

### 8.2 Target implementation

Replace hardcoded checks with dynamic lookup:

```typescript
// keyboard-handler.ts
function handleKeyDown(event: KeyboardEvent, currentScope: CommandScope): string | null {
    const shortcut = formatKeyCombo(event)
    const activeScopes = getActiveScopes(currentScope)

    // Find command matching this shortcut in active scopes
    // More specific scopes take priority (they're first in the array)
    for (const scope of activeScopes) {
        for (const command of getCommandsInScope(scope)) {
            const shortcuts = getEffectiveShortcuts(command.id)
            if (shortcuts.includes(shortcut)) {
                return command.id  // Return command to execute
            }
        }
    }

    return null  // No matching command
}
```

### 8.3 Command execution

The returned command ID is passed to `handleCommandExecute()` which already handles command dispatch.

---

## 9. File structure

```
src/lib/shortcuts/
├── types.ts              # KeyCombo, ShortcutConflict interfaces
├── scope-hierarchy.ts    # Scope definitions and hierarchy
├── key-capture.ts        # formatKeyCombo, matchesShortcut, normalizeKeyName
├── shortcuts-store.ts    # Persistence layer for custom shortcuts
├── conflict-detector.ts  # findConflictsForShortcut, getAllConflicts
└── keyboard-handler.ts   # handleKeyDown integration
```

---

## 10. Testing requirements

### 10.1 Unit tests

- Key capture: all modifier combinations
- Key capture: special keys (arrows, function keys, etc.)
- Scope hierarchy: getActiveScopes for all scopes
- Scope overlap: all permutations
- Conflict detection: same scope, overlapping scopes, non-overlapping scopes
- Persistence: save/load cycle
- Persistence: migration from older schema

### 10.2 Integration tests

- Edit flow: capture → display → save
- Conflict resolution: all three options
- Reset to defaults: single and all
- Filter chips: correct filtering
- Blue dot: appears when modified

### 10.3 E2E tests

- Open settings, navigate to keyboard shortcuts
- Edit a shortcut, verify it saves
- Create a conflict, resolve it
- Reset to defaults
