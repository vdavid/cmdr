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

- Uses `tauri-plugin-store` for persistence to `~/Library/Application Support/com.veszelovszki.cmdr/settings.json`
- Debounced saves: 500ms after last change, atomic write (temp file + rename)
- In-memory cache for synchronous reads via `getSetting()`
- Cross-window sync: emits `settings:changed` events when values change

### Reactive state (`reactive-settings.svelte.ts`)

- Svelte 5 `$state` for settings that affect UI rendering (density, date format, file size format, directory sort mode)
- Subscribes to `onSettingChange()` and updates reactive values immediately
- Provides derived getters like `getRowHeight()` based on current density

### Sections (`sections/`)

13 section components rendered inside the settings window. `ListingSection` includes:
- `listing.sizeDisplay` — enum (smart/logical/physical), default smart, toggle-group. Reactive getter: `getSizeDisplayMode()`.
- `listing.sizeMismatchWarning` — boolean, default true, switch. Reactive getter: `getSizeMismatchWarning()`.

Full list: `AppearanceSection`, `ListingSection`,
`FileOperationsSection`, `KeyboardShortcutsSection`, `NetworkSection`, `LoggingSection`, `McpServerSection`,
`UpdatesSection`, `ThemesSection`, `AdvancedSection`, `DriveIndexingSection`, `AiSection`, `LicenseSection`.

`AiSection` is a hybrid special section (like `LicenseSection` above): it combines dynamic runtime state from the
backend (via `getAiRuntimeStatus()` and Tauri events) with registry settings (`ai.provider`, `ai.cloudProvider`,
`ai.cloudProviderConfigs`, etc.). It conditionally renders provider-specific content, handles auto-stop/start of the
local server on provider switch. Context size changes are not auto-applied; the user must click an explicit "Apply"
button, which triggers a server restart. A RAM gauge (stacked bar) shows memory usage relative to system total, with
warning icons at >70% and >90% projected usage. System memory info is polled every 5 seconds via
`get_system_memory_info`. The "Cloud / API" provider mode uses a preset dropdown (`cloud-providers.ts`) with
per-provider API key storage in a JSON blob (`ai.cloudProviderConfigs`). Old flat settings (`ai.openaiApiKey`,
`ai.openaiBaseUrl`, `ai.openaiModel`) are migrated on first load. The Cloud/API section includes a two-step connection
check (`check_ai_connection` Tauri command) that auto-triggers on API key or base URL changes (1s debounce), fetches
available models from the `/models` endpoint, and shows connection status (connected, auth error, unreachable). When
models are available, the Model field becomes a combobox with filtered dropdown; otherwise it's a plain text input.

### Components (`components/`)

11 reusable setting UI primitives used by section components: `SettingsSection` (wrapper providing shared section title
and action button styles), `SettingRow`, `SettingSwitch`, `SettingSelect`, `SettingSlider`, `SettingNumberInput`,
`SettingPasswordInput` (supports both settings-store-driven and controlled/external value+onchange modes),
`SettingRadioGroup`, `SettingToggleGroup`, `SettingsSidebar`, `SettingsContent`. Also `SectionSummary` for
collapsed-section previews.

### 50-50 split layout guideline

`SettingRow` has a `split` prop that enforces a 50-50 grid layout (label left, control right). This keeps left edges of
controls vertically aligned across rows for visual consistency. The settings window is resizable, so the split is
percentage-based, not pixel-based.

**When to use `split`:** Setting rows where the control is a select, text input, password input, slider, number input,
radio group, or combobox — anything that benefits from consistent horizontal alignment.

**When NOT to use `split`:**

- Switches (too small; 50-50 wastes space and doesn't improve alignment)
- Toggle groups (multi-button controls that may not fit in 50% width at narrow window sizes)
- Full-width custom layouts (keyboard shortcuts table, license card, advanced auto-generated rows)

**When adding a new setting row**, decide if it should use `split` based on the rules above. If the control is a
dropdown, text field, slider, or similar right-aligned input, add `split` to `<SettingRow>`. Description text below each
row intentionally spans the full width.

### Other files

- **cloud-providers.ts** — Cloud provider preset definitions (OpenAI, Anthropic, Groq, etc.) and per-provider config
  helpers (`getProviderConfigs`, `setProviderConfig`, `resolveCloudConfig`). Used by `AiSection` and the startup flow in
  `+layout.svelte` to resolve the effective API key, base URL, and model before calling `configureAi`.
- **settings-search.ts** — Fuzzy search over setting definitions; returns ranked matches with highlight ranges
- **settings-applier.ts** — Listens for setting changes and applies side effects (CSS vars, backend config sync)
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

### Why hybrid declarative registry with custom UI?

JSON schema → generated UI was rejected (loses per-section UX polish). Manual pages + parser script was rejected (parser
is a second source of truth that drifts on refactors). Full architectural enforcement was overkill for a solo dev +
agents workflow. The hybrid approach gives: search that works perfectly (registry IS the search index), full UX freedom
per section, single source of truth for metadata, and CI catches missing UI or orphaned registry entries automatically.
UI components use Ark UI (see `../ui/CLAUDE.md`).

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

When shortcuts are modified, menu accelerators don't auto-update. Call `invoke('update_menu_accelerator')` for commands
that have menu items (`view.fullMode`, `view.briefMode`). Most commands don't need this.

### Advanced section is auto-generated

Settings with `showInAdvanced: true` appear in the Advanced section with auto-generated UI. No custom component needed.
Just add to registry and it works.

### Density mapping is internal

Users see "Compact/Comfortable/Spacious" but the code sees rowHeight/iconSize pixel values. The `densityMappings` object
in `types.ts` bridges this gap. Don't expose raw pixel values in the UI.

### Shortcuts conflict only when scopes overlap

`⌘N` in "File list" scope and `⌘N` in "Settings window" scope do NOT conflict because their scope hierarchies don't
overlap. Only warn when the same key combo is used in overlapping scopes (e.g., "File list" and "Main window").

### No undo

Both settings and shortcuts save immediately (after debounce). There's no undo stack. Users must use "Reset to default"
to recover from mistakes.
