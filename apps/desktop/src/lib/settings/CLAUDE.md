# Settings system

## Purpose

The settings system provides user-configurable options for Cmdr through a registry-based architecture. All settings are
defined once in `settings-registry.ts` and accessed uniformly by both UI and MCP tools.

## Architecture

### Registry (`settings-registry.ts`)

Single source of truth for all settings. Each `SettingDefinition` contains:

- `id`: Unique key (e.g., `appearance.uiDensity`)
- `section`: Path in settings tree (e.g., `['General', 'Appearance']`)
- `type`: boolean, number, string, enum, duration
- `default`: Default value
- `constraints`: Type-specific validation (min/max, enum options, etc.)
- `component`: UI hint (switch, select, slider, etc.)

### Store (`settings-store.ts`)

- Uses `tauri-plugin-store` for persistence to `~/Library/Application Support/com.veszelovszki.cmdr/settings-v2.json`
- Debounced saves: 500ms after last change, atomic write (temp file + rename)
- In-memory cache for synchronous reads via `getSetting()`
- Cross-window sync: emits `settings:changed` events when values change

### Reactive state (`reactive-settings.svelte.ts`)

- Svelte 5 `$state` for settings that affect UI rendering (density, date format, file size format)
- Subscribes to `onSettingChange()` and updates reactive values immediately
- Provides derived getters like `getRowHeight()` based on current density

### Sections (`sections/`)

10 section components rendered inside the settings window: `AppearanceSection`, `FileOperationsSection`,
`KeyboardShortcutsSection`, `NetworkSection`, `LoggingSection`, `McpServerSection`, `UpdatesSection`, `ThemesSection`,
`AdvancedSection`, `DriveIndexingSection`.

### Components (`components/`)

9 reusable setting UI primitives used by section components: `SettingRow`, `SettingSwitch`, `SettingSelect`,
`SettingSlider`, `SettingNumberInput`, `SettingRadioGroup`, `SettingToggleGroup`, `SettingsSidebar`, `SettingsContent`.
Also `SectionSummary` for collapsed-section previews.

### Other files

- **settings-search.ts** — Fuzzy search over setting definitions; returns ranked matches with highlight ranges
- **settings-applier.ts** — Applies a setting change (validates, calls `setSetting`, emits events)
- **network-settings.ts** — Network-specific setting helpers (proxy config, SMB auth defaults)
- **settings-window.ts** — Logic for opening/focusing/closing the settings window (Tauri window management)
- **format-utils.ts** — Shared formatters used in settings UI (e.g., duration, file-size display strings)
- **mcp-settings-bridge.ts** — MCP bridge functionality for settings; enables AI agents to query and modify settings
  programmatically

### Shortcuts (separate subsystem)

- Stored in separate `shortcuts.json` file
- Delta-only persistence: only customized shortcuts are saved
- Platform-specific display strings (macOS: `⌘⇧P`, Windows: `Ctrl+Shift+P`)
- Scope hierarchy determines conflict detection

## Key decisions

### Why separate shortcuts from settings?

Settings and shortcuts have different access patterns. Settings are mostly static config loaded at startup. Shortcuts
are dynamic, queried on every keypress, and frequently customized. Separate stores keep them isolated and optimized for
their use cases.

### Why registry-first?

Defining all settings in a registry enables:

1. Automatic MCP tool generation (agents can query/modify any setting)
2. Guaranteed validation (types and constraints enforced at runtime)
3. UI generation for Advanced section (technical settings don't need custom UI)
4. Schema migration (registry knows what's valid, can transform old data)

### Why debounced saves?

Users often change multiple related settings in quick succession (e.g., tweaking slider values). Debouncing reduces disk
I/O from potentially hundreds of writes to one per change batch.

### Why cross-window sync?

The settings window and main window both need to react to setting changes. Without sync, changing UI density in settings
wouldn't update the main window until restart.

## Gotchas

### `$state` requires `.svelte.ts` extension

Reactive settings must live in `reactive-settings.svelte.ts` (not `.ts`). Svelte 5 `$state()` only works in `.svelte` or
`.svelte.ts` files.

### Schema version is mandatory

When modifying the settings format, increment `SCHEMA_VERSION` in `settings-store.ts` and add a migration case to
`migrateSettings()`. Otherwise old settings files may cause crashes.

### Settings cache is write-through

`setSetting()` updates both the in-memory cache and schedules a debounced save. If the app crashes between the cache
update and the save, the change is lost. This is acceptable for settings (worst case: revert to old value on restart).

### Shortcut menu sync is manual

When shortcuts are modified, menu accelerators don't auto-update. Call `invoke('update_menu_accelerators')` for commands
that have menu items (`view.fullMode`, `view.briefMode`). Most commands don't need this.

### Advanced section is auto-generated

Settings with `showInAdvanced: true` appear in the Advanced section with auto-generated UI. No custom component needed.
Just add to registry and it works.

### Density mapping is internal

Users see "Compact/Comfortable/Spacious" but the code sees rowHeight/iconSize pixel values. The `densityMappings` object
in `reactive-settings.svelte.ts` bridges this gap. Don't expose raw pixel values in the UI.

### Shortcuts conflict only when scopes overlap

`⌘N` in "File list" scope and `⌘N` in "Settings window" scope do NOT conflict because their scope hierarchies don't
overlap. Only warn when the same key combo is used in overlapping scopes (e.g., "File list" and "Main window").

### No undo

Both settings and shortcuts save immediately (after debounce). There's no undo stack. Users must use "Reset to default"
to recover from mistakes.
