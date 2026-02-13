# Settings system specification

This document specifies the complete settings system for Cmdr, including window structure, UI components, registry
architecture, and persistence. See [ADR 018](../adr/018-settings-architecture.md) for architectural decisions.

## Table of contents

1. [Window structure](#1-window-structure)
2. [Settings registry](#2-settings-registry)
3. [Settings tree](#3-settings-tree)
4. [General › Appearance](#4-general--appearance)
5. [General › File operations](#5-general--file-operations)
6. [General › Updates](#6-general--updates)
7. [Network › SMB/Network shares](#7-network--smbnetwork-shares)
8. [Keyboard shortcuts](#8-keyboard-shortcuts)
9. [Themes](#9-themes)
10. [Developer › MCP server](#10-developer--mcp-server)
11. [Developer › Logging](#11-developer--logging)
12. [Advanced section](#12-advanced-section)
13. [Search behavior](#13-search-behavior)
14. [Accessibility](#14-accessibility)
15. [Persistence and sync](#15-persistence-and-sync)

---

## 1. Window structure

### 1.1 Window chrome

- **Type**: Separate Tauri window (not HTML dialog)
- **Size**: 800×600px default, resizable, minimum 600×400px
- **Position**: Centered on main window when opened
- **Title**: "Settings"

### 1.2 Layout

- Left sidebar: 220px fixed width, contains search bar and tree navigation
- Right content area: Flexible width, contains settings panels
- No splitter between sidebar and content

### 1.3 Search bar

- Pinned at top of sidebar, always visible
- Full width within sidebar (220px minus padding)
- Placeholder: "Search settings..."
- See [section 13](#13-search-behavior) for search behavior details

### 1.4 Tree behavior

- Tree is always fully expanded (not collapsible)
- Selecting a section or subsection scrolls the right pane to that location
- Active section/subsection is highlighted in the tree

### 1.5 Close behavior

- ESC closes the window
- Standard window close button (×) closes the window
- Cmd+, while Settings is already open brings it to front (no duplicate windows)

### 1.6 Apply behavior

- All changes apply immediately (no Apply/Cancel buttons)
- Changes persist to disk on each change (debounced 500ms)
- Settings requiring restart show inline indicator

---

## 2. Settings registry

The settings registry (`settings-registry.ts`) is the single source of truth for all settings metadata.

### 2.1 Registry entry structure

```typescript
interface SettingDefinition {
  // Identity
  id: string                           // Unique key, e.g., 'appearance.uiDensity'
  section: string[]                    // Path in tree, e.g., ['General', 'Appearance']

  // Display
  label: string                        // Human-readable name
  description: string                  // Explanatory text shown below the control
  keywords: string[]                   // Additional search terms

  // Type and constraints
  type: 'boolean' | 'number' | 'string' | 'enum' | 'duration'
  default: unknown                     // Default value

  // Constraints (type-specific)
  constraints?: {
    // For 'number' type
    min?: number
    max?: number
    step?: number

    // For 'enum' type
    options?: Array<{
      value: string | number
      label: string
      description?: string            // Shown inline, not in tooltip
    }>
    allowCustom?: boolean             // Whether "Custom..." option is available
    customMin?: number                // Min value for custom input
    customMax?: number                // Max value for custom input

    // For 'duration' type
    unit: 'ms' | 's' | 'min' | 'h' | 'd'
    minMs?: number                    // Minimum in milliseconds
    maxMs?: number                    // Maximum in milliseconds
  }

  // Behavior
  requiresRestart?: boolean           // Show restart indicator when changed
  disabled?: boolean                  // Grayed out with optional badge
  disabledReason?: string             // e.g., "Coming soon"

  // UI hints
  component?: 'switch' | 'select' | 'radio' | 'slider' | 'toggle-group' | 'number-input' | 'text-input'
  showInAdvanced?: boolean            // If true, appears in Advanced section with generated UI
}
```

### 2.2 Access API

A single pair of functions for both UI and programmatic (AI agent) access:

```typescript
// Load a setting value (returns default if not set)
function getSetting<T>(id: string): T

// Store a setting value (validates against constraints, throws if invalid)
function setSetting<T>(id: string, value: T): void

// Get setting metadata (for UI rendering, validation, etc.)
function getSettingDefinition(id: string): SettingDefinition

// Get all settings in a section
function getSettingsInSection(sectionPath: string[]): SettingDefinition[]

// Search settings by query
function searchSettings(query: string): SettingDefinition[]

// Reset a setting to default
function resetSetting(id: string): void

// Reset all settings to defaults
function resetAllSettings(): void

// Check if a setting differs from default
function isModified(id: string): boolean
```

### 2.3 Validation

- `setSetting()` validates against constraints before storing
- For `enum` types with `allowCustom: true`, validates against `customMin`/`customMax`
- For `number` types, validates against `min`/`max`
- For `duration` types, converts to canonical unit and validates against `minMs`/`maxMs`
- Throws `SettingValidationError` with descriptive message on failure

### 2.4 AI agent access

AI agents use the same `getSetting()`/`setSetting()` API. The registry constraints ensure agents cannot set
invalid values. Example Tauri command exposure:

```rust
#[tauri::command]
fn set_setting(id: String, value: serde_json::Value) -> Result<(), String> {
    // Delegates to the same validation logic as UI
}
```

---

## 3. Settings tree

```
General
  ├─ Appearance
  ├─ File operations
  └─ Updates

Network
  └─ SMB/Network shares

Keyboard shortcuts    (dedicated UI, no subsections)

Themes                (dedicated UI, no subsections)

Developer
  ├─ MCP server
  └─ Logging

Advanced              (generated UI, scrollable)
```

---

## 4. General › Appearance

### 4.1 UI density

- **ID**: `appearance.uiDensity`
- **Component**: ToggleGroup (3 segments)
- **Options**: "Compact", "Comfortable" (default), "Spacious"
- **Behavior**: Immediate preview. Maps internally to:
  - Compact: rowHeight=16px, iconSize=24
  - Comfortable: rowHeight=20px, iconSize=32
  - Spacious: rowHeight=28px, iconSize=40
- **Keyboard**: Arrow keys navigate between options

### 4.2 Use app icons for documents

- **ID**: `appearance.useAppIconsForDocuments`
- **Component**: Switch with inline label
- **Label**: "Use app icons for documents"
- **Description**: "Show the app's icon for documents instead of generic file type icons. More colorful but slightly slower."
- **Default**: true

### 4.3 File size format

- **ID**: `appearance.fileSizeFormat`
- **Component**: Select dropdown
- **Options**:
  - `binary`: "Binary (KiB, MiB, GiB) — 1 KiB = 1024 bytes"
  - `si`: "SI decimal (KB, MB, GB) — 1 KB = 1000 bytes"
- **Default**: `binary`
- **Note**: Clarifications shown inline in dropdown, not as tooltips

### 4.4 Date and time format

- **ID**: `appearance.dateTimeFormat`
- **Component**: RadioGroup with conditional custom input
- **Options**:
  - `system`: "System default" — shows live preview
  - `iso`: "ISO 8601" — e.g., "2025-01-25 14:30"
  - `short`: "Short" — e.g., "Jan 25, 2:30 PM"
  - `custom`: "Custom..."
- **Default**: `system`
- **Custom sub-UI** (when "Custom" selected):
  - Text input for format string
  - Live preview of current date/time
  - Collapsible help with format placeholders (YYYY, MM, DD, HH, mm, ss, etc.)

---

## 5. General › File operations

### 5.1 Confirm before delete

- **ID**: `fileOperations.confirmBeforeDelete`
- **Component**: Switch
- **Label**: "Confirm before delete"
- **Description**: "Show a confirmation dialog before moving files to trash."
- **Default**: true
- **State**: Disabled, shows "Coming soon" badge

### 5.2 Delete permanently

- **ID**: `fileOperations.deletePermanently`
- **Component**: Switch
- **Label**: "Delete permanently instead of using trash"
- **Description**: "Bypass trash and delete files immediately. This cannot be undone."
- **Default**: false
- **State**: Disabled, shows "Coming soon" badge
- **Future behavior**: When enabled, shows warning icon and description turns orange

### 5.3 Progress update interval

- **ID**: `fileOperations.progressUpdateInterval`
- **Component**: Slider + NumberInput combo
- **Label**: "Progress update interval"
- **Description**: "How often to refresh progress during file operations. Lower values feel more responsive but use more CPU."
- **Constraints**:
  - Slider snaps to: 100, 250, 500, 1000, 2000 ms
  - NumberInput allows custom: min 50ms, max 5000ms
- **Default**: 500ms (marked on slider)
- **Display**: NumberInput shows "ms" suffix

### 5.4 Maximum conflicts to show

- **ID**: `fileOperations.maxConflictsToShow`
- **Component**: Select with custom option
- **Options**: 1, 2, 3, 5, 10, 50, 100 (default), 200, 500, "Custom..."
- **Constraints**: Custom range 1–1000
- **Description**: "Maximum number of file conflicts to display in the preview before an operation."

---

## 6. General › Updates

### 6.1 Automatically check for updates

- **ID**: `updates.autoCheck`
- **Component**: Switch
- **Label**: "Automatically check for updates"
- **Description**: "Periodically check for new versions in the background."
- **Default**: true

### 6.2 Update channel (future)

- **ID**: `updates.channel`
- **Component**: Select
- **Options**: "Stable" (default), "Beta"
- **Description**: "Beta releases include new features but may have bugs."
- **State**: Hidden until beta channel exists

---

## 7. Network › SMB/Network shares

### 7.1 Share cache duration

- **ID**: `network.shareCacheDuration`
- **Component**: Select with custom option
- **Options**: "30 seconds" (default), "5 minutes", "1 hour", "1 day", "30 days", "Custom..."
- **Custom sub-UI**: NumberInput + unit dropdown (seconds/minutes/hours/days)
- **Description**: "How long to cache the list of available shares on a server before refreshing."

### 7.2 Network timeout mode

- **ID**: `network.timeoutMode`
- **Component**: RadioGroup (vertical, with inline descriptions)
- **Options**:
  - `normal`: "Normal" — "For typical local networks (15s timeout)"
  - `slow`: "Slow network" — "For VPNs or high-latency connections (45s timeout)"
  - `custom`: "Custom" — shows NumberInput for timeout in seconds
- **Default**: `normal`
- **Description at top**: "How long to wait when connecting to network shares."

---

## 8. Keyboard shortcuts

Dedicated UI — uses the full right pane with no tree navigation.

### 8.1 Layout

- **Top bar**: Two search inputs side by side
  - Left (wider): "Search by action name..." — text search
  - Right (narrower): "Press keys..." — key combination search
- **Filter chips** (below search): "All", "Modified", "Conflicts"
  - "Conflicts" shows count badge when shortcuts are bound to multiple actions
- **Main area**: Virtualized list grouped by scope (App, Navigation, File list, etc.)

### 8.2 Text search behavior

- Searches action names and descriptions
- Results highlight matched characters (same as command palette)
- Tree narrows to matching commands with scope headers preserved

### 8.3 Key combination search

- Field captures key presses instead of typing text
- Pressing Cmd+Shift+P searches for that exact combination
- Shows matching commands that use that shortcut
- Clear button (×) to reset

### 8.4 Command row layout

```
[Scope badge] Action name                    [Shortcut pill] [Shortcut pill] [+]
              Muted description text
```

- **Scope badge**: Small colored tag (e.g., "App", "File list")
- **Shortcut pills**: Rounded rectangles showing key combo. Click to edit.
- **[+] button**: Add additional shortcut to this action
- **Blue dot**: Shown next to modified shortcuts

### 8.5 Editing a shortcut

1. Click shortcut pill → pill shows "Press keys..." placeholder
2. User presses key combination → shows combo, waits 500ms for confirmation
3. If conflict: Inline warning "Also bound to [Action name]" with "Remove from other" or "Cancel"
4. Press Escape to cancel editing
5. Press Backspace/Delete on focused pill to remove that shortcut

### 8.6 Reset to defaults

- **Button at bottom**: "Reset all to defaults" — always shows confirmation dialog
- **Per-row**: Right-click context menu → "Reset to default" — always shows confirmation dialog
- Modified shortcuts show blue dot indicator

### 8.7 Conflict handling

- Commands with conflicting shortcuts show orange warning icon
- Filter chip "Conflicts" filters to only conflicting commands

---

## 9. Themes

Dedicated UI for theme selection.

### 9.1 Theme mode

- **ID**: `theme.mode`
- **Component**: ToggleGroup (3 segments with icons)
- **Options**: "☀️ Light", "🌙 Dark", "💻 System"
- **Behavior**: Immediate switch. "System" follows OS preference.
- **Default**: `system`

### 9.2 Preset themes (future)

- Horizontal scrollable row of theme preview cards
- Click to apply
- Initially: Only "Default Light" and "Default Dark" shown

### 9.3 Custom theme editor (future)

- Collapsible section: "Customize colors"
- Grid of color swatches by category
- Color picker popover on click
- Export/Import as JSON
- "Reset to theme defaults" button
- **Initial implementation**: Shows "Coming soon" placeholder

---

## 10. Developer › MCP server

### 10.1 Enable MCP server

- **ID**: `developer.mcpEnabled`
- **Component**: Switch
- **Label**: "Enable MCP server"
- **Description**: "Start a Model Context Protocol server for AI assistant integration."
- **Restart indicator**: Shows "Restart required to apply" when toggled
- **Default**: true (dev builds), false (prod builds)

### 10.2 MCP port

- **ID**: `developer.mcpPort`
- **Component**: NumberInput with validation and port scanner
- **Label**: "Port"
- **Constraints**: 1024–65535
- **Default**: 9224
- **Description**: "The port number for the MCP server."
- **Disabled state**: Grayed out when MCP server is disabled
- **Port availability check**:
  - Auto-checks if port is available on blur/change
  - If unavailable: Shows warning "Port 9224 is in use"
  - Offers button: "Find available port" — scans and suggests an open port
  - Scan range: starts at preferred port, increments until finding open port (max 100 attempts)

---

## 11. Developer › Logging

### 11.1 Verbose logging

- **ID**: `developer.verboseLogging`
- **Component**: Switch
- **Label**: "Verbose logging"
- **Description**: "Log detailed debug information. Useful for troubleshooting. May impact performance."
- **Default**: false

### 11.2 Open log file

- **Component**: Button (secondary style)
- **Label**: "Open log file"
- **Behavior**: Opens log file location in Finder

### 11.3 Copy diagnostic info

- **Component**: Button (secondary style)
- **Label**: "Copy diagnostic info"
- **Behavior**: Copies system info, app version, settings summary to clipboard
- **Feedback**: Brief toast "Copied to clipboard"

---

## 12. Advanced section

Generated UI for technical settings. Scrollable, unlike other sections.

### 12.1 Section header

- Warning banner: "⚠️ These settings are for advanced users. Incorrect values may cause performance issues or unexpected behavior."
- "Reset all to defaults" button (secondary, right-aligned) — shows confirmation dialog

### 12.2 Setting row layout

```
┌─────────────────────────────────────────────────────────────────────┐
│ ● Setting name                                         [UI control] │
│   Description text explaining what this does                        │
│   Default: 200                                    [Reset to default]│
└─────────────────────────────────────────────────────────────────────┘
```

- Blue dot (●) shown only when value differs from default
- "Reset to default" link visible only when modified

### 12.3 UI component mapping

| Type | Component |
|------|-----------|
| `boolean` | Switch |
| `number` (bounded) | Slider + NumberInput |
| `number` (unbounded) | NumberInput |
| `enum` | Select dropdown |
| `duration` | NumberInput + unit dropdown |
| `string` | TextInput |

### 12.4 Settings included in Advanced

| ID | Name | Type | Default | Description |
|----|------|------|---------|-------------|
| `advanced.dragThreshold` | Drag threshold | number (px) | 5 | Minimum distance in pixels before a drag operation starts |
| `advanced.prefetchBufferSize` | Prefetch buffer size | number | 200 | Number of items to prefetch around the visible range |
| `advanced.virtualizationBufferRows` | Virtualization buffer (rows) | number | 20 | Extra rows to render above and below the visible area |
| `advanced.virtualizationBufferColumns` | Virtualization buffer (columns) | number | 2 | Extra columns to render in brief view |
| `advanced.fileWatcherDebounce` | File watcher debounce | duration | 200ms | Delay after file system changes before refreshing |
| `advanced.serviceResolveTimeout` | Service resolve timeout | duration | 5s | Timeout for resolving network services via Bonjour |
| `advanced.mountTimeout` | Mount timeout | duration | 20s | Timeout for mounting network shares |
| `advanced.updateCheckInterval` | Update check interval | duration | 60min | How often to check for updates in the background |

### 12.5 Settings explicitly excluded

| Setting | Reason |
|---------|--------|
| License validation interval | Business logic, not user-configurable |
| Offline grace period | Would enable license bypass |
| Commercial reminder interval | Business logic |
| License server URL | Security risk |
| License HTTP timeout | Internal, rarely relevant |
| Window resize debounce | Internal, no user benefit |
| Icon size | Controlled by UI density |
| Row heights | Controlled by UI density |
| Default SMB port | Standard protocol |
| MCP protocol version | Internal compatibility |
| JSON-RPC error codes | Internal constants |
| Pane width min/max | UX guardrails |
| Default volume ID | Internal identifier |
| Keychain service name | Would orphan stored credentials |
| Debug log categories | Covered by verbose logging toggle |
| Benchmark mode | Dev-only |
| Support email | Hardcoded contact |
| Full disk access choice | Handled via permission flow |

---

## 13. Search behavior

### 13.1 Search index

For each setting, index:
- Section path (e.g., "General › Appearance")
- Label
- Description
- Keywords array

### 13.2 Search engine

- Uses uFuzzy (same config as command palette)
- `intraMode: 1` for typo tolerance
- `interIns: 3` for character insertions

### 13.3 Results display

- Tree shows only sections containing matches
- Matched settings highlighted with character-level match indicators
- Clicking result scrolls to setting and briefly pulses it (200ms highlight)

### 13.4 Empty state

"No settings found for '[query]'" with suggestion: "Try different keywords or check Keyboard shortcuts"

### 13.5 Keyboard navigation

- Arrow Up/Down: Navigate between results
- Enter: Jump to selected result
- Escape: Clear search and return to full tree

---

## 14. Accessibility

- All interactive elements have visible focus states
- Switch/Toggle components have proper ARIA labels
- Color choices meet WCAG AA contrast requirements
- Full keyboard navigation for all settings
- Screen reader announces: setting name, current value, description
- Focus trap within Settings window when open

---

## 15. Persistence and sync

### 15.1 Storage location

`~/Library/Application Support/com.veszelovszki.cmdr/settings.json`

### 15.2 Save behavior

- Debounced 500ms after last change
- Atomic write: write to temp file, then rename
- On error: log warning, retry once, then show toast

### 15.3 Schema migration

- Version field in settings file
- On load, migrate old schemas forward
- Unknown keys preserved (forward compatibility)

### 15.4 Defaults

- Registry provides all defaults
- Missing keys use registry default
- Explicit `null` resets to default
