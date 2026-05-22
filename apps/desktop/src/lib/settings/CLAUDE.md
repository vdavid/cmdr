# Settings system

## Live-apply rule

**Every setting MUST apply immediately without restart.** The frontend side of this contract lives in
`settings-applier.ts`: every setting that the backend (or a global JS module) reads must have a matching `case` in the
`handleSettingChange` switch that calls the right Tauri command or module helper. When adding a new setting that changes
backend behavior, you MUST add: (a) a Tauri command on the Rust side (see the mirrored rule in
`src-tauri/src/settings/CLAUDE.md`), (b) a typed wrapper in `$lib/tauri-commands/settings.ts`, and (c) an
`onSettingChange` case in `settings-applier.ts` that invokes it. Restart-required is a bug, not a design choice. If the
setting looks "structural" (like re-opening a TCP connection, rebinding a port, swapping a thread pool), still
live-apply. Reconnect, rebind, restart the worker, whatever it takes. **MUST.** No exceptions.

## Purpose

The settings system provides user-configurable options for Cmdr through a registry-based architecture. All settings are
defined once in `settings-registry.ts` and accessed uniformly by both UI and MCP tools.

## Architecture

### Registry (`settings-registry.ts`)

Single source of truth for all settings. Each `SettingDefinition` contains:

- `id`: Unique key (e.g., `appearance.uiDensity`)
- `section`: Path in settings tree (e.g., `['Appearance', 'Colors and formats']`)
- `type`: boolean, number, string, enum, duration
- `default`: Default value
- `constraints`: Type-specific validation (min/max, enum options, etc.)
- `component`: UI hint (switch, select, slider, etc.)

### Store (`settings-store.ts`)

- Uses `tauri-plugin-store` for persistence to `~/Library/Application Support/com.veszelovszki.cmdr/settings.json`
- Debounced saves: 500ms after last change, atomic write (temp file + rename)
- In-memory cache for synchronous reads via `getSetting()`
- Cross-window sync: emits `settings:changed` events when values change

### Text size (`appearance.textSize`)

`appearance.textSize` (slider 50–200%, default 100%) compounds with the macOS Accessibility > Display > Text Size value
to produce the effective scale that `lib/text-size.ts` writes to `--font-scale` on `:root`. **Compounding lives in
exactly one place**: `text-size.ts`'s `computeAndApply()`. The CSS
`html { font-size: calc(16px * var(--font-scale, 1)) }` plus rem-based `--font-size-*` tokens in `app.css` cover
typography; `applyDensity()` in `settings-applier.ts` multiplies row-height/icon-size/density-spacing by the same
`--font-scale` so layout grows with text. After each scale change, `text-size.ts` re-triggers
`ensureFontMetricsLoaded()` on a 1 s debounce so Rust gets fresh Brief-mode width data for the new font ID.

### Date display (one source of truth)

Every site that shows a modified date in the UI flows through one entry point:

- **`formatDateForDisplay(ts, format, customFormat, nowMs?)`** in `format-utils.ts`: pure. Returns a `FormattedDate`
  with the joined `text` and structured `parts` (an ordered list of `DateSegment`s per half). Each segment carries a
  `text` and an optional `ageClass` covering one of four per-component tiers (year, month, day, time). Handles all four
  format modes: token-based (`iso`, `short`, `custom`, default) via `applyTokens`, and `system` via
  `Intl.DateTimeFormat#formatToParts` (component type comes from `part.type`, not from string-parsing locale output).
- Per-component coloring rules live in `age-tier-utils.ts`: `tierForYear` colors every year (current → `age-fresh`, last
  → `age-recent`, two back → `age-aging`, three or more back → `age-old`). `tierForMonth` only colors when the year
  matches now (same scale). `tierForDay` only colors when the year and month both match (today / yesterday / two days /
  three+ days). `tierForTime` only colors when the file's date equals today, distance in full hours. Future timestamps
  in any component clamp to the freshest tier. Segments outside their coloring window carry `ageClass: null`, and the
  renderer leaves them in default text color.
- **`formattedDate(ts)`** in `reactive-settings.svelte.ts`: reactive wrapper that reads the current setting values. This
  is the canonical entry point for the rest of the app.
- **`<DateLabel modifiedAt={ts} />`** in `$lib/ui/DateLabel.svelte`: the render-side equivalent. Use it anywhere a
  modified date appears in the UI and you don't have special layout needs (status bar, dialogs, search results, etc.).
  It walks `parts.left` / `parts.right` and wraps each segment with a non-null `ageClass` in `<span class={ageClass}>`.
- `FullList.svelte` is the one consumer that opts out of `<DateLabel>` because its column-alignment story needs the two
  halves rendered into specific elements (`.date-left` / `.date-right`). It uses the same `formattedDate(...)` data; do
  the same if you add another consumer with bespoke layout.
- `buildDateTooltip` in `selection-info-utils.ts` mirrors the renderer for HTML tooltips: it walks segments and wraps
  the colored ones into `<span class="age-…">` directly.
- The plain-string shortcut `formatDateTime(ts)` is `formattedDate(ts).text`. Use it for tooltips, MCP responses,
  clipboard copies, anywhere you need a one-line label.

### Color palettes (size + date)

`appearance.sizeColors` (default `none`) and `appearance.dateColors` (default `none`) each pick a color palette applied
via `data-size-colors` / `data-date-colors` attributes on `<html>`. Settings applier wires both. CSS tokens
(`--color-size-*`, `--color-age-*`) live in `app.css`. Date coloring uses four tiers (`age-fresh`, `age-recent`,
`age-aging`, `age-old`) applied per-component (year, month, day, time) by the helpers in
`lib/settings/age-tier-utils.ts`. The setting value `app` (renamed from the older `accent`) refers to the user-facing
"app color" (internally the underlying CSS token is still `--color-accent`).

### Reactive state (`reactive-settings.svelte.ts`)

- Svelte 5 `$state` for settings that affect UI rendering (density, date format, file size format, directory sort mode)
- Subscribes to `onSettingChange()` and updates reactive values immediately
- Provides derived getters like `getRowHeight()` based on current density

### Sections (`sections/`)

Top-level sidebar order (declared in `SettingsSidebar.svelte`'s `TOP_LEVEL_ORDER`; keep in sync with the E2E test in
`settings.spec.ts`):

1. **Appearance** — `Colors and formats`, `Zoom and density`, `File and folder sizes`, `Listing`
2. **Behavior** — `File operations`, `Drive indexing`, `Search`
3. **AI** (no subsections)
4. **File systems** — `SMB/Network shares`, `MTP (Android/Kindle/cameras)`, `Git`
5. **Viewer** (no subsections)
6. **Keyboard shortcuts** (special, non-registry)
7. **Developer** — `MCP server`, `Logging`
8. **Updates** (no subsections)
9. **License** (special, non-registry)
10. **Advanced** (special, auto-generated from `showInAdvanced: true` entries)

Section ↔ component map (`sections/`):

- `AppearanceSection.svelte` → `Appearance > Colors and formats` (theme mode, app color, size/date colors, date/time
  format, striped rows)
- `AppearanceZoomSection.svelte` → `Appearance > Zoom and density` (text size, UI density)
- `AppearanceSizesSection.svelte` → `Appearance > File and folder sizes` (size display, size unit, file size format,
  size mismatch warning). The size-unit toggle group's `kB` tile reflects the binary/SI base live (binary → `KB`, SI →
  `kB`), via `SettingToggleGroup`'s `labelOverrides` prop.
- `ListingSection.svelte` → `Appearance > Listing` (document icons, directory sort, brief column width)
- `FileOperationsSection.svelte` → `Behavior > File operations` (extension changes only; `maxConflictsToShow` and
  `progressUpdateInterval` live in Advanced)
- `DriveIndexingSection.svelte` → `Behavior > Drive indexing` (toggle + clear-index action)
- `SearchSection.svelte` → `Behavior > Search` (auto-apply switch; mirrors the `search.recentSearches.maxCount` row from
  Advanced so users hunting under "search" find it)
- `AiSection.svelte` (+ `AiCloudSection.svelte`, `AiLocalSection.svelte`) → `AI`
- `NetworkSection.svelte` → `File systems > SMB/Network shares`
- `MtpSection.svelte` → `File systems > MTP (Android/Kindle/cameras)`
- `GitSection.svelte` → `File systems > Git`
- `ViewerSection.svelte` → `Viewer`
- `KeyboardShortcutsSection.svelte` → `Keyboard shortcuts` (special; renders the shortcut table)
- `McpServerSection.svelte` → `Developer > MCP server`
- `LoggingSection.svelte` → `Developer > Logging`
- `UpdatesSection.svelte` → `Updates`
- `LicenseSection.svelte` → `License` (special; reads `getLicenseInfo`/`getLicenseStatus`)
- `AdvancedSection.svelte` → `Advanced` (auto-generated UI for every `showInAdvanced: true` entry)

`AdvancedSection` includes `advanced.maxLogStorageMb`: number, default 200, range 0–5000, MB suffix. `0` disables log
storage entirely (the `Folder` target is dropped from the plugin builder, no error reports possible). Toggling between
`0` and any non-zero value, or raising the cap beyond its baked-in value, requires an app restart (the in-RAM cap
updates live but the rotation strategy doesn't).

`AdvancedSection` also includes `fileExplorer.typeToJump.resetDelay`: number, default 1000 ms, range 300–3000, step 100,
ms suffix. Reactive getter: `getTypeToJumpResetDelay()`. The type-to-jump factory in
`file-explorer/pane/type-to-jump-state.svelte.ts` reads this via its `getResetMs` callback on every keystroke, so slider
changes take effect on the next keystroke without restart.

`UpdatesSection` includes `updates.errorReports`: boolean, default false, switch. Opt-in for Flow B (auto-send error
reports when a user-visible error fires). Flow A (the **Help > Send error report…** menu item and the toast button) is
always available regardless of this setting. Clicking is the consent.

`NetworkSection` includes `network.enabled`: boolean, default true, switch. The top-of-section toggle. When off, the
volume picker shows "Network (disabled)" and the backend stops mDNS + clears discovered hosts. Below the switch is a
non-interactive Local Network access info card with a deep link to System Settings > Privacy & Security > Local Network
(via `openSystemSettingsUrl`). See `network/CLAUDE.md` (frontend and backend) for the full lifecycle.

`AiSection` is a hybrid special section (like `LicenseSection` above): it combines dynamic runtime state from the
backend (via `getAiRuntimeStatus()` and Tauri events) with registry settings (`ai.provider`, `ai.cloudProvider`,
`ai.cloudProviderConfigs`, etc.). It's split into three files:

- **`AiSection.svelte`**: Thin wrapper. Loads initial AI status, renders the provider toggle (Off / Cloud / Local),
  handles provider switching (auto-stops local server when switching away), and conditionally renders one of the two
  sub-sections.
- **`AiCloudSection.svelte`**: Cloud/API provider config. Provider preset dropdown (`cloud-providers.ts`), per-provider
  endpoint URL and model stored in `ai.cloudProviderConfigs`, per-provider API key stored in the OS secret store via
  `saveAiApiKey` / `getAiApiKey`. Includes a two-step connection check (`check_ai_connection` Tauri command) that
  auto-triggers on API key or base URL changes (1s debounce), fetches available models from the `/models` endpoint, and
  shows connection status (connected, auth error, unreachable). When models are available, the Model field becomes a
  combobox with filtered dropdown; otherwise it's a plain text input.
- **`AiLocalSection.svelte`**: Local LLM management. Server lifecycle (start/stop), model download with multi-step
  install tracking, context window setting with explicit "Apply" button (triggers server restart), RAM gauge (stacked
  bar) showing memory usage relative to system total with warning icons at >70% and >90% projected usage, system memory
  polled every 5 seconds via `get_system_memory_info`, and delete model confirmation dialog.

Cloud and local are independent features with separate state machines (`connectionStatus` for cloud, `installStep` for
local). They share almost nothing except the `provider` toggle and `shouldShow` function, passed as props from the
wrapper.

### Mirroring a setting in multiple sections

A setting can appear in more than one UI section without duplicating it in the registry. Each `Section.svelte`
explicitly renders its rows by ID (`getSettingDefinition('foo.bar')` + `shouldShow('foo.bar')` + `SettingRow` /
`SettingSwitch` / etc.), so adding the same ID block to a second section just makes it visible there too.

Example (live): `appearance.sizeColors` is registered under `Appearance > Colors and formats` and rendered there in
`AppearanceSection.svelte`. The same row is mirrored in `AppearanceSizesSection.svelte` because users hunt for it under
"file sizes" just as often.

What this gets you for free:

- **Single source of truth.** Label, description, keywords, default, type constraints, persistence — all live once in
  the registry. Editing them updates every place the setting is rendered.
- **Search returns one hit per setting.** `searchSettings` walks the registry, not the UI tree, so a search for "size
  colors" produces exactly one result that links to the registered (canonical) section.
- **Active search still filters correctly.** `shouldShow(id)` returns `true` whenever the query matches the id,
  regardless of which section is currently rendering. So a "size" search inside the mirror section keeps the row
  visible.

Edge case: the sidebar's "section contains a match" highlight reads only `setting.section`, so it only marks the
canonical section. Mirror sections are discoverable by browsing, not via the sidebar highlight. If a setting earns a
sidebar highlight in multiple sections, add a small optional `mirrorSections: SectionPath[]` field to the registry and
fold it into `getMatchingSectionsForSettings` in `settings-search.ts` — but ship the mirror without it first; the
canonical-only highlight is usually the right behavior.

### Components (`components/`)

13 reusable setting UI primitives used by section components: `SettingsSection` (wrapper providing shared section title
and action button styles), `SettingRow`, `SettingSwitch`, `SettingCheckbox` (less prominent than switch, for secondary
boolean options), `SettingSelect`, `SettingSlider`, `SettingNumberInput`, `SettingPasswordInput` (supports both
settings-store-driven and controlled/external value+onchange modes), `SettingRadioGroup`, `SettingToggleGroup`,
`SettingColorSwatchPicker` (circle trigger + 4×4 swatch popover for picking a tint color; used by Appearance > Colors
and formats for the per-volume-type pane tints), `SettingsSidebar`, `SettingsContent`. Also `SectionSummary` for
collapsed-section previews.

### 50-50 split layout guideline

`SettingRow` has a `split` prop that enforces a 50-50 grid layout (label left, control right). This keeps left edges of
controls vertically aligned across rows for visual consistency. The settings window is resizable, so the split is
percentage-based, not pixel-based.

**When to use `split`:** Setting rows where the control is a select, text input, password input, slider, number input,
radio group, or combobox, anything that benefits from consistent horizontal alignment.

**When NOT to use `split`:**

- Switches (too small; 50-50 wastes space and doesn't improve alignment)
- Toggle groups (multi-button controls that may not fit in 50% width at narrow window sizes)
- Full-width custom layouts (keyboard shortcuts table, license card, advanced auto-generated rows)

**When adding a new setting row**, decide if it should use `split` based on the rules above. If the control is a
dropdown, text field, slider, or similar right-aligned input, add `split` to `<SettingRow>`. Description text below each
row intentionally spans the full width.

### Other files

- **cloud-providers.ts**: Cloud provider preset definitions (OpenAI, Anthropic, Groq, etc.) and per-provider config
  helpers (`getProviderConfigs`, `setProviderConfig`, `resolveCloudConfig`). Used by `AiSection` and the startup flow in
  `+layout.svelte` to resolve the effective base URL and model. The API key is fetched separately from the OS secret
  store via `getAiApiKey(providerId)` before calling `configureAi`.
- **settings-search.ts**: Fuzzy search over setting definitions; returns ranked matches with highlight ranges
- **settings-applier.ts**: Listens for setting changes and applies side effects (CSS vars, backend config sync)
- **network-settings.ts**: Network-specific setting helpers (proxy config, SMB auth defaults)
- **settings-window.ts**: Logic for opening/focusing/closing the settings window (Tauri window management). Accepts an
  optional `section` array (e.g. `['File systems', 'SMB/Network shares']`) to deep-link a specific section. Two delivery
  paths: (a) new-window: JSON-encoded array on the URL as `?section=...` (JSON because section names can contain `/`,
  e.g. "SMB/Network shares"); (b) already-open window: emits a `navigate-to-section` Tauri event the settings page
  listens for. The settings page also reads the URL param at mount, so reloads or fresh-opens land on the same section.
  Position: opens centered on the main window on first open of the session (via `lib/window-positioning.ts`). After
  that, the position+size persists in-memory (via the `get_child_window_rect` / `set_child_window_rect` Tauri commands)
  so reopening lands in the same spot. On app start the cache is empty again. Saved rects that no longer fit any monitor
  (display disconnected, etc.) are clamped to the nearest monitor.
- **format-utils.ts**: Shared formatters used in settings UI (e.g., duration, file-size display strings). Date/time is
  covered in detail under § "Date display" above. `formatDateForDisplay` is the canonical entry point. Built-in `iso`
  and `short` formats include a `|` internally so the file-list renderer can split the date and time halves into two
  aligned columns; custom format default is `YYYY-MM-DD | HH:mm`. The `'system'` formatter is memoized at module scope
  (constructing `Intl.DateTimeFormat` per call shows up in virtualized scroll profiles).
- **mcp-main-bridge.ts**: MCP bridge for settings; handles `mcp-get-all-settings` and `mcp-set-setting` round-trip
  events in the main window (always alive), enabling AI agents to query and modify settings without the settings window
  open

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

### Why store cloud AI API keys in the OS secret store, not `settings.json`?

API keys live in the OS-native secret store (macOS Keychain, Linux Secret Service, or an encrypted file fallback on
Linux without Secret Service) via `crate::secrets`. Access goes through the `saveAiApiKey` / `getAiApiKey` /
`deleteAiApiKey` / `hasAiApiKey` Tauri commands. `ai.cloudProviderConfigs` in `settings.json` only holds non-secret
fields (`model`, `baseUrl`). This keeps API keys out of Time Machine, cloud-sync backups, and any tool that mirrors
`~/Library/Application Support`. Same secret store backs SMB credentials. See `src-tauri/src/secrets/CLAUDE.md`.

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

### Hidden internal-state settings

`hidden: true` on a `SettingDefinition` excludes it from both the main section tree and the Advanced section, but the
value is still persisted via the same store and accessible via `getSetting`/`setSetting`. Use this for internal flags
the backend or business logic needs to track but the user shouldn't see (for example, `network.firstTriggerDone`, which
records whether we've ever performed a gated network action so subsequent launches can start mDNS eagerly without
re-prompting).

### Density mapping is internal

Users see "Compact/Comfortable/Spacious" but the code sees rowHeight/iconSize pixel values. The `densityMappings` object
in `types.ts` bridges this gap. Don't expose raw pixel values in the UI.

### Shortcuts conflict only when scopes overlap

`⌘N` in "File list" scope and `⌘N` in "Settings window" scope do NOT conflict because their scope hierarchies don't
overlap. Only warn when the same key combo is used in overlapping scopes (e.g., "File list" and "Main window").

### No undo

Both settings and shortcuts save immediately (after debounce). There's no undo stack. Users must use "Reset to default"
to recover from mistakes.

### Escape closing the settings window must defer `close()` via two `requestAnimationFrame`s

`routes/settings/+page.svelte`'s `handleKeydown` wraps `getCurrentWindow().close()` in two nested rAFs (mirroring
`routes/viewer/+page.svelte`'s `closeWindow()`). Calling `close()` synchronously from inside the keydown handler runs
the destruction on the same GTK main-loop tick that handled the keydown. On Linux/webkit2gtk this stalls **any IPC call
queued behind the destruction from other webviews** (e.g. the main window) for an undefined time (observed 60-65 % of
test runs landing in the fast path, others timing out past 30 s). macOS uses WKWebView with per-webview processes and
doesn't exhibit the issue, so it's invisible there. The +16 ms from the rAFs is invisible to the user.

When adding a new self-closing webview (escape, close button, etc.), defer the `close()` call the same way. See commit
`46481b29` for the bug-fix that revealed this.
